//! Application state shared across screens.

use dioxus::prelude::*;

use ft_types::api::BotSummary;

use crate::api;

/// Key under which the active bot is persisted, server-side.
const ACTIVE_BOT: &str = "active_bot";
const THEME: &str = "theme";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "light" => Self::Light,
            _ => Self::Dark,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }
}

/// Shared handle to app state. `Signal` is `Copy`, so this is cheap to pass
/// around and to capture in event handlers.
#[derive(Clone, Copy)]
pub struct App {
    pub bots: Signal<Vec<BotSummary>>,
    pub active: Signal<Option<String>>,
    pub theme: Signal<Theme>,
    /// True until the first bot list has loaded, so the UI can avoid flashing
    /// the "no bots" empty state before it knows.
    pub loading: Signal<bool>,
    pub error: Signal<Option<String>>,
}

impl App {
    /// Installs the state into context and kicks off the initial load.
    pub fn provide() -> Self {
        let app = use_context_provider(|| App {
            bots: Signal::new(Vec::new()),
            active: Signal::new(None),
            theme: Signal::new(Theme::Dark),
            loading: Signal::new(true),
            error: Signal::new(None),
        });

        use_future(move || async move {
            let mut app = app;
            app.load_initial().await;
        });

        app
    }

    pub fn get() -> Self {
        use_context::<App>()
    }

    /// Loads bots, the persisted active bot and the theme.
    async fn load_initial(&mut self) {
        if let Ok(Some(value)) = api::setting(THEME).await {
            self.theme.set(Theme::parse(&value));
        }

        let saved = api::setting(ACTIVE_BOT).await.ok().flatten();
        self.reload_bots(saved).await;
        self.loading.set(false);
    }

    /// Refetches the bot list, keeping the active selection valid.
    ///
    /// `prefer` is the bot to select if it still exists; otherwise the first
    /// bot is used, matching the Flutter app's fallback to `_bots.first`.
    pub async fn reload_bots(&mut self, prefer: Option<String>) {
        match api::list_bots().await {
            Ok(bots) => {
                let wanted = prefer.or_else(|| self.active.peek().clone());
                let still_there = wanted.filter(|id| bots.iter().any(|b| &b.id == id));
                let next = still_there.or_else(|| bots.first().map(|b| b.id.clone()));
                self.active.set(next);
                self.bots.set(bots);
                self.error.set(None);
            }
            Err(e) => self.error.set(Some(e.message)),
        }
    }

    /// Selects a bot and remembers the choice.
    pub fn select(&mut self, id: String) {
        self.active.set(Some(id.clone()));
        spawn(async move {
            let _ = api::set_setting(ACTIVE_BOT, Some(id)).await;
        });
    }

    pub fn active_bot(&self) -> Option<BotSummary> {
        let active = self.active.read();
        let id = active.as_ref()?;
        self.bots.read().iter().find(|b| &b.id == id).cloned()
    }

    pub fn toggle_theme(&mut self) {
        let next = self.theme.peek().toggled();
        self.theme.set(next);
        spawn(async move {
            let _ = api::set_setting(THEME, Some(next.as_str().to_owned())).await;
        });
    }
}

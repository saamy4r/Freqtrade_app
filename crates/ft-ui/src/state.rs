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
    /// Bumped whenever the server reports new data. Screens read it inside
    /// their resource closure, which makes a push arrive as an ordinary
    /// dependency change rather than a separate update path.
    pub revision: Signal<u64>,
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
            revision: Signal::new(0),
        });

        use_future(move || async move {
            let mut app = app;
            app.load_initial().await;
        });

        // Live updates from the server.
        //
        // The browser calls back on an event, but that callback runs outside
        // Dioxus's runtime, and writing a signal from there marks it dirty
        // without scheduling a render — the value changes and nothing redraws.
        // So the callback only sends on a channel, and the signal is written
        // here inside the future, where the runtime is active.
        //
        // The subscription itself lives in this future, which lives as long as
        // the app; dropping it would close the stream and, with it, stop the
        // server's background sync.
        use_future(move || async move {
            use futures_util::StreamExt;

            let (sender, mut receiver) = futures_channel::mpsc::unbounded::<()>();
            let _subscription = crate::events::subscribe(move || {
                let _ = sender.unbounded_send(());
            });

            let mut revision = app.revision;
            while receiver.next().await.is_some() {
                revision += 1;
            }
        });

        app
    }

    /// Reads the shared state out of context.
    ///
    /// This is a hook. It must be called from a component body, never from an
    /// event handler or a spawned future — doing so runs a hook outside render
    /// and corrupts hook ordering, which manifests as the renderer locking up
    /// rather than as a clean panic. `App` is `Copy`, so capture it once in the
    /// component body and move it into handlers instead.
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

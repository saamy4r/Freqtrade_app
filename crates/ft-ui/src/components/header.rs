//! Top bar: active bot, its mode badge, and the theme toggle.

use dioxus::prelude::*;

use crate::api;
use crate::state::{App, Theme};

/// Which badge the header shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// No bot selected yet, so there is nothing to report. Distinct from
    /// `Connecting`: folding the two together left a spinner running forever
    /// on the empty-state screen.
    None,
    Connecting,
    Dry,
    Live,
    Offline,
}

#[component]
pub fn Header() -> Element {
    let mut app = App::get();
    let active = app.active;

    // Re-runs whenever the active bot changes. Loading the config is also how
    // we learn whether the bot is reachable at all.
    //
    // The signal is read in the closure body, NOT inside the async block.
    // Reading it inside the future makes the resource subscribe to something
    // that changes as a result of its own completion, so it restarts forever —
    // a livelock that presents as the browser tab hanging on the first
    // re-render, with no panic and nothing in the console.
    let config = use_resource(move || {
        let id = active.read().clone();
        async move {
            let id = id?;
            Some(api::config(&id, false).await)
        }
    });

    let mode = match &*config.read_unchecked() {
        // The resource has not resolved yet.
        None => Mode::Connecting,
        // It resolved, and there was no active bot to ask about.
        Some(None) => Mode::None,
        Some(Some(Ok(envelope))) if envelope.stale => Mode::Offline,
        Some(Some(Ok(envelope))) => {
            if envelope.data.dry_run {
                Mode::Dry
            } else {
                Mode::Live
            }
        }
        Some(Some(Err(_))) => Mode::Offline,
    };

    let name = app
        .active_bot()
        .map(|b| b.name)
        .unwrap_or_else(|| "Freqtrade".to_owned());
    let theme = *app.theme.read();

    rsx! {
        header { class: "header",
            span { class: "header__name", "{name}" }
            match mode {
                Mode::None => rsx! {},
                Mode::Connecting => rsx! { span { class: "spinner muted" } },
                Mode::Dry => rsx! { span { class: "badge badge--dry", "DRY" } },
                Mode::Live => rsx! { span { class: "badge badge--live", "LIVE" } },
                Mode::Offline => rsx! { span { class: "badge badge--offline", "OFFLINE" } },
            }
            span { class: "header__spacer" }
            button {
                class: "header__btn",
                title: "Switch theme",
                onclick: move |_| app.toggle_theme(),
                if theme == Theme::Dark { "☀" } else { "🌙" }
            }
        }
    }
}

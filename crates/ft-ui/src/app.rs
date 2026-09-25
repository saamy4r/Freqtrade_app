//! Root component, routing and layout.

use dioxus::prelude::*;

use crate::components::{BottomNav, Header};
use crate::screens::{Bots, Chart, ClosedTrades, Dashboard, Logs, OpenTrades};
use crate::state::App as AppState;

const MAIN_CSS: Asset = asset!("/assets/main.css");

/// Screens, in the legacy tab order.
#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
        #[route("/")]
        OpenTrades {},
        #[route("/closed")]
        ClosedTrades {},
        #[route("/dashboard")]
        Dashboard {},
        #[route("/chart")]
        Chart {},
        #[route("/logs")]
        Logs {},
        #[route("/bots")]
        Bots {},
}

#[component]
pub fn App() -> Element {
    let state = AppState::provide();
    let theme = state.theme.read().as_str();

    // A failed start, or a crash the previous run recorded, is shown instead
    // of the app. On a phone there is no console to check, so the screen is
    // the only place a reason can reach the user.
    let blocking = crate::startup_failure().map(|r| ("The app could not start", r));
    let previous_crash = crate::logging::last_crash();

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        // Dioxus renders into <body>, so the theme attribute goes on a wrapper
        // rather than <html>; the CSS selector matches either.
        div { "data-theme": "{theme}", class: "shell",
            if let Some((title, detail)) = blocking {
                FailureReport { title: title.to_string(), detail: detail.to_string() }
            } else {
                if let Some(note) = previous_crash {
                    CrashNotice { note: note.to_string() }
                }
                Router::<Route> {}
            }
        }
    }
}

/// Chrome shared by every screen.
///
/// With no bots configured there is nothing for five of the six tabs to show,
/// so the Bots screen takes over the whole shell — the same thing `AppShell`
/// did when `_activeBot` was null.
#[component]
fn Shell() -> Element {
    let state = AppState::get();
    let has_bots = !state.bots.read().is_empty();
    let settled = !*state.loading.read();
    let has_active = state.active.read().is_some();

    rsx! {
        Header {}
        if settled && !has_bots {
            Bots {}
        } else if !has_active {
            // Screens are not mounted until a bot is selected. Mounting them
            // earlier meant each one ran its fetch with no bot id, returned
            // nothing, and then sat on that result — a permanent spinner
            // behind a header that was showing the bot's name perfectly well.
            div { class: "empty", span { class: "spinner" } }
        } else {
            Outlet::<Route> {}
        }
        BottomNav {}
    }
}

/// Shown in place of the app when it cannot run at all.
#[component]
fn FailureReport(title: String, detail: String) -> Element {
    rsx! {
        div { class: "content",
            div { class: "empty",
                div { class: "empty__icon", "⚠" }
                div { class: "empty__title", "{title}" }
                pre { class: "failure", "{detail}" }
            }
        }
    }
}

/// Shown once after a crash, above an otherwise working app.
#[component]
fn CrashNotice(note: String) -> Element {
    let mut dismissed = use_signal(|| false);
    if dismissed() {
        return rsx! {};
    }
    rsx! {
        div { class: "content", style: "padding-bottom:0",
            div { class: "banner banner--crash",
                div { style: "flex:1;min-width:0",
                    div { style: "font-weight:650;margin-bottom:4px", "The app closed unexpectedly last time" }
                    pre { class: "failure", "{note}" }
                }
                button {
                    class: "btn btn--ghost btn--small",
                    onclick: move |_| dismissed.set(true),
                    "Dismiss"
                }
            }
        }
    }
}

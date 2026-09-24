//! Root component, routing and layout.

use dioxus::prelude::*;

use crate::components::{BottomNav, Header};
use crate::screens::{Bots, ClosedTrades, Dashboard, OpenTrades, Placeholder};
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

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        // Dioxus renders into <body>, so the theme attribute goes on a wrapper
        // rather than <html>; the CSS selector matches either.
        div { "data-theme": "{theme}", class: "shell",
            Router::<Route> {}
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

    rsx! {
        Header {}
        if settled && !has_bots {
            Bots {}
        } else {
            Outlet::<Route> {}
        }
        BottomNav {}
    }
}

#[component]
fn Chart() -> Element {
    rsx! { Placeholder { title: "Chart", milestone: "M8" } }
}

#[component]
fn Logs() -> Element {
    rsx! { Placeholder { title: "Logs", milestone: "M9" } }
}

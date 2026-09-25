//! Bottom navigation, six tabs as in the app it replaces.

use dioxus::prelude::*;

use crate::app::Route;

#[component]
pub fn BottomNav() -> Element {
    let current: Route = use_route();

    // Icon, label, destination. Order matches the legacy tab order so muscle
    // memory survives the rewrite.
    let tabs = [
        ("📈", "Open", Route::OpenTrades {}),
        ("🕓", "Closed", Route::ClosedTrades {}),
        ("📊", "Dashboard", Route::Dashboard {}),
        ("📉", "Chart", Route::Chart {}),
        ("🖥", "Logs", Route::Logs {}),
        ("🤖", "Bots", Route::Bots {}),
    ];

    rsx! {
        nav { class: "nav",
            for (icon, label, route) in tabs {
                Link {
                    key: "{label}",
                    class: if std::mem::discriminant(&current) == std::mem::discriminant(&route) {
                        "nav__item nav__item--active"
                    } else {
                        "nav__item"
                    },
                    to: route,
                    span { class: "nav__icon", "{icon}" }
                    span { "{label}" }
                }
            }
        }
    }
}

//! Freshness and failure banners.

use dioxus::prelude::*;
use ft_types::api::ErrorKind;

/// "Offline · Last synced Xm ago", shown above a screen served from cache.
///
/// The server decides staleness, so every screen renders the same banner from
/// one field instead of each deciding for itself.
#[component]
pub fn OfflineBanner(last_synced: Option<String>) -> Element {
    rsx! {
        div { class: "banner",
            span { "⚠" }
            match last_synced {
                Some(ago) => rsx! { span { "Offline · last synced {ago}" } },
                None => rsx! { span { "Offline · showing cached data" } },
            }
        }
    }
}

/// A failed load, with the retry the error actually warrants.
///
/// The Flutter app showed one error screen for every cause. Here an
/// unreachable bot offers retry, bad credentials say so plainly, and an
/// internal failure is not dressed up as a network problem.
#[component]
pub fn ErrorView(kind: ErrorKind, message: String, on_retry: EventHandler<()>) -> Element {
    let (icon, title) = match kind {
        ErrorKind::Offline => ("📡", "Can't reach this bot"),
        ErrorKind::Auth => ("🔑", "Credentials rejected"),
        ErrorKind::NotFound => ("🔍", "Not found"),
        ErrorKind::BadRequest => ("⚠", "Bad request"),
        ErrorKind::Internal => ("⚠", "Something went wrong"),
    };

    rsx! {
        div { class: "empty",
            div { class: "empty__icon", "{icon}" }
            div { class: "empty__title", "{title}" }
            p { class: "muted", "{message}" }
            if kind == ErrorKind::Auth {
                p { class: "muted",
                    "Remove the bot and add it again with the correct password."
                }
            } else {
                div { style: "margin-top:16px",
                    button {
                        class: "btn btn--ghost",
                        onclick: move |_| on_retry.call(()),
                        "Retry"
                    }
                }
            }
        }
    }
}

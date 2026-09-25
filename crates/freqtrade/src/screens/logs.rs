//! Logs: the bot's own output, in a terminal.

use dioxus::prelude::*;

use ft_types::freqtrade::{LogEntry, LogSeverity};

use crate::api;
use crate::components::{ErrorView, OfflineBanner};
use crate::format;
use crate::screens::loader::synced_label;
use crate::state::App;

/// Id used to scroll the viewport to the newest line.
const VIEWPORT: &str = "log-viewport";

#[component]
pub fn Logs() -> Element {
    let app = App::get();
    let active = app.active;
    let revision = app.revision;
    let mut force = use_signal(|| false);

    let mut data = use_resource(move || {
        let id = active.read().clone();
        // A server push bumps this, which re-runs the resource. Reading it
        // here rather than reacting separately means live updates and manual
        // loads share one code path.
        let _ = revision.read();

        let forced = *force.peek();
        async move {
            let id = id?;
            let result = api::logs(&id, forced).await;
            if forced {
                let mut force = force;
                force.set(false);
            }
            Some(result)
        }
    });

    let mut refresh = move || {
        force.set(true);
        data.restart();
    };

    // Newest output is what matters, and it is at the bottom.
    let jump_to_latest = move || {
        document::eval(&format!(
            "const el = document.getElementById('{VIEWPORT}');
             if (el) el.scrollTop = el.scrollHeight;"
        ));
    };

    rsx! {
        div { class: "content",
            match &*data.read_unchecked() {
                None | Some(None) => rsx! { div { class: "empty", span { class: "spinner" } } },
                Some(Some(Err(e))) => rsx! {
                    ErrorView {
                        kind: e.kind,
                        message: e.message.clone(),
                        on_retry: move |_| refresh(),
                    }
                },
                Some(Some(Ok(envelope))) => {
                    let entries = envelope.data.entries.clone();
                    let count = entries.len();

                    rsx! {
                        if envelope.stale {
                            OfflineBanner { last_synced: synced_label(envelope.last_synced) }
                        }

                        div { class: "term",
                            div { class: "term__bar",
                                span { class: "term__dot" }
                                span { class: "term__count", "{count} lines" }
                                button {
                                    class: "term__btn",
                                    onclick: move |_| refresh(),
                                    "Refresh"
                                }
                                button {
                                    class: "term__btn",
                                    onclick: move |_| jump_to_latest(),
                                    "↓ Latest"
                                }
                            }

                            div {
                                class: "term__view",
                                id: "{VIEWPORT}",
                                // Scroll to the newest line as soon as the
                                // viewport exists, so the screen opens where a
                                // terminal would.
                                onmounted: move |_| jump_to_latest(),

                                if entries.is_empty() {
                                    div { class: "term__empty", "No log output." }
                                } else {
                                    for (index, entry) in entries.iter().enumerate() {
                                        LogRow { key: "{index}", entry: entry.clone() }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One log line.
///
/// Severity is carried by the level word itself as well as by colour, so the
/// rows stay readable without hue.
#[component]
fn LogRow(entry: LogEntry) -> Element {
    let severity = entry.severity();
    let class = match severity {
        LogSeverity::Error => "error",
        LogSeverity::Warning => "warning",
        LogSeverity::Debug => "debug",
        LogSeverity::Info => "info",
    };
    let time = entry
        .at()
        .map(|d| format::clock((d.unix_timestamp_nanos() / 1_000_000) as i64))
        .unwrap_or_default();

    rsx! {
        div { class: "term__row term__row--{class}",
            span { class: "term__time", "{time}" }
            span { class: "term__level term__level--{class}", "{entry.level}" }
            span { class: "term__msg", "{entry.message}" }
        }
    }
}

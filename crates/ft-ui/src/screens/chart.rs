//! Chart: price history for one pair, with the bot's trades drawn on it.

use dioxus::prelude::*;

use crate::api;
use crate::components::{ErrorView, OfflineBanner, PriceChart};
use crate::format;
use crate::screens::loader::synced_label;
use crate::state::App;

#[component]
pub fn Chart() -> Element {
    let app = App::get();
    let active = app.active;
    let mut selected = use_signal(|| None::<String>);
    let mut force = use_signal(|| false);

    // The dropdown's contents: whitelist plus any pair with an open trade.
    let pairs = use_resource(move || {
        let id = active.read().clone();
        async move {
            let id = id?;
            Some(api::pairs(&id, false).await)
        }
    });

    // For the dropdown. Reading here subscribes the component, which is what
    // we want: the list should appear as soon as it loads.
    let available: Vec<ft_types::api::ChartPair> = pairs
        .read_unchecked()
        .as_ref()
        .and_then(|p| p.as_ref())
        .and_then(|r| r.as_ref().ok())
        .map(|e| e.data.clone())
        .unwrap_or_default();

    let mut candles = use_resource(move || {
        let id = active.read().clone();
        // Both dependencies are read in the closure body. Computing the pair
        // outside and capturing the result would freeze this resource at the
        // value it had on first render -- which, before the pair list has
        // loaded, is None, leaving the chart spinning forever.
        let chosen = selected.read().clone();
        let default_pair = pairs
            .read()
            .as_ref()
            .and_then(|p| p.as_ref())
            .and_then(|r| r.as_ref().ok())
            .and_then(|e| {
                // Prefer a pair the bot is actually in: that is the one worth
                // looking at first.
                e.data
                    .iter()
                    .find(|p| p.open_profit_ratio.is_some())
                    .or_else(|| e.data.first())
                    .map(|p| p.pair.clone())
            });
        let pair = chosen.or(default_pair);
        let forced = *force.peek();
        async move {
            let (id, pair) = (id?, pair?);
            let result = api::candles(&id, &pair, forced).await;
            if forced {
                let mut force = force;
                force.set(false);
            }
            Some(result)
        }
    });

    let mut refresh = move || {
        force.set(true);
        candles.restart();
    };

    let charted = candles
        .read_unchecked()
        .as_ref()
        .and_then(|c| c.as_ref())
        .and_then(|r| r.as_ref().ok())
        .map(|e| e.data.pair.clone());
    let selected_pair = selected().or(charted);

    rsx! {
        div { class: "content",
            if available.is_empty() {
                div { class: "empty", span { class: "spinner" } }
            } else {
                div { class: "pair-bar",
                    select {
                        class: "pair-select",
                        value: selected_pair.clone().unwrap_or_default(),
                        onchange: move |event| selected.set(Some(event.value())),
                        for pair in available.iter() {
                            option {
                                key: "{pair.pair}",
                                value: "{pair.pair}",
                                // A pair the bot is currently in carries its
                                // live P/L, so the dropdown itself says where
                                // the action is.
                                match pair.open_profit_ratio {
                                    Some(ratio) => format!("{}  ({})", pair.pair, format::percent(ratio)),
                                    None => pair.pair.clone(),
                                }
                            }
                        }
                    }
                    button { class: "section__action", onclick: move |_| refresh(), "Refresh" }
                }

                match &*candles.read_unchecked() {
                    None | Some(None) => rsx! { div { class: "empty", span { class: "spinner" } } },
                    Some(Some(Err(e))) => rsx! {
                        ErrorView {
                            kind: e.kind,
                            message: e.message.clone(),
                            on_retry: move |_| refresh(),
                        }
                    },
                    Some(Some(Ok(envelope))) => rsx! {
                        if envelope.stale {
                            OfflineBanner { last_synced: synced_label(envelope.last_synced) }
                        }
                        PriceChart {
                            candles: envelope.data.candles.clone(),
                            overlays: envelope.data.overlays.clone(),
                        }
                        div { class: "muted", style: "font-size:12px;margin-top:8px",
                            "{envelope.data.pair} · {envelope.data.timeframe} · "
                            "{envelope.data.overlays.len()} trades in view"
                        }
                    },
                }
            }
        }
    }
}

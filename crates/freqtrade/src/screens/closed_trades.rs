//! Closed Trades: realised results, newest first.

use dioxus::prelude::*;

use ft_types::api::ClosedTrades as ClosedData;

use crate::api;
use crate::components::{ErrorView, OfflineBanner, StatTile, TradeCard};
use crate::format::{self, Tone};
use crate::screens::loader::synced_label;
use crate::state::App;

#[component]
pub fn ClosedTrades() -> Element {
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
            let result = api::closed(&id, forced).await;
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
                    let c: &ClosedData = &envelope.data;
                    let currency = c.stake_currency.clone();
                    let profit_tone = Tone::of(c.closed_profit);
                    let trades = c.trades.clone();
                    let total = c.total;

                    rsx! {
                        if envelope.stale {
                            OfflineBanner { last_synced: synced_label(envelope.last_synced) }
                        }

                        div { class: "stat-row",
                            StatTile {
                                label: "Portfolio value".to_string(),
                                value: format::money(c.portfolio_value, &currency),
                                tone: None,
                            }
                            StatTile {
                                label: "Realised profit".to_string(),
                                value: format::signed_money(c.closed_profit, &currency),
                                tone: Some(profit_tone.class().to_string()),
                            }
                        }

                        div { class: "section",
                            // `total` is everything the bot has closed; the
                            // list itself is one page of it.
                            span { class: "section__title",
                                "Closed trades ({trades.len()} of {total})"
                            }
                            button {
                                class: "section__action",
                                onclick: move |_| refresh(),
                                "Refresh"
                            }
                        }

                        if trades.is_empty() {
                            div { class: "empty",
                                div { class: "empty__icon", "📭" }
                                div { class: "empty__title", "No closed trades yet" }
                                p { "Results will appear here once the bot closes a position." }
                            }
                        } else {
                            for trade in trades {
                                TradeCard {
                                    key: "{trade.trade_id}",
                                    trade: trade.clone(),
                                    currency: currency.clone(),
                                    on_exit: None,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

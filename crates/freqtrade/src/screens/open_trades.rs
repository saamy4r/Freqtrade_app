//! Open Trades: portfolio summary, live positions, and force exit.

use dioxus::prelude::*;

use ft_types::api::Overview;
use ft_types::freqtrade::Trade;

use crate::api;
use crate::components::{ErrorView, OfflineBanner, StatTile, TradeCard};
use crate::format::{self, Tone};
use crate::screens::loader::synced_label;
use crate::state::App;

#[component]
pub fn OpenTrades() -> Element {
    let app = App::get();
    let active = app.active;
    let revision = app.revision;
    // Read with `peek` in the resource, so it is not a dependency: a refresh
    // is triggered by restarting the resource, not by this flag changing,
    // which would otherwise fetch twice.
    let mut force = use_signal(|| false);

    let mut data = use_resource(move || {
        // Signals read here, in the closure body, are the dependencies.
        // Reading them inside the async block instead makes the resource
        // restart on its own completion.
        let id = active.read().clone();
        // A server push bumps this, which re-runs the resource. Reading it
        // here rather than reacting separately means live updates and manual
        // loads share one code path.
        let _ = revision.read();

        let forced = *force.peek();
        async move {
            let id = id?;
            let result = api::overview(&id, forced).await;
            if forced {
                let mut force = force;
                force.set(false);
            }
            Some(result)
        }
    });
    // `Resource` is `Copy`, so refreshing is just a closure over it.

    let mut refresh = move || {
        force.set(true);
        data.restart();
    };

    let mut exiting = use_signal(|| None::<Trade>);

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
                    let o: &Overview = &envelope.data;
                    let stale = envelope.stale;
                    let currency = o.stake_currency.clone();
                    let pl_tone = Tone::of(o.open_pl);
                    let trades = o.open_trades.clone();

                    rsx! {
                        if stale {
                            OfflineBanner { last_synced: synced_label(envelope.last_synced) }
                        }

                        div { class: "stat-row stat-row--hero",
                            StatTile {
                                label: "Portfolio value".to_string(),
                                value: format::money(o.portfolio_value, &currency),
                                tone: None,
                            }
                        }
                        div { class: "stat-row",
                            StatTile {
                                label: "Free".to_string(),
                                value: format::money(o.free, &currency),
                                tone: None,
                            }
                            StatTile {
                                label: "Staked".to_string(),
                                value: format::money(o.used, &currency),
                                tone: None,
                            }
                            StatTile {
                                label: "Open P/L".to_string(),
                                value: format::signed_money(o.open_pl, &currency),
                                tone: Some(pl_tone.class().to_string()),
                            }
                        }

                        div { class: "section",
                            span { class: "section__title", "Open trades ({trades.len()})" }
                            button {
                                class: "section__action",
                                onclick: move |_| refresh(),
                                "Refresh"
                            }
                        }

                        if trades.is_empty() {
                            div { class: "empty",
                                div { class: "empty__icon", "💤" }
                                div { class: "empty__title", "No open trades" }
                                p { "The bot has nothing on the exchange right now." }
                            }
                        } else {
                            for trade in trades {
                                TradeCard {
                                    key: "{trade.trade_id}",
                                    trade: trade.clone(),
                                    currency: currency.clone(),
                                    // Exiting needs a reachable bot; offering
                                    // the button while offline would only
                                    // produce a failure.
                                    on_exit: (!stale).then_some(EventHandler::new(move |t: Trade| {
                                        exiting.set(Some(t));
                                    })),
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(trade) = exiting.read().clone() {
            ExitDialog {
                trade,
                currency: data
                    .read_unchecked()
                    .as_ref()
                    .and_then(|o| o.as_ref())
                    .and_then(|r| r.as_ref().ok())
                    .map(|e| e.data.stake_currency.clone())
                    .unwrap_or_default(),
                on_cancel: move |_| exiting.set(None),
                on_done: move |_| {
                    exiting.set(None);
                    refresh();
                },
            }
        }
    }
}

/// Force-exit confirmation.
///
/// Shows the P/L that would be realised, and says plainly that a limit order
/// goes in at the current bid — the same wording the legacy dialog used,
/// because it sets the right expectation about fills.
#[component]
fn ExitDialog(
    trade: Trade,
    currency: String,
    on_cancel: EventHandler<()>,
    on_done: EventHandler<()>,
) -> Element {
    let app = App::get();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let tone = Tone::of(trade.profit_ratio);
    let trade_id = trade.trade_id;
    let pair = trade.pair.clone();
    let rate = trade.current_rate.unwrap_or(trade.open_rate);
    let profit_abs = trade.profit_abs;
    let profit_ratio = trade.profit_ratio;

    let confirm = move |_| {
        busy.set(true);
        error.set(None);
        let bot = app.active.peek().clone();
        spawn(async move {
            let Some(bot) = bot else { return };
            match api::force_exit(&bot, trade_id).await {
                Ok(_) => on_done.call(()),
                Err(e) => {
                    error.set(Some(e.message));
                    busy.set(false);
                }
            }
        });
    };

    rsx! {
        div { class: "modal__scrim", onclick: move |_| if !busy() { on_cancel.call(()) },
            div { class: "modal", onclick: move |e| e.stop_propagation(),
                h2 { class: "modal__title", "Exit {pair}?" }

                div { class: "exit-preview",
                    div { class: "exit-preview__row",
                        span { class: "muted", "Current price" }
                        span { "{format::price(rate)}" }
                    }
                    div { class: "exit-preview__row",
                        span { class: "muted", "Profit / loss" }
                        span { class: "trade__pl trade__pl--{tone.class()}",
                            "{format::percent(profit_ratio)} · {format::signed_money(profit_abs, &currency)}"
                        }
                    }
                }

                div { class: "modal__body",
                    "A limit order will be placed at the current bid price. "
                    "It may not fill immediately if the market moves."
                }

                if let Some(message) = error.read().clone() {
                    div { class: "error", "{message}" }
                }

                div { class: "row",
                    button {
                        class: "btn btn--ghost btn--row",
                        disabled: busy(),
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn--danger btn--row",
                        disabled: busy(),
                        onclick: confirm,
                        if busy() { span { class: "spinner" } "Placing…" } else { "Limit exit" }
                    }
                }
            }
        }
    }
}

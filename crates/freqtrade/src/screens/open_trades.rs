//! Open Trades: portfolio summary, live positions, and manual trade
//! management — full or partial exit, and adding to a position (DCA).

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
    let mut adding = use_signal(|| None::<Trade>);
    let stake_currency = move || {
        data.read_unchecked()
            .as_ref()
            .and_then(|o| o.as_ref())
            .and_then(|r| r.as_ref().ok())
            .map(|e| e.data.stake_currency.clone())
            .unwrap_or_default()
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
                                    on_add: (!stale).then_some(EventHandler::new(move |t: Trade| {
                                        adding.set(Some(t));
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
                currency: stake_currency(),
                on_cancel: move |_| exiting.set(None),
                on_done: move |_| {
                    exiting.set(None);
                    refresh();
                },
            }
        }

        if let Some(trade) = adding.read().clone() {
            AddDialog {
                trade,
                currency: stake_currency(),
                on_cancel: move |_| adding.set(None),
                on_done: move |_| {
                    adding.set(None);
                    refresh();
                },
            }
        }
    }
}

/// Share of the position a partial exit can sell, in percent. 100 is a full
/// exit.
const EXIT_SIZES: [u32; 4] = [25, 50, 75, 100];

/// Force-exit confirmation, for the whole trade or part of it.
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
    let mut size = use_signal(|| 100u32);

    let tone = Tone::of(trade.profit_ratio);
    let trade_id = trade.trade_id;
    let pair = trade.pair.clone();
    let base = trade.base_currency.clone().unwrap_or_default();
    let rate = trade.current_rate.unwrap_or(trade.open_rate);
    let total_amount = trade.amount;
    let profit_ratio = trade.profit_ratio;

    let share = size() as f64 / 100.0;
    let full = size() == 100;
    let sell_amount = total_amount * share;
    // P/L is linear in the amount sold at the same rate, so a share of the
    // trade realises the same share of its P/L (fees aside).
    let profit_abs = trade.profit_abs * share;
    let stake_out = trade.stake_amount * share;

    let confirm = move |_| {
        busy.set(true);
        error.set(None);
        let bot = app.active.peek().clone();
        // `None` lets Freqtrade sell exactly what it holds, rather than an
        // amount recomputed here that could fall a rounding step short.
        let amount = (!full).then_some(sell_amount);
        spawn(async move {
            let Some(bot) = bot else { return };
            match api::force_exit(&bot, trade_id, amount).await {
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

                div { class: "choices",
                    for pct in EXIT_SIZES {
                        button {
                            class: if size() == pct { "choice choice--on" } else { "choice" },
                            disabled: busy(),
                            onclick: move |_| size.set(pct),
                            "{pct}%"
                        }
                    }
                }

                div { class: "exit-preview",
                    div { class: "exit-preview__row",
                        span { class: "muted", "Current price" }
                        span { "{format::price(rate)}" }
                    }
                    div { class: "exit-preview__row",
                        span { class: "muted", "Amount" }
                        span { "{format::price(sell_amount)} {base}" }
                    }
                    div { class: "exit-preview__row",
                        span { class: "muted", "Stake released" }
                        span { "{format::money(stake_out, &currency)}" }
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
                    if !full {
                        " The rest of the trade stays open."
                    }
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
                        if busy() {
                            span { class: "spinner" } "Placing…"
                        } else if full {
                            "Limit exit"
                        } else {
                            "Exit {size}%"
                        }
                    }
                }
            }
        }
    }
}

/// DCA presets, as multiples of the trade's current stake: 2× adds twice what
/// is in the trade now. Paired with their labels so ½ reads as a fraction.
const ADD_MULTIPLES: [(f64, &str); 4] = [(0.5, "½×"), (1.0, "1×"), (2.0, "2×"), (3.0, "3×")];

/// Manual DCA: adds stake to an open trade with a limit entry.
///
/// Freqtrade refuses this unless the bot has both `force_entry_enable` and
/// `position_adjustment_enable`, so the dialog reads the config and explains
/// what is missing rather than letting the order fail with a terse error.
#[component]
fn AddDialog(
    trade: Trade,
    currency: String,
    on_cancel: EventHandler<()>,
    on_done: EventHandler<()>,
) -> Element {
    let app = App::get();
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);

    let current_stake = trade.stake_amount;
    let mut stake = use_signal(move || format!("{:.2}", current_stake));

    let config = use_resource(move || {
        let id = app.active.peek().clone();
        async move { api::config(&id?, false).await.ok() }
    });
    // `None` while loading; don't block the form on it, Freqtrade has the
    // final say anyway.
    let missing: Vec<&str> = match &*config.read_unchecked() {
        Some(Some(envelope)) => {
            let c = &envelope.data;
            let mut missing = Vec::new();
            if !c.force_entry_enable {
                missing.push("force_entry_enable");
            }
            if !c.position_adjustment_enable {
                missing.push("position_adjustment_enable");
            }
            missing
        }
        _ => Vec::new(),
    };
    let allowed = missing.is_empty();

    let pair = trade.pair.clone();
    let is_short = trade.is_short;
    let open_rate = trade.open_rate;
    let rate = trade.current_rate.unwrap_or(trade.open_rate);
    let amount = trade.amount;
    let leverage = trade.leverage.unwrap_or(1.0).max(1.0);

    let parsed = stake.read().trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0);
    // Average entry if the order fills at the current price: weighted by
    // amount, and the added amount is stake × leverage / price.
    let new_average = parsed.map(|add| {
        let added = add * leverage / rate;
        (open_rate * amount + rate * added) / (amount + added)
    });

    let confirm = {
        let pair = pair.clone();
        move |_| {
            let Some(add) = parsed else { return };
            busy.set(true);
            error.set(None);
            let bot = app.active.peek().clone();
            let pair = pair.clone();
            spawn(async move {
                let Some(bot) = bot else { return };
                match api::add_to_trade(&bot, &pair, is_short, add).await {
                    Ok(_) => on_done.call(()),
                    Err(e) => {
                        error.set(Some(e.message));
                        busy.set(false);
                    }
                }
            });
        }
    };

    rsx! {
        div { class: "modal__scrim", onclick: move |_| if !busy() { on_cancel.call(()) },
            div { class: "modal", onclick: move |e| e.stop_propagation(),
                h2 { class: "modal__title", "Add to {pair}" }

                if !allowed {
                    div { class: "notice",
                        "This bot does not allow adding to trades. Set "
                        for (i, key) in missing.iter().enumerate() {
                            if i > 0 { " and " }
                            code { "{key}" }
                        }
                        " to true in its config and restart it."
                    }
                }

                div { class: "field",
                    label { class: "field__label", "Stake to add ({currency})" }
                    input {
                        class: "input",
                        r#type: "number",
                        inputmode: "decimal",
                        min: "0",
                        step: "any",
                        disabled: busy(),
                        value: "{stake}",
                        oninput: move |e| stake.set(e.value()),
                    }
                }

                div { class: "choices",
                    for (multiple, label) in ADD_MULTIPLES {
                        button {
                            class: if *stake.read() == format!("{:.2}", current_stake * multiple) {
                                "choice choice--on"
                            } else {
                                "choice"
                            },
                            disabled: busy(),
                            onclick: move |_| stake.set(format!("{:.2}", current_stake * multiple)),
                            "{label}"
                        }
                    }
                }

                div { class: "exit-preview",
                    div { class: "exit-preview__row",
                        span { class: "muted", "Current stake" }
                        span { "{format::money(current_stake, &currency)}" }
                    }
                    div { class: "exit-preview__row",
                        span { class: "muted", "Avg. entry" }
                        span { "{format::price(open_rate)}" }
                    }
                    div { class: "exit-preview__row",
                        span { class: "muted", "Current price" }
                        span { "{format::price(rate)}" }
                    }
                    if let Some(avg) = new_average {
                        div { class: "exit-preview__row",
                            span { class: "muted", "New avg. entry (est.)" }
                            span { "{format::price(avg)}" }
                        }
                    }
                }

                div { class: "modal__body",
                    "A limit entry order will be placed at the bot's entry price. "
                    "Presets add a multiple of the current stake; 2× triples the position."
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
                        class: "btn btn--row",
                        disabled: busy() || !allowed || parsed.is_none(),
                        onclick: confirm,
                        if busy() { span { class: "spinner" } "Placing…" } else { "Add to trade" }
                    }
                }
            }
        }
    }
}

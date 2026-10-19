//! Summary tiles and trade cards, shared by Open and Closed Trades.

use dioxus::prelude::*;

use ft_types::freqtrade::Trade;

use crate::format::{self, Tone};

/// One headline number with a label.
#[component]
pub fn StatTile(label: String, value: String, tone: Option<String>) -> Element {
    let tone_class = tone
        .map(|t| format!(" stat__value--{t}"))
        .unwrap_or_default();
    rsx! {
        div { class: "stat",
            div { class: "stat__label", "{label}" }
            div { class: "stat__value{tone_class}", "{value}" }
        }
    }
}

/// A row of tiles.
#[component]
pub fn StatRow(children: Element) -> Element {
    rsx! { div { class: "stat-row", {children} } }
}

/// One trade.
///
/// The card is tinted by profit, as the legacy cards were, so the list can be
/// read at a glance without parsing every number.
#[component]
pub fn TradeCard(
    trade: Trade,
    currency: String,
    /// Shown only for open trades, and only when the bot is reachable.
    on_exit: Option<EventHandler<Trade>>,
    /// Manual DCA. Same visibility rules as `on_exit`.
    on_add: Option<EventHandler<Trade>>,
    /// Cancels the trade's unfilled order. Offered in place of DCA while one
    /// is pending, since Freqtrade refuses to add to a trade until it clears.
    on_cancel_order: Option<EventHandler<Trade>>,
    /// The cancel request is in flight.
    #[props(default)]
    cancelling: bool,
) -> Element {
    let tone = Tone::of(trade.profit_ratio);
    let is_open = trade.is_open;
    let is_short = trade.is_short;
    let pair = trade.pair.clone();
    let profit_ratio = trade.profit_ratio;
    let profit_abs = trade.profit_abs;
    let stake_amount = trade.stake_amount;
    let open_rate = trade.open_rate;
    // The exit handler needs its own copy; everything above is read from the
    // original before it moves into the closure.
    let for_exit = trade.clone();
    let for_add = trade.clone();
    let for_cancel = trade.clone();
    let pending = is_open && trade.has_open_orders;
    let entries = trade.nr_of_successful_entries.filter(|n| *n > 1);

    // Open trades compare against the live rate; closed ones against the exit.
    let (rate_label, rate_value) = if is_open {
        ("Current", trade.current_rate.unwrap_or(trade.open_rate))
    } else {
        ("Close", trade.close_rate.unwrap_or(trade.open_rate))
    };

    let opened = trade
        .opened_at()
        .map(|d| (d.unix_timestamp_nanos() / 1_000_000) as i64);
    let closed = trade
        .closed_at()
        .map(|d| (d.unix_timestamp_nanos() / 1_000_000) as i64);

    rsx! {
        div { class: "trade trade--{tone.class()}",
            div { class: "trade__head",
                span { class: "trade__pair", "{pair}" }
                span {
                    class: if is_short { "chip chip--short" } else { "chip chip--long" },
                    if is_short { "SHORT" } else { "LONG" }
                }
                if pending {
                    span { class: "chip chip--pending", "ORDER PENDING" }
                }
                span { class: "trade__spacer" }
                span { class: "trade__pct trade__pct--{tone.class()}",
                    "{tone.arrow()} {format::percent(profit_ratio)}"
                }
            }

            div { class: "trade__grid",
                div { class: "trade__cell",
                    span { class: "trade__cell-label", "Stake" }
                    span { "{format::money(stake_amount, &currency)}" }
                }
                div { class: "trade__cell",
                    span { class: "trade__cell-label", "Open" }
                    span { "{format::price(open_rate)}" }
                }
                div { class: "trade__cell",
                    span { class: "trade__cell-label", "{rate_label}" }
                    span { "{format::price(rate_value)}" }
                }
                div { class: "trade__cell",
                    span { class: "trade__cell-label", "P/L" }
                    span { class: "trade__pl trade__pl--{tone.class()}",
                        "{format::signed_money(profit_abs, &currency)}"
                    }
                }
            }

            div { class: "trade__foot",
                if let Some(ms) = opened {
                    span { class: "muted", "Opened {format::datetime(ms)}" }
                }
                if let Some(ms) = closed {
                    span { class: "muted", "· Closed {format::datetime(ms)}" }
                }
                if let Some(n) = entries {
                    span { class: "muted", "· {n} entries" }
                }
                if on_exit.is_some() || on_add.is_some() {
                    span { class: "trade__spacer" }
                }
                if let (true, Some(cancel)) = (pending, on_cancel_order) {
                    button {
                        class: "btn btn--small btn--ghost",
                        disabled: cancelling,
                        onclick: move |_| cancel.call(for_cancel.clone()),
                        if cancelling { span { class: "spinner" } } else { "Cancel order" }
                    }
                } else if let Some(add) = on_add {
                    button {
                        class: "btn btn--small btn--ghost",
                        onclick: move |_| add.call(for_add.clone()),
                        "DCA"
                    }
                }
                if let Some(exit) = on_exit {
                    button {
                        class: "btn btn--small btn--danger",
                        onclick: move |_| exit.call(for_exit.clone()),
                        "Exit"
                    }
                }
            }
        }
    }
}

//! Dashboard: cumulative profit, performance, and the bot's configuration.

use dioxus::prelude::*;

use ft_types::api::Dashboard as DashboardData;
use ft_types::freqtrade::BotConfig;

use crate::api;
use crate::components::{ErrorView, OfflineBanner, ProfitChart};
use crate::format::{self, Tone};
use crate::screens::loader::synced_label;
use crate::state::App;

#[component]
pub fn Dashboard() -> Element {
    let app = App::get();
    let active = app.active;
    let mut force = use_signal(|| false);

    let mut data = use_resource(move || {
        let id = active.read().clone();
        let forced = *force.peek();
        async move {
            let id = id?;
            let result = api::dashboard(&id, forced).await;
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
                    let d: &DashboardData = &envelope.data;
                    let currency = d.stake_currency.clone();
                    let overall = d.profit.profit_closed_coin;
                    let tone = Tone::of(overall);

                    rsx! {
                        if envelope.stale {
                            OfflineBanner { last_synced: synced_label(envelope.last_synced) }
                        }

                        // The headline is a number, not a chart: "how am I
                        // doing" is a single value, and the chart answers the
                        // different question of how it got there.
                        div { class: "hero hero--{tone.class()}",
                            div { class: "hero__label", "Realised profit" }
                            div { class: "hero__value",
                                "{format::signed_money(overall, &currency)}"
                            }
                            div { class: "hero__sub",
                                "{d.profit.closed_trade_count} closed trades"
                                if d.profit.winning_trades + d.profit.losing_trades > 0 {
                                    " · {d.profit.winning_trades}W / {d.profit.losing_trades}L"
                                }
                            }
                        }

                        div { class: "section",
                            span { class: "section__title", "Cumulative profit" }
                            button {
                                class: "section__action",
                                onclick: move |_| refresh(),
                                "Refresh"
                            }
                        }
                        ProfitChart { series: d.series.clone() }

                        div { class: "section", span { class: "section__title", "Performance" } }
                        div { class: "tiles",
                            Tile {
                                label: "Avg profit / trade".to_string(),
                                value: format::percent(d.profit.profit_closed_percent_mean / 100.0),
                            }
                            Tile {
                                label: "Win rate".to_string(),
                                value: win_rate(&d.profit.winning_trades, &d.profit.losing_trades),
                            }
                            Tile {
                                label: "Profit factor".to_string(),
                                value: if d.profit.profit_factor > 0.0 {
                                    format!("{:.2}", d.profit.profit_factor)
                                } else { "—".to_string() },
                            }
                            Tile {
                                label: "Avg duration".to_string(),
                                value: d.profit.avg_duration.clone().unwrap_or_else(|| "—".into()),
                            }
                            Tile {
                                label: "Best pair".to_string(),
                                value: d.profit.best_pair.clone().unwrap_or_else(|| "—".into()),
                            }
                            Tile {
                                label: "Trading volume".to_string(),
                                value: format::money(d.profit.trading_volume, &currency),
                            }
                            Tile {
                                label: "Free balance".to_string(),
                                value: format::money(d.free_balance, &currency),
                            }
                            Tile {
                                label: "Max drawdown".to_string(),
                                // Freqtrade reports the magnitude, and a
                                // drawdown is a loss by definition, so a "+"
                                // in front of it reads as a gain.
                                value: format::magnitude_percent(d.profit.max_drawdown),
                            }
                        }

                        div { class: "section", span { class: "section__title", "Configuration" } }
                        ConfigTiles { config: d.config.clone(), currency: currency.clone() }
                    }
                }
            }
        }
    }
}

#[component]
fn Tile(label: String, value: String) -> Element {
    rsx! {
        div { class: "tile",
            span { class: "tile__label", "{label}" }
            span { class: "tile__value", "{value}" }
        }
    }
}

#[component]
fn ConfigTiles(config: BotConfig, currency: String) -> Element {
    let stake = if config.stake_amount.is_unlimited() {
        "Unlimited".to_owned()
    } else {
        format::money(config.stake_amount.as_f64().unwrap_or_default(), &currency)
    };

    // Freqtrade uses -1.0 as the "no stoploss" sentinel; rendering it as
    // -100% would be actively misleading.
    let stoploss = if config.stoploss_disabled() {
        "Disabled".to_owned()
    } else {
        format::percent(config.stoploss)
    };

    rsx! {
        div { class: "tiles",
            Tile { label: "Strategy".to_string(), value: config.strategy.clone().unwrap_or_else(|| "—".into()) }
            Tile { label: "Exchange".to_string(), value: config.exchange.clone() }
            Tile { label: "Trading mode".to_string(), value: config.trading_mode.clone() }
            Tile { label: "Timeframe".to_string(), value: config.timeframe.clone().unwrap_or_else(|| "—".into()) }
            Tile { label: "Stake amount".to_string(), value: stake }
            Tile { label: "Max open trades".to_string(), value: config.max_open_trades_display() }
            Tile { label: "Stoploss".to_string(), value: stoploss }
            Tile { label: "Stoploss on exchange".to_string(), value: yes_no(config.stoploss_on_exchange) }
            Tile { label: "Shorting".to_string(), value: yes_no(config.short_allowed) }
            Tile { label: "Version".to_string(), value: config.version.clone() }
        }
    }
}

fn yes_no(value: bool) -> String {
    if value { "Yes" } else { "No" }.to_owned()
}

/// Win rate, or an em dash when there is nothing to divide.
fn win_rate(wins: &i64, losses: &i64) -> String {
    let total = wins + losses;
    if total == 0 {
        return "—".to_owned();
    }
    format!("{:.0}%", *wins as f64 / total as f64 * 100.0)
}

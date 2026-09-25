//! Screen calculations, moved off the client.
//!
//! These ran in Dart inside widget `build` paths — the cumulative series was
//! recomputed from up to 500 trades on every Dashboard rebuild. Doing them
//! here means the UI receives numbers and draws them.

use ft_types::api::{CumulativeSeries, SeriesPoint, SeriesUnit};
use ft_types::freqtrade::{Balance, ProfitSummary, Trade};

/// Builds the cumulative profit curve from closed trades.
///
/// Ported from `dashboard_screen.dart:109-147`: sort by close time, accumulate
/// `profit_abs`, and express the running total as a percentage of starting
/// capital when Freqtrade reported one, otherwise as an absolute amount in the
/// stake currency. Keeping that fallback matters — `starting_capital` is absent
/// on plenty of configurations, and a percentage of zero is not a number.
///
/// `trades` may arrive in any order; this sorts a copy of the keys rather than
/// trusting the caller.
pub fn cumulative_series(
    trades: &[Trade],
    starting_capital: f64,
    stake_currency: &str,
) -> CumulativeSeries {
    let mut ordered: Vec<(i64, f64)> = trades
        .iter()
        .filter(|t| !t.is_open)
        .filter_map(|t| {
            let at = t.closed_at()?;
            Some(((at.unix_timestamp_nanos() / 1_000_000) as i64, t.profit_abs))
        })
        .collect();
    ordered.sort_by_key(|(time, _)| *time);

    let as_percent = starting_capital > 0.0;
    let mut running = 0.0;
    let points = ordered
        .into_iter()
        .map(|(time, profit)| {
            running += profit;
            SeriesPoint {
                time,
                cumulative: if as_percent {
                    running / starting_capital * 100.0
                } else {
                    running
                },
            }
        })
        .collect();

    CumulativeSeries {
        points,
        unit: if as_percent {
            SeriesUnit::Percent
        } else {
            SeriesUnit::Currency
        },
        currency: stake_currency.to_owned(),
    }
}

/// Unrealized P/L across open trades.
pub fn open_pl(open_trades: &[Trade]) -> f64 {
    open_trades.iter().map(|t| t.profit_abs).sum()
}

/// Total portfolio value: settled balance plus unrealized P/L.
///
/// Matches `open_trades_screen.dart`, which showed `balance.total` plus the
/// sum of `profit_abs` so the headline number moves with open positions.
pub fn portfolio_value(balance: &Balance, open_trades: &[Trade]) -> f64 {
    balance.total + open_pl(open_trades)
}

/// The stake currency, preferring `/balance` and falling back to `/profit`.
///
/// Neither endpoint reports it reliably across Freqtrade versions, and an
/// empty currency label on every number looks broken.
pub fn stake_currency(balance: Option<&Balance>, profit: Option<&ProfitSummary>) -> String {
    balance
        .map(|b| b.stake.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| profit.and_then(|p| p.stake_currency.clone()))
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ft_types::freqtrade::CurrencyBalance;

    fn closed(trade_id: i64, close_ts: i64, profit: f64) -> Trade {
        Trade {
            trade_id,
            is_open: false,
            profit_abs: profit,
            close_timestamp: Some(close_ts * 1000),
            ..Default::default()
        }
    }

    #[test]
    fn the_series_accumulates_in_close_order() {
        // Deliberately out of order, and close time is not monotonic with id.
        let trades = vec![
            closed(1, 3000, 10.0),
            closed(2, 1000, 5.0),
            closed(3, 2000, -2.0),
        ];
        let series = cumulative_series(&trades, 0.0, "USDT");

        let values: Vec<_> = series.points.iter().map(|p| p.cumulative).collect();
        assert_eq!(values, vec![5.0, 3.0, 13.0]);
        assert!(series.points.windows(2).all(|w| w[0].time < w[1].time));
        assert_eq!(series.unit, SeriesUnit::Currency);
        assert_eq!(series.currency, "USDT");
    }

    #[test]
    fn starting_capital_switches_the_series_to_percent() {
        let trades = vec![closed(1, 1000, 50.0), closed(2, 2000, 50.0)];
        let series = cumulative_series(&trades, 1000.0, "USDT");
        assert_eq!(series.unit, SeriesUnit::Percent);
        let values: Vec<_> = series.points.iter().map(|p| p.cumulative).collect();
        assert_eq!(values, vec![5.0, 10.0]);
    }

    #[test]
    fn without_starting_capital_it_plots_currency_not_a_division_by_zero() {
        let series = cumulative_series(&[closed(1, 1000, 7.5)], 0.0, "USDT");
        assert_eq!(series.unit, SeriesUnit::Currency);
        assert_eq!(series.points[0].cumulative, 7.5);
    }

    #[test]
    fn open_trades_and_untimed_trades_are_excluded() {
        // A trade with no usable close time cannot be placed on the x-axis.
        let trades = vec![
            closed(1, 1000, 5.0),
            Trade {
                trade_id: 2,
                is_open: true,
                profit_abs: 99.0,
                ..Default::default()
            },
            Trade {
                trade_id: 3,
                is_open: false,
                profit_abs: 99.0,
                close_timestamp: None,
                ..Default::default()
            },
        ];
        let series = cumulative_series(&trades, 0.0, "USDT");
        assert_eq!(series.points.len(), 1);
        assert_eq!(series.points[0].cumulative, 5.0);
    }

    #[test]
    fn an_empty_history_produces_an_empty_series() {
        let series = cumulative_series(&[], 1000.0, "USDT");
        assert!(series.points.is_empty());
    }

    #[test]
    fn portfolio_value_includes_unrealized_pl() {
        let balance = Balance {
            total: 1000.0,
            ..Default::default()
        };
        let open = vec![
            Trade {
                profit_abs: 12.5,
                ..Default::default()
            },
            Trade {
                profit_abs: -2.5,
                ..Default::default()
            },
        ];
        assert_eq!(open_pl(&open), 10.0);
        assert_eq!(portfolio_value(&balance, &open), 1010.0);
        assert_eq!(portfolio_value(&balance, &[]), 1000.0);
    }

    #[test]
    fn stake_currency_falls_back_from_balance_to_profit() {
        let with_stake = Balance {
            stake: "USDT".into(),
            ..Default::default()
        };
        let profit = ProfitSummary {
            stake_currency: Some("BTC".into()),
            ..Default::default()
        };
        assert_eq!(stake_currency(Some(&with_stake), Some(&profit)), "USDT");
        // An empty string from /balance must not win over a real value.
        assert_eq!(
            stake_currency(Some(&Balance::default()), Some(&profit)),
            "BTC"
        );
        assert_eq!(stake_currency(None, None), "");
    }

    #[test]
    fn free_and_used_come_from_the_spot_row() {
        // Futures positions share the currencies array; reading the first row
        // blindly would report a position's zeroes as the free balance.
        let balance = Balance {
            stake: "USDT".into(),
            currencies: vec![
                CurrencyBalance {
                    currency: "ETH/USDT:USDT".into(),
                    is_position: true,
                    ..Default::default()
                },
                CurrencyBalance {
                    currency: "USDT".into(),
                    free: 812.44,
                    used: 200.0,
                    is_position: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(ft_types::api::spot_free_used(&balance), (812.44, 200.0));
    }
}

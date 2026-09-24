//! Validates real responses captured from a live bot.
//!
//! `scripts/capture-fixtures.sh <url> <user> <pass>` writes into
//! `tests/fixtures/live/`, which is gitignored because it holds real balances.
//! When that directory is absent this file passes trivially, so CI and a fresh
//! clone stay green; when it is present, every captured response must parse.
//!
//! This is the M1 acceptance check: hand-written fixtures prove we understood
//! the documented schema, these prove we understood *your bot's* schema.

use ft_types::freqtrade::*;
use std::path::PathBuf;

fn live_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/live"))
}

/// Parses `live/<name>.json` as `T`, or returns `None` if it was not captured
/// (an empty whitelist means no candles, a fresh bot means no trades).
fn live<T: serde::de::DeserializeOwned>(name: &str) -> Option<T> {
    let path = live_dir().join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&raw) {
        Ok(value) => Some(value),
        Err(e) => panic!("live fixture {}: {e}", path.display()),
    }
}

fn captured() -> bool {
    live_dir().is_dir()
}

#[test]
fn live_responses_all_deserialize() {
    if !captured() {
        eprintln!("no live fixtures; run scripts/capture-fixtures.sh to enable this check");
        return;
    }

    if let Some(p) = live::<Ping>("ping") {
        assert!(p.is_pong(), "bot answered /ping with {:?}", p.status);
    }

    if let Some(tok) = live::<TokenResponse>("token_login") {
        assert!(!tok.access_token.is_empty());
        // If a real bot ever omits this, the whole refresh design needs revisiting.
        assert!(
            tok.refresh_token.is_some(),
            "this bot returned no refresh_token -- ft-client's refresh flow cannot work against it"
        );
    }

    if let Some(cfg) = live::<BotConfig>("show_config") {
        assert!(
            !cfg.exchange.is_empty(),
            "exchange missing from /show_config"
        );
        assert!(
            !cfg.stake_currency.is_empty(),
            "stake_currency missing from /show_config"
        );
        eprintln!(
            "live config: {} on {} ({}), stake {} {}, stoploss {}",
            cfg.strategy.as_deref().unwrap_or("?"),
            cfg.exchange,
            if cfg.dry_run { "dry" } else { "LIVE" },
            cfg.stake_amount,
            cfg.stake_currency,
            if cfg.stoploss_disabled() {
                "disabled".to_owned()
            } else {
                format!("{:.1}%", cfg.stoploss * 100.0)
            },
        );
    }

    if let Some(open) = live::<Vec<Trade>>("status") {
        for t in &open {
            assert!(!t.pair.is_empty(), "open trade {} has no pair", t.trade_id);
            assert!(
                t.opened_at().is_some(),
                "open trade {} has no usable open time ({:?} / {:?}) -- \
                 sorting and chart markers depend on this",
                t.trade_id,
                t.open_timestamp,
                t.open_date,
            );
        }
        eprintln!("live: {} open trades", open.len());
    }

    if let Some(resp) = live::<TradesResponse>("trades") {
        for t in &resp.trades {
            assert!(
                t.closed_at().is_some(),
                "closed trade {} has no usable close time",
                t.trade_id
            );
        }
        eprintln!(
            "live: {} closed trades returned of {} total",
            resp.trades.len(),
            resp.total_trades
        );
    }

    let _ = live::<ProfitSummary>("profit");
    let _ = live::<Balance>("balance");

    if let Some(logs) = live::<LogsResponse>("logs") {
        for entry in &logs.logs {
            assert!(!entry.level.is_empty(), "log entry with no level");
            assert!(entry.at().is_some(), "log entry with unparseable timestamp");
        }
        eprintln!("live: {} log entries", logs.logs.len());
    }

    let _ = live::<WhitelistResponse>("whitelist");

    if let Some(pc) = live::<PairCandles>("pair_candles") {
        let candles = pc.candles();
        assert!(
            !pc.data.is_empty() == !candles.is_empty(),
            "candle rows were dropped: {} raw rows projected to {} candles -- \
             the date column was not understood (columns: {:?})",
            pc.data.len(),
            candles.len(),
            pc.columns,
        );
        assert!(
            candles.windows(2).all(|w| w[0].time < w[1].time),
            "candles are not strictly ordered by time"
        );
        eprintln!("live: {} candles for {}", candles.len(), pc.pair);
    }
}

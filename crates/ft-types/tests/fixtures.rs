//! Deserialization tests against captured Freqtrade responses.
//!
//! Two jobs. First, prove each DTO reads the real wire format — including the
//! awkward parts the Flutter app hand-decoded: positional log tuples, columnar
//! candles, `"unlimited"` stake amounts, and the `-1.0` stoploss sentinel.
//! Second, prove the tolerance claim: a response missing everything must
//! degrade to defaults rather than fail, because a Freqtrade upgrade should not
//! black out a screen.

use ft_types::flex::StakeAmount;
use ft_types::freqtrade::*;

fn fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/");
    let raw = std::fs::read_to_string(format!("{path}{name}.json"))
        .unwrap_or_else(|e| panic!("fixture {name}.json: {e}"));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("fixture {name}.json failed to parse: {e}"))
}

#[test]
fn ping_reports_pong() {
    assert!(fixture::<Ping>("ping").is_pong());
    assert!(!Ping::default().is_pong());
}

#[test]
fn login_keeps_the_refresh_token() {
    // The defect this rewrite exists to fix: the Flutter client read
    // `access_token` and dropped `refresh_token`, so it could never refresh.
    let tok: TokenResponse = fixture("token_login");
    assert!(tok.access_token.contains("access"));
    assert_eq!(
        tok.refresh_token.as_deref(),
        Some("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.refresh.sig")
    );

    // /token/refresh returns only a new access token.
    let refreshed: TokenResponse = fixture("token_refresh");
    assert!(refreshed.refresh_token.is_none());
    assert_ne!(refreshed.access_token, tok.access_token);
}

#[test]
fn dry_run_config_reads_every_dashboard_field() {
    let cfg: BotConfig = fixture("show_config");
    assert!(cfg.dry_run);
    assert_eq!(cfg.exchange, "binance");
    assert_eq!(cfg.strategy.as_deref(), Some("SampleStrategy"));
    assert_eq!(cfg.timeframe.as_deref(), Some("5m"));
    assert_eq!(cfg.trading_mode, "spot");
    assert!(!cfg.short_allowed);
    assert!(!cfg.stoploss_on_exchange);
    assert_eq!(cfg.max_open_trades_display(), "5");

    // "unlimited" arrives as a string where a number normally sits.
    assert!(cfg.stake_amount.is_unlimited());
    assert_eq!(cfg.stake_amount.to_string(), "unlimited");

    assert!(!cfg.stoploss_disabled());
    assert!((cfg.stoploss - -0.1).abs() < 1e-9);
}

#[test]
fn live_config_handles_numeric_stake_and_disabled_stoploss() {
    let cfg: BotConfig = fixture("show_config_live");
    assert!(!cfg.dry_run);
    assert!(cfg.short_allowed);
    assert_eq!(cfg.stake_amount, StakeAmount::Amount(50.5));
    assert_eq!(cfg.stake_amount.as_f64(), Some(50.5));
    assert!(!cfg.stake_amount.is_unlimited());

    // -1.0 is Freqtrade's "no stoploss" sentinel; the Dashboard shows "Disabled"
    // rather than -100%.
    assert!(cfg.stoploss_disabled());
    // -1 max_open_trades means unlimited, not "minus one trades".
    assert_eq!(cfg.max_open_trades_display(), "Unlimited");
}

#[test]
fn open_trades_survive_a_null_current_rate() {
    let trades: Vec<Trade> = fixture("status");
    assert_eq!(trades.len(), 2);

    let btc = &trades[0];
    assert_eq!(btc.trade_id, 42);
    assert_eq!(btc.pair, "BTC/USDT");
    assert!(btc.is_open && !btc.is_short);
    assert_eq!(btc.current_rate, Some(67200.5));
    assert_eq!(btc.close_rate, None);
    assert_eq!(btc.effective_rate(), 67200.5);
    assert!(btc.closed_at().is_none());

    // An unpriced short: `current_rate: null` used to render as a blank cell.
    let eth = &trades[1];
    assert!(eth.is_short);
    assert_eq!(eth.current_rate, None);
    assert_eq!(
        eth.effective_rate(),
        eth.open_rate,
        "falls back to open rate"
    );
}

#[test]
fn closed_trades_expose_close_rate_and_ordering() {
    let resp: TradesResponse = fixture("trades");
    assert_eq!(resp.trades.len(), 2);
    assert_eq!(resp.total_trades, 2);

    let sol = &resp.trades[0];
    assert!(!sol.is_open);
    assert_eq!(sol.close_rate, Some(147.0));
    assert_eq!(sol.effective_rate(), 147.0);
    assert_eq!(sol.exit_reason.as_deref(), Some("roi"));

    // Closed Trades sorts by close_date descending; that needs a real instant.
    let a = resp.trades[0].closed_at().expect("sol close time");
    let b = resp.trades[1].closed_at().expect("ada close time");
    assert!(a < b, "fixture order is ascending by close time");
    assert!(sol.opened_at().unwrap() < a);
}

#[test]
fn profit_summary_reads_and_tolerates_an_empty_bot() {
    let p: ProfitSummary = fixture("profit");
    assert_eq!(p.closed_trade_count, 2);
    assert_eq!(p.best_pair.as_deref(), Some("SOL/USDT"));
    assert_eq!(p.starting_capital, 1000.0);
    assert_eq!(p.avg_duration.as_deref(), Some("0:52:30"));

    // A bot with no closed trades sends nulls where the UI wants numbers.
    let empty: ProfitSummary = fixture("profit_empty");
    assert_eq!(empty.closed_trade_count, 0);
    assert_eq!(empty.best_pair, None);
    assert_eq!(empty.trading_volume, 0.0, "null trading_volume became 0.0");
    assert_eq!(empty.starting_capital, 0.0);
}

#[test]
fn balance_picks_the_spot_entry_not_the_position() {
    let b: Balance = fixture("balance");
    assert_eq!(b.total, 1012.44);
    assert_eq!(b.stake, "USDT");

    // Futures positions share the `currencies` array; Free/Staked must come
    // from the spot row, not from `BTC/USDT:USDT`.
    let spot = b.spot_entry().expect("spot entry");
    assert_eq!(spot.currency, "USDT");
    assert_eq!(spot.free, 812.44);
    assert_eq!(spot.used, 200.0);
    assert!(!spot.is_position);
}

#[test]
fn logs_decode_from_positional_tuples() {
    let resp: LogsResponse = fixture("logs");
    // The 5th fixture row is malformed and must be skipped, not fatal.
    assert_eq!(resp.logs.len(), 4, "short row dropped");

    let first = &resp.logs[0];
    assert_eq!(first.logger, "freqtrade.worker");
    assert_eq!(first.level, "INFO");
    assert!(first.message.starts_with("Bot heartbeat"));
    assert_eq!(first.severity(), LogSeverity::Info);

    let levels: Vec<_> = resp.logs.iter().map(LogEntry::severity).collect();
    assert_eq!(
        levels,
        vec![
            LogSeverity::Info,
            LogSeverity::Warning,
            LogSeverity::Error,
            LogSeverity::Debug
        ]
    );

    // The legacy screen sliced chars 11..19 out of the string to get a clock;
    // we get a real instant from the epoch field instead.
    let at = first.at().expect("timestamp");
    assert_eq!(at.hour(), 8);
    assert_eq!(at.minute(), 0);
    assert_eq!(at.second(), 1);
}

#[test]
fn whitelist_reads_pairs() {
    let w: WhitelistResponse = fixture("whitelist");
    assert_eq!(w.whitelist, ["BTC/USDT", "ETH/USDT", "SOL/USDT"]);
    assert_eq!(w.length, 3);
}

#[test]
fn candles_project_out_of_the_columnar_payload() {
    let pc: PairCandles = fixture("pair_candles");
    assert_eq!(pc.pair, "BTC/USDT");
    assert!(pc.last_refresh.is_some());

    let candles = pc.candles();
    assert_eq!(candles.len(), 3);

    // Indicator columns (`rsi`) must not shift the OHLCV projection.
    let first = &candles[0];
    assert_eq!(first.time, 1_727_164_800_000);
    assert_eq!(first.open, 66500.0);
    assert_eq!(first.high, 66620.0);
    assert_eq!(first.low, 66480.0);
    assert_eq!(first.close, 66590.0);
    assert_eq!(first.volume, 12.5);

    // Strictly increasing time, which the chart's x-axis depends on.
    assert!(candles.windows(2).all(|w| w[0].time < w[1].time));
}

#[test]
fn close_only_candles_still_render() {
    // Older bots send no __date_ts and no OHLC. The x-axis must come from the
    // naive date string, and the missing bars must collapse onto close so the
    // line chart still draws.
    let pc: PairCandles = fixture("pair_candles_legacy");
    let candles = pc.candles();
    assert_eq!(candles.len(), 2);

    let first = &candles[0];
    assert_eq!(
        first.time, 1_727_164_800_000,
        "parsed from 'YYYY-MM-DD HH:MM:SS'"
    );
    assert_eq!(first.close, 2500.5);
    assert_eq!(first.open, 2500.5, "absent OHLC collapses onto close");
    assert_eq!(first.high, 2500.5);
    assert_eq!(first.low, 2500.5);
    assert_eq!(first.volume, 0.0);
}

#[test]
fn every_type_degrades_instead_of_failing() {
    // The tolerance contract: a Freqtrade version that stops sending a field
    // must cost us that field, not the whole screen.
    macro_rules! degrades {
        ($($t:ty),+ $(,)?) => {$(
            serde_json::from_str::<$t>("{}").unwrap_or_else(|e| {
                panic!("{} should accept an empty object: {e}", stringify!($t))
            });
        )+};
    }
    degrades!(
        Ping,
        TokenResponse,
        BotConfig,
        Trade,
        TradesResponse,
        ProfitSummary,
        Balance,
        CurrencyBalance,
        LogsResponse,
        WhitelistResponse,
        PairCandles,
        StatusMessage,
    );

    // And an entirely empty bot renders zeroes, not a parse error.
    let cfg: BotConfig = serde_json::from_str("{}").unwrap();
    assert!(cfg.stake_amount.is_unlimited());
    assert_eq!(cfg.max_open_trades_display(), "0");
    let empty: PairCandles = serde_json::from_str("{}").unwrap();
    assert!(empty.candles().is_empty());
}

#[test]
fn force_exit_builds_a_limit_order() {
    let body = ForceExitRequest::limit(42);
    assert_eq!(
        serde_json::to_value(&body).unwrap(),
        serde_json::json!({"tradeid": "42", "ordertype": "limit"}),
    );
}

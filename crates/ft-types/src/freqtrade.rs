//! DTOs mirroring the Freqtrade REST API.
//!
//! Scope is deliberately the set of fields the app actually renders (see
//! `legacy-flutter-notes.md` for the screen-by-screen contract) plus the ones
//! the Flutter version should have used and did not — chiefly `refresh_token`
//! and the `*_timestamp` variants, which are far more robust than re-parsing
//! Freqtrade's human-readable date strings.
//!
//! Every struct carries `#[serde(default)]` so a Freqtrade upgrade that drops or
//! renames a field degrades that one field instead of failing the response.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::flex::{de_timestamp_opt, null_to_default, parse_timestamp, StakeAmount};

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

/// `GET /ping` — the only unauthenticated endpoint. Drives the per-bot dot on
/// the Bots screen.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Ping {
    pub status: String,
}

impl Ping {
    pub fn is_pong(&self) -> bool {
        self.status == "pong"
    }
}

/// `POST /token/login` and `POST /token/refresh`.
///
/// `refresh_token` is absent from the refresh response, which is why it is
/// optional. The Flutter client discarded it entirely and so could never
/// refresh — the single biggest defect being fixed by this rewrite.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// `GET /show_config`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BotConfig {
    pub version: String,
    pub strategy_version: Option<String>,
    /// Drives the DRY / LIVE badge in the header.
    #[serde(deserialize_with = "null_to_default")]
    pub dry_run: bool,
    pub trading_mode: String,
    #[serde(deserialize_with = "null_to_default")]
    pub short_allowed: bool,
    pub stake_currency: String,
    pub stake_amount: StakeAmount,
    pub available_capital: Option<f64>,
    #[serde(deserialize_with = "null_to_default")]
    pub stake_currency_decimals: u32,
    /// `-1` means unlimited; see [`BotConfig::max_open_trades_display`].
    #[serde(deserialize_with = "null_to_default")]
    pub max_open_trades: i64,
    /// A ratio, e.g. `-0.1` for -10%. `-1.0` is Freqtrade's "disabled" sentinel.
    #[serde(deserialize_with = "null_to_default")]
    pub stoploss: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub stoploss_on_exchange: bool,
    #[serde(deserialize_with = "null_to_default")]
    pub trailing_stop: bool,
    pub timeframe: Option<String>,
    pub exchange: String,
    pub strategy: Option<String>,
    pub bot_name: String,
    pub state: String,
    pub runmode: String,
    #[serde(deserialize_with = "null_to_default")]
    pub force_entry_enable: bool,
    #[serde(deserialize_with = "null_to_default")]
    pub position_adjustment_enable: bool,
    pub api_version: Option<f64>,
}

impl BotConfig {
    /// Freqtrade encodes "no stoploss" as exactly `-1.0`; the Dashboard screen
    /// renders that as "Disabled" rather than "-100%".
    pub fn stoploss_disabled(&self) -> bool {
        (self.stoploss + 1.0).abs() < f64::EPSILON
    }

    pub fn max_open_trades_display(&self) -> String {
        if self.max_open_trades < 0 {
            "Unlimited".to_owned()
        } else {
            self.max_open_trades.to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Trades
// ---------------------------------------------------------------------------

/// One entry from `GET /status` (open) or `GET /trades` (closed).
///
/// Freqtrade returns the same object for both, with the close-related fields
/// populated only once the trade is closed, so a single type serves both
/// screens. Nullability tracks that: `close_rate` and `close_date` are `None`
/// while the trade is open.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Trade {
    pub trade_id: i64,
    pub pair: String,
    pub base_currency: Option<String>,
    pub quote_currency: Option<String>,
    #[serde(deserialize_with = "null_to_default")]
    pub is_short: bool,
    #[serde(deserialize_with = "null_to_default")]
    pub is_open: bool,
    #[serde(deserialize_with = "null_to_default")]
    pub amount: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub stake_amount: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub open_rate: f64,
    /// `None` until the exchange has priced the pair.
    pub current_rate: Option<f64>,
    /// `None` while the trade is open.
    pub close_rate: Option<f64>,
    /// Ratio, e.g. `0.0123` for +1.23%.
    #[serde(deserialize_with = "null_to_default")]
    pub profit_ratio: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub profit_abs: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub profit_pct: f64,
    pub open_date: Option<String>,
    pub close_date: Option<String>,
    /// Epoch milliseconds. Preferred over [`Trade::open_date`] for ordering.
    pub open_timestamp: Option<i64>,
    pub close_timestamp: Option<i64>,
    pub exit_reason: Option<String>,
    pub strategy: Option<String>,
    pub timeframe: Option<i64>,
    pub leverage: Option<f64>,
    pub stop_loss_abs: Option<f64>,
    pub trading_mode: Option<String>,
}

impl Trade {
    /// Opening time, preferring the numeric timestamp and falling back to
    /// parsing the display string.
    pub fn opened_at(&self) -> Option<OffsetDateTime> {
        timestamp_or_string(self.open_timestamp, self.open_date.as_deref())
    }

    /// Closing time, or `None` for an open trade.
    pub fn closed_at(&self) -> Option<OffsetDateTime> {
        timestamp_or_string(self.close_timestamp, self.close_date.as_deref())
    }

    /// The rate to show as "current": the close rate once closed, otherwise the
    /// live rate, otherwise the open rate so the UI never renders a blank.
    pub fn effective_rate(&self) -> f64 {
        self.close_rate
            .or(self.current_rate)
            .unwrap_or(self.open_rate)
    }
}

fn timestamp_or_string(millis: Option<i64>, text: Option<&str>) -> Option<OffsetDateTime> {
    if let Some(ms) = millis {
        if let Some(dt) = parse_timestamp(&Value::from(ms)) {
            return Some(dt);
        }
    }
    text.and_then(crate::flex::parse_timestamp_str)
}

/// `GET /trades?limit=&offset=`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TradesResponse {
    pub trades: Vec<Trade>,
    #[serde(deserialize_with = "null_to_default")]
    pub trades_count: i64,
    #[serde(deserialize_with = "null_to_default")]
    pub offset: i64,
    #[serde(deserialize_with = "null_to_default")]
    pub total_trades: i64,
}

// ---------------------------------------------------------------------------
// Profit
// ---------------------------------------------------------------------------

/// `GET /profit`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProfitSummary {
    #[serde(deserialize_with = "null_to_default")]
    pub profit_closed_coin: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub profit_closed_percent: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub profit_closed_percent_mean: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub profit_closed_ratio_mean: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub profit_all_coin: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub profit_all_percent: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub trade_count: i64,
    #[serde(deserialize_with = "null_to_default")]
    pub closed_trade_count: i64,
    /// Human string such as `2:14:33`; Freqtrade formats it, we pass it through.
    pub avg_duration: Option<String>,
    pub best_pair: Option<String>,
    #[serde(deserialize_with = "null_to_default")]
    pub best_pair_profit_ratio: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub winning_trades: i64,
    #[serde(deserialize_with = "null_to_default")]
    pub losing_trades: i64,
    #[serde(deserialize_with = "null_to_default")]
    pub profit_factor: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub trading_volume: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub max_drawdown: f64,
    /// Zero or absent means the Dashboard plots absolute profit instead of a
    /// percentage of capital.
    #[serde(deserialize_with = "null_to_default")]
    pub starting_capital: f64,
    /// Not always present on `/profit`; the Dashboard falls back to `/balance`.
    pub stake_currency: Option<String>,
    pub first_trade_timestamp: Option<i64>,
    pub latest_trade_timestamp: Option<i64>,
}

// ---------------------------------------------------------------------------
// Balance
// ---------------------------------------------------------------------------

/// One row of `GET /balance`'s `currencies` array.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CurrencyBalance {
    pub currency: String,
    #[serde(deserialize_with = "null_to_default")]
    pub free: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub used: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub balance: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub est_stake: f64,
    pub side: Option<String>,
    /// Futures positions appear in the same array; the Open Trades screen reads
    /// free/used from the entry where this is false.
    #[serde(deserialize_with = "null_to_default")]
    pub is_position: bool,
    #[serde(deserialize_with = "null_to_default")]
    pub is_bot_managed: bool,
}

/// `GET /balance`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Balance {
    pub currencies: Vec<CurrencyBalance>,
    #[serde(deserialize_with = "null_to_default")]
    pub total: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub total_bot: f64,
    /// The stake currency. Freqtrade calls this `stake`; older builds also sent
    /// `stake_currency`, so both are accepted.
    #[serde(alias = "stake_currency")]
    pub stake: String,
    pub symbol: Option<String>,
    #[serde(deserialize_with = "null_to_default")]
    pub value: f64,
    #[serde(deserialize_with = "null_to_default")]
    pub starting_capital: f64,
    pub note: Option<String>,
}

impl Balance {
    /// The spot entry the Open Trades screen reads Free / Staked from.
    pub fn spot_entry(&self) -> Option<&CurrencyBalance> {
        self.currencies
            .iter()
            .find(|c| !c.is_position && c.currency == self.stake)
            .or_else(|| self.currencies.iter().find(|c| !c.is_position))
    }
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

/// One line from `GET /logs`.
///
/// The wire format is a positional array, not an object:
/// `[display_time, epoch_seconds, logger_name, level, message]`. The Flutter
/// code indexed 0, 3 and 4 by hand and sliced characters 11..19 out of the
/// string to get a clock time; we decode the tuple properly instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LogEntry {
    pub timestamp: String,
    pub epoch: Option<f64>,
    pub logger: String,
    pub level: String,
    pub message: String,
}

impl LogEntry {
    pub fn at(&self) -> Option<OffsetDateTime> {
        self.epoch
            .and_then(|e| parse_timestamp(&Value::from(e)))
            .or_else(|| crate::flex::parse_timestamp_str(&self.timestamp))
    }

    /// Severity bucket driving the badge colour, matching the legacy palette:
    /// ERROR/CRITICAL red, WARNING amber, DEBUG grey, everything else blue.
    pub fn severity(&self) -> LogSeverity {
        match self.level.to_ascii_uppercase().as_str() {
            "ERROR" | "CRITICAL" | "FATAL" => LogSeverity::Error,
            "WARNING" | "WARN" => LogSeverity::Warning,
            "DEBUG" | "TRACE" => LogSeverity::Debug,
            _ => LogSeverity::Info,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSeverity {
    Error,
    Warning,
    Info,
    Debug,
}

/// `GET /logs?limit=`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LogsResponse {
    #[serde(deserialize_with = "de_log_entries")]
    pub logs: Vec<LogEntry>,
    #[serde(deserialize_with = "null_to_default")]
    pub log_count: i64,
}

/// Decodes the positional log tuples, skipping any row too short to be useful
/// rather than failing the whole response.
fn de_log_entries<'de, D>(de: D) -> Result<Vec<LogEntry>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let rows = Option::<Vec<Vec<Value>>>::deserialize(de)?.unwrap_or_default();
    Ok(rows
        .into_iter()
        .filter(|row| row.len() >= 5)
        .map(|row| {
            let text = |i: usize| -> String {
                row.get(i)
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            };
            LogEntry {
                timestamp: text(0),
                epoch: row.get(1).and_then(Value::as_f64),
                logger: text(2),
                level: text(3),
                message: text(4),
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Whitelist
// ---------------------------------------------------------------------------

/// `GET /whitelist`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WhitelistResponse {
    pub whitelist: Vec<String>,
    #[serde(deserialize_with = "null_to_default")]
    pub length: i64,
    pub method: Vec<String>,
}

// ---------------------------------------------------------------------------
// Candles
// ---------------------------------------------------------------------------

/// A single OHLCV bar decoded out of `/pair_candles`.
///
/// The Flutter chart only ever plotted `close`; we decode the full bar so the
/// chart screen can become a real candlestick chart without another round trip.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Candle {
    /// Epoch milliseconds.
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// `GET /pair_candles`.
///
/// The wire format is columnar — a `columns` array of names plus a `data` array
/// of positional rows, interleaving OHLCV with whatever indicator columns the
/// strategy produced. [`PairCandles::candles`] projects out the OHLCV bars.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PairCandles {
    pub pair: String,
    pub timeframe: String,
    pub strategy: String,
    pub columns: Vec<String>,
    pub data: Vec<Vec<Value>>,
    #[serde(deserialize_with = "null_to_default")]
    pub length: i64,
    #[serde(default, deserialize_with = "de_timestamp_opt")]
    pub last_refresh: Option<OffsetDateTime>,
}

impl PairCandles {
    fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    /// Projects the columnar payload into typed OHLCV bars, in wire order.
    ///
    /// Rows without a usable timestamp are dropped — a bar we cannot place on
    /// the x-axis is worse than no bar. Missing OHLC columns fall back to the
    /// close price so a close-only response still renders.
    pub fn candles(&self) -> Vec<Candle> {
        // `__date_ts` is already epoch millis; `date` needs parsing.
        let ts_idx = self.column("__date_ts");
        let date_idx = self.column("date");
        let close_idx = self.column("close");
        let (open_idx, high_idx, low_idx, vol_idx) = (
            self.column("open"),
            self.column("high"),
            self.column("low"),
            self.column("volume"),
        );

        self.data
            .iter()
            .filter_map(|row| {
                let at = |idx: Option<usize>| idx.and_then(|i| row.get(i));
                let time = at(ts_idx).and_then(Value::as_i64).or_else(|| {
                    at(date_idx)
                        .and_then(parse_timestamp)
                        .map(|dt| (dt.unix_timestamp_nanos() / 1_000_000) as i64)
                })?;
                let close = at(close_idx).and_then(Value::as_f64).unwrap_or_default();
                let num = |idx: Option<usize>| at(idx).and_then(Value::as_f64);
                Some(Candle {
                    time,
                    open: num(open_idx).unwrap_or(close),
                    high: num(high_idx).unwrap_or(close),
                    low: num(low_idx).unwrap_or(close),
                    close,
                    volume: num(vol_idx).unwrap_or_default(),
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Force exit
// ---------------------------------------------------------------------------

/// Body of `POST /forceexit`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForceExitRequest {
    /// Freqtrade accepts the trade id as a string here.
    pub tradeid: String,
    pub ordertype: String,
}

impl ForceExitRequest {
    /// A limit exit at the current bid, which is the only variant the UI offers.
    pub fn limit(trade_id: i64) -> Self {
        Self {
            tradeid: trade_id.to_string(),
            ordertype: "limit".to_owned(),
        }
    }
}

/// Freqtrade replies to most commands with a bare `{"result": "..."}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct StatusMessage {
    pub result: String,
    pub error: Option<String>,
}

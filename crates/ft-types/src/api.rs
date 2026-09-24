//! The shapes `ft-server` hands to the UI.
//!
//! Aggregated per screen, so each screen costs one request. The Flutter app
//! fanned out three or four calls per screen and repeated them on every tab
//! mount and every bot switch: Open Trades did `/status` + `/balance`, Closed
//! Trades and Dashboard each pulled `/trades?limit=500`, and against a real bot
//! those calls measure 138-166ms apiece.
//!
//! These types live here rather than in the server so the UI can deserialize
//! exactly what the server serialized, with no hand-kept duplicate.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::freqtrade::{Balance, BotConfig, Candle, LogEntry, ProfitSummary, Trade};

/// Wraps every screen payload with its freshness.
///
/// `stale` means the bot could not be reached and this came from cache, which
/// is what drives the offline banner. The Flutter app decided this per screen
/// with a `_loadFromCache()` branch in each one; here it is one server-side
/// field and the UI just renders it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Envelope<T> {
    pub data: T,
    pub stale: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_synced: Option<OffsetDateTime>,
}

impl<T> Envelope<T> {
    /// Freshly fetched from the bot.
    pub fn fresh(data: T) -> Self {
        Self {
            data,
            stale: false,
            last_synced: Some(OffsetDateTime::now_utc()),
        }
    }

    /// Served from cache because the bot was unreachable.
    pub fn stale(data: T, last_synced: Option<OffsetDateTime>) -> Self {
        Self {
            data,
            stale: true,
            last_synced,
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Envelope<U> {
        Envelope {
            data: f(self.data),
            stale: self.stale,
            last_synced: self.last_synced,
        }
    }
}

// ---------------------------------------------------------------------------
// Bots
// ---------------------------------------------------------------------------

/// A bot in the list. Never carries the password.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotSummary {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddBotRequest {
    pub name: String,
    /// Anything the user typed; the server normalizes it.
    pub url: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorderRequest {
    /// Bot ids in the new display order.
    pub ids: Vec<String>,
}

/// Result of pinging a bot, for the per-bot dot on the Bots screen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PingResult {
    pub online: bool,
}

/// Header state: which badge to show and whether the bot is answering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BotMode {
    Dry,
    Live,
    /// Unreachable; the cached config, if any, decided nothing.
    Offline,
}

// ---------------------------------------------------------------------------
// Open Trades
// ---------------------------------------------------------------------------

/// Everything the Open Trades screen renders, in one payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Overview {
    /// Newest first, matching the legacy sort by open date descending.
    pub open_trades: Vec<Trade>,
    /// `balance.total` plus unrealized P/L across open trades.
    pub portfolio_value: f64,
    /// From the spot currency row, not a futures position row.
    pub free: f64,
    pub used: f64,
    /// Sum of `profit_abs` across open trades.
    pub open_pl: f64,
    pub stake_currency: String,
}

// ---------------------------------------------------------------------------
// Closed Trades
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ClosedTrades {
    /// Newest first, by close time.
    pub trades: Vec<Trade>,
    /// Total held locally, which after a sync equals the bot's count.
    pub total: u32,
    pub portfolio_value: f64,
    pub closed_profit: f64,
    pub stake_currency: String,
}

// ---------------------------------------------------------------------------
// Dashboard
// ---------------------------------------------------------------------------

/// Whether the cumulative series is a percentage or an absolute amount.
///
/// Freqtrade only knows `starting_capital` on some configurations; without it
/// a percentage is meaningless, so the legacy chart fell back to plotting the
/// raw stake-currency amount. Making the unit explicit means the UI labels the
/// axis correctly instead of inferring it.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SeriesUnit {
    /// Percent of starting capital.
    Percent,
    /// Absolute profit in the stake currency. The fallback, because
    /// `starting_capital` is absent on plenty of configurations.
    #[default]
    Currency,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct SeriesPoint {
    /// Epoch milliseconds, the trade's close time.
    pub time: i64,
    /// Running total up to and including this trade.
    pub cumulative: f64,
}

/// The cumulative profit curve, computed server-side.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CumulativeSeries {
    pub points: Vec<SeriesPoint>,
    pub unit: SeriesUnit,
    /// Stake currency, for labelling when `unit` is `Currency`.
    pub currency: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Dashboard {
    pub profit: ProfitSummary,
    pub config: BotConfig,
    pub series: CumulativeSeries,
    /// From `/balance`, shown as a stat tile.
    pub free_balance: f64,
    pub stake_currency: String,
}

// ---------------------------------------------------------------------------
// Chart
// ---------------------------------------------------------------------------

/// A pair offered in the chart screen's dropdown.
///
/// Open-trade pairs appear even when absent from the whitelist, carrying the
/// trade's current profit so the dropdown can badge it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChartPair {
    pub pair: String,
    pub in_whitelist: bool,
    /// Profit ratio of the open trade on this pair, if any.
    pub open_profit_ratio: Option<f64>,
}

/// One trade drawn over the price chart.
///
/// Entry and exit are separate points joined by a line, which is what makes a
/// chart worth looking at: you can see where the bot got in and out relative to
/// the price it was reacting to. `exit_*` is `None` while the trade is open.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeOverlay {
    pub trade_id: i64,
    pub is_short: bool,
    pub is_open: bool,
    pub profit_ratio: f64,
    /// Epoch milliseconds.
    pub entry_time: i64,
    pub entry_price: f64,
    pub exit_time: Option<i64>,
    pub exit_price: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Candles {
    pub pair: String,
    pub timeframe: String,
    /// Oldest first.
    pub candles: Vec<Candle>,
    /// Trades on this pair that fall within the candle window.
    pub overlays: Vec<TradeOverlay>,
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Logs {
    /// Oldest first, as Freqtrade returns them; the screen scrolls to the end.
    pub entries: Vec<LogEntry>,
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceExitBody {
    pub trade_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionResult {
    pub message: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Why a request failed, so the UI can react rather than showing one generic
/// error screen for every cause — the thing the Flutter app could not do,
/// because its client collapsed everything into a single `Exception`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Credentials rejected. Re-prompt the user.
    Auth,
    /// Bot unreachable and nothing cached. Offer retry.
    Offline,
    /// No such bot, pair, or trade.
    NotFound,
    /// The request itself was wrong.
    BadRequest,
    /// The bot answered with an error, or we failed internally.
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiErrorBody {
    pub kind: ErrorKind,
    pub message: String,
}

/// Helper: does the raw `/balance` payload's spot row say this much is free?
///
/// Shared by the server and any UI that wants to recompute locally.
pub fn spot_free_used(balance: &Balance) -> (f64, f64) {
    balance
        .spot_entry()
        .map_or((0.0, 0.0), |c| (c.free, c.used))
}

/// The dry/live/offline badge, from a config that may be cached.
pub fn bot_mode(config: Option<&BotConfig>, reachable: bool) -> BotMode {
    match (config, reachable) {
        (_, false) => BotMode::Offline,
        (Some(c), true) if c.dry_run => BotMode::Dry,
        (Some(_), true) => BotMode::Live,
        (None, true) => BotMode::Offline,
    }
}

//! One endpoint per screen, each already aggregated.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use ft_store::kind;
use ft_types::api::{
    ActionResult, Candles, ChartPair, ClosedTrades, CumulativeSeries, Dashboard, Envelope,
    ForceExitBody, Logs, Overview, TradeOverlay,
};
use ft_types::freqtrade::{
    Balance, BotConfig, LogsResponse, ProfitSummary, Trade, WhitelistResponse,
};

use crate::compute;
use crate::error::{ApiError, ApiResult};
use crate::fetch::{self, ttl};
use crate::state::AppState;

/// Query flags common to every screen endpoint.
#[derive(Debug, Default, Deserialize)]
pub struct ScreenQuery {
    /// Bypass the cache TTL and ask the bot now.
    ///
    /// This is pull-to-refresh. It deliberately does not *discard* the cache:
    /// if the forced fetch fails because the bot is unreachable, the cached
    /// data is still there to fall back on, flagged stale. Clearing first
    /// would turn a refresh attempt on a flaky connection into a blank screen.
    #[serde(default, deserialize_with = "de_flag")]
    pub refresh: bool,
}

/// Reads a query flag written any of the ways a client might write it.
///
/// Necessary because `#[serde(flatten)]` makes every query value arrive as a
/// string, so a plain `bool` field rejects `?refresh=true`. Accepting `1` and
/// `yes` too costs nothing and removes a class of silent no-ops.
fn de_flag<'de, D>(de: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Ok(match serde_json::Value::deserialize(de)? {
        serde_json::Value::Bool(b) => b,
        serde_json::Value::String(s) => {
            matches!(s.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
        }
        serde_json::Value::Number(n) => n.as_f64().is_some_and(|v| v != 0.0),
        _ => false,
    })
}

impl ScreenQuery {
    /// Zero TTL means every cached entry is already expired, so the fetch
    /// happens while the cache stays available as a fallback.
    fn ttl(&self, normal: std::time::Duration) -> std::time::Duration {
        if self.refresh {
            std::time::Duration::ZERO
        } else {
            normal
        }
    }
}

/// How many closed trades a screen asks for at once.
const CLOSED_PAGE: u32 = 100;
/// Matches the legacy chart's 100-candle window, with headroom to pan.
const CANDLE_LIMIT: u32 = 300;
const LOG_LIMIT: u32 = 500;

/// `GET /api/bots/:id/config`
pub async fn config(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(q): Query<ScreenQuery>,
) -> ApiResult<Json<Envelope<BotConfig>>> {
    // The header badge reads this endpoint, so it has to learn about an
    // outage even when the config itself is cached and long-lived.
    let offline = fetch::known_offline(&state, &bot_id);
    let mut envelope = load_config(&state, &bot_id, &q).await?.into_envelope();
    envelope.stale |= offline;
    Ok(Json(envelope))
}

async fn load_config(
    state: &AppState,
    bot_id: &str,
    q: &ScreenQuery,
) -> ApiResult<fetch::Fetched<BotConfig>> {
    if let Some(cached) = fetch::offline_cache(state, bot_id, kind::CONFIG, "configuration")? {
        return Ok(cached);
    }
    let client = state.client(bot_id).await?;
    fetch::snapshot(
        state,
        bot_id,
        kind::CONFIG,
        q.ttl(ttl::CONFIG),
        "configuration",
        || async move { client.show_config().await },
    )
    .await
}

async fn load_balance(
    state: &AppState,
    bot_id: &str,
    q: &ScreenQuery,
) -> ApiResult<fetch::Fetched<Balance>> {
    if let Some(cached) = fetch::offline_cache(state, bot_id, kind::BALANCE, "balance")? {
        return Ok(cached);
    }
    let client = state.client(bot_id).await?;
    fetch::snapshot(
        state,
        bot_id,
        kind::BALANCE,
        q.ttl(ttl::BALANCE),
        "balance",
        || async move { client.balance().await },
    )
    .await
}

async fn load_profit(
    state: &AppState,
    bot_id: &str,
    q: &ScreenQuery,
) -> ApiResult<fetch::Fetched<ProfitSummary>> {
    if let Some(cached) = fetch::offline_cache(state, bot_id, kind::PROFIT, "profit summary")? {
        return Ok(cached);
    }
    let client = state.client(bot_id).await?;
    fetch::snapshot(
        state,
        bot_id,
        kind::PROFIT,
        q.ttl(ttl::PROFIT),
        "profit summary",
        || async move { client.profit().await },
    )
    .await
}

/// Refreshes open trades from `/status` into the store, or falls back to what
/// is already there.
///
/// `/status` is authoritative for what is open, so a successful fetch replaces
/// the stored set outright.
async fn load_open_trades(
    state: &AppState,
    bot_id: &str,
    q: &ScreenQuery,
) -> ApiResult<fetch::Fetched<Vec<Trade>>> {
    let store = state.store();
    let mark = store.snapshot::<()>(bot_id, kind::OPEN_MARK)?;

    // Known unreachable: serve the stored set rather than waiting out another
    // connection timeout. See the note in `fetch::snapshot`.
    if fetch::known_offline(state, bot_id) {
        return match mark {
            Some(hit) => Ok(fetch::Fetched {
                value: store.open_trades(bot_id)?,
                fetched_at: Some(hit.fetched_at),
                stale: true,
            }),
            None => Err(ApiError::NoCache {
                what: "open trades",
            }),
        };
    }

    let client = state.client(bot_id).await?;

    // The mark records when open trades were last synced; the trades
    // themselves live in their own table, so there is nothing to cache twice.
    if let Some(hit) = &mark {
        if hit.age() <= q.ttl(ttl::TRADES) {
            return Ok(fetch::Fetched {
                value: store.open_trades(bot_id)?,
                fetched_at: Some(hit.fetched_at),
                stale: false,
            });
        }
    }

    match client.open_trades().await {
        Ok(trades) => {
            store.replace_open_trades(bot_id, &trades)?;
            store.put_snapshot(bot_id, kind::OPEN_MARK, &())?;
            fetch::record_reachable(state, bot_id, true);
            Ok(fetch::Fetched {
                value: trades,
                fetched_at: Some(fetch::now()),
                stale: false,
            })
        }
        // Having synced before is what makes an empty list meaningful: without
        // a mark we cannot tell "no open trades" from "never fetched".
        Err(e) if e.is_offline() && mark.is_some() => {
            fetch::record_reachable(state, bot_id, false);
            Ok(fetch::Fetched {
                value: store.open_trades(bot_id)?,
                fetched_at: mark.map(|m| m.fetched_at),
                stale: true,
            })
        }
        Err(e) if e.is_offline() => Err(ApiError::NoCache {
            what: "open trades",
        }),
        Err(e) => Err(e.into()),
    }
}

/// Brings closed trades up to date using the incremental window, then reads
/// them back from the store.
///
/// The sync fetches only what is new. `/trades` returns oldest-first, so the
/// offset is simply how many we already hold — see `ft_store::SyncPlan`.
async fn sync_closed_trades(
    state: &AppState,
    bot_id: &str,
    q: &ScreenQuery,
) -> ApiResult<fetch::Fetched<u32>> {
    let store = state.store();
    let mark = store.snapshot::<()>(bot_id, kind::CLOSED_MARK)?;

    // Known unreachable: report what we hold rather than waiting out a login
    // that cannot succeed.
    if fetch::known_offline(state, bot_id) {
        return Ok(fetch::Fetched {
            value: store.closed_trade_count(bot_id)?,
            fetched_at: mark.map(|m| m.fetched_at),
            stale: true,
        });
    }

    let client = state.client(bot_id).await?;
    if let Some(hit) = &mark {
        if hit.age() <= q.ttl(ttl::TRADES) {
            return Ok(fetch::Fetched {
                value: store.closed_trade_count(bot_id)?,
                fetched_at: Some(hit.fetched_at),
                stale: false,
            });
        }
    }

    // One cheap call tells us the bot's total; the plan turns that into the
    // smallest window that closes the gap.
    let probe = match client.closed_trades(1, 0).await {
        Ok(probe) => probe,
        Err(e) if e.is_offline() => {
            return Ok(fetch::Fetched {
                value: store.closed_trade_count(bot_id)?,
                fetched_at: mark.map(|m| m.fetched_at),
                stale: true,
            })
        }
        Err(e) => return Err(e.into()),
    };

    let remote_total = probe.total_trades.max(0) as u32;
    match store.sync_plan(bot_id, remote_total)? {
        ft_store::SyncPlan::UpToDate => {}
        ft_store::SyncPlan::FullResync { total } => {
            tracing::info!(
                bot_id,
                total,
                "remote shrank; resyncing trades from scratch"
            );
            store.clear_trades(bot_id)?;
            fetch_closed_window(&client, store, bot_id, 0, total).await?;
        }
        ft_store::SyncPlan::Fetch { offset, limit } => {
            tracing::debug!(bot_id, offset, limit, "incremental trade sync");
            fetch_closed_window(&client, store, bot_id, offset, limit).await?;
        }
    }

    store.put_snapshot(bot_id, kind::CLOSED_MARK, &())?;
    Ok(fetch::Fetched {
        value: store.closed_trade_count(bot_id)?,
        fetched_at: Some(fetch::now()),
        stale: false,
    })
}

/// Pulls a window of closed trades in pages, so a first sync of a bot with
/// thousands of trades does not become one enormous request.
async fn fetch_closed_window(
    client: &ft_client::FreqtradeClient,
    store: &ft_store::Store,
    bot_id: &str,
    offset: u32,
    count: u32,
) -> ApiResult<()> {
    const PAGE: u32 = 500;
    let mut fetched = 0;
    while fetched < count {
        let take = PAGE.min(count - fetched);
        let page = client.closed_trades(take, offset + fetched).await?;
        if page.trades.is_empty() {
            break;
        }
        let received = page.trades.len() as u32;
        store.upsert_trades(bot_id, &page.trades)?;
        fetched += received;
        if received < take {
            break;
        }
    }
    Ok(())
}

/// `GET /api/bots/:id/overview` — the Open Trades screen.
pub async fn overview(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(q): Query<ScreenQuery>,
) -> ApiResult<Json<Envelope<Overview>>> {
    // Run both together: they are independent, and a cache hit makes either
    // essentially free.
    let (trades, balance) = tokio::join!(
        load_open_trades(&state, &bot_id, &q),
        load_balance(&state, &bot_id, &q)
    );
    let trades = trades?;
    let balance = balance?;

    let (free, used) = ft_types::api::spot_free_used(&balance.value);
    let data = Overview {
        portfolio_value: compute::portfolio_value(&balance.value, &trades.value),
        open_pl: compute::open_pl(&trades.value),
        free,
        used,
        stake_currency: compute::stake_currency(Some(&balance.value), None),
        open_trades: trades.value,
    };

    Ok(Json(Envelope {
        data,
        stale: trades.stale || balance.stale || fetch::known_offline(&state, &bot_id),
        last_synced: oldest(trades.fetched_at, balance.fetched_at),
    }))
}

/// `GET /api/bots/:id/closed` — the Closed Trades screen.
pub async fn closed(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(page): Query<Page>,
) -> ApiResult<Json<Envelope<ClosedTrades>>> {
    let (sync, balance, profit) = tokio::join!(
        sync_closed_trades(&state, &bot_id, &page.screen),
        load_balance(&state, &bot_id, &page.screen),
        load_profit(&state, &bot_id, &page.screen)
    );
    let sync = sync?;
    let balance = balance?;
    let profit = profit?;

    let limit = page.limit.unwrap_or(CLOSED_PAGE).clamp(1, 1000);
    let trades = state
        .store()
        .closed_trades(&bot_id, limit, page.offset.unwrap_or(0))?;

    let data = ClosedTrades {
        trades,
        total: sync.value,
        portfolio_value: balance.value.total,
        closed_profit: profit.value.profit_closed_coin,
        stake_currency: compute::stake_currency(Some(&balance.value), Some(&profit.value)),
    };

    Ok(Json(Envelope {
        data,
        stale: sync.stale || balance.stale || profit.stale || fetch::known_offline(&state, &bot_id),
        last_synced: oldest(
            sync.fetched_at,
            oldest(balance.fetched_at, profit.fetched_at),
        ),
    }))
}

#[derive(Debug, Deserialize)]
pub struct Page {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    #[serde(flatten)]
    pub screen: ScreenQuery,
}

/// `GET /api/bots/:id/dashboard`
pub async fn dashboard(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(q): Query<ScreenQuery>,
) -> ApiResult<Json<Envelope<Dashboard>>> {
    let (sync, profit, config, balance) = tokio::join!(
        sync_closed_trades(&state, &bot_id, &q),
        load_profit(&state, &bot_id, &q),
        load_config(&state, &bot_id, &q),
        load_balance(&state, &bot_id, &q)
    );
    let sync = sync?;
    let profit = profit?;
    let config = config?;
    let balance = balance?;

    let stake_currency = compute::stake_currency(Some(&balance.value), Some(&profit.value));
    // Ascending by close time, which is the order the curve accumulates in.
    let trades = state.store().closed_trades_ascending(&bot_id)?;
    let series: CumulativeSeries =
        compute::cumulative_series(&trades, profit.value.starting_capital, &stake_currency);

    let (free, _) = ft_types::api::spot_free_used(&balance.value);
    let data = Dashboard {
        profit: profit.value,
        config: config.value,
        series,
        free_balance: free,
        stake_currency,
    };

    Ok(Json(Envelope {
        data,
        stale: sync.stale
            || profit.stale
            || config.stale
            || balance.stale
            || fetch::known_offline(&state, &bot_id),
        last_synced: oldest(
            sync.fetched_at,
            oldest(
                profit.fetched_at,
                oldest(config.fetched_at, balance.fetched_at),
            ),
        ),
    }))
}

/// `GET /api/bots/:id/logs`
pub async fn logs(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(q): Query<ScreenQuery>,
) -> ApiResult<Json<Envelope<Logs>>> {
    let offline = fetch::offline_cache::<LogsResponse>(&state, &bot_id, kind::LOGS, "logs")?;
    let fetched = match offline {
        Some(cached) => cached,
        None => {
            let client = state.client(&bot_id).await?;
            fetch::snapshot(
                &state,
                &bot_id,
                kind::LOGS,
                q.ttl(ttl::LOGS),
                "logs",
                || async move { client.logs(LOG_LIMIT).await },
            )
            .await?
        }
    };

    let offline = fetch::known_offline(&state, &bot_id);
    let mut envelope = fetched
        .into_envelope()
        .map(|r: LogsResponse| Logs { entries: r.logs });
    envelope.stale |= offline;
    Ok(Json(envelope))
}

/// `GET /api/bots/:id/pairs` — the chart screen's dropdown.
///
/// Whitelist pairs plus any open-trade pair missing from it, each carrying the
/// open trade's profit ratio so the dropdown can badge it.
pub async fn pairs(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(q): Query<ScreenQuery>,
) -> ApiResult<Json<Envelope<Vec<ChartPair>>>> {
    let cached_whitelist =
        fetch::offline_cache::<WhitelistResponse>(&state, &bot_id, kind::WHITELIST, "whitelist")?;
    let whitelist = match cached_whitelist {
        Some(cached) => cached,
        None => {
            let client = state.client(&bot_id).await?;
            fetch::snapshot(
                &state,
                &bot_id,
                kind::WHITELIST,
                q.ttl(ttl::WHITELIST),
                "whitelist",
                || async move { client.whitelist().await },
            )
            .await?
        }
    };
    let open = load_open_trades(&state, &bot_id, &q).await?;

    let profit_for = |pair: &str| {
        open.value
            .iter()
            .find(|t| t.pair == pair)
            .map(|t| t.profit_ratio)
    };

    let whitelist_pairs: WhitelistResponse = whitelist.value;
    let mut pairs: Vec<ChartPair> = whitelist_pairs
        .whitelist
        .iter()
        .map(|pair| ChartPair {
            pair: pair.clone(),
            in_whitelist: true,
            open_profit_ratio: profit_for(pair),
        })
        .collect();

    // A pair can be traded and then dropped from the whitelist; the chart must
    // still offer it while the trade is open.
    for trade in &open.value {
        if !pairs.iter().any(|p| p.pair == trade.pair) {
            pairs.push(ChartPair {
                pair: trade.pair.clone(),
                in_whitelist: false,
                open_profit_ratio: Some(trade.profit_ratio),
            });
        }
    }

    Ok(Json(Envelope {
        data: pairs,
        stale: whitelist.stale || open.stale || fetch::known_offline(&state, &bot_id),
        last_synced: oldest(whitelist.fetched_at, open.fetched_at),
    }))
}

#[derive(Debug, Deserialize)]
pub struct CandleQuery {
    pub pair: String,
    pub timeframe: Option<String>,
    pub limit: Option<u32>,
}

/// `GET /api/bots/:id/candles?pair=…`
pub async fn candles(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Query(query): Query<CandleQuery>,
) -> ApiResult<Json<Envelope<Candles>>> {
    if query.pair.trim().is_empty() {
        return Err(ApiError::BadRequest("a pair is required".into()));
    }
    // Default to the bot's own timeframe rather than guessing 5m.
    let timeframe = match query.timeframe {
        Some(tf) if !tf.is_empty() => tf,
        _ => load_config(&state, &bot_id, &ScreenQuery::default())
            .await?
            .value
            .timeframe
            .unwrap_or_else(|| "5m".to_owned()),
    };
    let limit = query.limit.unwrap_or(CANDLE_LIMIT).clamp(1, 1500);
    let store = state.store();

    if fetch::known_offline(&state, &bot_id) {
        let cached = store.candles(&bot_id, &query.pair, &timeframe, limit)?;
        if cached.is_empty() {
            return Err(ApiError::NoCache { what: "candles" });
        }
        let overlays = build_overlays(store, &bot_id, &query.pair, &cached)?;
        return Ok(Json(Envelope {
            stale: true,
            last_synced: None,
            data: Candles {
                pair: query.pair,
                timeframe,
                candles: cached,
                overlays,
            },
        }));
    }

    let client = state.client(&bot_id).await?;
    let pair = query.pair.clone();
    let tf = timeframe.clone();
    let fetched = fetch::with_fallback(
        async {
            let response = client.pair_candles(&pair, &tf, limit).await?;
            Ok(response.candles())
        },
        "candles",
        || {
            let cached = store.candles(&bot_id, &query.pair, &timeframe, limit)?;
            Ok((!cached.is_empty()).then_some((cached, None)))
        },
    )
    .await?;

    if !fetched.stale {
        store.put_candles(&bot_id, &query.pair, &timeframe, &fetched.value)?;
    }

    // Markers are computed here rather than in the UI: the trades are already
    // in the store, and sending only what falls inside the visible window
    // keeps a bot with thousands of trades from shipping all of them.
    let overlays = build_overlays(store, &bot_id, &query.pair, &fetched.value)?;

    Ok(Json(Envelope {
        stale: fetched.stale || fetch::known_offline(&state, &bot_id),
        last_synced: fetched.fetched_at,
        data: Candles {
            pair: query.pair,
            timeframe,
            candles: fetched.value,
            overlays,
        },
    }))
}

/// Trades on `pair` that intersect the candle window.
///
/// A trade counts if either end lands in the window, or if it spans the whole
/// of it — a position opened before the first candle and closed after the last
/// one is still the most interesting thing on the chart.
fn build_overlays(
    store: &ft_store::Store,
    bot_id: &str,
    pair: &str,
    candles: &[ft_types::freqtrade::Candle],
) -> ApiResult<Vec<TradeOverlay>> {
    let (Some(first), Some(last)) = (candles.first(), candles.last()) else {
        return Ok(Vec::new());
    };
    let (from, to) = (first.time, last.time);

    Ok(store
        .trades_for_pair(bot_id, pair)?
        .into_iter()
        .filter_map(|trade| {
            let entry_time = trade.opened_at()?.unix_timestamp_nanos() / 1_000_000;
            let entry_time = entry_time as i64;
            let exit_time = trade
                .closed_at()
                .map(|d| (d.unix_timestamp_nanos() / 1_000_000) as i64);

            let ends_inside = (from..=to).contains(&entry_time)
                || exit_time.is_some_and(|t| (from..=to).contains(&t));
            let spans_window = entry_time < from && exit_time.is_none_or(|t| t > to);
            if !ends_inside && !spans_window {
                return None;
            }

            Some(TradeOverlay {
                trade_id: trade.trade_id,
                is_short: trade.is_short,
                is_open: trade.is_open,
                profit_ratio: trade.profit_ratio,
                entry_time,
                entry_price: trade.open_rate,
                exit_time,
                exit_price: trade.close_rate,
            })
        })
        .collect())
}

/// `POST /api/bots/:id/forceexit`
///
/// Places a real limit order. The UI confirms first; nothing here second-guesses
/// that, but it is deliberately not part of any aggregated read.
pub async fn force_exit(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Json(body): Json<ForceExitBody>,
) -> ApiResult<Json<ActionResult>> {
    let client = state.client(&bot_id).await?;
    let result = client.force_exit(body.trade_id).await?;
    tracing::info!(bot_id, trade_id = body.trade_id, "force exit requested");

    // The open set just changed. Drop the freshness mark so the next read
    // refetches, rather than serving a cached list that still shows the trade
    // we just exited.
    state.store().clear_snapshot(&bot_id, kind::OPEN_MARK)?;

    Ok(Json(ActionResult {
        message: if result.result.is_empty() {
            "exit order placed".to_owned()
        } else {
            result.result
        },
    }))
}

/// The older of two sync times, so an aggregate reports its weakest part.
fn oldest(
    a: Option<time::OffsetDateTime>,
    b: Option<time::OffsetDateTime>,
) -> Option<time::OffsetDateTime> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

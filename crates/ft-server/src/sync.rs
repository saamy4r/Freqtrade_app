//! Background refresh.
//!
//! Runs only while a client is connected, which on Android means only while
//! the app is open. A timer that keeps waking the radio to poll a bot nobody
//! is looking at is exactly the behaviour that makes a phone app feel heavy.

use std::time::Duration;

use ft_store::kind;

use crate::error::ApiResult;
use crate::events::ServerEvent;
use crate::state::AppState;

/// How often to refresh a bot while someone is watching.
///
/// Prices move constantly, but this is a monitoring app, not a trading
/// terminal; a quarter of a minute is well inside the time it takes to notice
/// a number is stale, and it is gentle on both the bot and the battery.
const INTERVAL: Duration = Duration::from_secs(15);

/// How often to check whether anyone has started watching.
const IDLE_POLL: Duration = Duration::from_secs(2);

/// Starts the background refresh loop.
pub fn spawn(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if !state.has_watchers() {
                tokio::time::sleep(IDLE_POLL).await;
                continue;
            }
            refresh_all(&state).await;
            tokio::time::sleep(INTERVAL).await;
        }
    })
}

async fn refresh_all(state: &AppState) {
    let bots = match state.store().list_bots() {
        Ok(bots) => bots,
        Err(e) => {
            tracing::warn!(error = %e, "background sync could not list bots");
            return;
        }
    };

    for bot in bots {
        let stale = refresh_bot(state, &bot.id).await;
        // Recorded so a screen served from a still-fresh cache reports the
        // outage too, rather than claiming a freshness it never verified.
        crate::fetch::record_reachable(state, &bot.id, !stale);
        let _ = state.events().send(ServerEvent::BotUpdated {
            bot_id: bot.id,
            stale,
        });
    }
}

/// Refreshes one bot's cached data. Returns whether it was unreachable.
///
/// Only the data that actually moves: open trades change with every tick,
/// balance follows them, and closed trades arrive when a position exits.
/// Config and whitelist are left to their own long TTLs.
async fn refresh_bot(state: &AppState, bot_id: &str) -> bool {
    let Ok(client) = state.client(bot_id).await else {
        // Cannot even log in. Credentials are the user's problem to fix, and
        // hammering a rejected login on a timer helps nobody.
        return true;
    };
    let store = state.store();

    match client.open_trades().await {
        Ok(trades) => {
            if let Err(e) = store
                .replace_open_trades(bot_id, &trades)
                .and_then(|()| store.put_snapshot(bot_id, kind::OPEN_MARK, &()))
            {
                tracing::warn!(error = %e, "could not cache open trades");
            }
        }
        Err(e) if e.is_offline() => return true,
        Err(e) => tracing::debug!(error = %e, "background open-trade refresh failed"),
    }

    match client.balance().await {
        Ok(balance) => {
            let _ = store.put_snapshot(bot_id, kind::BALANCE, &balance);
        }
        Err(e) if e.is_offline() => return true,
        Err(e) => tracing::debug!(error = %e, "background balance refresh failed"),
    }

    match client.profit().await {
        Ok(profit) => {
            let _ = store.put_snapshot(bot_id, kind::PROFIT, &profit);
        }
        Err(e) if e.is_offline() => return true,
        Err(e) => tracing::debug!(error = %e, "background profit refresh failed"),
    }

    // Config, whitelist and logs change slowly and have long TTLs, so the
    // foreground rarely refetches them. Warming them here is what makes a
    // screen work offline that the user has never opened while the bot was up
    // — otherwise the Chart and Logs tabs have nothing to show on a phone that
    // went out of signal before they were first visited.
    if let Ok(config) = client.show_config().await {
        let _ = store.put_snapshot(bot_id, kind::CONFIG, &config);
    }
    if let Ok(whitelist) = client.whitelist().await {
        let _ = store.put_snapshot(bot_id, kind::WHITELIST, &whitelist);
    }
    if let Ok(logs) = client.logs(500).await {
        let _ = store.put_snapshot(bot_id, kind::LOGS, &logs);
    }

    // One cheap call reveals whether any trade has closed since last time;
    // the catch-up then fetches only the tail of the list that holds them.
    match client.closed_trades(1, 0).await {
        Ok(probe) => {
            let remote = probe.total_trades.max(0) as u32;
            if let Err(e) = catch_up_closed(&client, store, bot_id, remote).await {
                tracing::warn!(error = %e, "background trade sync failed");
            }
            let _ = store.put_snapshot(bot_id, kind::CLOSED_MARK, &());
        }
        Err(e) if e.is_offline() => return true,
        Err(e) => tracing::debug!(error = %e, "background trade probe failed"),
    }

    false
}

/// Most trades one `/trades` request may return.
const PAGE: u32 = 500;

/// Brings the stored closed trades level with a bot that reports
/// `remote_total` of them.
///
/// `/trades` is ordered by trade id, and a trade that stayed open a long time
/// closes into the middle of that list rather than at its end (see
/// `ft_store::SyncPlan`). So this reads windows that end at the tail of the
/// list, each twice the size of the last, until the local count reaches the
/// bot's. The common case — a trade or two just closed — is still a single
/// small request; a long-held trade costs a few more.
pub(crate) async fn catch_up_closed(
    client: &ft_client::FreqtradeClient,
    store: &ft_store::Store,
    bot_id: &str,
    remote_total: u32,
) -> ApiResult<()> {
    let mut window = match store.sync_plan(bot_id, remote_total)? {
        ft_store::SyncPlan::UpToDate => return Ok(()),
        ft_store::SyncPlan::FullResync { total } => {
            tracing::info!(bot_id, total, "remote shrank; resyncing trades from scratch");
            store.clear_trades(bot_id)?;
            total
        }
        ft_store::SyncPlan::Fetch { missing } => missing,
    };

    // Everything at or past `end` has been read in this sync.
    let mut end = remote_total;
    while end > 0 && store.closed_trade_count(bot_id)? < remote_total {
        let take = window.clamp(1, PAGE).min(end);
        let start = end - take;
        tracing::debug!(bot_id, offset = start, limit = take, "closed trade sync");
        let page = client.closed_trades(take, start).await?;
        if page.trades.is_empty() {
            break;
        }
        store.upsert_trades(bot_id, &page.trades)?;
        end = start;
        window = window.saturating_mul(2);
    }
    Ok(())
}

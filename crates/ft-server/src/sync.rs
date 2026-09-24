//! Background refresh.
//!
//! Runs only while a client is connected, which on Android means only while
//! the app is open. A timer that keeps waking the radio to poll a bot nobody
//! is looking at is exactly the behaviour that makes a phone app feel heavy.

use std::sync::Arc;
use std::time::Duration;

use ft_store::kind;

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

    // One cheap call reveals whether any trade has closed since last time; the
    // incremental plan then fetches only those.
    match client.closed_trades(1, 0).await {
        Ok(probe) => {
            let remote = probe.total_trades.max(0) as u32;
            match store.sync_plan(bot_id, remote) {
                Ok(ft_store::SyncPlan::UpToDate) => {}
                Ok(plan) => {
                    if let Err(e) = apply_plan(&client, store, bot_id, plan).await {
                        tracing::warn!(error = %e, "background trade sync failed");
                    }
                }
                Err(e) => tracing::warn!(error = %e, "could not plan a trade sync"),
            }
            let _ = store.put_snapshot(bot_id, kind::CLOSED_MARK, &());
        }
        Err(e) if e.is_offline() => return true,
        Err(e) => tracing::debug!(error = %e, "background trade probe failed"),
    }

    false
}

async fn apply_plan(
    client: &ft_client::FreqtradeClient,
    store: &Arc<ft_store::Store>,
    bot_id: &str,
    plan: ft_store::SyncPlan,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (offset, limit) = match plan {
        ft_store::SyncPlan::UpToDate => return Ok(()),
        ft_store::SyncPlan::Fetch { offset, limit } => (offset, limit),
        ft_store::SyncPlan::FullResync { total } => {
            store.clear_trades(bot_id)?;
            (0, total)
        }
    };
    if limit == 0 {
        return Ok(());
    }
    let page = client.closed_trades(limit.min(500), offset).await?;
    store.upsert_trades(bot_id, &page.trades)?;
    Ok(())
}

//! Shared server state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};

use crate::events::ServerEvent;

use ft_client::FreqtradeClient;
use ft_store::{BotRecord, Store};

use crate::error::{ApiError, ApiResult};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

/// Decrements the watcher count when dropped, so a client that disconnects
/// without ceremony still stops the background sync.
pub struct WatcherGuard(Arc<AtomicUsize>);

impl Drop for WatcherGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

struct Inner {
    store: Arc<Store>,
    events: broadcast::Sender<ServerEvent>,
    /// How many clients are holding the event stream open.
    watchers: Arc<AtomicUsize>,
    /// One logged-in client per bot, kept alive so the token (and its
    /// connection pool) survives between requests. Rebuilding per request
    /// would mean a fresh login every time and throw away `ft-client`'s whole
    /// refresh mechanism.
    clients: RwLock<HashMap<String, FreqtradeClient>>,
}

impl AppState {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                // Capacity is generous: events are tiny and a slow client
                // lagging is handled by telling it to re-read everything.
                events: broadcast::channel(64).0,
                watchers: Arc::new(AtomicUsize::new(0)),
                clients: RwLock::new(HashMap::new()),
            }),
        }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.inner.store
    }

    pub fn events(&self) -> &broadcast::Sender<ServerEvent> {
        &self.inner.events
    }

    /// Registers a watcher for as long as the returned guard lives.
    pub fn subscribe(&self) -> WatcherGuard {
        self.inner.watchers.fetch_add(1, Ordering::Relaxed);
        WatcherGuard(Arc::clone(&self.inner.watchers))
    }

    /// Whether anyone is listening, which gates the background sync.
    pub fn has_watchers(&self) -> bool {
        self.inner.watchers.load(Ordering::Relaxed) > 0
    }

    /// Looks up a bot, or 404s.
    pub fn bot(&self, bot_id: &str) -> ApiResult<BotRecord> {
        self.inner
            .store
            .get_bot(bot_id)?
            .ok_or_else(|| ApiError::UnknownBot(bot_id.to_owned()))
    }

    /// A logged-in client for `bot_id`, created on first use and cached.
    ///
    /// Once built, the client handles its own token refresh, so this only logs
    /// in when there is no client yet.
    pub async fn client(&self, bot_id: &str) -> ApiResult<FreqtradeClient> {
        if let Some(client) = self.inner.clients.read().await.get(bot_id) {
            return Ok(client.clone());
        }

        let bot = self.bot(bot_id)?;
        let password = self.inner.store.password(bot_id)?;
        let client = FreqtradeClient::new(&bot.url, &bot.username, &password)?;
        if let Err(e) = client.login().await {
            // Logging in is itself a network call, so this is the first place
            // an outage shows up. Record it, or every later request repeats
            // the same timeout before discovering the same thing.
            if e.is_offline() {
                let _ = self
                    .inner
                    .store
                    .put_snapshot(bot_id, ft_store::kind::REACHABLE, &false);
            }
            return Err(e.into());
        }

        let mut guard = self.inner.clients.write().await;
        // Another request may have logged in while we were; keep theirs so we
        // do not discard a working token for an equally working one.
        Ok(guard.entry(bot_id.to_owned()).or_insert(client).clone())
    }

    /// Drops a bot's cached client, so the next request logs in again.
    /// Used when a bot is deleted or its credentials change.
    pub async fn forget_client(&self, bot_id: &str) {
        self.inner.clients.write().await.remove(bot_id);
    }
}

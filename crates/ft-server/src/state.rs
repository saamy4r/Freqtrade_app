//! Shared server state.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use ft_client::FreqtradeClient;
use ft_store::{BotRecord, Store};

use crate::error::{ApiError, ApiResult};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    store: Arc<Store>,
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
                clients: RwLock::new(HashMap::new()),
            }),
        }
    }

    pub fn store(&self) -> &Store {
        &self.inner.store
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
        client.login().await?;

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

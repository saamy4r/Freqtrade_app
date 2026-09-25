//! Records the store hands back.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A configured bot, without its password.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BotRecord {
    pub id: String,
    pub name: String,
    /// Normalized, ending in `/api/v1`.
    pub url: String,
    pub username: String,
    pub sort_order: i64,
}

/// A bot being added. The password is separate so it is obvious at every call
/// site where the secret travels.
#[derive(Debug, Clone)]
pub struct NewBot {
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
}

/// Cached data plus when it was fetched, which drives the offline banner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Cached<T> {
    pub value: T,
    #[serde(with = "time::serde::rfc3339")]
    pub fetched_at: OffsetDateTime,
}

impl<T> Cached<T> {
    /// How long ago this was fetched, saturating at zero if the clock moved.
    pub fn age(&self) -> time::Duration {
        (OffsetDateTime::now_utc() - self.fetched_at).max(time::Duration::ZERO)
    }
}

/// What a closed-trade sync needs to fetch.
///
/// `/trades` returns the *oldest* first (verified against a live bot:
/// `offset=0` yields trade ids 1..50), so new trades land at the end and
/// `offset = what we already have` fetches exactly the new ones. The Flutter
/// app always asked for `limit=500&offset=0`, which silently returns the
/// oldest 500 — it worked only because that bot had fewer than 500 trades.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncPlan {
    /// Local count matches the bot's; nothing to do.
    UpToDate,
    /// Fetch this window and upsert it.
    Fetch { offset: u32, limit: u32 },
    /// The bot reports fewer trades than we hold, so its database was reset or
    /// trades were deleted. Our ids no longer mean what they meant; drop the
    /// local set and refetch.
    FullResync { total: u32 },
}

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
/// `/trades` returns closed trades ordered by trade *id* (verified against a
/// live bot: `offset=0` yields the lowest ids). Trades do not close in id
/// order, though: a long-running trade opened early closes after younger ones,
/// and its row lands in the middle of the list, not at the end. So the local
/// count is *not* a safe offset — fetching `offset = local` re-reads a trade we
/// already hold and skips the one that closed. The sync instead walks back
/// from the end of the list until the counts agree; see
/// `ft_server::sync::catch_up_closed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncPlan {
    /// Local count matches the bot's; nothing to do.
    UpToDate,
    /// The bot holds this many closed trades we do not.
    Fetch { missing: u32 },
    /// The bot reports fewer trades than we hold, so its database was reset or
    /// trades were deleted. Our ids no longer mean what they meant; drop the
    /// local set and refetch.
    FullResync { total: u32 },
}

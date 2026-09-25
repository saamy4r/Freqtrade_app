//! Cache-first reads with a live fallback.
//!
//! Three outcomes, in priority order:
//!
//! 1. Cache is present and younger than the TTL — return it untouched. This is
//!    what makes revisiting a tab free. The Flutter app had no such path: every
//!    tab mount and every bot switch re-issued the whole fan-out, and against a
//!    real bot `/trades` and `/profit` measure 138-166ms each.
//! 2. Cache is missing or stale — fetch from the bot, store it, return it.
//! 3. The fetch fails and the bot is simply unreachable — return whatever is
//!    cached, flagged `stale`, so the screen still renders. An auth failure or
//!    a bot-side error is *not* swallowed this way; those propagate, because
//!    showing yesterday's numbers to someone whose password is wrong just
//!    hides the problem.

use std::future::Future;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Serialize;
use time::OffsetDateTime;

use ft_types::api::Envelope;

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// How long a cached response is served without re-asking the bot.
pub mod ttl {
    use std::time::Duration;

    /// Prices and P/L move constantly, but a tab switch inside a few seconds
    /// should not pay for a round trip.
    pub const TRADES: Duration = Duration::from_secs(5);
    pub const BALANCE: Duration = Duration::from_secs(5);
    pub const PROFIT: Duration = Duration::from_secs(10);
    /// Configuration effectively never changes while the bot runs.
    pub const CONFIG: Duration = Duration::from_secs(300);
    pub const WHITELIST: Duration = Duration::from_secs(120);
    pub const LOGS: Duration = Duration::from_secs(5);
}

/// The current time at whole-second precision.
///
/// The store persists `fetched_at` as a unix timestamp in seconds, so a live
/// fetch stamped with nanoseconds would report a more precise `last_synced`
/// than the same data reports once it comes back from cache. Truncating keeps
/// the field stable across that boundary rather than claiming precision we
/// cannot preserve.
pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
}

/// Records whether the bot answered, so every later read agrees about it.
pub fn record_reachable(state: &AppState, bot_id: &str, reachable: bool) {
    let previous = state
        .store()
        .snapshot::<bool>(bot_id, ft_store::kind::REACHABLE)
        .ok()
        .flatten()
        .map(|c| c.value);
    if previous != Some(reachable) {
        tracing::debug!(bot_id, reachable, "bot reachability changed");
    }
    let _ = state
        .store()
        .put_snapshot(bot_id, ft_store::kind::REACHABLE, &reachable);
}

/// Whether the bot was unreachable the last time anyone tried.
///
/// A screen served entirely from a still-fresh cache has learned nothing about
/// the bot, so it asks this rather than claiming freshness it cannot vouch for.
pub fn known_offline(state: &AppState, bot_id: &str) -> bool {
    state
        .store()
        .snapshot::<bool>(bot_id, ft_store::kind::REACHABLE)
        .ok()
        .flatten()
        .is_some_and(|c| !c.value)
}

/// Cached data for a bot already known to be unreachable, if any.
///
/// Must be consulted *before* obtaining a client: logging in is itself a
/// network call, so a dead bot fails there first and the caller never reaches
/// any later check. Returns `Ok(None)` when the bot is not known to be down,
/// meaning the caller should go ahead and fetch.
pub fn offline_cache<T: serde::de::DeserializeOwned>(
    state: &AppState,
    bot_id: &str,
    kind: &str,
    what: &'static str,
) -> ApiResult<Option<Fetched<T>>> {
    if !known_offline(state, bot_id) {
        return Ok(None);
    }
    match state.store().snapshot::<T>(bot_id, kind)? {
        Some(hit) => Ok(Some(Fetched {
            value: hit.value,
            fetched_at: Some(hit.fetched_at),
            stale: true,
        })),
        None => Err(ApiError::NoCache { what }),
    }
}

/// Obtains a client, or `None` when the bot cannot be reached.
///
/// Logging in is itself a network call, so a dead bot fails here — before any
/// cache is consulted. Reporting that as `None` rather than an error lets the
/// caller serve what it has, which is the whole point of keeping a cache.
pub async fn client_or_offline(
    state: &AppState,
    bot_id: &str,
) -> ApiResult<Option<ft_client::FreqtradeClient>> {
    match state.client(bot_id).await {
        Ok(client) => Ok(Some(client)),
        Err(ApiError::Client(e)) if e.is_offline() => Ok(None),
        Err(e) => Err(e),
    }
}

/// Cached data, or an honest error when there is none.
///
/// Unlike [`offline_cache`] this does not ask whether the bot is known to be
/// down — the caller has just found out for itself.
pub fn cached_only<T: serde::de::DeserializeOwned>(
    state: &AppState,
    bot_id: &str,
    kind: &str,
    what: &'static str,
) -> ApiResult<Fetched<T>> {
    match state.store().snapshot::<T>(bot_id, kind)? {
        Some(hit) => Ok(Fetched {
            value: hit.value,
            fetched_at: Some(hit.fetched_at),
            stale: true,
        }),
        None => Err(ApiError::NoCache { what }),
    }
}

/// A value plus where it came from.
pub struct Fetched<T> {
    pub value: T,
    pub fetched_at: Option<OffsetDateTime>,
    pub stale: bool,
}

impl<T> Fetched<T> {
    pub fn into_envelope(self) -> Envelope<T> {
        Envelope {
            data: self.value,
            stale: self.stale,
            last_synced: self.fetched_at,
        }
    }
}

/// Returns the freshest acceptable value for a snapshot-shaped endpoint.
///
/// `what` names the data for the error message when there is nothing at all.
pub async fn snapshot<T, F, Fut>(
    state: &AppState,
    bot_id: &str,
    kind: &str,
    ttl: Duration,
    what: &'static str,
    fetch: F,
) -> ApiResult<Fetched<T>>
where
    T: Serialize + DeserializeOwned,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, ft_client::ClientError>>,
{
    let cached = state.store().snapshot::<T>(bot_id, kind)?;

    // Already known to be unreachable: serve what we have and do not spend a
    // connection timeout rediscovering it. A screen aggregates several of
    // these, so re-attempting each one costs tens of seconds -- long enough
    // that the next background refresh restarts the request before it
    // finishes, leaving the screen stuck mid-load forever. Noticing the bot
    // has come back is the background sync's job, off the request path.
    if known_offline(state, bot_id) {
        if let Some(hit) = cached {
            return Ok(Fetched {
                value: hit.value,
                fetched_at: Some(hit.fetched_at),
                stale: true,
            });
        }
        return Err(ApiError::NoCache { what });
    }

    if let Some(hit) = &cached {
        if hit.age() <= ttl {
            tracing::trace!(kind, "cache hit");
            return Ok(Fetched {
                fetched_at: Some(hit.fetched_at),
                stale: false,
                // Re-read rather than clone: T is not required to be Clone.
                value: state
                    .store()
                    .snapshot::<T>(bot_id, kind)?
                    .map(|c| c.value)
                    .expect("snapshot present a moment ago"),
            });
        }
    }

    match fetch().await {
        Ok(value) => {
            state.store().put_snapshot(bot_id, kind, &value)?;
            record_reachable(state, bot_id, true);
            Ok(Fetched {
                value,
                fetched_at: Some(now()),
                stale: false,
            })
        }
        Err(e) if e.is_offline() => {
            record_reachable(state, bot_id, false);
            match cached {
                // Unreachable, but we have something to show.
                Some(hit) => {
                    tracing::debug!(kind, "bot unreachable, serving cache");
                    Ok(Fetched {
                        value: hit.value,
                        fetched_at: Some(hit.fetched_at),
                        stale: true,
                    })
                }
                None => Err(ApiError::NoCache { what }),
            }
        }
        // Auth failures and bot-side errors are real and must surface.
        Err(e) => Err(e.into()),
    }
}

/// Runs `fetch`, falling back to `cached` when the bot is merely unreachable.
///
/// For endpoints whose cache lives in a real table (trades, candles) rather
/// than a snapshot blob.
pub async fn with_fallback<T, Fut>(
    fetch: Fut,
    what: &'static str,
    cached: impl FnOnce() -> ApiResult<Option<(T, Option<OffsetDateTime>)>>,
) -> ApiResult<Fetched<T>>
where
    Fut: Future<Output = Result<T, ft_client::ClientError>>,
{
    match fetch.await {
        Ok(value) => Ok(Fetched {
            value,
            fetched_at: Some(now()),
            stale: false,
        }),
        Err(e) if e.is_offline() => match cached()? {
            Some((value, fetched_at)) => Ok(Fetched {
                value,
                fetched_at,
                stale: true,
            }),
            None => Err(ApiError::NoCache { what }),
        },
        Err(e) => Err(e.into()),
    }
}

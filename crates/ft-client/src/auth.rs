//! Token state and the single-flight refresh.
//!
//! Freqtrade issues a short-lived access token (~15 min by default) alongside a
//! long-lived refresh token. The Flutter client kept only the access token and
//! had no 401 handling, so once it expired every screen showed the error view
//! until the user switched bots and triggered a fresh login. That is the defect
//! this module exists to remove.
//!
//! The wrinkle is concurrency. A single screen fans out several requests at
//! once, so an expired token produces a burst of simultaneous 401s. Refreshing
//! per-401 would fire N refreshes, and the losers would install stale tokens
//! over the winner's. Instead each refresh is tagged with a generation; a
//! caller that takes the lock and finds the generation already moved on simply
//! uses the token that is now there.

use tokio::sync::{Mutex, RwLock};

/// Credentials for the initial login and for re-login when a refresh token has
/// itself expired.
#[derive(Clone)]
pub(crate) struct Credentials {
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for Credentials {
    /// Hand-written so a password cannot reach a log through `{:?}`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// A token pair plus the generation it was issued in.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tokens {
    pub access: String,
    pub refresh: Option<String>,
    /// Incremented on every successful login or refresh. A request captures
    /// this before sending; if it 401s and the generation has since moved, some
    /// other task already fixed the problem.
    pub generation: u64,
}

/// Shared, concurrently-accessed token state.
#[derive(Debug, Default)]
pub(crate) struct TokenStore {
    tokens: RwLock<Option<Tokens>>,
    /// Held across a refresh so only one runs at a time.
    refresh_gate: Mutex<()>,
}

impl TokenStore {
    /// The current access token and its generation, or `None` before login.
    pub async fn current(&self) -> Option<(String, u64)> {
        self.tokens
            .read()
            .await
            .as_ref()
            .map(|t| (t.access.clone(), t.generation))
    }

    pub async fn refresh_token(&self) -> Option<String> {
        self.tokens
            .read()
            .await
            .as_ref()
            .and_then(|t| t.refresh.clone())
    }

    #[cfg(test)]
    pub async fn generation(&self) -> u64 {
        self.tokens
            .read()
            .await
            .as_ref()
            .map_or(0, |t| t.generation)
    }

    /// Installs a freshly issued token pair, bumping the generation.
    ///
    /// `refresh` is `None` on a `/token/refresh` response, which returns only a
    /// new access token; the existing refresh token is kept in that case.
    pub async fn install(&self, access: String, refresh: Option<String>) -> u64 {
        let mut guard = self.tokens.write().await;
        let generation = guard.as_ref().map_or(0, |t| t.generation) + 1;
        let refresh = refresh.or_else(|| guard.as_ref().and_then(|t| t.refresh.clone()));
        *guard = Some(Tokens {
            access,
            refresh,
            generation,
        });
        generation
    }

    pub async fn clear(&self) {
        *self.tokens.write().await = None;
    }

    /// Serializes refreshes. See [`RefreshOutcome`].
    pub async fn begin_refresh(&self, seen_generation: u64) -> RefreshOutcome<'_> {
        let guard = self.refresh_gate.lock().await;
        match self.current().await {
            // Someone refreshed while we waited for the gate; use their token.
            Some((access, generation)) if generation > seen_generation => {
                RefreshOutcome::AlreadyRefreshed(access)
            }
            _ => RefreshOutcome::Ours(guard),
        }
    }
}

/// Result of trying to become the task that refreshes.
pub(crate) enum RefreshOutcome<'a> {
    /// This task holds the gate and must perform the refresh.
    Ours(tokio::sync::MutexGuard<'a, ()>),
    /// Another task already refreshed; retry with this access token instead.
    AlreadyRefreshed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn install_bumps_the_generation() {
        let store = TokenStore::default();
        assert_eq!(store.current().await, None);
        assert_eq!(store.generation().await, 0);

        assert_eq!(store.install("a".into(), Some("r".into())).await, 1);
        assert_eq!(store.current().await, Some(("a".into(), 1)));
        assert_eq!(store.refresh_token().await, Some("r".into()));

        assert_eq!(store.install("b".into(), None).await, 2);
        assert_eq!(store.current().await, Some(("b".into(), 2)));
    }

    #[tokio::test]
    async fn a_refresh_response_keeps_the_existing_refresh_token() {
        // /token/refresh returns only an access token. Dropping the refresh
        // token there would give us exactly one refresh and then the same
        // dead-end the Flutter client had.
        let store = TokenStore::default();
        store.install("a".into(), Some("r".into())).await;
        store.install("b".into(), None).await;
        assert_eq!(store.refresh_token().await, Some("r".into()));
    }

    #[tokio::test]
    async fn clear_forgets_everything() {
        let store = TokenStore::default();
        store.install("a".into(), Some("r".into())).await;
        store.clear().await;
        assert_eq!(store.current().await, None);
        assert_eq!(store.refresh_token().await, None);
    }

    #[tokio::test]
    async fn the_loser_of_a_refresh_race_reuses_the_winners_token() {
        let store = TokenStore::default();
        store.install("expired".into(), Some("r".into())).await;
        let seen = store.generation().await;

        // Task A takes the gate and refreshes.
        let outcome = store.begin_refresh(seen).await;
        assert!(matches!(outcome, RefreshOutcome::Ours(_)));
        store.install("fresh".into(), None).await;
        drop(outcome);

        // Task B 401'd against the same old token and now reaches the gate.
        // It must not refresh again. Bound to a local so the guard inside the
        // outcome drops before `store` does.
        let second = store.begin_refresh(seen).await;
        match second {
            RefreshOutcome::AlreadyRefreshed(token) => assert_eq!(token, "fresh"),
            RefreshOutcome::Ours(_) => panic!("refreshed twice for one expiry"),
        }
    }

    #[tokio::test]
    async fn credentials_do_not_leak_through_debug() {
        let creds = Credentials {
            username: "sami".into(),
            password: "hunter2".into(),
        };
        let rendered = format!("{creds:?}");
        assert!(rendered.contains("sami"));
        assert!(!rendered.contains("hunter2"), "password leaked: {rendered}");
    }
}

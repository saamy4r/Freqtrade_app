//! Client errors.
//!
//! The split that matters downstream is [`ClientError::is_offline`]: a timeout
//! or a connection failure means "serve the cache and flag it stale", whereas a
//! 4xx means the bot answered and genuinely refused. The Flutter app collapsed
//! both into one `Exception` and showed the same full-screen error for a
//! flat-out-wrong password as for a phone that had wandered off Wi-Fi.

/// Longest response body echoed back in an error message.
const BODY_SNIPPET: usize = 300;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("{url:?} is not a usable bot URL: {reason}")]
    InvalidUrl { url: String, reason: String },

    #[error("{endpoint} timed out")]
    Timeout { endpoint: String },

    #[error("could not reach the bot ({endpoint}): {source}")]
    Transport {
        endpoint: String,
        #[source]
        source: reqwest::Error,
    },

    /// Credentials were rejected, or the token could not be refreshed.
    #[error("authentication failed: {reason}")]
    Unauthorized { reason: String },

    #[error("{endpoint} returned HTTP {status}: {body}")]
    Http {
        endpoint: String,
        status: u16,
        body: String,
    },

    #[error("could not decode {endpoint}: {source} (body started {body:?})")]
    Decode {
        endpoint: String,
        #[source]
        source: serde_json::Error,
        body: String,
    },

    /// A call needing a bearer token was made before `login`.
    #[error("not logged in")]
    NotAuthenticated,
}

impl ClientError {
    /// True when the bot could not be reached at all, so cached data is the
    /// right thing to show. False when the bot answered, even to refuse.
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Timeout { .. } | Self::Transport { .. })
    }

    /// True when the problem is credentials rather than connectivity — the one
    /// case where re-prompting the user is the correct response.
    pub fn is_auth(&self) -> bool {
        matches!(self, Self::Unauthorized { .. } | Self::NotAuthenticated)
    }

    /// Builds the right variant for a failed reqwest call, so callers do not
    /// each have to remember that a timeout arrives as a transport error.
    pub(crate) fn from_reqwest(endpoint: &str, source: reqwest::Error) -> Self {
        if source.is_timeout() {
            Self::Timeout {
                endpoint: endpoint.to_owned(),
            }
        } else {
            Self::Transport {
                endpoint: endpoint.to_owned(),
                source,
            }
        }
    }
}

/// Trims a response body down to something reasonable to put in an error.
pub(crate) fn snippet(body: &str) -> String {
    let trimmed = body.trim();
    match trimmed.char_indices().nth(BODY_SNIPPET) {
        Some((cut, _)) => format!("{}…", &trimmed[..cut]),
        None => trimmed.to_owned(),
    }
}

pub type Result<T> = std::result::Result<T, ClientError>;

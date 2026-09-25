//! The Freqtrade REST client.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;

use ft_types::freqtrade::*;

use crate::auth::{Credentials, RefreshOutcome, TokenStore};
use crate::error::{snippet, ClientError, Result};
use crate::url::normalize_base_url;

/// Timeout for ordinary calls, matching the Flutter client.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// `/ping` drives the per-bot dot on the Bots screen, where every bot is pinged
/// at once and a slow one must not hold up the list.
pub const PING_TIMEOUT: Duration = Duration::from_secs(3);
/// Distinct from the read timeout so an unreachable host fails fast instead of
/// burning the full budget on a TCP connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Installs the rustls crypto provider once per process.
///
/// We build reqwest with `rustls-no-provider` to keep `aws-lc-rs` out of the
/// Android build (see docs/android-notes.md), which means nothing registers a
/// default provider for us.
///
/// Public because Cargo unifies features across a build: any crate sharing a
/// dependency graph with this one gets reqwest's rustls whether it asked for
/// TLS or not, and `reqwest::Client::new()` then **panics** with "No provider
/// set". That is exactly how the first Android build died — the UI's own
/// client talks only to loopback over plain HTTP and still needed this. Call
/// it once at startup on any native target.
pub fn install_crypto_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Errs only if another provider is already installed, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// A client bound to one Freqtrade bot.
///
/// Cheap to clone: the underlying connection pool and token state are shared,
/// so cloning per request is the intended usage.
#[derive(Clone, Debug)]
pub struct FreqtradeClient {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// Already normalized, ending in `/api/v1`.
    base_url: String,
    http: reqwest::Client,
    tokens: TokenStore,
    credentials: Credentials,
}

impl FreqtradeClient {
    /// Builds a client for a bot. `base_url` is normalized, so a bare
    /// `192.168.1.10:8080` is accepted.
    ///
    /// No network traffic happens here; call [`FreqtradeClient::login`].
    pub fn new(base_url: &str, username: &str, password: &str) -> Result<Self> {
        Self::with_timeout(base_url, username, password, DEFAULT_TIMEOUT)
    }

    /// As [`FreqtradeClient::new`] but with an explicit request timeout.
    ///
    /// Used by tests, which cannot wait out the ten-second default, and
    /// available to the server for a bot known to be slow.
    pub fn with_timeout(
        base_url: &str,
        username: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<Self> {
        install_crypto_provider();
        let base_url = normalize_base_url(base_url)?;
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(CONNECT_TIMEOUT.min(timeout))
            .user_agent(concat!("ft-client/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ClientError::InvalidUrl {
                url: base_url.clone(),
                reason: e.to_string(),
            })?;

        Ok(Self {
            inner: Arc::new(Inner {
                base_url,
                http,
                tokens: TokenStore::default(),
                credentials: Credentials {
                    username: username.to_owned(),
                    password: password.to_owned(),
                },
            }),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.inner.base_url
    }

    /// True once a token has been obtained.
    pub async fn is_authenticated(&self) -> bool {
        self.inner.tokens.current().await.is_some()
    }

    // -----------------------------------------------------------------------
    // Auth
    // -----------------------------------------------------------------------

    /// `POST /token/login` using HTTP Basic, storing both tokens.
    pub async fn login(&self) -> Result<()> {
        let token = self.request_token().await?;
        self.inner
            .tokens
            .install(token.access_token, token.refresh_token)
            .await;
        Ok(())
    }

    async fn request_token(&self) -> Result<TokenResponse> {
        let endpoint = "/token/login";
        let basic = base64::engine::general_purpose::STANDARD.encode(format!(
            "{}:{}",
            self.inner.credentials.username, self.inner.credentials.password
        ));

        let response = self
            .inner
            .http
            .post(self.url(endpoint))
            .header(reqwest::header::AUTHORIZATION, format!("Basic {basic}"))
            .send()
            .await
            .map_err(|e| ClientError::from_reqwest(endpoint, e))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ClientError::from_reqwest(endpoint, e))?;

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(ClientError::Unauthorized {
                reason: format!("the bot rejected these credentials ({})", snippet(&body)),
            });
        }
        if !status.is_success() {
            return Err(ClientError::Http {
                endpoint: endpoint.to_owned(),
                status: status.as_u16(),
                body: snippet(&body),
            });
        }
        decode(endpoint, &body)
    }

    /// Exchanges the refresh token for a new access token.
    ///
    /// Note the unusual scheme: Freqtrade wants the *refresh* token in the
    /// bearer header here, not the access token.
    async fn refresh_access_token(&self) -> Result<String> {
        let endpoint = "/token/refresh";
        let Some(refresh) = self.inner.tokens.refresh_token().await else {
            return Err(ClientError::Unauthorized {
                reason: "no refresh token was issued at login".to_owned(),
            });
        };

        let response = self
            .inner
            .http
            .post(self.url(endpoint))
            .bearer_auth(&refresh)
            .send()
            .await
            .map_err(|e| ClientError::from_reqwest(endpoint, e))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ClientError::from_reqwest(endpoint, e))?;

        if !status.is_success() {
            return Err(ClientError::Unauthorized {
                reason: format!("refresh rejected with HTTP {status}"),
            });
        }
        let token: TokenResponse = decode(endpoint, &body)?;
        self.inner
            .tokens
            .install(token.access_token.clone(), token.refresh_token)
            .await;
        Ok(token.access_token)
    }

    /// Recovers from a 401: refresh if possible, otherwise log in again.
    ///
    /// Returns the access token to retry with. Only one task per expiry does
    /// the work; the rest pick up the result.
    async fn recover_auth(&self, seen_generation: u64) -> Result<String> {
        let _gate = match self.inner.tokens.begin_refresh(seen_generation).await {
            RefreshOutcome::AlreadyRefreshed(token) => return Ok(token),
            RefreshOutcome::Ours(gate) => gate,
        };

        match self.refresh_access_token().await {
            Ok(token) => Ok(token),
            Err(e) if e.is_offline() => Err(e),
            Err(_) => {
                // The refresh token has expired too (a phone asleep for days).
                // Fall back to a full login rather than stranding the user on
                // an error screen, which is what the old app did.
                tracing::debug!("refresh failed, falling back to a full login");
                self.inner.tokens.clear().await;
                let token = self.request_token().await?;
                self.inner
                    .tokens
                    .install(token.access_token.clone(), token.refresh_token)
                    .await;
                Ok(token.access_token)
            }
        }
    }

    /// Forces a token refresh now.
    ///
    /// Exposed for the diagnostic probe and for a future server-side keepalive;
    /// ordinary calls recover on their own via the 401 path.
    pub async fn refresh_now(&self) -> Result<()> {
        self.refresh_access_token().await.map(|_| ())
    }

    // -----------------------------------------------------------------------
    // Request plumbing
    // -----------------------------------------------------------------------

    fn url(&self, endpoint: &str) -> String {
        format!("{}{endpoint}", self.inner.base_url)
    }

    /// Sends an authenticated request, retrying once through [`Self::recover_auth`]
    /// if the bot answers 401.
    async fn send<T, B>(
        &self,
        method: Method,
        endpoint: &str,
        query: &[(&str, String)],
        body: Option<&B>,
    ) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let (mut token, generation) = match self.inner.tokens.current().await {
            Some(pair) => pair,
            None => return Err(ClientError::NotAuthenticated),
        };

        for attempt in 0..2 {
            let mut request = self
                .inner
                .http
                .request(method.clone(), self.url(endpoint))
                .bearer_auth(&token);
            if !query.is_empty() {
                request = request.query(query);
            }
            if let Some(body) = body {
                request = request.json(body);
            }

            let response = request
                .send()
                .await
                .map_err(|e| ClientError::from_reqwest(endpoint, e))?;
            let status = response.status();

            if status == StatusCode::UNAUTHORIZED && attempt == 0 {
                tracing::debug!(endpoint, "token rejected, recovering");
                token = self.recover_auth(generation).await?;
                continue;
            }

            let text = response
                .text()
                .await
                .map_err(|e| ClientError::from_reqwest(endpoint, e))?;

            if status == StatusCode::UNAUTHORIZED {
                return Err(ClientError::Unauthorized {
                    reason: format!("still rejected after refreshing ({})", snippet(&text)),
                });
            }
            if !status.is_success() {
                return Err(ClientError::Http {
                    endpoint: endpoint.to_owned(),
                    status: status.as_u16(),
                    body: snippet(&text),
                });
            }
            return decode(endpoint, &text);
        }
        unreachable!("the retry loop always returns or continues exactly once")
    }

    async fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        self.send::<T, ()>(Method::GET, endpoint, &[], None).await
    }

    async fn get_with<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        self.send::<T, ()>(Method::GET, endpoint, query, None).await
    }

    // -----------------------------------------------------------------------
    // Endpoints
    // -----------------------------------------------------------------------

    /// `GET /ping`, unauthenticated and on a short timeout.
    ///
    /// Associated rather than a method so the Bots screen can check a bot it has
    /// no credentials loaded for.
    pub async fn ping(base_url: &str) -> Result<bool> {
        install_crypto_provider();
        let base = normalize_base_url(base_url)?;
        let http = reqwest::Client::builder()
            .timeout(PING_TIMEOUT)
            .connect_timeout(PING_TIMEOUT)
            .build()
            .map_err(|e| ClientError::InvalidUrl {
                url: base.clone(),
                reason: e.to_string(),
            })?;

        let response = http
            .get(format!("{base}/ping"))
            .send()
            .await
            .map_err(|e| ClientError::from_reqwest("/ping", e))?;
        if !response.status().is_success() {
            return Ok(false);
        }
        let body = response
            .text()
            .await
            .map_err(|e| ClientError::from_reqwest("/ping", e))?;
        Ok(decode::<Ping>("/ping", &body)?.is_pong())
    }

    /// Same as [`FreqtradeClient::ping`] but for this client's own bot.
    pub async fn ping_self(&self) -> Result<bool> {
        Self::ping(&self.inner.base_url).await
    }

    pub async fn show_config(&self) -> Result<BotConfig> {
        self.get("/show_config").await
    }

    /// `GET /status` — open trades. Returns a bare JSON array.
    pub async fn open_trades(&self) -> Result<Vec<Trade>> {
        self.get("/status").await
    }

    /// `GET /trades` — closed trades, newest last.
    pub async fn closed_trades(&self, limit: u32, offset: u32) -> Result<TradesResponse> {
        self.get_with(
            "/trades",
            &[("limit", limit.to_string()), ("offset", offset.to_string())],
        )
        .await
    }

    pub async fn profit(&self) -> Result<ProfitSummary> {
        self.get("/profit").await
    }

    pub async fn balance(&self) -> Result<Balance> {
        self.get("/balance").await
    }

    pub async fn logs(&self, limit: u32) -> Result<LogsResponse> {
        self.get_with("/logs", &[("limit", limit.to_string())])
            .await
    }

    pub async fn whitelist(&self) -> Result<WhitelistResponse> {
        self.get("/whitelist").await
    }

    /// `GET /pair_candles`.
    ///
    /// The pair goes through reqwest's query encoder rather than string
    /// interpolation, which matters for futures pairs such as
    /// `ETH/USDT:USDT` — both the slash and the colon need escaping.
    pub async fn pair_candles(
        &self,
        pair: &str,
        timeframe: &str,
        limit: u32,
    ) -> Result<PairCandles> {
        self.get_with(
            "/pair_candles",
            &[
                ("pair", pair.to_owned()),
                ("timeframe", timeframe.to_owned()),
                ("limit", limit.to_string()),
            ],
        )
        .await
    }

    /// `POST /forceexit` — places a limit order at the current bid.
    ///
    /// Real money on a live bot; the UI confirms before calling this.
    pub async fn force_exit(&self, trade_id: i64) -> Result<StatusMessage> {
        let body = ForceExitRequest::limit(trade_id);
        self.send(Method::POST, "/forceexit", &[], Some(&body))
            .await
    }
}

/// Decodes a response body, keeping a snippet of it in the error.
///
/// `reqwest`'s own `.json()` discards the body on failure, which makes a
/// schema surprise very hard to diagnose — exactly the class of bug the float
/// `max_open_trades` turned out to be.
fn decode<T: DeserializeOwned>(endpoint: &str, body: &str) -> Result<T> {
    serde_json::from_str(body).map_err(|source| ClientError::Decode {
        endpoint: endpoint.to_owned(),
        source,
        body: snippet(body),
    })
}

//! Client for our own server.
//!
//! Every screen is one call. There is no Freqtrade-shaped logic here at all —
//! aggregation, caching, auth and the offline decision already happened
//! server-side, so this layer only moves typed structs.
//!
//! `reqwest` builds for `wasm32-unknown-unknown` on top of `fetch`, so the same
//! code serves the browser dev loop and the Android build.

use std::sync::OnceLock;

use serde::de::DeserializeOwned;
use serde::Serialize;

use ft_types::api::{
    ActionResult, AddBotRequest, ApiErrorBody, BotSummary, Candles, ChartPair, ClosedTrades,
    Dashboard, Envelope, ErrorKind, Logs, Overview, PingResult, ReorderRequest,
};
use ft_types::freqtrade::BotConfig;

/// Fallback for non-browser builds with nothing else configured: the
/// standalone dev server.
const DEV_BASE: &str = "http://127.0.0.1:3000/api";

static BASE: OnceLock<String> = OnceLock::new();

/// Points the client at a specific server. Called once, before anything
/// renders; ignored afterwards.
///
/// The Android build uses this, because its embedded server binds an
/// OS-chosen port that is not known until startup.
pub fn set_base_url(url: impl Into<String>) {
    let _ = BASE.set(url.into());
}

/// Where the API lives.
///
/// In a browser this is the page's own origin, which holds for both
/// deployments: `dx serve` proxies `/api` to the dev server, and the shipped
/// build is served by `ft-server` itself. Same-origin means no CORS in either
/// case. `reqwest` needs an absolute URL even on wasm, so the origin is read
/// rather than left relative.
pub fn base_url() -> &'static str {
    BASE.get_or_init(|| {
        if let Some(from_env) = option_env!("FT_API_BASE") {
            return from_env.trim_end_matches('/').to_owned();
        }
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(origin) = web_sys::window().and_then(|w| w.location().origin().ok()) {
                return format!("{}/api", origin.trim_end_matches('/'));
            }
        }
        DEV_BASE.to_owned()
    })
}

fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// A failure, carrying enough to react rather than just apologise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    /// The bot is unreachable. Offer retry, not a password prompt.
    pub fn is_offline(&self) -> bool {
        self.kind == ErrorKind::Offline
    }

    pub fn is_auth(&self) -> bool {
        self.kind == ErrorKind::Auth
    }

    /// Reaching *our own* server failed, which on web means the dev server is
    /// not running. Worth saying plainly rather than blaming the bot.
    fn transport(e: reqwest::Error) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: format!("could not reach the app server at {}: {e}", base_url()),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

async fn send<T: DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T> {
    let response = request.send().await.map_err(Error::transport)?;
    let status = response.status();
    let body = response.text().await.map_err(Error::transport)?;

    if !status.is_success() {
        // The server always sends a typed error body; fall back only if
        // something else answered.
        return Err(serde_json::from_str::<ApiErrorBody>(&body)
            .map(|e| Error {
                kind: e.kind,
                message: e.message,
            })
            .unwrap_or_else(|_| Error {
                kind: ErrorKind::Internal,
                message: format!("HTTP {status}: {body}"),
            }));
    }

    serde_json::from_str(&body).map_err(|e| Error {
        kind: ErrorKind::Internal,
        message: format!("unexpected response: {e}"),
    })
}

fn url(path: &str) -> String {
    format!("{}{path}", base_url())
}

async fn get<T: DeserializeOwned>(path: &str) -> Result<T> {
    send(client().get(url(path))).await
}

async fn post<T: DeserializeOwned, B: Serialize>(path: &str, body: &B) -> Result<T> {
    send(client().post(url(path)).json(body)).await
}

// ---------------------------------------------------------------------------
// Bots
// ---------------------------------------------------------------------------

pub async fn list_bots() -> Result<Vec<BotSummary>> {
    get("/bots").await
}

/// Adds a bot. The server validates the credentials by really logging in, so a
/// failure here distinguishes a wrong password from an unreachable host.
pub async fn add_bot(request: AddBotRequest) -> Result<BotSummary> {
    post("/bots", &request).await
}

pub async fn delete_bot(id: &str) -> Result<ActionResult> {
    send(client().delete(url(&format!("/bots/{id}")))).await
}

pub async fn reorder_bots(ids: Vec<String>) -> Result<Vec<BotSummary>> {
    send(
        client()
            .put(url("/bots/order"))
            .json(&ReorderRequest { ids }),
    )
    .await
}

/// Liveness for the per-bot dot. Never errors for a bot that is merely down.
pub async fn ping(id: &str) -> Result<PingResult> {
    get(&format!("/bots/{id}/ping")).await
}

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

/// Appends `?refresh=true` for a pull-to-refresh, which bypasses the cache TTL
/// but keeps the cache as a fallback if the bot has gone away.
fn screen_url(id: &str, screen: &str, refresh: bool) -> String {
    if refresh {
        format!("/bots/{id}/{screen}?refresh=true")
    } else {
        format!("/bots/{id}/{screen}")
    }
}

pub async fn config(id: &str, refresh: bool) -> Result<Envelope<BotConfig>> {
    get(&screen_url(id, "config", refresh)).await
}

pub async fn overview(id: &str, refresh: bool) -> Result<Envelope<Overview>> {
    get(&screen_url(id, "overview", refresh)).await
}

pub async fn closed(id: &str, refresh: bool) -> Result<Envelope<ClosedTrades>> {
    get(&screen_url(id, "closed", refresh)).await
}

pub async fn dashboard(id: &str, refresh: bool) -> Result<Envelope<Dashboard>> {
    get(&screen_url(id, "dashboard", refresh)).await
}

pub async fn logs(id: &str, refresh: bool) -> Result<Envelope<Logs>> {
    get(&screen_url(id, "logs", refresh)).await
}

pub async fn pairs(id: &str, refresh: bool) -> Result<Envelope<Vec<ChartPair>>> {
    get(&screen_url(id, "pairs", refresh)).await
}

pub async fn candles(id: &str, pair: &str, refresh: bool) -> Result<Envelope<Candles>> {
    let encoded = urlencode(pair);
    let suffix = if refresh { "&refresh=true" } else { "" };
    get(&format!("/bots/{id}/candles?pair={encoded}{suffix}")).await
}

pub async fn force_exit(id: &str, trade_id: i64) -> Result<ActionResult> {
    post(
        &format!("/bots/{id}/forceexit"),
        &ft_types::api::ForceExitBody { trade_id },
    )
    .await
}

/// Percent-encodes a query value.
///
/// Hand-rolled because the pairs that matter are futures-style
/// (`ETH/USDT:USDT`) and both the slash and the colon must be escaped.
fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct SettingValue {
    value: Option<String>,
}

pub async fn setting(key: &str) -> Result<Option<String>> {
    let value: SettingValue = get(&format!("/settings/{key}")).await?;
    Ok(value.value.filter(|v| !v.is_empty()))
}

pub async fn set_setting(key: &str, value: Option<String>) -> Result<()> {
    let _: SettingValue = send(
        client()
            .put(url(&format!("/settings/{key}")))
            .json(&SettingValue { value }),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn futures_pairs_are_escaped() {
        // Both characters must go; a raw colon or slash would be read as part
        // of the URL structure.
        assert_eq!(urlencode("ETH/USDT:USDT"), "ETH%2FUSDT%3AUSDT");
        assert_eq!(urlencode("BTC/USDT"), "BTC%2FUSDT");
        assert_eq!(urlencode("plain-name_1.0~x"), "plain-name_1.0~x");
    }

    #[test]
    fn refresh_is_only_added_when_asked() {
        assert_eq!(screen_url("abc", "overview", false), "/bots/abc/overview");
        assert_eq!(
            screen_url("abc", "overview", true),
            "/bots/abc/overview?refresh=true"
        );
    }
}

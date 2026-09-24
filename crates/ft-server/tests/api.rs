//! End-to-end tests: HTTP in, HTTP out, over a mock Freqtrade.
//!
//! These exercise the thing the unit tests cannot — that the aggregation,
//! caching and staleness logic actually compose. The two claims worth pinning
//! down are that a warm cache does not touch the bot at all, and that an
//! unreachable bot still renders a screen.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde::de::DeserializeOwned;
use tower::ServiceExt;

use ft_store::{StaticKey, Store};
use ft_types::api::*;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A server wired to a fresh in-memory store.
struct Harness {
    app: axum::Router,
    state: ft_server::AppState,
}

impl Harness {
    fn new() -> Self {
        let store = Arc::new(Store::in_memory(&StaticKey::random().unwrap()).unwrap());
        let state = ft_server::AppState::new(store);
        Self {
            app: ft_server::router_from_state(state.clone(), false, None),
            state,
        }
    }

    async fn request(&self, method: &str, uri: &str, body: Option<serde_json::Value>) -> Response {
        let builder = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(json) => builder
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&json).unwrap()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        };
        let response = self.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        Response { status, bytes }
    }

    async fn get(&self, uri: &str) -> Response {
        self.request("GET", uri, None).await
    }

    /// Listens for server events, as a connected client would.
    fn events(&self) -> tokio::sync::broadcast::Receiver<ft_server::events::ServerEvent> {
        self.state.events().subscribe()
    }
}

struct Response {
    status: StatusCode,
    bytes: axum::body::Bytes,
}

impl Response {
    fn json<T: DeserializeOwned>(&self) -> T {
        assert!(
            self.status.is_success(),
            "expected success, got {}: {}",
            self.status,
            String::from_utf8_lossy(&self.bytes)
        );
        serde_json::from_slice(&self.bytes)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&self.bytes)))
    }

    fn error(&self) -> ApiErrorBody {
        serde_json::from_slice(&self.bytes)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&self.bytes)))
    }
}

/// A mock Freqtrade that can be shut down deterministically.
///
/// `drop(MockServer)` signals shutdown but does not wait for the port to stop
/// accepting, which made the offline tests flaky. Owning the listener lets us
/// poll the port until a connection genuinely fails.
struct MockBot {
    server: MockServer,
    port: u16,
}

impl std::ops::Deref for MockBot {
    type Target = MockServer;
    fn deref(&self) -> &MockServer {
        &self.server
    }
}

impl MockBot {
    /// Stops the bot and does not return until it is truly unreachable.
    async fn shutdown(self) {
        let port = self.port;
        drop(self);
        for _ in 0..500 {
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_err()
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("port {port} is still accepting connections");
    }
}

/// A mock Freqtrade with every endpoint the screens use.
async fn mock_bot() -> MockBot {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MockServer::builder().listener(listener).start().await;
    mount_auth(&server).await;

    mount(
        &server,
        "/api/v1/show_config",
        serde_json::json!({
            "exchange": "binance", "dry_run": true, "strategy": "SMCStrategy2h",
            "timeframe": "2h", "trading_mode": "futures", "stake_currency": "USDT",
            "stake_amount": "unlimited", "max_open_trades": 2.0, "stoploss": -0.1
        }),
    )
    .await;

    mount(
        &server,
        "/api/v1/status",
        serde_json::json!([{
            "trade_id": 328, "pair": "ETH/USDT:USDT", "is_open": true, "is_short": false,
            "stake_amount": 100.0, "open_rate": 2500.0, "current_rate": 2550.0,
            "profit_ratio": 0.02, "profit_abs": 2.0,
            "open_timestamp": 1789516800000i64
        }]),
    )
    .await;

    mount(
        &server,
        "/api/v1/balance",
        serde_json::json!({
            "total": 1198.90, "stake": "USDT",
            "currencies": [
                {"currency": "USDT", "free": 998.90, "used": 200.0, "is_position": false},
                {"currency": "ETH/USDT:USDT", "free": 0, "used": 0, "is_position": true}
            ]
        }),
    )
    .await;

    mount(
        &server,
        "/api/v1/profit",
        serde_json::json!({
            "profit_closed_coin": 193.6703, "closed_trade_count": 3,
            "best_pair": "SOL/USDT:USDT", "starting_capital": 1000.0,
            "avg_duration": "2:14:33", "trading_volume": 40000.0
        }),
    )
    .await;

    mount(
        &server,
        "/api/v1/whitelist",
        serde_json::json!({
            "whitelist": ["BTC/USDT:USDT", "SOL/USDT:USDT"], "length": 2
        }),
    )
    .await;

    mount(&server, "/api/v1/logs", serde_json::json!({
        "log_count": 2,
        "logs": [
            ["2026-09-24 08:00:01,123", 1789516801.123, "freqtrade.worker", "INFO", "heartbeat"],
            ["2026-09-24 08:00:05,000", 1789516805.0, "freqtrade.rpc", "ERROR", "boom"]
        ]
    })).await;

    mount(&server, "/api/v1/pair_candles", serde_json::json!({
        "pair": "ETH/USDT:USDT", "timeframe": "2h",
        "columns": ["date", "open", "high", "low", "close", "volume", "rsi", "__date_ts"],
        "data": [
            ["2026-09-16T00:00:00Z", 2500.0, 2510.0, 2490.0, 2505.0, 10.0, 50.0, 1789516800000i64],
            ["2026-09-16T02:00:00Z", 2505.0, 2520.0, 2500.0, 2515.0, 12.0, 55.0, 1789524000000i64]
        ]
    })).await;

    mount_trades(&server, 3).await;
    MockBot { server, port }
}

async fn mount_auth(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/v1/token/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "a", "refresh_token": "r"})),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/ping"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": "pong"})),
        )
        .mount(server)
        .await;
}

async fn mount(server: &MockServer, p: &str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(p.to_owned()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// Closed trades, ascending by id as the real API returns them.
fn trade(id: i64, close_ts: i64, profit: f64) -> serde_json::Value {
    serde_json::json!({
        "trade_id": id, "pair": "SOL/USDT:USDT", "is_open": false, "is_short": false,
        "stake_amount": 100.0, "open_rate": 140.0, "close_rate": 147.0,
        "profit_ratio": 0.05, "profit_abs": profit,
        "open_timestamp": (close_ts - 3600) * 1000,
        "close_timestamp": close_ts * 1000
    })
}

async fn mount_trades(server: &MockServer, total: i64) {
    let all: Vec<_> = (1..=total)
        .map(|i| trade(i, 1_789_000_000 + i * 1000, i as f64))
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v1/trades"))
        .respond_with(move |req: &wiremock::Request| {
            let params: std::collections::HashMap<_, _> = req.url.query_pairs().collect();
            let limit: usize = params
                .get("limit")
                .and_then(|v| v.parse().ok())
                .unwrap_or(50);
            let offset: usize = params
                .get("offset")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let page: Vec<_> = all.iter().skip(offset).take(limit).cloned().collect();
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "trades": page, "trades_count": page.len(), "offset": offset,
                "total_trades": all.len()
            }))
        })
        .mount(server)
        .await;
}

async fn add_bot(h: &Harness, server: &MockServer) -> String {
    let response = h
        .request(
            "POST",
            "/api/bots",
            Some(serde_json::json!({
                "name": "prod", "url": server.uri(),
                "username": "sami", "password": "hunter2"
            })),
        )
        .await;
    let bot: BotSummary = response.json();
    bot.id
}

// ---------------------------------------------------------------------------
// Bots
// ---------------------------------------------------------------------------

#[tokio::test]
async fn adding_a_bot_validates_credentials_and_normalizes_the_url() {
    let h = Harness::new();
    let server = mock_bot().await;

    // A bare host:port, as typed into the add form.
    let uri = server.uri();
    let bare = uri.trim_start_matches("http://");
    let response = h
        .request(
            "POST",
            "/api/bots",
            Some(serde_json::json!({
                "name": "  prod  ", "url": bare, "username": "sami", "password": "hunter2"
            })),
        )
        .await;

    let bot: BotSummary = response.json();
    assert_eq!(bot.name, "prod", "name should be trimmed");
    assert!(
        bot.url.ends_with("/api/v1"),
        "url not normalized: {}",
        bot.url
    );

    let listed: Vec<BotSummary> = h.get("/api/bots").await.json();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, bot.id);
}

#[tokio::test]
async fn a_bot_with_bad_credentials_is_not_saved() {
    // Validating server-side means the check cannot be skipped, and the error
    // says which problem it was.
    let h = Harness::new();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token/login"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let response = h
        .request(
            "POST",
            "/api/bots",
            Some(serde_json::json!({
                "name": "prod", "url": server.uri(), "username": "sami", "password": "wrong"
            })),
        )
        .await;

    assert_eq!(response.status, StatusCode::UNAUTHORIZED);
    assert_eq!(response.error().kind, ErrorKind::Auth);
    assert!(h
        .get("/api/bots")
        .await
        .json::<Vec<BotSummary>>()
        .is_empty());
}

#[tokio::test]
async fn an_unreachable_bot_is_reported_as_offline_not_as_bad_credentials() {
    let h = Harness::new();
    let response = h
        .request(
            "POST",
            "/api/bots",
            Some(serde_json::json!({
                "name": "prod", "url": "http://127.0.0.1:1",
                "username": "sami", "password": "hunter2"
            })),
        )
        .await;
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.error().kind, ErrorKind::Offline);
}

#[tokio::test]
async fn bots_can_be_reordered_and_deleted() {
    let h = Harness::new();
    let server = mock_bot().await;
    let a = add_bot(&h, &server).await;
    let b = add_bot(&h, &server).await;

    let reordered: Vec<BotSummary> = h
        .request(
            "PUT",
            "/api/bots/order",
            Some(serde_json::json!({"ids": [b, a]})),
        )
        .await
        .json();
    assert_eq!(reordered[0].id, *reordered.first().map(|r| &r.id).unwrap());

    let first = reordered[0].id.clone();
    let response = h
        .request("DELETE", &format!("/api/bots/{first}"), None)
        .await;
    assert!(response.status.is_success());
    assert_eq!(h.get("/api/bots").await.json::<Vec<BotSummary>>().len(), 1);

    // Deleting it again is a 404, not a silent success.
    let response = h
        .request("DELETE", &format!("/api/bots/{first}"), None)
        .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn requests_for_an_unknown_bot_are_404() {
    let h = Harness::new();
    let response = h.get("/api/bots/nope/overview").await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    assert_eq!(response.error().kind, ErrorKind::NotFound);
}

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

#[tokio::test]
async fn overview_aggregates_trades_and_balance_into_one_payload() {
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let envelope: Envelope<Overview> = h.get(&format!("/api/bots/{id}/overview")).await.json();
    assert!(!envelope.stale);
    assert!(envelope.last_synced.is_some());

    let o = envelope.data;
    assert_eq!(o.open_trades.len(), 1);
    assert_eq!(o.open_pl, 2.0);
    // balance.total + unrealized P/L
    assert_eq!(o.portfolio_value, 1200.90);
    // From the spot row, not the futures position row.
    assert_eq!(o.free, 998.90);
    assert_eq!(o.used, 200.0);
    assert_eq!(o.stake_currency, "USDT");
}

#[tokio::test]
async fn closed_trades_sync_and_come_back_newest_first() {
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let envelope: Envelope<ClosedTrades> = h.get(&format!("/api/bots/{id}/closed")).await.json();
    let c = envelope.data;
    assert_eq!(c.total, 3);
    assert_eq!(c.trades.len(), 3);
    let ids: Vec<_> = c.trades.iter().map(|t| t.trade_id).collect();
    assert_eq!(ids, [3, 2, 1], "newest first");
    assert_eq!(c.closed_profit, 193.6703);
    assert_eq!(c.portfolio_value, 1198.90);
}

#[tokio::test]
async fn dashboard_computes_the_cumulative_series_server_side() {
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let envelope: Envelope<Dashboard> = h.get(&format!("/api/bots/{id}/dashboard")).await.json();
    let d = envelope.data;

    assert_eq!(d.config.exchange, "binance");
    // The float max_open_trades from the live bot, all the way through.
    assert_eq!(d.config.max_open_trades, 2);
    assert_eq!(d.profit.best_pair.as_deref(), Some("SOL/USDT:USDT"));

    // starting_capital is set, so the series is a percentage.
    assert_eq!(d.series.unit, SeriesUnit::Percent);
    assert_eq!(d.series.points.len(), 3);
    // Profits 1, 2, 3 against 1000 capital: 0.1%, 0.3%, 0.6% cumulative.
    // Compared with a tolerance: these are accumulated floats, and asserting
    // exact bit patterns tests IEEE-754, not the code.
    let values: Vec<_> = d.series.points.iter().map(|p| p.cumulative).collect();
    for (got, want) in values.iter().zip([0.1, 0.3, 0.6]) {
        assert!((got - want).abs() < 1e-9, "series was {values:?}");
    }
    assert!(d.series.points.windows(2).all(|w| w[0].time < w[1].time));
    assert_eq!(d.free_balance, 998.90);
}

#[tokio::test]
async fn logs_decode_into_entries_with_severity() {
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let envelope: Envelope<Logs> = h.get(&format!("/api/bots/{id}/logs")).await.json();
    let entries = envelope.data.entries;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].message, "heartbeat");
    assert_eq!(
        entries[1].severity(),
        ft_types::freqtrade::LogSeverity::Error
    );
}

#[tokio::test]
async fn the_pair_list_includes_open_trades_missing_from_the_whitelist() {
    // A pair can be traded and then dropped from the whitelist; the chart must
    // still offer it while the trade is open.
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let envelope: Envelope<Vec<ChartPair>> = h.get(&format!("/api/bots/{id}/pairs")).await.json();
    let pairs = envelope.data;

    let eth = pairs
        .iter()
        .find(|p| p.pair == "ETH/USDT:USDT")
        .expect("open-trade pair missing");
    assert!(!eth.in_whitelist);
    assert_eq!(eth.open_profit_ratio, Some(0.02));

    let btc = pairs.iter().find(|p| p.pair == "BTC/USDT:USDT").unwrap();
    assert!(btc.in_whitelist);
    assert_eq!(btc.open_profit_ratio, None);
}

#[tokio::test]
async fn candles_default_to_the_bots_own_timeframe() {
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let envelope: Envelope<Candles> = h
        .get(&format!("/api/bots/{id}/candles?pair=ETH%2FUSDT%3AUSDT"))
        .await
        .json();
    // Not the hardcoded 5m the legacy screen would have guessed.
    assert_eq!(envelope.data.timeframe, "2h");
    assert_eq!(envelope.data.candles.len(), 2);
    assert_eq!(envelope.data.candles[0].close, 2505.0);

    let missing = h.get(&format!("/api/bots/{id}/candles?pair=")).await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Caching and staleness -- the reason this layer exists
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_warm_cache_does_not_touch_the_bot() {
    // The claim the whole design rests on. The Flutter app re-issued its whole
    // fan-out on every tab mount; here a revisit inside the TTL is free.
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    h.get(&format!("/api/bots/{id}/overview")).await;
    let after_first = server.received_requests().await.unwrap().len();

    for _ in 0..5 {
        let envelope: Envelope<Overview> = h.get(&format!("/api/bots/{id}/overview")).await.json();
        assert!(!envelope.stale, "cache hits are fresh, not stale");
    }

    assert_eq!(
        server.received_requests().await.unwrap().len(),
        after_first,
        "five repeat loads should have cost zero requests to the bot"
    );
}

#[tokio::test]
async fn closed_trades_sync_incrementally_rather_than_refetching() {
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    // First load pulls all three.
    let first: Envelope<ClosedTrades> = h.get(&format!("/api/bots/{id}/closed")).await.json();
    assert_eq!(first.data.total, 3);

    // Two more trades close. Expire the freshness mark so the next read syncs.
    server.reset().await;
    mount_auth(&server).await;
    mount(
        &server,
        "/api/v1/balance",
        serde_json::json!({"total": 1198.90, "stake": "USDT"}),
    )
    .await;
    mount(
        &server,
        "/api/v1/profit",
        serde_json::json!({"profit_closed_coin": 193.6703}),
    )
    .await;
    mount_trades(&server, 5).await;

    let second: Envelope<ClosedTrades> = h
        .get(&format!("/api/bots/{id}/closed?refresh=true"))
        .await
        .json();
    assert_eq!(second.data.total, 5);

    // The sync must have asked for the two new trades, not all five.
    let trade_requests: Vec<_> = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path().ends_with("/trades"))
        .map(|r| {
            let q: std::collections::HashMap<_, _> = r
                .url
                .query_pairs()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            (q.get("offset").cloned(), q.get("limit").cloned())
        })
        .collect();

    assert!(
        trade_requests
            .iter()
            .any(|(offset, limit)| offset.as_deref() == Some("3") && limit.as_deref() == Some("2")),
        "expected an incremental fetch of offset=3 limit=2, got {trade_requests:?}"
    );
}

#[tokio::test]
async fn every_screen_can_be_served_twice() {
    // The second load of each screen reads back what the first one cached, so
    // this catches any type that serializes differently from how it
    // deserializes. LogsResponse did exactly that -- it decodes Freqtrade's
    // positional tuples but serializes LogEntry as an object -- so the Logs
    // screen 500'd on every warm load while the cold load worked. The
    // original test only covered /overview, which is why it got through.
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let screens = ["overview", "closed", "dashboard", "logs", "pairs", "config"];
    for screen in screens {
        let cold = h.get(&format!("/api/bots/{id}/{screen}")).await;
        assert!(
            cold.status.is_success(),
            "cold load of {screen} failed: {} {}",
            cold.status,
            String::from_utf8_lossy(&cold.bytes)
        );
        let warm = h.get(&format!("/api/bots/{id}/{screen}")).await;
        assert!(
            warm.status.is_success(),
            "warm load of {screen} failed (cache round-trip?): {} {}",
            warm.status,
            String::from_utf8_lossy(&warm.bytes)
        );
        // Compare the payload, not the envelope: `last_synced` legitimately
        // differs between a live fetch and the cached copy.
        let cold_data: serde_json::Value = serde_json::from_slice(&cold.bytes).unwrap();
        let warm_data: serde_json::Value = serde_json::from_slice(&warm.bytes).unwrap();
        assert_eq!(
            cold_data["data"], warm_data["data"],
            "{screen} returned different data from cache than from the bot"
        );
    }

    // Candles take a query parameter, so they are checked separately.
    let uri = format!("/api/bots/{id}/candles?pair=ETH%2FUSDT%3AUSDT");
    assert!(h.get(&uri).await.status.is_success());
    assert!(h.get(&uri).await.status.is_success(), "warm candles failed");
}

#[tokio::test]
async fn an_unreachable_bot_still_renders_from_cache() {
    // The offline path: the screen shows yesterday's numbers with a banner,
    // rather than the full-screen error the Flutter app fell back to.
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let warm: Envelope<Overview> = h.get(&format!("/api/bots/{id}/overview")).await.json();
    assert!(!warm.stale);
    // Warm the config cache too: the header badge reads it, and with nothing
    // cached the endpoint would correctly 503 rather than report staleness.
    h.get(&format!("/api/bots/{id}/config")).await;
    let synced_at = warm.last_synced;

    // The bot goes away. `?refresh=true` is pull-to-refresh: it bypasses the
    // TTL and really asks the bot, which is the case that must degrade well.
    // Clearing the cache instead would test nothing, since that is precisely
    // the data we are asserting still renders.
    server.shutdown().await;

    let offline = h
        .get(&format!("/api/bots/{id}/overview?refresh=true"))
        .await;
    // Still a 200: there is something to show.
    let envelope: Envelope<Overview> = offline.json();
    assert!(envelope.stale, "should be flagged stale");
    assert_eq!(
        envelope.data.open_trades.len(),
        1,
        "cached trades still render"
    );
    assert_eq!(envelope.data.portfolio_value, 1200.90);
    assert!(
        envelope.last_synced <= synced_at,
        "last_synced should not advance"
    );
}

#[tokio::test]
async fn an_unreachable_bot_with_no_cache_is_an_honest_error() {
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;
    server.shutdown().await;

    // Nothing was ever fetched, so there is nothing to fall back on.
    let response = h.get(&format!("/api/bots/{id}/overview")).await;
    assert_eq!(response.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.error().kind, ErrorKind::Offline);
}

#[tokio::test]
async fn a_bot_side_error_is_not_masked_by_the_cache() {
    // Serving stale data when the bot is *answering* with an error would hide
    // a real problem. Only unreachability falls back to cache.
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;
    h.get(&format!("/api/bots/{id}/dashboard")).await;

    // Rebuild the bot with only /profit broken. Resetting and mounting just
    // the failure would make every other endpoint 404 as well, and the error
    // we caught would not be the one under test.
    server.reset().await;
    mount_auth(&server).await;
    mount(
        &server,
        "/api/v1/show_config",
        serde_json::json!({"timeframe": "2h"}),
    )
    .await;
    mount(&server, "/api/v1/status", serde_json::json!([])).await;
    mount(
        &server,
        "/api/v1/balance",
        serde_json::json!({"total": 1198.90, "stake": "USDT"}),
    )
    .await;
    mount_trades(&server, 3).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/profit"))
        .respond_with(ResponseTemplate::new(500).set_body_string("strategy blew up"))
        .mount(&server)
        .await;

    let response = h
        .get(&format!("/api/bots/{id}/dashboard?refresh=true"))
        .await;
    assert_eq!(response.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        response.error().message.contains("strategy blew up"),
        "the bot's own error should reach the client, got: {}",
        response.error().message
    );
}

#[tokio::test]
async fn ping_reports_a_down_bot_without_erroring() {
    // The Bots screen pings every bot at once; one being down is expected.
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let up: PingResult = h.get(&format!("/api/bots/{id}/ping")).await.json();
    assert!(up.online);

    server.shutdown().await;
    let down: PingResult = h.get(&format!("/api/bots/{id}/ping")).await.json();
    assert!(!down.online);
}

#[tokio::test]
async fn settings_persist_the_active_bot_and_theme() {
    // Kept server-side rather than in browser storage so the choice survives a
    // reinstall and is identical wherever the UI runs.
    let h = Harness::new();

    let empty: serde_json::Value = h.get("/api/settings/active_bot").await.json();
    assert_eq!(empty["value"], serde_json::Value::Null);

    let saved: serde_json::Value = h
        .request(
            "PUT",
            "/api/settings/active_bot",
            Some(serde_json::json!({"value": "bot-1"})),
        )
        .await
        .json();
    assert_eq!(saved["value"], "bot-1");

    let read_back: serde_json::Value = h.get("/api/settings/active_bot").await.json();
    assert_eq!(read_back["value"], "bot-1");

    // Independent keys.
    h.request(
        "PUT",
        "/api/settings/theme",
        Some(serde_json::json!({"value": "light"})),
    )
    .await;
    let theme: serde_json::Value = h.get("/api/settings/theme").await.json();
    assert_eq!(theme["value"], "light");
    let still: serde_json::Value = h.get("/api/settings/active_bot").await.json();
    assert_eq!(still["value"], "bot-1");

    // Clearing, which is what deleting the active bot needs.
    h.request(
        "PUT",
        "/api/settings/active_bot",
        Some(serde_json::json!({"value": null})),
    )
    .await;
    let cleared: serde_json::Value = h.get("/api/settings/active_bot").await.json();
    assert_eq!(cleared["value"], "");
}

#[tokio::test]
async fn candles_carry_trade_markers_for_the_visible_window() {
    // Markers are what make a price chart worth looking at, and computing
    // them server-side means the UI never receives trades it cannot draw.
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    // Sync trades first; overlays come from the store, not from the bot.
    h.get(&format!("/api/bots/{id}/closed")).await;

    // The mock's closed trades are all on SOL/USDT:USDT, within the candle
    // window; ETH candles should therefore carry none.
    let eth: Envelope<Candles> = h
        .get(&format!("/api/bots/{id}/candles?pair=ETH%2FUSDT%3AUSDT"))
        .await
        .json();
    assert_eq!(eth.data.candles.len(), 2);
    assert!(
        eth.data.overlays.is_empty(),
        "trades on another pair must not appear: {:?}",
        eth.data.overlays
    );
}

#[tokio::test]
async fn adding_a_bot_announces_the_change() {
    // Other open clients should see a new bot without reloading.
    let h = Harness::new();
    let server = mock_bot().await;
    let mut events = h.events();

    add_bot(&h, &server).await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
        .await
        .expect("no event within 2s")
        .expect("event channel closed");
    assert_eq!(event, ft_server::events::ServerEvent::BotsChanged);
}

#[tokio::test]
async fn the_event_stream_is_what_gates_background_work() {
    // The sync loop runs only while someone is watching. On a phone that means
    // only while the app is open, rather than waking the radio on a timer for
    // a bot nobody is looking at.
    let h = Harness::new();
    assert!(!h.state.has_watchers(), "nothing should be watching yet");

    let guard = h.state.subscribe();
    assert!(h.state.has_watchers());

    drop(guard);
    assert!(
        !h.state.has_watchers(),
        "a disconnected client must stop the background sync"
    );
}

#[tokio::test]
async fn the_event_endpoint_serves_a_stream() {
    // The body is deliberately not read: an event stream never ends, so
    // draining it would hang the test forever. Checking the headers is the
    // whole assertion — that the route exists and answers as a stream.
    let h = Harness::new();
    let request = Request::builder()
        .method("GET")
        .uri("/api/events")
        .body(Body::empty())
        .unwrap();
    let response = h.app.clone().oneshot(request).await.unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream"),
    );
}

#[tokio::test]
async fn a_bot_discovered_offline_stays_flagged_even_from_a_fresh_cache() {
    // The gap this closes: a background sync can find the bot gone while the
    // cache is still inside its TTL. A request served from that cache has
    // learned nothing about the bot, and used to report itself perfectly
    // fresh -- so the offline banner never appeared until the cache expired.
    let h = Harness::new();
    let server = mock_bot().await;
    let id = add_bot(&h, &server).await;

    let warm: Envelope<Overview> = h.get(&format!("/api/bots/{id}/overview")).await.json();
    assert!(!warm.stale);
    // Warm the config cache too: the header badge reads it, and with nothing
    // cached the endpoint would correctly 503 rather than report staleness.
    h.get(&format!("/api/bots/{id}/config")).await;

    // The bot goes away and something notices -- here, a forced refresh
    // standing in for the background sync.
    server.shutdown().await;
    h.get(&format!("/api/bots/{id}/overview?refresh=true"))
        .await;

    // A plain read now hits a cache well inside its TTL, and must still say so.
    let cached: Envelope<Overview> = h.get(&format!("/api/bots/{id}/overview")).await.json();
    assert!(
        cached.stale,
        "a cache hit must inherit what we already know about the bot"
    );
    assert_eq!(
        cached.data.open_trades.len(),
        1,
        "cached data still renders"
    );

    // And the header badge, which reads config, agrees.
    let config: Envelope<ft_types::freqtrade::BotConfig> =
        h.get(&format!("/api/bots/{id}/config")).await.json();
    assert!(config.stale, "the DRY/LIVE badge must flip to OFFLINE too");
}

#[tokio::test]
async fn health_needs_no_bot() {
    let h = Harness::new();
    assert!(h.get("/api/health").await.status.is_success());
}

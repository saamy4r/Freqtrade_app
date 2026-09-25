//! Per-endpoint request shape and error classification.

use std::time::Duration;

use ft_client::{ClientError, FreqtradeClient};
use wiremock::matchers::{body_json, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER: &str = "sami";
const PASS: &str = "hunter2";

async fn logged_in(server: &MockServer) -> FreqtradeClient {
    Mock::given(method("POST"))
        .and(path("/api/v1/token/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"access_token": "a", "refresh_token": "r"})),
        )
        .mount(server)
        .await;
    let client =
        FreqtradeClient::with_timeout(&server.uri(), USER, PASS, Duration::from_millis(300))
            .unwrap();
    client.login().await.unwrap();
    client
}

#[tokio::test]
async fn futures_pairs_are_url_encoded() {
    // The live bot trades ETH/USDT:USDT. Interpolating that into a query string
    // would send a raw `/` and `:`; it has to go through the query encoder.
    let server = MockServer::start().await;
    let client = logged_in(&server).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/pair_candles"))
        .and(query_param("pair", "ETH/USDT:USDT"))
        .and(query_param("timeframe", "2h"))
        .and(query_param("limit", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "pair": "ETH/USDT:USDT",
            "columns": ["date", "close", "__date_ts"],
            "data": [["2026-09-16T00:00:00Z", 2500.0, 1789516800000i64]]
        })))
        .expect(1)
        .mount(&server)
        .await;

    let candles = client
        .pair_candles("ETH/USDT:USDT", "2h", 100)
        .await
        .unwrap();
    assert_eq!(candles.pair, "ETH/USDT:USDT");
    assert_eq!(candles.candles().len(), 1);

    // Confirm the wire form was escaped, not sent raw.
    let sent = &server.received_requests().await.unwrap();
    let url = sent.last().unwrap().url.as_str();
    assert!(
        url.contains("pair=ETH%2FUSDT%3AUSDT"),
        "pair was not percent-encoded: {url}"
    );
}

#[tokio::test]
async fn closed_trades_paginate() {
    let server = MockServer::start().await;
    let client = logged_in(&server).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/trades"))
        .and(query_param("limit", "500"))
        .and(query_param("offset", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "trades": [], "trades_count": 0, "total_trades": 327
        })))
        .expect(1)
        .mount(&server)
        .await;

    let page = client.closed_trades(500, 100).await.unwrap();
    assert_eq!(page.total_trades, 327);
}

#[tokio::test]
async fn logs_request_sends_the_limit() {
    let server = MockServer::start().await;
    let client = logged_in(&server).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/logs"))
        .and(query_param("limit", "500"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "log_count": 1,
            "logs": [["2026-09-24 08:00:01,123", 1789516801.123, "freqtrade.worker", "INFO", "up"]]
        })))
        .mount(&server)
        .await;

    let logs = client.logs(500).await.unwrap();
    assert_eq!(logs.logs.len(), 1);
    assert_eq!(logs.logs[0].message, "up");
}

#[tokio::test]
async fn force_exit_posts_a_limit_order() {
    // Real money on a live bot, so the body shape is worth pinning down.
    let server = MockServer::start().await;
    let client = logged_in(&server).await;

    Mock::given(method("POST"))
        .and(path("/api/v1/forceexit"))
        .and(body_json(serde_json::json!({
            "tradeid": "42", "ordertype": "limit"
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"result": "Created exit order for trade 42."})),
        )
        .expect(1)
        .mount(&server)
        .await;

    let result = client.force_exit(42).await.unwrap();
    assert!(result.result.contains("exit order"));
}

#[tokio::test]
async fn open_trades_parse_a_bare_array() {
    // /status returns a top-level array, unlike /trades which wraps in an object.
    let server = MockServer::start().await;
    let client = logged_in(&server).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/status"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {"trade_id": 1, "pair": "ETH/USDT:USDT", "is_short": true, "current_rate": null}
        ])))
        .mount(&server)
        .await;

    let trades = client.open_trades().await.unwrap();
    assert_eq!(trades.len(), 1);
    assert!(trades[0].is_short);
    assert_eq!(trades[0].current_rate, None);
}

#[tokio::test]
async fn ping_distinguishes_alive_dead_and_unreachable() {
    let alive = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/ping"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": "pong"})),
        )
        .mount(&alive)
        .await;
    assert!(FreqtradeClient::ping(&alive.uri()).await.unwrap());

    // Something is listening but it is not Freqtrade.
    let impostor = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/ping"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": "?"})))
        .mount(&impostor)
        .await;
    assert!(!FreqtradeClient::ping(&impostor.uri()).await.unwrap());

    // Nothing listening: an error, not `false`, so the UI can tell "offline"
    // from "answered but wrong".
    let err = FreqtradeClient::ping("http://127.0.0.1:1")
        .await
        .unwrap_err();
    assert!(err.is_offline(), "expected offline, got {err}");
}

#[tokio::test]
async fn an_unreachable_bot_is_offline_not_an_auth_problem() {
    // This is the distinction the server needs to decide between serving cache
    // and asking the user to re-enter their password.
    let client =
        FreqtradeClient::with_timeout("http://127.0.0.1:1", USER, PASS, Duration::from_millis(300))
            .unwrap();
    let err = client.login().await.unwrap_err();
    assert!(err.is_offline(), "expected offline, got {err}");
    assert!(!err.is_auth());
}

#[tokio::test]
async fn a_slow_bot_times_out_as_offline() {
    let server = MockServer::start().await;
    let client = logged_in(&server).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/balance"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;

    let err = client.balance().await.unwrap_err();
    assert!(matches!(err, ClientError::Timeout { .. }), "got {err}");
    assert!(err.is_offline());
}

#[tokio::test]
async fn a_server_error_keeps_the_status_and_body() {
    let server = MockServer::start().await;
    let client = logged_in(&server).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/profit"))
        .respond_with(ResponseTemplate::new(500).set_body_string("strategy blew up"))
        .mount(&server)
        .await;

    let err = client.profit().await.unwrap_err();
    match &err {
        ClientError::Http { status, body, .. } => {
            assert_eq!(*status, 500);
            assert!(body.contains("strategy blew up"));
        }
        other => panic!("expected an HTTP error, got {other}"),
    }
    // A 500 means the bot answered, so cached data is not the right response.
    assert!(!err.is_offline());
    assert!(!err.is_auth());
}

#[tokio::test]
async fn a_decode_failure_reports_what_arrived() {
    // reqwest's own .json() throws the body away on failure, which is what made
    // the float `max_open_trades` surprise hard to diagnose. Keep a snippet.
    let server = MockServer::start().await;
    let client = logged_in(&server).await;

    Mock::given(method("GET"))
        .and(path("/api/v1/show_config"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>login page</html>"))
        .mount(&server)
        .await;

    let err = client.show_config().await.unwrap_err();
    match &err {
        ClientError::Decode { body, endpoint, .. } => {
            assert_eq!(endpoint, "/show_config");
            assert!(body.contains("login page"), "body not preserved: {body}");
        }
        other => panic!("expected a decode error, got {other}"),
    }
}

#[tokio::test]
async fn a_bad_url_is_rejected_before_any_request() {
    let err = FreqtradeClient::new("ftp://nope", USER, PASS).unwrap_err();
    assert!(matches!(err, ClientError::InvalidUrl { .. }));
    // The add-bot form relies on this to validate before saving.
    assert!(FreqtradeClient::new("192.168.1.10:8080", USER, PASS).is_ok());
}

//! Auth-flow tests against a mock Freqtrade.
//!
//! This is the milestone's whole point: the Flutter client kept only the access
//! token and had no 401 handling, so ~15 minutes after launch every screen
//! dead-ended on an error view. These tests pin down that a 401 recovers, that
//! it recovers exactly once no matter how many requests race, and that it gives
//! up rather than looping when recovery genuinely fails.

use std::time::Duration;

use ft_client::{ClientError, FreqtradeClient};
use wiremock::matchers::{basic_auth, bearer_token, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER: &str = "sami";
const PASS: &str = "hunter2";

/// Short so a timeout test does not take ten seconds.
fn client(server: &MockServer) -> FreqtradeClient {
    FreqtradeClient::with_timeout(&server.uri(), USER, PASS, Duration::from_millis(300)).unwrap()
}

fn token_body(access: &str, refresh: Option<&str>) -> serde_json::Value {
    match refresh {
        Some(r) => serde_json::json!({"access_token": access, "refresh_token": r}),
        None => serde_json::json!({"access_token": access}),
    }
}

/// Mounts `/token/login`, accepting the Basic credentials.
async fn mount_login(server: &MockServer, access: &str, refresh: Option<&str>) {
    Mock::given(method("POST"))
        .and(path("/api/v1/token/login"))
        .and(basic_auth(USER, PASS))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body(access, refresh)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn login_keeps_both_tokens_and_uses_the_access_token() {
    let server = MockServer::start().await;
    mount_login(&server, "access-1", Some("refresh-1")).await;

    // The access token, not the refresh token, authenticates ordinary calls.
    Mock::given(method("GET"))
        .and(path("/api/v1/show_config"))
        .and(bearer_token("access-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "exchange": "binance", "dry_run": true, "max_open_trades": 2.0
        })))
        .mount(&server)
        .await;

    let client = client(&server);
    assert!(!client.is_authenticated().await);
    client.login().await.unwrap();
    assert!(client.is_authenticated().await);

    let config = client.show_config().await.unwrap();
    assert_eq!(config.exchange, "binance");
    assert!(config.dry_run);
    // The float-integer coercion from M1, now end-to-end through the client.
    assert_eq!(config.max_open_trades, 2);
}

#[tokio::test]
async fn calls_before_login_fail_without_touching_the_network() {
    let server = MockServer::start().await;
    let err = client(&server).show_config().await.unwrap_err();
    assert!(matches!(err, ClientError::NotAuthenticated));
    assert!(err.is_auth());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_expired_token_refreshes_and_the_call_succeeds() {
    let server = MockServer::start().await;
    mount_login(&server, "expired", Some("refresh-1")).await;

    // Freqtrade wants the *refresh* token in the bearer header here, not the
    // access token. Getting that backwards would 401 forever.
    Mock::given(method("POST"))
        .and(path("/api/v1/token/refresh"))
        .and(bearer_token("refresh-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("fresh", None)))
        .expect(1)
        .mount(&server)
        .await;

    // The stale token is rejected...
    Mock::given(method("GET"))
        .and(path("/api/v1/profit"))
        .and(bearer_token("expired"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Could not validate credentials"))
        .mount(&server)
        .await;
    // ...and the refreshed one is accepted.
    Mock::given(method("GET"))
        .and(path("/api/v1/profit"))
        .and(bearer_token("fresh"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"closed_trade_count": 327})),
        )
        .mount(&server)
        .await;

    let client = client(&server);
    client.login().await.unwrap();

    // The caller sees a normal success; the 401 never surfaces.
    let profit = client.profit().await.unwrap();
    assert_eq!(profit.closed_trade_count, 327);
}

#[tokio::test]
async fn a_burst_of_parallel_requests_refreshes_only_once() {
    // A screen fans out several calls at once, so an expired token produces a
    // burst of simultaneous 401s. Refreshing per-401 would fire one refresh per
    // request and let the losers install stale tokens over the winner's.
    let server = MockServer::start().await;
    mount_login(&server, "expired", Some("refresh-1")).await;

    Mock::given(method("POST"))
        .and(path("/api/v1/token/refresh"))
        .respond_with(
            ResponseTemplate::new(200)
                // Make the race real: hold the gate long enough that every
                // other task reaches it while this one is still in flight.
                .set_delay(Duration::from_millis(80))
                .set_body_json(token_body("fresh", None)),
        )
        .expect(1) // the assertion that matters
        .mount(&server)
        .await;

    for endpoint in ["status", "balance", "profit", "whitelist"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/{endpoint}")))
            .and(bearer_token("expired"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v1/status"))
        .and(bearer_token("fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    for endpoint in ["balance", "profit", "whitelist"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v1/{endpoint}")))
            .and(bearer_token("fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;
    }

    let client = client(&server);
    client.login().await.unwrap();

    let (open, balance, profit, whitelist) = tokio::join!(
        client.open_trades(),
        client.balance(),
        client.profit(),
        client.whitelist(),
    );
    open.unwrap();
    balance.unwrap();
    profit.unwrap();
    whitelist.unwrap();
    // `.expect(1)` on the refresh mock is verified when the server drops.
}

#[tokio::test]
async fn an_expired_refresh_token_falls_back_to_a_full_login() {
    // A phone asleep for days comes back with both tokens dead. Re-logging in
    // is strictly better than stranding the user on an error screen.
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/token/refresh"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token/login"))
        .and(basic_auth(USER, PASS))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("expired", Some("dead"))))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    // Second login, after the refresh fails, issues a working token.
    Mock::given(method("POST"))
        .and(path("/api/v1/token/login"))
        .and(basic_auth(USER, PASS))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(token_body("revived", Some("refresh-2"))),
        )
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v1/whitelist"))
        .and(bearer_token("expired"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/whitelist"))
        .and(bearer_token("revived"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"whitelist": ["ETH/USDT:USDT"]})),
        )
        .mount(&server)
        .await;

    let client = client(&server);
    client.login().await.unwrap();
    let wl = client.whitelist().await.unwrap();
    assert_eq!(wl.whitelist, ["ETH/USDT:USDT"]);
}

#[tokio::test]
async fn a_persistent_401_gives_up_instead_of_looping() {
    // Password changed on the bot. Recovery cannot work, and the one thing we
    // must not do is retry forever.
    let server = MockServer::start().await;
    mount_login(&server, "expired", Some("refresh-1")).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token/refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("also-bad", None)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/balance"))
        .respond_with(ResponseTemplate::new(401))
        // Exactly two: the original attempt and one retry after refreshing.
        .expect(2)
        .mount(&server)
        .await;

    let client = client(&server);
    client.login().await.unwrap();
    let err = client.balance().await.unwrap_err();
    assert!(err.is_auth(), "expected an auth error, got {err}");
    assert!(!err.is_offline());
}

#[tokio::test]
async fn bad_credentials_are_reported_as_auth_not_as_connectivity() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/token/login"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Incorrect username or password"))
        .mount(&server)
        .await;

    let err = client(&server).login().await.unwrap_err();
    assert!(err.is_auth());
    assert!(!err.is_offline(), "a wrong password is not an outage");
    assert!(err.to_string().contains("rejected these credentials"));
}

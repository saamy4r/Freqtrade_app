//! Axum service backing the UI.
//!
//! One process, two deployments: a standalone binary during development on
//! desktop, and [`spawn_embedded`] inside the Android app, listening on
//! loopback. The UI talks HTTP either way, so there is exactly one code path
//! to reason about.
//!
//! Every screen is one endpoint, already aggregated. The Flutter app fanned
//! out three or four Freqtrade calls per screen and repeated them on every tab
//! mount and bot switch; against a real bot those calls measure 138-166ms
//! apiece. Here they are joined server-side, cached in SQLite, and served from
//! cache within a short TTL, so revisiting a tab costs nothing.

pub mod compute;
mod error;
pub mod events;
mod fetch;
pub mod routes;
mod state;
mod sync;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::routing::{delete, get, post, put};
use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use ft_store::Store;

pub use error::{ApiError, ApiResult};
pub use fetch::ttl;
pub use routes::settings::ACTIVE_BOT;
pub use state::AppState;

/// Builds the API router.
///
/// `dev_cors` relaxes CORS for `dx serve`, whose dev server runs on a different
/// origin than this one. It is off in the shipped app, where the UI is served
/// from this same origin.
pub fn router(store: Arc<Store>, dev_cors: bool) -> Router {
    router_with_ui(store, dev_cors, None)
}

/// As [`router`], but also serves a built UI bundle.
///
/// This is how the app actually ships: one origin serving both the API and the
/// page, which is why it is worth using in development too. Routing is
/// client-side, so anything not matching a file falls back to `index.html` —
/// without that, a reload on `/dashboard` would 404.
pub fn router_with_ui(
    store: Arc<Store>,
    dev_cors: bool,
    ui_dir: Option<std::path::PathBuf>,
) -> Router {
    router_from_state(AppState::new(store), dev_cors, ui_dir)
}

/// As [`router_with_ui`], from an existing state.
///
/// Exposed so a caller can hold the same [`AppState`] the router uses — tests
/// subscribe to its event channel to observe what the server announces.
pub fn router_from_state(
    state: AppState,
    dev_cors: bool,
    ui_dir: Option<std::path::PathBuf>,
) -> Router {
    // The background refresh runs for the life of the process, gated on
    // whether anyone is actually watching.
    sync::spawn(state.clone());

    let api = Router::new()
        .route("/health", get(health))
        .route("/events", get(events::stream))
        .route("/bots", get(routes::bots::list).post(routes::bots::add))
        .route("/bots/order", put(routes::bots::reorder))
        .route("/bots/{id}", delete(routes::bots::delete))
        .route("/bots/{id}/ping", get(routes::bots::ping))
        .route("/bots/{id}/config", get(routes::screens::config))
        .route("/bots/{id}/overview", get(routes::screens::overview))
        .route("/bots/{id}/closed", get(routes::screens::closed))
        .route("/bots/{id}/dashboard", get(routes::screens::dashboard))
        .route("/bots/{id}/logs", get(routes::screens::logs))
        .route("/bots/{id}/pairs", get(routes::screens::pairs))
        .route("/bots/{id}/candles", get(routes::screens::candles))
        .route("/bots/{id}/forceexit", post(routes::screens::force_exit))
        .route(
            "/settings/{key}",
            get(routes::settings::get).put(routes::settings::put),
        )
        .with_state(state);

    let mut app = Router::new().nest("/api", api);

    if let Some(dir) = ui_dir {
        let index = dir.join("index.html");
        // The wasm bundle has a stable filename, so a browser will cache it
        // indefinitely and keep running an old build after a rebuild -- which
        // is extremely confusing, because the page looks fine and simply
        // behaves like the code you are no longer running. The server is
        // either on loopback (Android) or localhost (development), so there is
        // nothing to gain from caching here anyway.
        app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(&index)));
        app = app.layer(SetResponseHeaderLayer::overriding(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-store, must-revalidate"),
        ));
        tracing::info!(dir = %dir.display(), "serving UI");
    }

    app = app.layer(TraceLayer::new_for_http());
    if dev_cors {
        app = app.layer(CorsLayer::very_permissive());
    }
    app
}

async fn health() -> &'static str {
    "ok"
}

/// A running server.
pub struct Running {
    /// The port actually bound. With port 0 the OS chooses, which is what the
    /// Android build wants: a fixed port could already be taken.
    pub port: u16,
    pub handle: tokio::task::JoinHandle<()>,
}

/// Starts the server on loopback and returns once it is listening.
///
/// This is the Android entry point: the UI cannot ask for data until it knows
/// the port, so binding has to finish before this returns rather than being
/// left to a spawned task to get around to.
pub async fn spawn_embedded(store: Arc<Store>) -> std::io::Result<Running> {
    spawn_on(store, SocketAddr::from((Ipv4Addr::LOCALHOST, 0)), false).await
}

/// Starts the server on `addr`.
pub async fn spawn_on(
    store: Arc<Store>,
    addr: SocketAddr,
    dev_cors: bool,
) -> std::io::Result<Running> {
    spawn_on_with_ui(store, addr, dev_cors, None).await
}

/// As [`spawn_on`], additionally serving a built UI bundle.
pub async fn spawn_on_with_ui(
    store: Arc<Store>,
    addr: SocketAddr,
    dev_cors: bool,
    ui_dir: Option<std::path::PathBuf>,
) -> std::io::Result<Running> {
    let listener = TcpListener::bind(addr).await?;
    let port = listener.local_addr()?.port();
    let app = router_with_ui(store, dev_cors, ui_dir);

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "server stopped");
        }
    });

    tracing::info!(port, "ft-server listening");
    Ok(Running { port, handle })
}

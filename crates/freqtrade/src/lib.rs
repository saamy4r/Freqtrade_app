//! Dioxus UI for the Freqtrade visualizer.
//!
//! One codebase, three targets: the browser during development, and Android as
//! the product. Every screen is one call to `ft-server`, which has already
//! done the aggregation, caching, auth and the offline decision — so there is
//! no Freqtrade-shaped logic on this side at all.

pub mod api;
pub mod app;
pub mod backend;
pub mod components;
pub mod events;
pub mod format;
pub mod keystore;
pub mod logging;
pub mod screens;
pub mod state;

pub use app::{App, Route};

use std::sync::OnceLock;

/// A startup failure the UI should show instead of the app.
static STARTUP_FAILURE: OnceLock<String> = OnceLock::new();

/// Records a failure that makes the app unusable, for the UI to render.
///
/// Better than aborting: the reason reaches the screen instead of vanishing
/// with the process, which on a phone is the difference between a bug report
/// and "it just closes".
pub fn startup_failed(reason: impl Into<String>) {
    let reason = reason.into();
    tracing::error!(%reason, "startup failed");
    let _ = STARTUP_FAILURE.set(reason);
}

/// The startup failure, if there was one.
pub fn startup_failure() -> Option<&'static str> {
    STARTUP_FAILURE.get().map(String::as_str)
}

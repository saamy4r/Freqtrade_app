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
pub mod screens;
pub mod state;

pub use app::{App, Route};

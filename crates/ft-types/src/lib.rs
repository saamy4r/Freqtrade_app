//! Shared data types for the Freqtrade visualizer.
//!
//! Split in two halves:
//! - [`freqtrade`]: DTOs mirroring the Freqtrade REST API, deliberately tolerant
//!   of version drift (a missing field degrades instead of failing the response).
//! - [`api`]: the shapes `ft-server` hands to the UI, already aggregated per
//!   screen so each screen costs one request instead of the legacy app's
//!   three-to-four-way fan-out.

pub mod api;
pub mod flex;
pub mod freqtrade;

pub use freqtrade::*;

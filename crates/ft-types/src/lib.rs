//! Shared data types for the Freqtrade visualizer.
//!
//! Split in two halves:
//! - [`freqtrade`]: DTOs mirroring the Freqtrade REST API, deliberately tolerant of
//!   version drift (missing fields degrade instead of failing the whole response).
//! - [`api`]: the shapes `ft-server` hands to the UI, already aggregated per screen.

pub mod api;
pub mod freqtrade;

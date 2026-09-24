//! Axum service backing the UI.
//!
//! Runs standalone on desktop during development and is spawned in-process on
//! loopback inside the Android app (see `spawn_embedded`, landing in M4).

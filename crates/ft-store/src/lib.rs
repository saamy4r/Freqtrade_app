//! SQLite persistence: bots, encrypted credentials, trades, cached snapshots.
//!
//! Replaces the Flutter app's per-bot JSON blob with a real schema, so closed
//! trades can be synced incrementally instead of re-fetching 500 rows per screen.

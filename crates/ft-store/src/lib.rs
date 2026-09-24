//! SQLite persistence: bots, encrypted credentials, trades, cached responses.
//!
//! Replaces two Flutter storage mechanisms at once — bot definitions in
//! SharedPreferences (with plaintext passwords) and a per-bot JSON blob at
//! `<docs>/bot_cache/<botId>.json` that was shallow-merged and rewritten whole
//! on every sync.
//!
//! The reason for a real schema is not tidiness. Individually addressable
//! trades make an incremental sync possible: the old app re-fetched
//! `/trades?limit=500` on three separate screens, every mount and every bot
//! switch, at roughly 166ms a call against a real bot.

mod crypto;
mod error;
mod model;
mod schema;
mod store;

pub use crypto::{Crypto, FileKey, KeyProvider, StaticKey, KEY_LEN};
pub use error::{Result, StoreError};
pub use model::{BotRecord, Cached, NewBot, SyncPlan};
pub use schema::SCHEMA_VERSION;
pub use store::{kind, plan_sync, Store};

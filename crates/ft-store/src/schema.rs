//! Schema and migrations.
//!
//! Replaces the Flutter app's per-bot JSON blob
//! (`<docs>/bot_cache/<botId>.json`, shallow-merged and rewritten whole on
//! every sync) with a real schema. The point is not tidiness: it is that
//! trades become individually addressable, so a sync can fetch only what is
//! new instead of re-downloading 500 rows on three separate screens.

use rusqlite::Connection;

use crate::error::Result;

/// Bumped whenever the schema changes; [`migrate`] steps up from whatever the
/// file has.
pub const SCHEMA_VERSION: i32 = 1;

/// Applies any migrations the database is missing.
pub fn migrate(conn: &Connection) -> Result<()> {
    // WAL keeps a background sync from blocking a read the UI is waiting on.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Durability is worth less than latency here: everything in this database
    // can be re-fetched from the bot.
    conn.pragma_update(None, "synchronous", "NORMAL")?;

    let current: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current >= SCHEMA_VERSION {
        return Ok(());
    }

    if current < 1 {
        conn.execute_batch(V1)?;
    }

    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tracing::info!(from = current, to = SCHEMA_VERSION, "migrated store");
    Ok(())
}

const V1: &str = r#"
CREATE TABLE bots (
    id          TEXT    PRIMARY KEY,
    name        TEXT    NOT NULL,
    -- Already normalized by ft-client, so it ends in /api/v1.
    url         TEXT    NOT NULL,
    username    TEXT    NOT NULL,
    -- Display order. The Flutter app used the position in a JSON array, which
    -- meant a reorder rewrote every bot.
    sort_order  INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);

-- Separate table so a bot row can be read (for the list, the ping, the header)
-- without touching the secret at all.
CREATE TABLE credentials (
    bot_id     TEXT PRIMARY KEY REFERENCES bots(id) ON DELETE CASCADE,
    nonce      BLOB NOT NULL,
    ciphertext BLOB NOT NULL
);

-- One row per trade. `payload` holds the full Trade as JSON so a Freqtrade
-- upgrade that adds a field costs nothing here; the extracted columns exist
-- only to sort and filter without decoding every row.
CREATE TABLE trades (
    bot_id          TEXT    NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    trade_id        INTEGER NOT NULL,
    is_open         INTEGER NOT NULL,
    pair            TEXT    NOT NULL,
    open_timestamp  INTEGER,
    close_timestamp INTEGER,
    payload         TEXT    NOT NULL,
    PRIMARY KEY (bot_id, trade_id)
);

-- Closed Trades sorts by close time descending, and close time is NOT
-- monotonic with trade_id: trades open in order but close out of order.
CREATE INDEX trades_closed_by_time ON trades(bot_id, close_timestamp DESC)
    WHERE is_open = 0;
CREATE INDEX trades_open_by_time ON trades(bot_id, open_timestamp DESC)
    WHERE is_open = 1;

-- Whole-response caches that have no useful sub-structure: config, profit,
-- balance, whitelist. `fetched_at` drives the "Last synced Xm ago" banner.
CREATE TABLE snapshots (
    bot_id     TEXT    NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    kind       TEXT    NOT NULL,
    payload    TEXT    NOT NULL,
    fetched_at INTEGER NOT NULL,
    PRIMARY KEY (bot_id, kind)
);

CREATE TABLE candles (
    bot_id    TEXT    NOT NULL REFERENCES bots(id) ON DELETE CASCADE,
    pair      TEXT    NOT NULL,
    timeframe TEXT    NOT NULL,
    time      INTEGER NOT NULL,
    open      REAL    NOT NULL,
    high      REAL    NOT NULL,
    low       REAL    NOT NULL,
    close     REAL    NOT NULL,
    volume    REAL    NOT NULL,
    PRIMARY KEY (bot_id, pair, timeframe, time)
);

-- App-level key/value: active bot, theme.
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

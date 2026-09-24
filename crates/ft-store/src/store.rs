//! The SQLite store.
//!
//! Deliberately synchronous. SQLite calls here are sub-millisecond, and keeping
//! the API blocking means the server decides explicitly when to move work onto
//! a blocking thread rather than this crate guessing.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::de::DeserializeOwned;
use serde::Serialize;
use time::OffsetDateTime;

use ft_types::freqtrade::{Candle, Trade};

use crate::crypto::{Crypto, KeyProvider};
use crate::error::{Result, StoreError};
use crate::model::{BotRecord, Cached, NewBot, SyncPlan};
use crate::schema;

/// Snapshot kinds. Strings rather than an enum in the schema so adding one is
/// not a migration.
pub mod kind {
    pub const CONFIG: &str = "config";
    pub const PROFIT: &str = "profit";
    pub const BALANCE: &str = "balance";
    pub const WHITELIST: &str = "whitelist";
    pub const LOGS: &str = "logs";
    /// Freshness marks. The data lives in the `trades` table; these rows exist
    /// only to record when that table was last synced, reusing `fetched_at`
    /// rather than adding a column to every trade.
    pub const OPEN_MARK: &str = "mark:open";
    pub const CLOSED_MARK: &str = "mark:closed";
}

pub struct Store {
    conn: Mutex<Connection>,
    crypto: Crypto,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Store")
    }
}

impl Store {
    /// Opens (creating if needed) the database at `path`, migrating it.
    pub fn open(path: impl AsRef<Path>, keys: &dyn KeyProvider) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StoreError::Key(e.to_string()))?;
        }
        let conn = Connection::open(path).map_err(|source| StoreError::Open {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_connection(conn, keys)
    }

    /// An ephemeral database, for tests.
    pub fn in_memory(keys: &dyn KeyProvider) -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?, keys)
    }

    fn from_connection(conn: Connection, keys: &dyn KeyProvider) -> Result<Self> {
        schema::migrate(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            crypto: Crypto::new(keys)?,
        })
    }

    /// Runs `f` with the connection. Recovers from a poisoned mutex, since a
    /// panic in one request should not take the whole store down with it.
    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }

    // -----------------------------------------------------------------------
    // Bots
    // -----------------------------------------------------------------------

    /// Bots in display order.
    pub fn list_bots(&self) -> Result<Vec<BotRecord>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, url, username, sort_order
                 FROM bots ORDER BY sort_order, created_at",
            )?;
            let rows = stmt
                .query_map([], |row| {
                    Ok(BotRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        url: row.get(2)?,
                        username: row.get(3)?,
                        sort_order: row.get(4)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_bot(&self, id: &str) -> Result<Option<BotRecord>> {
        self.with(|conn| {
            conn.query_row(
                "SELECT id, name, url, username, sort_order FROM bots WHERE id = ?1",
                params![id],
                |row| {
                    Ok(BotRecord {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        url: row.get(2)?,
                        username: row.get(3)?,
                        sort_order: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
        })
    }

    /// Adds a bot, sealing its password. Appended to the end of the list.
    pub fn add_bot(&self, new: NewBot) -> Result<BotRecord> {
        let id = uuid::Uuid::new_v4().to_string();
        let (nonce, ciphertext) = self.crypto.seal(&new.password)?;
        let now = OffsetDateTime::now_utc().unix_timestamp();

        self.with(|conn| {
            let next: i64 = conn.query_row(
                "SELECT COALESCE(MAX(sort_order) + 1, 0) FROM bots",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO bots (id, name, url, username, sort_order, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, new.name, new.url, new.username, next, now],
            )?;
            conn.execute(
                "INSERT INTO credentials (bot_id, nonce, ciphertext) VALUES (?1, ?2, ?3)",
                params![id, nonce, ciphertext],
            )?;
            Ok(BotRecord {
                id: id.clone(),
                name: new.name.clone(),
                url: new.url.clone(),
                username: new.username.clone(),
                sort_order: next,
            })
        })
    }

    /// Deletes a bot and everything cached for it. Returns whether it existed.
    ///
    /// Cascades cover credentials, trades, snapshots and candles, which is the
    /// behaviour the delete dialog promises ("local cached data is also
    /// deleted") and which the Flutter app implemented by hand across two
    /// storage systems.
    pub fn delete_bot(&self, id: &str) -> Result<bool> {
        self.with(|conn| {
            let removed = conn.execute("DELETE FROM bots WHERE id = ?1", params![id])?;
            Ok(removed > 0)
        })
    }

    /// Rewrites display order to match `ids`. Ids not present are left after
    /// the listed ones, so a stale reorder cannot lose a bot.
    pub fn reorder_bots(&self, ids: &[String]) -> Result<()> {
        self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            // Push everything out of the way first: sort_order has no unique
            // constraint, but leaving old and new values interleaved mid-update
            // makes the intermediate state hard to reason about.
            tx.execute("UPDATE bots SET sort_order = sort_order + 100000", [])?;
            for (position, id) in ids.iter().enumerate() {
                tx.execute(
                    "UPDATE bots SET sort_order = ?1 WHERE id = ?2",
                    params![position as i64, id],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// The decrypted password for a bot.
    ///
    /// Deliberately separate from [`Store::get_bot`] so the secret is only read
    /// where it is actually needed — building a client.
    pub fn password(&self, bot_id: &str) -> Result<String> {
        let row: Option<(Vec<u8>, Vec<u8>)> = self.with(|conn| {
            conn.query_row(
                "SELECT nonce, ciphertext FROM credentials WHERE bot_id = ?1",
                params![bot_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
        })?;
        let (nonce, ciphertext) = row.ok_or_else(|| StoreError::UnknownBot(bot_id.to_owned()))?;
        self.crypto.open(&nonce, &ciphertext)
    }

    /// Replaces a bot's stored password, e.g. after it changed on the bot.
    pub fn set_password(&self, bot_id: &str, password: &str) -> Result<()> {
        let (nonce, ciphertext) = self.crypto.seal(password)?;
        self.with(|conn| {
            let updated = conn.execute(
                "INSERT INTO credentials (bot_id, nonce, ciphertext) VALUES (?1, ?2, ?3)
                 ON CONFLICT(bot_id) DO UPDATE SET nonce = ?2, ciphertext = ?3",
                params![bot_id, nonce, ciphertext],
            )?;
            if updated == 0 {
                return Err(StoreError::UnknownBot(bot_id.to_owned()));
            }
            Ok(())
        })
    }

    // -----------------------------------------------------------------------
    // Trades
    // -----------------------------------------------------------------------

    /// Inserts or updates trades, keyed by `(bot_id, trade_id)`.
    ///
    /// Idempotent: re-syncing the same window changes nothing, which is what
    /// makes a retry after a half-finished sync safe.
    pub fn upsert_trades(&self, bot_id: &str, trades: &[Trade]) -> Result<usize> {
        if trades.is_empty() {
            return Ok(0);
        }
        let encoded = encode(trades)?;
        self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            let written = write_trades(&tx, bot_id, &encoded)?;
            tx.commit()?;
            Ok(written)
        })
    }

    /// Makes the stored open set exactly `trades`.
    ///
    /// `/status` is authoritative for what is open: a trade that closed simply
    /// stops appearing there. Rows still marked open are cleared and the
    /// incoming set written in their place, in one transaction so no reader
    /// ever sees an empty open list.
    ///
    /// Closed rows are deliberately untouched, so a trade that closed between
    /// the `/status` and `/trades` calls is not lost.
    pub fn replace_open_trades(&self, bot_id: &str, trades: &[Trade]) -> Result<()> {
        let encoded = encode(trades)?;
        self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "DELETE FROM trades WHERE bot_id = ?1 AND is_open = 1",
                params![bot_id],
            )?;
            write_trades(&tx, bot_id, &encoded)?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn open_trades(&self, bot_id: &str) -> Result<Vec<Trade>> {
        self.query_trades(
            "SELECT payload FROM trades WHERE bot_id = ?1 AND is_open = 1
             ORDER BY open_timestamp DESC",
            params![bot_id],
        )
    }

    /// Closed trades, newest first, which is the order both the Closed Trades
    /// list and the cumulative chart want.
    pub fn closed_trades(&self, bot_id: &str, limit: u32, offset: u32) -> Result<Vec<Trade>> {
        self.query_trades(
            "SELECT payload FROM trades WHERE bot_id = ?1 AND is_open = 0
             ORDER BY close_timestamp DESC, trade_id DESC
             LIMIT ?2 OFFSET ?3",
            params![bot_id, limit, offset],
        )
    }

    /// Every closed trade, oldest first — the order the cumulative profit
    /// chart accumulates in.
    pub fn closed_trades_ascending(&self, bot_id: &str) -> Result<Vec<Trade>> {
        self.query_trades(
            "SELECT payload FROM trades WHERE bot_id = ?1 AND is_open = 0
             ORDER BY close_timestamp ASC, trade_id ASC",
            params![bot_id],
        )
    }

    /// Every trade on one pair, open and closed, oldest first.
    ///
    /// Used to draw trade markers over the price chart. Filtering in SQL keeps
    /// a bot with thousands of trades from deserializing all of them to find
    /// the handful on the pair being viewed.
    pub fn trades_for_pair(&self, bot_id: &str, pair: &str) -> Result<Vec<Trade>> {
        self.query_trades(
            "SELECT payload FROM trades WHERE bot_id = ?1 AND pair = ?2
             ORDER BY COALESCE(open_timestamp, 0) ASC",
            params![bot_id, pair],
        )
    }

    pub fn closed_trade_count(&self, bot_id: &str) -> Result<u32> {
        self.with(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM trades WHERE bot_id = ?1 AND is_open = 0",
                params![bot_id],
                |row| row.get(0),
            )?;
            Ok(count as u32)
        })
    }

    fn query_trades(&self, sql: &str, args: impl rusqlite::Params) -> Result<Vec<Trade>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(sql)?;
            let payloads = stmt
                .query_map(args, |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            payloads
                .iter()
                .map(|p| serde_json::from_str(p).map_err(StoreError::from))
                .collect()
        })
    }

    /// Works out the smallest fetch that brings closed trades up to date.
    ///
    /// See [`SyncPlan`] for why `offset` is the local count.
    pub fn sync_plan(&self, bot_id: &str, remote_total: u32) -> Result<SyncPlan> {
        let local = self.closed_trade_count(bot_id)?;
        Ok(plan_sync(local, remote_total))
    }

    /// Drops every trade for a bot, for [`SyncPlan::FullResync`].
    pub fn clear_trades(&self, bot_id: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute("DELETE FROM trades WHERE bot_id = ?1", params![bot_id])?;
            Ok(())
        })
    }

    // -----------------------------------------------------------------------
    // Snapshots
    // -----------------------------------------------------------------------

    /// Caches a whole response under `kind`, stamped now.
    pub fn put_snapshot<T: Serialize>(&self, bot_id: &str, kind: &str, value: &T) -> Result<()> {
        let payload = serde_json::to_string(value)?;
        let now = OffsetDateTime::now_utc().unix_timestamp();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO snapshots (bot_id, kind, payload, fetched_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(bot_id, kind) DO UPDATE SET payload = ?3, fetched_at = ?4",
                params![bot_id, kind, payload, now],
            )?;
            Ok(())
        })
    }

    /// Reads a cached response, with the time it was fetched.
    pub fn snapshot<T: DeserializeOwned>(
        &self,
        bot_id: &str,
        kind: &str,
    ) -> Result<Option<Cached<T>>> {
        let row: Option<(String, i64)> = self.with(|conn| {
            conn.query_row(
                "SELECT payload, fetched_at FROM snapshots WHERE bot_id = ?1 AND kind = ?2",
                params![bot_id, kind],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(Into::into)
        })?;

        row.map(|(payload, fetched_at)| {
            Ok(Cached {
                value: serde_json::from_str(&payload)?,
                fetched_at: OffsetDateTime::from_unix_timestamp(fetched_at)
                    .unwrap_or(OffsetDateTime::UNIX_EPOCH),
            })
        })
        .transpose()
    }

    /// Forgets one cached response, so the next read refetches.
    ///
    /// Used after an action that invalidates data we just cached -- a force
    /// exit changes the open set immediately.
    pub fn clear_snapshot(&self, bot_id: &str, kind: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "DELETE FROM snapshots WHERE bot_id = ?1 AND kind = ?2",
                params![bot_id, kind],
            )?;
            Ok(())
        })
    }

    /// The most recent sync time across every cached kind, for the offline
    /// banner's "Last synced Xm ago".
    pub fn last_synced(&self, bot_id: &str) -> Result<Option<OffsetDateTime>> {
        self.with(|conn| {
            let at: Option<i64> = conn.query_row(
                "SELECT MAX(fetched_at) FROM snapshots WHERE bot_id = ?1",
                params![bot_id],
                |row| row.get(0),
            )?;
            Ok(at.and_then(|s| OffsetDateTime::from_unix_timestamp(s).ok()))
        })
    }

    // -----------------------------------------------------------------------
    // Candles
    // -----------------------------------------------------------------------

    pub fn put_candles(
        &self,
        bot_id: &str,
        pair: &str,
        timeframe: &str,
        candles: &[Candle],
    ) -> Result<()> {
        if candles.is_empty() {
            return Ok(());
        }
        self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO candles
                        (bot_id, pair, timeframe, time, open, high, low, close, volume)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(bot_id, pair, timeframe, time) DO UPDATE SET
                        open = ?5, high = ?6, low = ?7, close = ?8, volume = ?9",
                )?;
                for c in candles {
                    stmt.execute(params![
                        bot_id, pair, timeframe, c.time, c.open, c.high, c.low, c.close, c.volume
                    ])?;
                }
            }
            tx.commit()?;
            Ok(())
        })
    }

    /// The most recent `limit` candles, oldest first.
    pub fn candles(
        &self,
        bot_id: &str,
        pair: &str,
        timeframe: &str,
        limit: u32,
    ) -> Result<Vec<Candle>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(
                "SELECT time, open, high, low, close, volume FROM (
                     SELECT * FROM candles
                     WHERE bot_id = ?1 AND pair = ?2 AND timeframe = ?3
                     ORDER BY time DESC LIMIT ?4
                 ) ORDER BY time ASC",
            )?;
            let rows = stmt
                .query_map(params![bot_id, pair, timeframe, limit], |row| {
                    Ok(Candle {
                        time: row.get(0)?,
                        open: row.get(1)?,
                        high: row.get(2)?,
                        low: row.get(3)?,
                        close: row.get(4)?,
                        volume: row.get(5)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    // -----------------------------------------------------------------------
    // Settings
    // -----------------------------------------------------------------------

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = ?2",
                params![key, value],
            )?;
            Ok(())
        })
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|conn| {
            conn.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
        })
    }
}

/// Serializes trades once, so a transaction holds the lock for as little time
/// as possible.
fn encode(trades: &[Trade]) -> Result<Vec<(&Trade, String)>> {
    trades
        .iter()
        .map(|t| Ok((t, serde_json::to_string(t)?)))
        .collect()
}

/// Writes trades through the upsert. Shared by [`Store::upsert_trades`] and
/// [`Store::replace_open_trades`] so the column list lives in one place.
fn write_trades(
    tx: &rusqlite::Transaction<'_>,
    bot_id: &str,
    encoded: &[(&Trade, String)],
) -> Result<usize> {
    if encoded.is_empty() {
        return Ok(0);
    }
    let mut stmt = tx.prepare(
        "INSERT INTO trades
            (bot_id, trade_id, is_open, pair, open_timestamp, close_timestamp, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(bot_id, trade_id) DO UPDATE SET
            is_open = ?3, pair = ?4, open_timestamp = ?5,
            close_timestamp = ?6, payload = ?7",
    )?;
    for (trade, payload) in encoded {
        stmt.execute(params![
            bot_id,
            trade.trade_id,
            trade.is_open as i64,
            trade.pair,
            trade.opened_at().map(|d| d.unix_timestamp()),
            trade.closed_at().map(|d| d.unix_timestamp()),
            payload,
        ])?;
    }
    Ok(encoded.len())
}

/// Pure sync-window arithmetic, separated so it is testable without a database.
pub fn plan_sync(local: u32, remote_total: u32) -> SyncPlan {
    if remote_total < local {
        // The bot has fewer trades than we do: its database was reset or rows
        // were deleted, so our ids no longer line up with its offsets.
        SyncPlan::FullResync {
            total: remote_total,
        }
    } else if remote_total == local {
        SyncPlan::UpToDate
    } else {
        SyncPlan::Fetch {
            offset: local,
            limit: remote_total - local,
        }
    }
}

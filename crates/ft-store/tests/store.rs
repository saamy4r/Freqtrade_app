//! Store behaviour: bot lifecycle, trade sync, snapshots.

use ft_store::{kind, plan_sync, BotRecord, NewBot, StaticKey, Store, SyncPlan};
use ft_types::freqtrade::{Balance, BotConfig, Candle, Trade};

fn store() -> Store {
    Store::in_memory(&StaticKey::random().unwrap()).unwrap()
}

fn new_bot(name: &str) -> NewBot {
    NewBot {
        name: name.to_owned(),
        url: format!("http://{name}:8080/api/v1"),
        username: "sami".to_owned(),
        password: format!("{name}-secret"),
    }
}

/// A closed trade with the given id and close time (epoch seconds).
fn closed(trade_id: i64, close_ts: i64, profit: f64) -> Trade {
    Trade {
        trade_id,
        pair: "ETH/USDT:USDT".to_owned(),
        is_open: false,
        profit_abs: profit,
        open_timestamp: Some((close_ts - 3600) * 1000),
        close_timestamp: Some(close_ts * 1000),
        ..Default::default()
    }
}

fn open(trade_id: i64, open_ts: i64) -> Trade {
    Trade {
        trade_id,
        pair: "BTC/USDT:USDT".to_owned(),
        is_open: true,
        open_timestamp: Some(open_ts * 1000),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Bots
// ---------------------------------------------------------------------------

#[test]
fn bots_round_trip_in_insertion_order() {
    let store = store();
    assert!(store.list_bots().unwrap().is_empty());

    let a = store.add_bot(new_bot("alpha")).unwrap();
    let b = store.add_bot(new_bot("beta")).unwrap();

    let names: Vec<_> = store
        .list_bots()
        .unwrap()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(names, ["alpha", "beta"]);
    assert_eq!(store.get_bot(&a.id).unwrap().unwrap().name, "alpha");
    assert_ne!(a.id, b.id, "ids must be unique");
}

#[test]
fn passwords_round_trip_but_are_not_stored_in_the_clear() {
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    assert_eq!(store.password(&bot.id).unwrap(), "alpha-secret");

    // The bot record itself must not carry the secret: it is read for the
    // list, the header and the ping, none of which should touch it.
    let record: BotRecord = store.get_bot(&bot.id).unwrap().unwrap();
    let rendered = format!("{record:?}");
    assert!(!rendered.contains("alpha-secret"), "leaked: {rendered}");
}

#[test]
fn a_changed_password_can_be_replaced() {
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    store.set_password(&bot.id, "rotated").unwrap();
    assert_eq!(store.password(&bot.id).unwrap(), "rotated");
}

#[test]
fn reordering_rewrites_display_order() {
    let store = store();
    let a = store.add_bot(new_bot("alpha")).unwrap();
    let b = store.add_bot(new_bot("beta")).unwrap();
    let c = store.add_bot(new_bot("gamma")).unwrap();

    store
        .reorder_bots(&[c.id.clone(), a.id.clone(), b.id.clone()])
        .unwrap();

    let names: Vec<_> = store
        .list_bots()
        .unwrap()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(names, ["gamma", "alpha", "beta"]);
}

#[test]
fn a_reorder_that_omits_a_bot_does_not_lose_it() {
    // The UI sends the list it knows about. If a bot was added concurrently,
    // dropping it from the table would be much worse than ordering it last.
    let store = store();
    let a = store.add_bot(new_bot("alpha")).unwrap();
    let b = store.add_bot(new_bot("beta")).unwrap();
    let _c = store.add_bot(new_bot("gamma")).unwrap();

    store.reorder_bots(&[b.id.clone(), a.id.clone()]).unwrap();

    let names: Vec<_> = store
        .list_bots()
        .unwrap()
        .iter()
        .map(|r| r.name.clone())
        .collect();
    assert_eq!(names.len(), 3, "gamma vanished");
    assert_eq!(&names[..2], ["beta", "alpha"]);
    assert_eq!(names[2], "gamma");
}

#[test]
fn deleting_a_bot_removes_everything_cached_for_it() {
    // The delete dialog promises local cached data goes too. The Flutter app
    // did this by hand across SharedPreferences and a JSON file; here it is a
    // cascade, so it cannot drift.
    let store = store();
    let a = store.add_bot(new_bot("alpha")).unwrap();
    let b = store.add_bot(new_bot("beta")).unwrap();

    store.upsert_trades(&a.id, &[closed(1, 1000, 5.0)]).unwrap();
    store
        .put_snapshot(&a.id, kind::CONFIG, &BotConfig::default())
        .unwrap();
    store
        .put_candles(&a.id, "ETH/USDT:USDT", "2h", &[Candle::default()])
        .unwrap();
    // The other bot's data must survive.
    store.upsert_trades(&b.id, &[closed(1, 1000, 5.0)]).unwrap();

    assert!(store.delete_bot(&a.id).unwrap());
    assert!(
        !store.delete_bot(&a.id).unwrap(),
        "second delete is a no-op"
    );

    assert_eq!(store.closed_trade_count(&a.id).unwrap(), 0);
    assert!(store
        .snapshot::<BotConfig>(&a.id, kind::CONFIG)
        .unwrap()
        .is_none());
    assert!(store
        .candles(&a.id, "ETH/USDT:USDT", "2h", 10)
        .unwrap()
        .is_empty());
    assert!(store.password(&a.id).is_err());

    assert_eq!(
        store.closed_trade_count(&b.id).unwrap(),
        1,
        "wrong bot purged"
    );
}

// ---------------------------------------------------------------------------
// Trades
// ---------------------------------------------------------------------------

#[test]
fn upserting_the_same_trades_twice_changes_nothing() {
    // Idempotency is what makes retrying a half-finished sync safe.
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    let trades = vec![closed(1, 1000, 5.0), closed(2, 2000, -3.0)];

    store.upsert_trades(&bot.id, &trades).unwrap();
    store.upsert_trades(&bot.id, &trades).unwrap();

    assert_eq!(store.closed_trade_count(&bot.id).unwrap(), 2);
}

#[test]
fn an_upsert_updates_an_existing_trade() {
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    store
        .upsert_trades(&bot.id, &[closed(1, 1000, 5.0)])
        .unwrap();
    store
        .upsert_trades(&bot.id, &[closed(1, 1000, 7.5)])
        .unwrap();

    let trades = store.closed_trades(&bot.id, 10, 0).unwrap();
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].profit_abs, 7.5);
}

#[test]
fn closed_trades_come_back_newest_first() {
    // Close time is NOT monotonic with trade_id -- trades open in order but
    // close out of order -- so ordering must use close_timestamp.
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    store
        .upsert_trades(
            &bot.id,
            &[
                closed(1, 3000, 1.0), // opened first, closed last
                closed(2, 1000, 2.0),
                closed(3, 2000, 3.0),
            ],
        )
        .unwrap();

    let ids: Vec<_> = store
        .closed_trades(&bot.id, 10, 0)
        .unwrap()
        .iter()
        .map(|t| t.trade_id)
        .collect();
    assert_eq!(ids, [1, 3, 2], "not sorted by close time descending");

    // The cumulative chart wants the opposite order.
    let ascending: Vec<_> = store
        .closed_trades_ascending(&bot.id)
        .unwrap()
        .iter()
        .map(|t| t.trade_id)
        .collect();
    assert_eq!(ascending, [2, 3, 1]);
}

#[test]
fn the_open_set_is_replaced_not_merged() {
    // /status is authoritative for what is open: a trade that closed simply
    // stops appearing there.
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();

    store
        .replace_open_trades(&bot.id, &[open(10, 100), open(11, 200)])
        .unwrap();
    assert_eq!(store.open_trades(&bot.id).unwrap().len(), 2);

    // Trade 11 closed; only 10 is still open.
    store
        .replace_open_trades(&bot.id, &[open(10, 100)])
        .unwrap();
    let still_open: Vec<_> = store
        .open_trades(&bot.id)
        .unwrap()
        .iter()
        .map(|t| t.trade_id)
        .collect();
    assert_eq!(still_open, [10]);
}

#[test]
fn replacing_the_open_set_never_touches_closed_rows() {
    // A trade that closes between the /status and /trades calls must not be
    // lost: the closed row is already there and must survive.
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    store
        .upsert_trades(&bot.id, &[closed(1, 1000, 5.0)])
        .unwrap();

    store.replace_open_trades(&bot.id, &[open(2, 100)]).unwrap();
    assert_eq!(store.closed_trade_count(&bot.id).unwrap(), 1);

    // Even when nothing at all is open.
    store.replace_open_trades(&bot.id, &[]).unwrap();
    assert_eq!(store.closed_trade_count(&bot.id).unwrap(), 1);
    assert!(store.open_trades(&bot.id).unwrap().is_empty());
}

#[test]
fn a_trade_that_closes_moves_from_open_to_closed() {
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    store.replace_open_trades(&bot.id, &[open(7, 100)]).unwrap();
    assert_eq!(store.open_trades(&bot.id).unwrap().len(), 1);

    // The closed sync brings it back with exit details, same trade_id.
    store
        .upsert_trades(&bot.id, &[closed(7, 500, 12.0)])
        .unwrap();
    assert!(store.open_trades(&bot.id).unwrap().is_empty());
    assert_eq!(store.closed_trade_count(&bot.id).unwrap(), 1);
}

// ---------------------------------------------------------------------------
// Sync planning
// ---------------------------------------------------------------------------

#[test]
fn the_sync_window_fetches_only_new_trades() {
    // /trades returns oldest first (verified live: offset=0 gives ids 1..50),
    // so new trades land at the end and offset = local count.
    assert_eq!(
        plan_sync(0, 327),
        SyncPlan::Fetch {
            offset: 0,
            limit: 327
        }
    );
    assert_eq!(plan_sync(327, 327), SyncPlan::UpToDate);
    assert_eq!(
        plan_sync(327, 329),
        SyncPlan::Fetch {
            offset: 327,
            limit: 2
        }
    );
}

#[test]
fn a_shrinking_remote_triggers_a_full_resync() {
    // The bot's database was reset or trades were deleted, so our ids no
    // longer line up with its offsets; fetching from offset=local would skip
    // real trades or duplicate them.
    assert_eq!(plan_sync(327, 10), SyncPlan::FullResync { total: 10 });
    assert_eq!(plan_sync(1, 0), SyncPlan::FullResync { total: 0 });
}

#[test]
fn sync_plan_reads_the_live_local_count() {
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    assert_eq!(
        store.sync_plan(&bot.id, 327).unwrap(),
        SyncPlan::Fetch {
            offset: 0,
            limit: 327
        }
    );

    // Simulate the first full sync of the live bot's 327 trades.
    let all: Vec<_> = (1..=327).map(|i| closed(i, 1000 + i, 1.0)).collect();
    store.upsert_trades(&bot.id, &all).unwrap();
    assert_eq!(store.sync_plan(&bot.id, 327).unwrap(), SyncPlan::UpToDate);

    // Two new trades close: the old app would have re-fetched 500 rows.
    assert_eq!(
        store.sync_plan(&bot.id, 329).unwrap(),
        SyncPlan::Fetch {
            offset: 327,
            limit: 2
        }
    );

    store.clear_trades(&bot.id).unwrap();
    assert_eq!(store.closed_trade_count(&bot.id).unwrap(), 0);
}

// ---------------------------------------------------------------------------
// Snapshots and candles
// ---------------------------------------------------------------------------

#[test]
fn snapshots_round_trip_with_a_fetch_time() {
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    assert!(store
        .snapshot::<BotConfig>(&bot.id, kind::CONFIG)
        .unwrap()
        .is_none());

    let config = BotConfig {
        exchange: "binance".to_owned(),
        dry_run: true,
        ..Default::default()
    };
    store.put_snapshot(&bot.id, kind::CONFIG, &config).unwrap();

    let cached = store
        .snapshot::<BotConfig>(&bot.id, kind::CONFIG)
        .unwrap()
        .unwrap();
    assert_eq!(cached.value.exchange, "binance");
    assert!(cached.age() < time::Duration::seconds(5));
    assert!(store.last_synced(&bot.id).unwrap().is_some());
}

#[test]
fn a_snapshot_is_replaced_not_appended() {
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    for total in [1.0, 2.0, 3.0] {
        store
            .put_snapshot(
                &bot.id,
                kind::BALANCE,
                &Balance {
                    total,
                    ..Default::default()
                },
            )
            .unwrap();
    }
    let cached = store
        .snapshot::<Balance>(&bot.id, kind::BALANCE)
        .unwrap()
        .unwrap();
    assert_eq!(cached.value.total, 3.0);
}

#[test]
fn candles_return_the_most_recent_window_oldest_first() {
    let store = store();
    let bot = store.add_bot(new_bot("alpha")).unwrap();
    let bars: Vec<_> = (0..200)
        .map(|i| Candle {
            time: 1_000_000 + i * 7_200_000, // 2h apart, matching the live bot
            close: i as f64,
            ..Default::default()
        })
        .collect();
    store
        .put_candles(&bot.id, "ETH/USDT:USDT", "2h", &bars)
        .unwrap();

    let window = store.candles(&bot.id, "ETH/USDT:USDT", "2h", 100).unwrap();
    assert_eq!(window.len(), 100);
    assert_eq!(window[0].close, 100.0, "should start at the 101st bar");
    assert_eq!(window[99].close, 199.0);
    assert!(window.windows(2).all(|w| w[0].time < w[1].time));

    // Re-inserting overlapping candles must not duplicate them.
    store
        .put_candles(&bot.id, "ETH/USDT:USDT", "2h", &bars)
        .unwrap();
    assert_eq!(
        store
            .candles(&bot.id, "ETH/USDT:USDT", "2h", 500)
            .unwrap()
            .len(),
        200
    );
}

#[test]
fn settings_round_trip() {
    let store = store();
    assert_eq!(store.setting("active_bot").unwrap(), None);
    store.set_setting("active_bot", "abc").unwrap();
    store.set_setting("active_bot", "def").unwrap();
    assert_eq!(store.setting("active_bot").unwrap().as_deref(), Some("def"));
}

#[test]
fn the_database_survives_being_reopened() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ft.db");
    let key = StaticKey::random().unwrap();

    let bot_id = {
        let store = Store::open(&path, &key).unwrap();
        let bot = store.add_bot(new_bot("alpha")).unwrap();
        store
            .upsert_trades(&bot.id, &[closed(1, 1000, 5.0)])
            .unwrap();
        bot.id
    };

    let store = Store::open(&path, &key).unwrap();
    assert_eq!(store.list_bots().unwrap().len(), 1);
    assert_eq!(store.closed_trade_count(&bot_id).unwrap(), 1);
    assert_eq!(store.password(&bot_id).unwrap(), "alpha-secret");
}

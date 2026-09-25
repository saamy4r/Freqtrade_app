# Legacy Flutter app — extracted contract

Reference notes captured from the Flutter implementation (v1.2.0, 17 Dart files, ~3.2k LOC)
before it was removed on the `rust-rewrite` branch. The original code remains in git history
on `main` and in commits prior to the M0 deletion commit.

Everything here is the behaviour the Rust rewrite must reproduce.

---

## 1. Freqtrade REST contract

Source: `lib/services/api_service.dart`. `baseUrl` always already ends in `/api/v1`
(normalized at bot-creation time: trailing `/` stripped, `/api/v1` appended if missing).

Timeouts: 10s for everything except `/ping` at 3s.

| Call | Method | Path | Fields consumed |
|---|---|---|---|
| ping | GET | `/ping` | `status == "pong"`. **Unauthenticated.** Drives the per-bot online dot. |
| login | POST | `/token/login` | Sends `Authorization: Basic base64(user:pass)`. Reads `access_token`. **`refresh_token` was ignored — this is the bug to fix.** |
| showConfig | GET | `/show_config` | `dry_run`, `strategy`, `exchange`, `stoploss_on_exchange`, `trading_mode`, `stoploss`, `timeframe`, `max_open_trades`, `stake_amount`, `short_allowed` |
| getOpenTrades | GET | `/status` | Top-level JSON **array**. Per item: `trade_id`, `pair`, `is_short`, `profit_ratio`, `profit_abs`, `stake_amount`, `open_rate`, `current_rate`, `open_date` |
| getClosedTrades | GET | `/trades?limit={limit}&offset={offset}` | `data["trades"]` array. Per item: `pair`, `is_short`, `profit_ratio`, `profit_abs`, `stake_amount`, `open_rate`, `close_rate`, `open_date`, `close_date`. Callers passed limit=500 (Closed, Dashboard) and 200 (Chart). |
| getProfitSummary | GET | `/profit` | `profit_closed_coin`, `profit_closed_percent`, `profit_closed_percent_mean`, `closed_trade_count`, `avg_duration`, `best_pair`, `trading_volume`, `stake_currency`, `starting_capital` |
| getBalance | GET | `/balance` | `total`, `stake_currency`, `currencies[]` each with `currency`, `free`, `used`, `is_position` |
| forceExit | POST | `/forceexit` | Body `{"tradeid": <id>, "ordertype": "limit"}`. Response unused beyond HTTP 200. |
| getLogs | GET | `/logs?limit=500` | `data["logs"]` — **array of arrays**. Index 0 = timestamp string, index 3 = level, index 4 = message. |
| getWhitelist | GET | `/whitelist` | `data["whitelist"]` → list of pair strings |
| getPairCandles | GET | `/pair_candles?pair={urlencoded}&timeframe={tf}&limit=300` | `columns` (name→index lookup for `date`, `close`) + `data` rows |

Auth header on every authenticated call: `Authorization: Bearer <access_token>`.
Non-200 raised a generic `Exception`, surfaced as a full-screen error view.

### Per-screen fan-out (what the rewrite replaces with one aggregated call each)

- Open Trades: `Future.wait([/status, /balance])`
- Closed Trades: `Future.wait([/profit, /trades?limit=500, /balance])`
- Dashboard: `Future.wait([/profit, /trades?limit=500, /show_config, /balance])`
- Chart: `Future.wait([/status, /trades?limit=200, /show_config, /whitelist])` then `/pair_candles`

---

## 2. Screens

Navigation: `BottomNavigationBar` + `IndexedStack`, 6 tabs, each screen
`AutomaticKeepAliveClientMixin` and re-fetching when `botId`/`isOffline` changed.

### Shell (`app_shell.dart`)
AppBar: active bot name, status badge — `OFFLINE` amber / `DRY` green / `LIVE` blue
(derived from `config["dry_run"]`), connecting spinner, light/dark toggle.
Renders the Bots screen full-screen when no bot is active.

### Tab 0 — Open Trades (`open_trades_screen.dart`, icon `open_in_browser`)
Three summary cards:
- **Total Portfolio Value** = `balance.total + Σ trade.profit_abs`
- **Free / Staked** = `free` / `used` from the balance currency entry where `is_position == false`
- **Total Open P/L** = `Σ profit_abs`, green/red

Then open-trade cards sorted by `open_date` **descending**: pair, LONG/SHORT chip from
`is_short`, profit % with trend icon, card tinted green/red/grey by `profit_ratio`, a row of
Stake Amount / Open Price / Current Price, open date, and an **Exit** button (hidden offline).

Force-exit dialog: shows current price, current P/L in % and absolute + currency, the note
"A limit order will be placed at the current bid price", and a **Limit Exit** button calling
`forceExit(tradeId, "limit")`.

### Tab 1 — Closed Trades (`closed_trades_screen.dart`, icon `history`)
Cards: **Current Portfolio Value** (`balance.total`), **Total Closed Profit**
(`profit.profit_closed_coin`). Then trades sorted by `close_date` **descending**: pair,
direction chip, profit %, trend icon, colored card, Stake Amount / Open Price / Close Price,
Opened/Closed timestamps. Empty state: "No closed trades found."

### Tab 2 — Dashboard (`dashboard_screen.dart`, icon `dashboard`)
Background `AnimatedContainer` tinted green/red by overall profit.

Cumulative profit chart — original algorithm (`_prepareChartData`, lines 109-147):
sort closed trades by `close_date`, accumulate `profit_abs`; if `starting_capital > 0` plot
**percent of starting capital**, otherwise plot **absolute profit in stake currency**.
Rendered as a step line (two spots per trade), blue with gradient fill, x-axis `d/M`,
tooltip `d MMM yyyy`. Title: "Cumulative Profit (N trades)" + "Overall Profit: X%".

Performance stat tiles: Avg Profit/Trade (`profit_closed_percent_mean`), Total Closed Trades
(`closed_trade_count`), Avg Duration, Best Pair, Trading Volume, Free Balance, Strategy.

Configuration tiles: Exchange, Stoploss On Exchange, Trading Mode, Timeframe, Stake Amount,
Max Open Trades, Stoploss (**`-1.0` renders as "Disabled"**), Shorting Allowed.

### Tab 3 — Chart (`chart_screen.dart`, icon `candlestick_chart`)
Dark canvas `0xFF1E1E1E`. Top row: pair dropdown (whitelist pairs **plus** any open-trade pair
not in the whitelist, each showing that trade's profit % badge) and a refresh button.

Close-price line from `/pair_candles`, windowed to **100 candles** (`_viewCount`) with
horizontal drag-to-pan via `_viewOffset`. Y-axis padded ±0.2%. Tooltip: price + `d MMM HH:mm`.

Trade markers (`_buildTradeBars`, lines 258-311): closed trades draw an entry→exit line colored
green/red by `profit_ratio`, with a **triangle at entry** (pointing down when `is_short`) and a
**circle at exit**; open trades draw a green triangle at entry only.

Legend: Price / Long Entry / Short Entry / Win Exit / Loss Exit.
Offline: "Chart unavailable offline" placeholder.

Note: despite the icon, this was never a candlestick chart — close price line only.

### Tab 4 — Logs (`logs_screen.dart`, icon `terminal`)
Terminal look, `0xFF1E1E1E`, monospace. Header: green terminal icon, "N log entries", refresh,
scroll-to-bottom. Rows: `HH:mm:ss` (taken as **chars 11-19 of the timestamp string**), a bordered
level badge, the message.

Level colors: ERROR/CRITICAL red, WARNING amber, DEBUG grey, else blue.
ERROR and WARNING rows additionally get a tinted row background.
Auto-scrolls to bottom after load. Offline: "Logs unavailable offline".

### Tab 5 — Bots (`bots_screen.dart`)
`ReorderableListView` of bot cards: drag handle, green/red/grey online dot (parallel `ping` of
every bot on mount), bot icon highlighted for the active bot, name, URL subtitle, delete button,
tap-to-switch, highlighted tile for the active bot. Footer "Add Another Bot".
Empty state: "No bots found." + "Add Your First Bot".

Delete confirmation is a red-themed dialog that warns local cached data is also deleted.

**Login screen** (same file, pushed as a route): Bot Name, URL (hint `http://192.168.1.10:8080`),
Username, Password (obscured). Validation **performs a real login** before saving, and
distinguishes `TimeoutException`, `SocketException`, and auth failure in its error messages.

---

## 3. Persistence (all being replaced by SQLite)

- `bot_storage.dart` — `shared_preferences` key `bots_list`, a `List<String>` of JSON-encoded bots. List order **is** display order.
- `theme_storage.dart` — key `theme_mode`, values `light` / `dark` / `system`.
- Active bot id — key `active_bot_id`, written only on explicit user selection so programmatic selection does not overwrite it; removed when the active bot is deleted.
- `bot_cache_service.dart` — `getApplicationDocumentsDirectory()/bot_cache/<botId>.json`, shallow-merged partial writes stamped with ISO8601 `lastSynced`. Cached keys: `config`, `openTrades`, `closedTrades`, `profit`, `balance`, `lastSynced`.
- `models/bot.dart` — `Bot { id: uuid-v4, name, url, username, password }`. **Password stored in plaintext.**

### Offline behaviour
If login or `/show_config` fails on bot selection, the shell falls back to cached config and sets
`_isOffline = true`; screens then read the cache instead of the network. `OfflineBanner` shows
"Offline · Last synced Xm ago". Chart and Logs are simply disabled offline.

---

## 4. Known defects the rewrite must fix

1. **No token refresh.** `refresh_token` discarded; no 401 retry. Freqtrade JWTs expire in ~15 min, after which every screen shows the error view until the user switches bots.
2. **No caching of significance.** Every tab mount / bot switch re-fetches; `/trades?limit=500` fetched by three separate screens.
3. **Plaintext credentials** in SharedPreferences.
4. **No polling.** No `Timer`, no `Stream.periodic`, no WebSocket. Refresh is entirely manual.
5. **Cleartext HTTP likely blocked.** `AndroidManifest.xml` declared only `INTERNET`, with no `usesCleartextTraffic` and no `network-security-config`, so `http://192.168.x.x:8080` bots hit Android 9+ defaults.
6. **Unvirtualized log list** — 500 rows built at once.
7. **Untyped models** — every field read by string key at the widget layer.

---

## 5. Dependencies being dropped

Runtime: `http 1.4.0`, `fl_chart 0.70.0`, `intl 0.20.2`, `shared_preferences 2.2.2`,
`uuid 4.2.1`, `path_provider 2.1.5`, `cupertino_icons ^1.0.8`.
Dev: `flutter_test`, `flutter_launcher_icons ^0.14.4`, `flutter_lints ^5.0.0`.

App icon source: `assets/icon/icon.png` (min sdk 21).

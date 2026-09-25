# Freqtrade Visualizer

Monitor your [Freqtrade](https://www.freqtrade.io/) bots from your phone or desktop.
Open trades, closed trades, cumulative profit, price charts with your own entries
and exits drawn on them, and the bot's log — for as many bots as you run.

Written in Rust. Version 2 is a complete rewrite of the original Flutter app.

| | Android | Linux |
|---|---|---|
| **v2 (Rust)** | **5.7 MB** | **3.3 MB** |
| v1 (Flutter) | 50 MB | 19 MB |

## Download

| Platform | |
|---|---|
| Android (arm64) | [![APK](https://img.shields.io/badge/Download-APK-brightgreen?style=for-the-badge&logo=android)](https://github.com/saamy4r/Freqtrade_app/releases/latest) |
| Linux (x86-64) | [![Linux](https://img.shields.io/badge/Download-Linux_x64-blue?style=for-the-badge&logo=linux)](https://github.com/saamy4r/Freqtrade_app/releases/latest) |

**Android** — enable *Install from unknown sources*, then open the APK.

**Linux** — extract and run:

```bash
tar -xzf freqtrade-visualizer-v2.0.0-linux-x64.tar.gz
./freqtrade-visualizer/freqtrade
```

You need a Freqtrade instance with the REST API enabled (`api_server` in your
config), reachable from the device.

## What it does

- **Open trades** — portfolio value, free and staked balance, live P/L, and each
  position tinted by outcome. Force-exit places a limit order at the current bid,
  after showing you the P/L you would realise.
- **Closed trades** — realised profit and full history, newest first.
- **Dashboard** — cumulative profit chart, win rate, profit factor, average
  duration, drawdown, and the bot's configuration.
- **Chart** — price history with your entries and exits drawn on it: triangles for
  entries pointing up for longs and down for shorts, dots for exits coloured by
  outcome, joined by a line.
- **Logs** — the bot's own output, with levels highlighted.
- **Multiple bots** — add as many as you like, reorder by dragging, switch from
  the header.

### Offline

Everything is kept on the device. Stop a bot, or lose signal, and the app still
shows its trades, performance and charts from local storage, with a banner
saying how stale they are. It reconnects on its own when the bot returns.

### Live

Values update by themselves while the app is open — no pull-to-refresh needed,
though it is there. The app only polls while you are looking at it, so it is not
waking your radio in the background.

## How it works

The app is one binary containing three parts:

```
UI (Dioxus)  ──HTTP──▶  server (Axum)  ──▶  SQLite
                             │
                             └──HTTP──▶  your Freqtrade bots
```

The server runs inside the app on loopback. It talks to your bots, keeps
everything in a local SQLite database, and hands the UI one prepared payload per
screen. That is why it stays quick: switching tabs reads from the database
rather than re-asking the bot, and only trades that are actually new are fetched.

Credentials are encrypted with AES-256-GCM. On Android the key is wrapped by a
non-exportable key in the platform Keystore, so only ciphertext reaches disk, and
backups are disabled so nothing leaves the device.

## Build from source

Requires Rust (stable) and [`dx`](https://dioxuslabs.com/learn/0.7/getting_started):

```bash
cargo install dioxus-cli@0.7.10 --locked
```

```bash
scripts/dev.sh              # run locally in a browser
scripts/build-apk.sh --release   # Android — see docs/android-notes.md for the SDK
dx build --package freqtrade --platform desktop --release
```

Tests:

```bash
cargo test --workspace
```

`scripts/mock-bot.py` is a fake Freqtrade with synthetic trades, so the app can
be exercised without pointing it at a real instance.

## Security

The app talks to your bot over whatever you configure. If your bot is exposed on
a public address over plain HTTP, your API password crosses the internet in
base64 — put it behind TLS or a VPN, or keep it on your LAN.

## License

MIT — see [LICENSE](LICENSE).

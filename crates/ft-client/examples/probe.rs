//! Asks a live bot for every endpoint and reports what came back.
//!
//!     cargo run -p ft-client --example probe -- http://192.168.1.10:8080 user pass
//!
//! The mock-backed tests prove the client behaves correctly against the schema
//! we believe in; this proves the schema. Run it against a real bot whenever a
//! Freqtrade upgrade lands, or when a screen starts showing zeroes.

use std::time::Instant;

use ft_client::{ClientError, FreqtradeClient};

/// Prints `label`, then either a one-line summary or the error.
fn report<T>(
    label: &str,
    started: Instant,
    result: &Result<T, ClientError>,
    summary: impl Fn(&T) -> String,
) {
    let ms = started.elapsed().as_millis();
    match result {
        Ok(value) => println!("  {label:<16} ok   {:>5}ms  {}", ms, summary(value)),
        Err(e) => println!("  {label:<16} FAIL {:>5}ms  {e}", ms),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ft_client=debug".into()),
        )
        .with_target(false)
        .init();

    let mut args = std::env::args().skip(1);
    let (Some(url), Some(user), Some(pass)) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: probe <bot-url> <username> <password>");
        eprintln!("  e.g. probe http://192.168.1.10:8080 freqtrader mypassword");
        std::process::exit(64);
    };

    let client = FreqtradeClient::new(&url, &user, &pass)?;
    println!("probing {}\n", client.base_url());

    // /ping first: unauthenticated, so it separates "unreachable" from "wrong
    // password" before credentials are even tried.
    let t = Instant::now();
    match FreqtradeClient::ping(&url).await {
        Ok(true) => println!(
            "  {:<16} ok   {:>5}ms  pong",
            "ping",
            t.elapsed().as_millis()
        ),
        Ok(false) => {
            println!(
                "  {:<16} FAIL              something answered, but it is not Freqtrade",
                "ping"
            );
            std::process::exit(1);
        }
        Err(e) => {
            println!("  {:<16} FAIL              {e}", "ping");
            std::process::exit(1);
        }
    }

    let t = Instant::now();
    if let Err(e) = client.login().await {
        println!(
            "  {:<16} FAIL {:>5}ms  {e}",
            "login",
            t.elapsed().as_millis()
        );
        std::process::exit(1);
    }
    println!("  {:<16} ok   {:>5}ms", "login", t.elapsed().as_millis());

    // Prove the refresh path works against this bot rather than only against
    // wiremock. A bot that issues no refresh token fails loudly here.
    let t = Instant::now();
    let refreshed = client.refresh_now().await;
    report("token/refresh", t, &refreshed, |_| {
        "refreshed; the 15-minute dead-end is gone".to_owned()
    });

    let t = Instant::now();
    let config = client.show_config().await;
    report("show_config", t, &config, |c| {
        format!(
            "{} on {} ({}), {} mode, tf {}",
            c.strategy.as_deref().unwrap_or("?"),
            c.exchange,
            if c.dry_run { "dry" } else { "LIVE" },
            c.trading_mode,
            c.timeframe.as_deref().unwrap_or("?"),
        )
    });

    let t = Instant::now();
    let open = client.open_trades().await;
    report("status", t, &open, |t| format!("{} open", t.len()));

    let t = Instant::now();
    let closed = client.closed_trades(50, 0).await;
    report("trades", t, &closed, |r| {
        format!("{} of {} total", r.trades.len(), r.total_trades)
    });

    let t = Instant::now();
    let profit = client.profit().await;
    report("profit", t, &profit, |p| {
        format!(
            "{} closed, {:.4} profit, best {}",
            p.closed_trade_count,
            p.profit_closed_coin,
            p.best_pair.as_deref().unwrap_or("-")
        )
    });

    let t = Instant::now();
    let balance = client.balance().await;
    report("balance", t, &balance, |b| {
        format!(
            "total {:.2} {}, {} rows",
            b.total,
            b.stake,
            b.currencies.len()
        )
    });

    let t = Instant::now();
    let logs = client.logs(50).await;
    report("logs", t, &logs, |l| format!("{} entries", l.logs.len()));

    let t = Instant::now();
    let whitelist = client.whitelist().await;
    report("whitelist", t, &whitelist, |w| {
        format!("{} pairs", w.whitelist.len())
    });

    // Candles need a pair and the bot's own timeframe, so derive both.
    let pair = whitelist
        .as_ref()
        .ok()
        .and_then(|w| w.whitelist.first().cloned())
        .or_else(|| {
            open.as_ref()
                .ok()
                .and_then(|t| t.first().map(|t| t.pair.clone()))
        });
    let timeframe = config
        .as_ref()
        .ok()
        .and_then(|c| c.timeframe.clone())
        .unwrap_or_else(|| "5m".to_owned());

    if let Some(pair) = pair {
        let t = Instant::now();
        let candles = client.pair_candles(&pair, &timeframe, 100).await;
        report("pair_candles", t, &candles, |c| {
            let bars = c.candles();
            format!(
                "{pair} @ {timeframe}: {} of {} rows projected, {} columns",
                bars.len(),
                c.data.len(),
                c.columns.len(),
            )
        });
        // A gap here means a date column we did not understand.
        if let Ok(c) = &candles {
            if c.candles().len() != c.data.len() {
                println!(
                    "\n  WARNING: {} rows dropped; columns were {:?}",
                    c.data.len() - c.candles().len(),
                    c.columns,
                );
            }
        }
    } else {
        println!(
            "  {:<16} skip              empty whitelist and no open trades",
            "pair_candles"
        );
    }

    println!("\n/forceexit not probed: it places a real order.");
    Ok(())
}

//! Standalone server, for desktop development.
//!
//!     cargo run -p ft-server -- --dev
//!
//! `--dev` enables permissive CORS so `dx serve`'s dev server, which runs on a
//! different origin, can reach the API. The shipped Android build serves the UI
//! from this same origin and does not need it.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use ft_store::{FileKey, Store};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ft_server=debug,ft_client=debug,tower_http=info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let dev = args.iter().any(|a| a == "--dev");
    let port: u16 = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let db_path = args
        .iter()
        .position(|a| a == "--db")
        .and_then(|i| args.get(i + 1).cloned())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(default_db_path);

    tracing::info!(path = %db_path.display(), "opening store");
    let store = Arc::new(Store::open(&db_path, &FileKey::beside(&db_path))?);

    // Serving the UI from this same origin is how the app ships; doing it in
    // development too removes a class of differences between the two.
    let ui_dir = args
        .iter()
        .position(|a| a == "--ui")
        .and_then(|i| args.get(i + 1).cloned())
        .map(std::path::PathBuf::from);

    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    let running = ft_server::spawn_on_with_ui(store, addr, dev, ui_dir).await?;
    if dev {
        tracing::warn!("--dev: CORS is permissive; do not expose this beyond localhost");
    }
    println!("ft-server on http://localhost:{}/api", running.port);

    running.handle.await?;
    Ok(())
}

/// Follows the XDG data directory, falling back to the working directory.
fn default_db_path() -> std::path::PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })
        .map(|base| base.join("freqtrade-visualizer/ft.db"))
        .unwrap_or_else(|| std::path::PathBuf::from("ft.db"))
}

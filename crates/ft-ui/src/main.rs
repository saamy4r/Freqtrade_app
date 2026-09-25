//! Entry point.
//!
//! Two shapes. On the web the UI talks to a standalone `ft-server` on the same
//! origin. In a packaged app — Android, or a desktop build — the same server
//! runs in-process on loopback and the UI is pointed at the port it chose.

fn main() {
    // First, so that anything failing after this point says so.
    ft_ui::logging::init();

    #[cfg(feature = "embedded-server")]
    start_embedded();

    #[cfg(any(feature = "web", feature = "desktop", feature = "mobile"))]
    dioxus::launch(ft_ui::App);

    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
    {
        eprintln!(
            "ft-ui was built without a renderer.\n\
             Development:  dx serve --package ft-ui --platform web\n\
             Android:      dx serve --package ft-ui --platform android"
        );
        std::process::exit(64);
    }
}

/// Brings up the in-process server and points the UI at it.
///
/// A failure here is fatal by design: without the server there is no data, no
/// bot list and nothing to render, so failing loudly beats an app that opens
/// to a permanently empty screen with no explanation.
#[cfg(feature = "embedded-server")]
fn start_embedded() {
    let data_dir = ft_ui::backend::data_dir().unwrap_or_else(|e| {
        panic!("could not find a writable directory for the database: {e}");
    });
    let db_path = data_dir.join("ft.db");
    tracing::info!(path = %db_path.display(), "opening database");

    let port = ft_ui::backend::start(db_path)
        .unwrap_or_else(|e| panic!("could not start the embedded server: {e}"));
    tracing::info!(port, "embedded server listening");

    // The port is chosen by the OS at bind time, so this cannot be a constant
    // and must be set before anything renders.
    ft_ui::api::set_base_url(format!("http://127.0.0.1:{port}/api"));
}

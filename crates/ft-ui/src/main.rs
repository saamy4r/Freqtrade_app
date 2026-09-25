//! Entry point.
//!
//! Two shapes. On the web the UI talks to a standalone `ft-server` on the same
//! origin. In a packaged app — Android, or a desktop build — the same server
//! runs in-process on loopback and the UI is pointed at the port it chose.

fn main() {
    #[cfg(feature = "embedded-server")]
    start_embedded();

    #[cfg(not(feature = "embedded-server"))]
    ft_ui::logging::init(std::path::Path::new("."));

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
/// Failures here are reported rather than swallowed: without the server there
/// is no data at all, and a blank screen with no explanation is the worst
/// outcome. The panic recorder is installed first so that anything failing
/// after this line leaves a note for the next launch to display.
#[cfg(feature = "embedded-server")]
fn start_embedded() {
    let data_dir = match ft_ui::backend::data_dir() {
        Ok(dir) => dir,
        Err(e) => {
            // Nowhere to write a crash note either, so this is as loud as it
            // gets. Panicking makes the failure visible rather than leaving a
            // window that closes itself.
            ft_ui::logging::init(std::path::Path::new("."));
            panic!("could not find a writable directory for the database: {e}");
        }
    };
    let _ = std::fs::create_dir_all(&data_dir);
    ft_ui::logging::init(&data_dir);

    let db_path = data_dir.join("ft.db");
    tracing::info!(path = %db_path.display(), "opening database");

    match ft_ui::backend::start(db_path) {
        Ok(port) => {
            tracing::info!(port, "embedded server listening");
            ft_ui::api::set_base_url(format!("http://127.0.0.1:{port}/api"));
        }
        // Recorded and rendered by the UI rather than aborting, so the reason
        // reaches the screen instead of disappearing with the process.
        Err(e) => ft_ui::startup_failed(format!("the embedded server did not start: {e}")),
    }
}

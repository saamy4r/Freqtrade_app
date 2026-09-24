//! Entry point.
//!
//! On web the API base is baked in at build time (`FT_API_BASE`), defaulting to
//! the standalone dev server. The Android build will instead call
//! `api::set_base_url` with the loopback port its embedded server chose.

fn main() {
    #[cfg(feature = "web")]
    dioxus::launch(ft_ui::App);

    #[cfg(all(not(feature = "web"), feature = "desktop"))]
    dioxus::launch(ft_ui::App);

    #[cfg(all(not(feature = "web"), not(feature = "desktop"), feature = "mobile"))]
    dioxus::launch(ft_ui::App);

    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
    {
        eprintln!(
            "ft-ui was built without a renderer.\n\
             Development:  dx serve --package ft-ui --features web\n\
             Android (M11): dx serve --package ft-ui --features mobile --platform android"
        );
        std::process::exit(64);
    }
}

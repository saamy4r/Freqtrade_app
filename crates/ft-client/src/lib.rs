//! HTTP client for the Freqtrade REST API.
//!
//! The headline difference from the Flutter implementation it replaces is that
//! this one can stay logged in. The old client stored the access token from
//! `/token/login` and threw away the refresh token, with no 401 handling, so
//! roughly fifteen minutes after opening the app every screen showed an error
//! and the only way out was switching bots to force a fresh login.
//!
//! Here a 401 triggers a refresh and a single retry, refreshes are
//! single-flight so a screen's parallel requests cost one refresh rather than
//! four, and an expired *refresh* token falls back to a full re-login instead
//! of surfacing an error.
//!
//! ```no_run
//! # async fn example() -> Result<(), ft_client::ClientError> {
//! let client = ft_client::FreqtradeClient::new("192.168.1.10:8080", "user", "pass")?;
//! client.login().await?;
//! let config = client.show_config().await?;
//! println!("{} on {}", config.strategy.unwrap_or_default(), config.exchange);
//! # Ok(())
//! # }
//! ```

mod auth;
mod client;
mod error;
mod url;

pub use client::{install_crypto_provider, FreqtradeClient, DEFAULT_TIMEOUT, PING_TIMEOUT};
pub use error::{ClientError, Result};
pub use url::{normalize_base_url, API_PREFIX};

//! Bot URL normalization.
//!
//! Users type `192.168.1.10:8080`, `http://box:8080/`, or the full
//! `http://box:8080/api/v1`. The Flutter app normalized this inline in the
//! login screen (strip a trailing slash, append `/api/v1` if absent) and stored
//! the result, which meant a typo was only discovered at first request. Doing
//! it here lets the add-bot flow validate before saving.

use crate::error::{ClientError, Result};

/// The API prefix every Freqtrade endpoint lives under.
pub const API_PREFIX: &str = "/api/v1";

/// Normalizes a user-typed bot address into a base URL ending in `/api/v1`.
///
/// Adds a scheme when none was typed (`http`, since bots usually sit on a LAN),
/// strips trailing slashes, and appends the API prefix unless it is already
/// there. Rejects anything that is not a usable absolute HTTP(S) URL.
pub fn normalize_base_url(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ClientError::InvalidUrl {
            url: input.to_owned(),
            reason: "no address given".to_owned(),
        });
    }

    let invalid = |reason: &str| ClientError::InvalidUrl {
        url: input.to_owned(),
        reason: reason.to_owned(),
    };

    // A bare `host:port` has no scheme; assume http rather than rejecting it.
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    };

    let parsed = reqwest::Url::parse(&with_scheme).map_err(|e| invalid(&e.to_string()))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(invalid(&format!("unsupported scheme {other:?}"))),
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err(invalid("no host"));
    }

    // Keep any path prefix (bots behind a reverse proxy sit under a subpath),
    // but do not stack a second /api/v1 onto one that is already there.
    let base = with_scheme.trim_end_matches('/');
    if base.ends_with(API_PREFIX) {
        Ok(base.to_owned())
    } else {
        Ok(format!("{base}{API_PREFIX}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_the_api_prefix() {
        for input in [
            "http://192.168.1.10:8080",
            "http://192.168.1.10:8080/",
            "http://192.168.1.10:8080///",
        ] {
            assert_eq!(
                normalize_base_url(input).unwrap(),
                "http://192.168.1.10:8080/api/v1",
                "input {input:?}"
            );
        }
    }

    #[test]
    fn leaves_an_existing_prefix_alone() {
        assert_eq!(
            normalize_base_url("http://box:8080/api/v1").unwrap(),
            "http://box:8080/api/v1"
        );
        assert_eq!(
            normalize_base_url("http://box:8080/api/v1/").unwrap(),
            "http://box:8080/api/v1"
        );
    }

    #[test]
    fn assumes_http_when_no_scheme_was_typed() {
        // Bots usually sit on a LAN, so http is the useful default; demanding a
        // scheme just makes the add-bot form annoying.
        assert_eq!(
            normalize_base_url("192.168.1.10:8080").unwrap(),
            "http://192.168.1.10:8080/api/v1"
        );
        assert_eq!(
            normalize_base_url("  my-vps.example.com  ").unwrap(),
            "http://my-vps.example.com/api/v1"
        );
    }

    #[test]
    fn keeps_https_and_reverse_proxy_subpaths() {
        assert_eq!(
            normalize_base_url("https://bots.example.com/freqtrade").unwrap(),
            "https://bots.example.com/freqtrade/api/v1"
        );
    }

    #[test]
    fn rejects_what_cannot_work() {
        for bad in ["", "   ", "ftp://box:8080", "http://"] {
            assert!(
                normalize_base_url(bad).is_err(),
                "{bad:?} should have been rejected"
            );
        }
    }
}

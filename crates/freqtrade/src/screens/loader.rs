//! Shared helpers for screens that load one aggregated payload.

use time::OffsetDateTime;

use crate::format;

/// The current time in epoch milliseconds.
///
/// Deliberately not `OffsetDateTime::now_utc()`: on `wasm32-unknown-unknown`
/// the `time` crate has no clock unless its `wasm-bindgen` feature is enabled,
/// and calling it **panics**. That panic aborted the render part-way through,
/// so the screen kept whatever it had shown before — a spinner that never
/// resolved, with no console error and a perfectly healthy network response.
/// It only ever fired on the offline path, because that is the only branch
/// that asks what time it is.
#[cfg(target_arch = "wasm32")]
fn now_millis() -> i64 {
    js_sys::Date::now() as i64
}

#[cfg(not(target_arch = "wasm32"))]
fn now_millis() -> i64 {
    (OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000) as i64
}

/// Formats a sync time for the offline banner, e.g. "3m ago".
pub fn synced_label(at: Option<OffsetDateTime>) -> Option<String> {
    let now = now_millis();
    at.map(|t| {
        let then = (t.unix_timestamp_nanos() / 1_000_000) as i64;
        format::ago(((now - then) / 1000).max(0))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recent_sync_reads_as_just_now() {
        let now = OffsetDateTime::from_unix_timestamp(now_millis() / 1000).unwrap();
        assert_eq!(synced_label(Some(now)).as_deref(), Some("just now"));
        assert_eq!(synced_label(None), None);
    }

    #[test]
    fn an_older_sync_reads_in_minutes() {
        let then = OffsetDateTime::from_unix_timestamp(now_millis() / 1000 - 600).unwrap();
        assert_eq!(synced_label(Some(then)).as_deref(), Some("10m ago"));
    }
}

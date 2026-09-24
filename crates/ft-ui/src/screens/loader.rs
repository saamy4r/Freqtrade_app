//! Shared helpers for screens that load one aggregated payload.

use time::OffsetDateTime;

use crate::format;

/// Formats a sync time for the offline banner, e.g. "3m ago".
pub fn synced_label(at: Option<OffsetDateTime>) -> Option<String> {
    at.map(|t| format::ago((OffsetDateTime::now_utc() - t).whole_seconds().max(0)))
}

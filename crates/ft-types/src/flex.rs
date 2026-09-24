//! Lenient deserialization helpers.
//!
//! Freqtrade's responses vary across versions and several fields are nullable in
//! practice (`current_rate` on a trade the exchange has not priced yet,
//! `close_rate` on an open trade, `best_pair` on a bot with no closed trades).
//! The Flutter app read these straight out of a `Map<String, dynamic>` and
//! crashed or rendered `null` when they were missing. These helpers let us keep
//! strong types without a version bump turning into a hard parse error.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use time::{format_description::BorrowedFormatItem, macros::format_description, OffsetDateTime};

/// Accepts an explicit `null` where a concrete value is expected and yields
/// `T::default()`.
///
/// `#[serde(default)]` alone only covers an *absent* key; a present `null`
/// still fails. Pair the two: `#[serde(default, deserialize_with = "null_to_default")]`.
pub fn null_to_default<'de, D, T>(de: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(de)?.unwrap_or_default())
}

/// A stake amount, which Freqtrade reports either as a number or as the literal
/// string `"unlimited"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StakeAmount {
    /// A concrete amount in the stake currency. Listed first so JSON numbers
    /// bind here rather than falling through to the string arm.
    Amount(f64),
    Text(String),
}

impl Default for StakeAmount {
    fn default() -> Self {
        Self::Text("unlimited".to_owned())
    }
}

impl StakeAmount {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Amount(v) => Some(*v),
            Self::Text(s) => s.parse().ok(),
        }
    }

    pub fn is_unlimited(&self) -> bool {
        matches!(self, Self::Text(s) if s.eq_ignore_ascii_case("unlimited"))
    }
}

impl std::fmt::Display for StakeAmount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Amount(v) => write!(f, "{v}"),
            Self::Text(s) => f.write_str(s),
        }
    }
}

/// `2024-01-01 12:00:00`, the shape Freqtrade uses for `open_date` / `close_date`.
const NAIVE: &[BorrowedFormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

/// Epoch milliseconds below this are far more plausible as epoch *seconds*.
/// 1e11 ms is 1973; 1e11 s is the year 5138.
const MILLIS_THRESHOLD: f64 = 1e11;

/// Best-effort timestamp parse across every shape Freqtrade has used.
///
/// Accepts epoch milliseconds (the `*_timestamp` fields, and what we prefer),
/// epoch seconds (the log tuple's second element), RFC 3339, and the naive
/// `YYYY-MM-DD HH:MM:SS` form with an optional `,mmm` or `.mmm` fraction.
/// Naive values are read as UTC, which is what Freqtrade emits.
pub fn parse_timestamp(value: &Value) -> Option<OffsetDateTime> {
    match value {
        Value::Number(n) => {
            let raw = n.as_f64()?;
            let nanos = if raw.abs() >= MILLIS_THRESHOLD {
                raw * 1e6
            } else {
                raw * 1e9
            };
            OffsetDateTime::from_unix_timestamp_nanos(nanos as i128).ok()
        }
        Value::String(s) => parse_timestamp_str(s),
        _ => None,
    }
}

/// String half of [`parse_timestamp`].
pub fn parse_timestamp_str(s: &str) -> Option<OffsetDateTime> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339) {
        return Some(dt);
    }
    // Drop a sub-second fraction (`,123` from the log formatter, `.123` from ISO)
    // and any trailing zone marker, then read the remainder as naive UTC.
    let trimmed = s.trim_end_matches('Z');
    let head = trimmed
        .split_once([',', '.'])
        .map_or(trimmed, |(head, _)| head);
    let head = head.replace('T', " ");
    time::PrimitiveDateTime::parse(&head, NAIVE)
        .ok()
        .map(|dt| dt.assume_utc())
}

/// Serde adapter for a nullable timestamp field of any accepted shape.
pub fn de_timestamp_opt<'de, D>(de: D) -> Result<Option<OffsetDateTime>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(de)?;
    Ok(value.as_ref().and_then(parse_timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    #[test]
    fn epoch_millis_and_seconds_are_told_apart() {
        // Trades carry `*_timestamp` in millis; the log tuple carries seconds.
        // Both are bare JSON numbers, so the magnitude is the only signal.
        let expected = datetime!(2024-09-24 08:00:00 UTC);
        assert_eq!(
            parse_timestamp(&json!(1_727_164_800_000i64)),
            Some(expected)
        );
        assert_eq!(parse_timestamp(&json!(1_727_164_800i64)), Some(expected));
        // Fractional epoch seconds, as the log endpoint sends them.
        assert_eq!(
            parse_timestamp(&json!(1_727_164_801.123f64)).map(|d| d.second()),
            Some(1)
        );
    }

    #[test]
    fn accepts_every_string_shape_freqtrade_emits() {
        let expected = datetime!(2024-09-24 08:00:00 UTC);
        // RFC 3339, from /pair_candles.
        assert_eq!(parse_timestamp_str("2024-09-24T08:00:00Z"), Some(expected));
        assert_eq!(
            parse_timestamp_str("2024-09-24T08:00:00+00:00"),
            Some(expected)
        );
        // Naive, from open_date / close_date.
        assert_eq!(parse_timestamp_str("2024-09-24 08:00:00"), Some(expected));
        // Naive with the log formatter's comma-separated millis.
        assert_eq!(
            parse_timestamp_str("2024-09-24 08:00:00,123"),
            Some(expected)
        );
        // ISO with a dot fraction and no zone.
        assert_eq!(
            parse_timestamp_str("2024-09-24T08:00:00.123"),
            Some(expected)
        );
    }

    #[test]
    fn rejects_what_it_cannot_place() {
        // A bar we cannot position on the x-axis is worse than no bar, so these
        // must be None rather than silently becoming the epoch.
        assert_eq!(parse_timestamp_str(""), None);
        assert_eq!(parse_timestamp_str("   "), None);
        assert_eq!(parse_timestamp_str("not a date"), None);
        assert_eq!(parse_timestamp(&json!(null)), None);
        assert_eq!(parse_timestamp(&json!(true)), None);
    }

    #[test]
    fn stake_amount_reads_both_wire_forms() {
        assert_eq!(
            serde_json::from_str::<StakeAmount>("50.5").unwrap(),
            StakeAmount::Amount(50.5)
        );
        let unlimited: StakeAmount = serde_json::from_str("\"unlimited\"").unwrap();
        assert!(unlimited.is_unlimited());
        assert_eq!(unlimited.as_f64(), None);
        // Some builds quote the number.
        let quoted: StakeAmount = serde_json::from_str("\"25\"").unwrap();
        assert_eq!(quoted.as_f64(), Some(25.0));
        assert!(!quoted.is_unlimited());
    }

    #[test]
    fn null_becomes_the_default() {
        #[derive(Deserialize)]
        struct Row {
            #[serde(default, deserialize_with = "null_to_default")]
            value: f64,
        }
        // `#[serde(default)]` alone handles the absent key but errors on an
        // explicit null, which Freqtrade does send.
        assert_eq!(serde_json::from_str::<Row>("{}").unwrap().value, 0.0);
        assert_eq!(
            serde_json::from_str::<Row>(r#"{"value":null}"#)
                .unwrap()
                .value,
            0.0
        );
        assert_eq!(
            serde_json::from_str::<Row>(r#"{"value":1.5}"#)
                .unwrap()
                .value,
            1.5
        );
    }
}

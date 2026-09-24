//! Display formatting.
//!
//! Centralised so a profit is coloured and signed the same way on every screen.
//! The Flutter version repeated `toStringAsFixed` and colour logic per widget,
//! which is how the Dashboard and Open Trades ended up disagreeing about how
//! many decimals a profit has.

/// Whether a number should read as a gain, a loss, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Profit,
    Loss,
    Flat,
}

impl Tone {
    pub fn of(value: f64) -> Self {
        if value > 0.0 {
            Self::Profit
        } else if value < 0.0 {
            Self::Loss
        } else {
            Self::Flat
        }
    }

    /// CSS modifier suffix, e.g. `tone--profit`.
    pub fn class(self) -> &'static str {
        match self {
            Self::Profit => "profit",
            Self::Loss => "loss",
            Self::Flat => "flat",
        }
    }

    pub fn arrow(self) -> &'static str {
        match self {
            Self::Profit => "▲",
            Self::Loss => "▼",
            Self::Flat => "–",
        }
    }
}

/// An amount with its currency, e.g. `1 198.90 USDT`.
pub fn money(value: f64, currency: &str) -> String {
    if currency.is_empty() {
        format!("{:.2}", value)
    } else {
        format!("{:.2} {currency}", value)
    }
}

/// An amount that is explicitly a gain or loss, so the sign is always shown.
pub fn signed_money(value: f64, currency: &str) -> String {
    let sign = if value > 0.0 { "+" } else { "" };
    format!("{sign}{}", money(value, currency))
}

/// A ratio as a percentage, e.g. `0.0123` becomes `+1.23%`.
///
/// Freqtrade reports `profit_ratio` as a ratio, not a percentage; multiplying
/// in one place stops the factor-of-100 mistakes that scattering it invites.
pub fn percent(ratio: f64) -> String {
    let sign = if ratio > 0.0 { "+" } else { "" };
    format!("{sign}{:.2}%", ratio * 100.0)
}

/// A price, with enough decimals to be useful for cheap assets.
///
/// A fixed two decimals would render ADA at 0.35 as `0.35` but SHIB as `0.00`.
pub fn price(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude == 0.0 {
        "0".to_owned()
    } else if magnitude >= 1000.0 {
        format!("{value:.2}")
    } else if magnitude >= 1.0 {
        format!("{value:.4}")
    } else {
        format!("{value:.8}")
    }
}

/// Epoch milliseconds as a local date and time.
#[cfg(target_arch = "wasm32")]
pub fn datetime(millis: i64) -> String {
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(millis as f64));
    let options = js_sys::Object::new();
    // Short and unambiguous: "24 Sep, 14:30".
    let _ = js_sys::Reflect::set(&options, &"day".into(), &"2-digit".into());
    let _ = js_sys::Reflect::set(&options, &"month".into(), &"short".into());
    let _ = js_sys::Reflect::set(&options, &"hour".into(), &"2-digit".into());
    let _ = js_sys::Reflect::set(&options, &"minute".into(), &"2-digit".into());
    date.to_locale_string("default", &options).into()
}

/// Non-browser fallback, used by tests.
#[cfg(not(target_arch = "wasm32"))]
pub fn datetime(millis: i64) -> String {
    use time::OffsetDateTime;
    OffsetDateTime::from_unix_timestamp_nanos(millis as i128 * 1_000_000)
        .map(|d| {
            format!(
                "{:02} {} {:02}:{:02}",
                d.day(),
                d.month(),
                d.hour(),
                d.minute()
            )
        })
        .unwrap_or_default()
}

/// How long ago, for the offline banner: "3m ago", "just now".
pub fn ago(seconds: i64) -> String {
    match seconds {
        s if s < 45 => "just now".to_owned(),
        s if s < 5400 => format!("{}m ago", (s as f64 / 60.0).round() as i64),
        s if s < 172_800 => format!("{}h ago", (s as f64 / 3600.0).round() as i64),
        s => format!("{}d ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentages_are_signed_and_scaled() {
        // Freqtrade sends a ratio; a missing x100 here is a whole class of bug.
        assert_eq!(percent(0.0123), "+1.23%");
        assert_eq!(percent(-0.05), "-5.00%");
        assert_eq!(percent(0.0), "0.00%");
    }

    #[test]
    fn money_carries_its_currency() {
        assert_eq!(money(1198.9, "USDT"), "1198.90 USDT");
        assert_eq!(signed_money(6.16, "USDT"), "+6.16 USDT");
        assert_eq!(signed_money(-2.5, "USDT"), "-2.50 USDT");
        assert_eq!(money(5.0, ""), "5.00");
    }

    #[test]
    fn prices_keep_precision_for_cheap_assets() {
        // Two fixed decimals would render this as 0.00.
        assert_eq!(price(0.000012), "0.00001200");
        assert_eq!(price(0.35), "0.35000000");
        assert_eq!(price(2500.5), "2500.50");
        assert_eq!(price(147.0), "147.0000");
        assert_eq!(price(0.0), "0");
    }

    #[test]
    fn tone_follows_the_sign() {
        assert_eq!(Tone::of(1.0), Tone::Profit);
        assert_eq!(Tone::of(-1.0), Tone::Loss);
        assert_eq!(Tone::of(0.0), Tone::Flat);
        assert_eq!(Tone::of(1.0).class(), "profit");
    }

    #[test]
    fn relative_times_read_naturally() {
        assert_eq!(ago(5), "just now");
        assert_eq!(ago(120), "2m ago");
        assert_eq!(ago(7200), "2h ago");
        assert_eq!(ago(200_000), "2d ago");
    }
}

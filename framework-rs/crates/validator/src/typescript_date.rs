//! A timezone-independent subset of `ForkLaunch` Zod Date preprocessing.
use crate::ParseError;
use serde_json::Value;

const TIME_CLIP: i64 = 8_640_000_000_000_000;

/// Separates actual validator rejection from JavaScript date-string forms this
/// profile deliberately does not interpret (legacy/locale/local-time formats).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DateCoercionError {
    Validation(ParseError),
    UnsupportedStringFormat,
}
impl std::fmt::Display for DateCoercionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(error) => formatter.write_str(&error.message),
            Self::UnsupportedStringFormat => {
                formatter.write_str("unsupported JavaScript date string format")
            }
        }
    }
}
impl std::error::Error for DateCoercionError {}

/// Coerce numeric epochs and explicit ISO dates to JavaScript epoch milliseconds.
///
/// Supports date-only `YYYY`, `YYYY-MM`, `YYYY-MM-DD` (UTC), signed six-digit
/// years, and full dates followed by `THH:mm[:ss[.fraction]]` with `Z` or a signed
/// `HH:mm`/`HHmm` offset. Fractional milliseconds truncate; day31 overflow and
/// zero-only 24:00 roll over as in V8. Numeric epochs use JavaScript `TimeClip`.
/// Returns `i64` milliseconds over the full Date range, independent of `chrono`.
///
/// Offsetless datetimes, legacy/locale forms, and V8's negative-zero-year
/// date-only fallback are unsupported, not misreported as validator failures.
/// Date-only strings with out-of-range month/day fields are also unsupported:
/// V8 may reinterpret them as legacy local-time dates.
/// Strings must use the stated ASCII grammar without padding. This is not an
/// arbitrary `Date.parse` implementation. Non-number/string JSON inputs retain
/// the actual Zod type diagnostics; existing Date objects are outside JSON.
/// # Errors
/// Returns public Zod diagnostics for admitted invalid dates, and a distinct
/// unsupported-format error for inputs outside this deterministic profile.
pub fn date_epoch_millis(value: Option<&Value>) -> Result<i64, DateCoercionError> {
    match value {
        Some(Value::Number(value)) => clip_number(value.as_f64().unwrap_or(f64::NAN)),
        Some(Value::String(value)) => parse_iso(value),
        _ => Err(DateCoercionError::Validation(ParseError {
            path: Vec::new(),
            message: match value {
                None => "Required".into(),
                Some(Value::Null) => "Expected date, received null".into(),
                Some(Value::Bool(_)) => "Expected date, received boolean".into(),
                Some(Value::Array(_)) => "Expected date, received array".into(),
                _ => "Expected date, received object".into(),
            },
        })),
    }
}

// TimeClip intentionally truncates finite values inside a range smaller than
// both i64 and the exact-integer binary64 limit.
#[allow(clippy::cast_possible_truncation)]
fn clip_number(value: f64) -> Result<i64, DateCoercionError> {
    if !value.is_finite() || value.abs() > 8_640_000_000_000_000.0 {
        return Err(invalid());
    }
    Ok(value.trunc() as i64)
}
fn invalid() -> DateCoercionError {
    DateCoercionError::Validation(ParseError {
        path: Vec::new(),
        message: "Invalid date".into(),
    })
}
fn unsupported() -> DateCoercionError {
    DateCoercionError::UnsupportedStringFormat
}

fn parse_iso(value: &str) -> Result<i64, DateCoercionError> {
    if !value.is_ascii() {
        return Err(unsupported());
    }
    let (date, time) = value
        .split_once('T')
        .map_or((value, None), |(date, time)| (date, Some(time)));
    if time.is_some_and(|time| !time.ends_with('Z') && !time.contains(['+', '-'])) {
        return Err(unsupported());
    }
    let signed = matches!(date.as_bytes().first(), Some(b'+' | b'-'));
    let year_width = if signed { 7 } else { 4 };
    let year_text = date.get(..year_width).ok_or_else(unsupported)?;
    let mut year = digits(if signed { &year_text[1..] } else { year_text })?;
    let negative_zero = signed && year_text.starts_with('-') && year == 0;
    if year_text.starts_with('-') {
        year = -year;
    }
    let tail = &date[year_width..];
    if negative_zero {
        return Err(if time.is_none() && !tail.is_empty() {
            unsupported()
        } else {
            invalid()
        });
    }
    let (month, day) = match tail.as_bytes() {
        [] => (1, 1),
        [b'-', _, _] => (digits(&tail[1..])?, 1),
        [b'-', _, _, b'-', _, _] => (digits(&tail[1..3])?, digits(&tail[4..])?),
        _ => return Err(unsupported()),
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        // V8 can reinterpret invalid date-only ISO components as a local-time
        // legacy date (for example 0001-01-00). Do not silently emulate that.
        return Err(if time.is_none() {
            unsupported()
        } else {
            invalid()
        });
    }
    let days = civil_days(year, month, day);
    let millis = if let Some(time) = time {
        if tail.len() != 6 {
            return Err(unsupported());
        }
        days * 86_400_000 + parse_time(time)?
    } else {
        days * 86_400_000
    };
    if millis.abs() > TIME_CLIP {
        Err(invalid())
    } else {
        Ok(millis)
    }
}

fn digits(value: &str) -> Result<i64, DateCoercionError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(unsupported());
    }
    value.parse().map_err(|_| unsupported())
}

// Proleptic Gregorian calendar, including astronomical year0 and negative years.
// Calendar arithmetic deliberately permits day29..31 to roll into next month.
fn civil_days(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn parse_time(value: &str) -> Result<i64, DateCoercionError> {
    let (clock, offset) = if let Some(clock) = value.strip_suffix('Z') {
        (clock, 0)
    } else {
        let index = value.find(['+', '-']).ok_or_else(unsupported)?;
        (&value[..index], parse_offset(&value[index..])?)
    };
    let (seconds_text, fraction) = clock
        .split_once('.')
        .map_or((clock, None), |(clock, fraction)| (clock, Some(fraction)));
    let bytes = seconds_text.as_bytes();
    if !matches!(bytes, [_, _, b':', _, _] | [_, _, b':', _, _, b':', _, _]) {
        return Err(unsupported());
    }
    let hour = digits(&seconds_text[..2])?;
    let minute = digits(&seconds_text[3..5])?;
    let second = if bytes.len() == 8 {
        digits(&seconds_text[6..])?
    } else {
        0
    };
    let (millisecond, fraction_nonzero) = match fraction {
        None => (0, false),
        Some(fraction) => {
            if bytes.len() != 8
                || fraction.is_empty()
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(unsupported());
            }
            let mut millis = 0;
            for index in 0..3 {
                millis = millis * 10
                    + i64::from(fraction.as_bytes().get(index).copied().unwrap_or(b'0') - b'0');
            }
            (millis, fraction.bytes().any(|byte| byte != b'0'))
        }
    };
    if hour > 24
        || minute > 59
        || second > 59
        || (hour == 24 && (minute != 0 || second != 0 || fraction_nonzero))
    {
        return Err(invalid());
    }
    Ok(((hour * 60 + minute) * 60 + second) * 1000 + millisecond - offset)
}

fn parse_offset(value: &str) -> Result<i64, DateCoercionError> {
    let (hour, minute) = match value.as_bytes() {
        [_, _, _, b':', _, _] => (digits(&value[1..3])?, digits(&value[4..])?),
        [_, _, _, _, _] => (digits(&value[1..3])?, digits(&value[3..])?),
        _ => return Err(unsupported()),
    };
    if hour > 23 || minute > 59 {
        return Err(invalid());
    }
    let offset = (hour * 60 + minute) * 60_000;
    Ok(if value.starts_with('-') {
        -offset
    } else {
        offset
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn epochs_cover_full_timeclip_without_chrono_limits() {
        assert_eq!(
            date_epoch_millis(Some(&json!(8_640_000_000_000_000_i64))),
            Ok(TIME_CLIP)
        );
        assert_eq!(
            date_epoch_millis(Some(&json!(-8_640_000_000_000_000_i64))),
            Ok(-TIME_CLIP)
        );
        assert_eq!(date_epoch_millis(Some(&json!(-0.9))), Ok(0));
        assert_eq!(
            date_epoch_millis(Some(&json!(8_640_000_000_000_001_i64))),
            Err(invalid())
        );
    }

    #[test]
    fn admitted_iso_rollover_offsets_and_fraction_rules_are_explicit() {
        assert!(
            crate::coerce::datetime(
                forklaunch_common::ContractValidationLocation::Query,
                "date",
                "2025"
            )
            .is_err()
        );
        assert_eq!(
            date_epoch_millis(Some(&json!("2025"))),
            Ok(1_735_689_600_000)
        );
        assert_eq!(
            date_epoch_millis(Some(&json!("2025-02-30"))),
            Ok(1_740_873_600_000)
        );
        assert_eq!(
            date_epoch_millis(Some(&json!("2025-01-01T24:00Z"))),
            Ok(1_735_776_000_000)
        );
        assert_eq!(
            date_epoch_millis(Some(&json!("2025-01-01T00:00:00.123456+01:30"))),
            Ok(1_735_684_200_123)
        );
        assert_eq!(
            date_epoch_millis(Some(&json!("2025-01-01T24:00:00.0001Z"))),
            Err(invalid())
        );
    }

    #[test]
    fn unsupported_forms_are_not_false_validator_rejections() {
        for input in [
            "2025-01-01T12:00",
            "-000000-01-01",
            "0001-01-00",
            "2025-01-01T00:00+€x",
            " Jan 1 2025 ",
        ] {
            assert_eq!(date_epoch_millis(Some(&json!(input))), Err(unsupported()));
        }
        assert_eq!(
            date_epoch_millis(Some(&json!("-000000-01-01T00:00:00Z"))),
            Err(invalid())
        );
        assert_eq!(date_epoch_millis(None).unwrap_err().to_string(), "Required");
    }
}

//! Node 24.2 / V8 string-date parsing, including legacy syntax and local time.
//!
//! Calendar rules are pinned to IANA 2025b. Explicit offsets and date-only ISO
//! inputs do not consult the host timezone. Local-time gaps advance by the gap;
//! repeated local times select the earlier instant, matching V8/ICU.

mod scanner;
use scanner::{Kind, Scanner, Token};
mod state;
use state::State;
mod compose;
mod data;
#[cfg(unix)]
mod host_cache;
mod iso;
mod legacy;
mod timezone;

use tz::{DateTime, TimeZone, UtcDateTime, datetime::FoundDateTimeKind};

/// Version of the bundled calendar timezone rules.
pub const TIME_ZONE_VERSION: Option<&str> = data::VERSION;

const TIME_CLIP: i64 = 8_640_000_000_000_000;
const BEFORE_UTC_LIMIT: i64 = TIME_CLIP + 30 * 86_400_000;

/// Failure to parse a date or establish the local timezone required by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateError {
    /// V8 rejects the grammar, calendar values, or final `TimeClip` value.
    InvalidDate,
    /// The host timezone cannot be identified under the supported discovery rules.
    TimeZoneUnavailable,
}

/// A timezone with pinned rules, suitable for repeated local-time conversions.
pub struct LocalTimeZone(TimeZone);
impl LocalTimeZone {
    /// Resolve an exact, case-sensitive IANA 2025b identifier.
    ///
    /// # Errors
    /// Returns `TimeZoneUnavailable` for unknown identifiers or unusable data.
    pub fn named(name: &str) -> Result<Self, DateError> {
        timezone::pinned(name)
            .map(Self)
            .ok_or(DateError::TimeZoneUnavailable)
    }
    /// Discover the process timezone using the Unix ICU host-detection policy.
    ///
    /// # Errors
    /// Returns `TimeZoneUnavailable` if discovery fails, `TZ` is not UTF-8, or
    /// the platform has no implemented host-discovery policy (including Windows).
    pub fn discover() -> Result<Self, DateError> {
        #[cfg(unix)]
        {
            let value = host_cache::environment()?;
            timezone::discover(value.as_deref())
                .map(Self)
                .map_err(|_| DateError::TimeZoneUnavailable)
        }
        #[cfg(not(unix))]
        {
            Err(DateError::TimeZoneUnavailable)
        }
    }
    fn local_to_utc(&self, wall: i64) -> Result<i64, DateError> {
        if !(-BEFORE_UTC_LIMIT..=BEFORE_UTC_LIMIT).contains(&wall) {
            return Err(DateError::InvalidDate);
        }
        let date = UtcDateTime::from_timespec(
            wall.div_euclid(1000),
            u32::try_from(wall.rem_euclid(1000)).expect("nonnegative millisecond remainder")
                * 1_000_000,
        )
        .map_err(|_| DateError::InvalidDate)?;
        let matches = DateTime::find(
            date.year(),
            date.month(),
            date.month_day(),
            date.hour(),
            date.minute(),
            date.second(),
            date.nanoseconds(),
            self.0.as_ref(),
        )
        .map_err(|_| DateError::TimeZoneUnavailable)?
        .into_inner();
        matches
            .iter()
            .map(|candidate| match candidate {
                FoundDateTimeKind::Normal(date) => {
                    date.unix_time() * 1000 + i64::from(date.nanoseconds() / 1_000_000)
                }
                FoundDateTimeKind::Skipped {
                    before_transition, ..
                } => wall - i64::from(before_transition.local_time_type().ut_offset()) * 1000,
            })
            .min()
            .ok_or(DateError::TimeZoneUnavailable)
    }
}

fn finish(
    parsed: (i64, Option<i32>),
    local: impl FnOnce(i64) -> Result<i64, DateError>,
) -> Result<i64, DateError> {
    let (wall, offset) = parsed;
    let utc = if let Some(offset) = offset {
        wall - i64::from(offset) * 1000
    } else {
        local(wall)?
    };
    if (-TIME_CLIP..=TIME_CLIP).contains(&utc) {
        Ok(utc)
    } else {
        Err(DateError::InvalidDate)
    }
}

/// Parse UTF-16 string input using an explicit local timezone. Lone surrogates
/// remain visible to the V8 scanner; this does not coerce non-string JS values.
///
/// # Errors
/// Returns `InvalidDate` for rejected grammar or `TimeClip` values, and
/// `TimeZoneUnavailable` if a local calendar value cannot be resolved.
pub fn parse_utf16(input: &[u16], timezone: &LocalTimeZone) -> Result<i64, DateError> {
    finish(
        legacy::parse(input).ok_or(DateError::InvalidDate)?,
        |wall| timezone.local_to_utc(wall),
    )
}

/// Parse a Rust string, discovering the process timezone only for local input.
/// No Node process, remote service, or timezone download is used at runtime.
///
/// # Errors
/// Returns the same parsing and discovery errors as [`parse_utf16_host`].
pub fn parse_host(input: &str) -> Result<i64, DateError> {
    parse_utf16_host(&input.encode_utf16().collect::<Vec<_>>())
}

/// Parse UTF-16 input with lazy process-timezone discovery, retaining lone
/// surrogate code units for the V8 scanner rather than replacing them.
///
/// # Errors
/// Returns `InvalidDate` for rejected grammar or `TimeClip` values, and
/// `TimeZoneUnavailable` if required host discovery or local resolution fails.
pub fn parse_utf16_host(input: &[u16]) -> Result<i64, DateError> {
    finish(
        legacy::parse(input).ok_or(DateError::InvalidDate)?,
        |wall| {
            #[cfg(unix)]
            {
                host_cache::timezone()?.local_to_utc(wall)
            }
            #[cfg(not(unix))]
            {
                let _ = wall;
                Err(DateError::TimeZoneUnavailable)
            }
        },
    )
}

#[cfg(test)]
mod tests;

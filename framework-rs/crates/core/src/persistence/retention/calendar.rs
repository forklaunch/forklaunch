//! Explicit UTC calendar policies matching TypeScript Date setter order in TZ=UTC.
use super::{RetentionAction, RetentionPolicy, invalid};
use crate::{common::FrameworkResult, persistence::compliance::ComplianceMetadata};
use chrono::{DateTime, Datelike as _, Duration, NaiveDate, Utc};

/// ISO date-only duration. Fractional units, weeks and time units are unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetentionDuration {
    years: u32,
    months: u32,
    days: u32,
}
impl std::str::FromStr for RetentionDuration {
    type Err = crate::common::FrameworkError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let fail = || invalid("Expected nonzero retention duration P[n]Y[n]M[n]D");
        if value.len() > 64 {
            return Err(fail());
        }
        let mut rest = value.strip_prefix('P').ok_or_else(fail)?;
        let mut values = [0_u32; 3];
        for (index, unit) in b"YMD".iter().copied().enumerate() {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if rest.as_bytes().get(digits) == Some(&unit) {
                values[index] = rest[..digits].parse().map_err(|_| fail())?;
                rest = &rest[digits + 1..];
            }
        }
        if !rest.is_empty() || values == [0, 0, 0] {
            return Err(fail());
        }
        Ok(Self {
            years: values[0],
            months: values[1],
            days: values[2],
        })
    }
}
impl RetentionDuration {
    /// Resolve a cutoff using year, month, then day setters, in that order.
    /// Month-end and leap-day overflow deliberately match JS Date, not clamping.
    /// This does not infer the server's local timezone or implement DST policies.
    /// # Errors
    /// Rejects submillisecond timestamps and calendar overflow outside Chrono's range.
    pub fn subtract_utc(self, now: DateTime<Utc>) -> FrameworkResult<DateTime<Utc>> {
        let fail = || invalid("Retention calendar cutoff is out of range");
        if !now.timestamp_subsec_nanos().is_multiple_of(1_000_000) {
            return Err(fail());
        }
        let year = i64::from(now.year()) - i64::from(self.years);
        let after_year = overflow_date(year, i64::from(now.month()), now.day()).ok_or_else(fail)?;
        let after_month = overflow_date(
            i64::from(after_year.year()),
            i64::from(after_year.month()) - i64::from(self.months),
            after_year.day(),
        )
        .ok_or_else(fail)?;
        let date = after_month
            .checked_sub_signed(Duration::days(i64::from(self.days)))
            .ok_or_else(fail)?;
        Ok(date.and_time(now.time()).and_utc())
    }
}
fn overflow_date(year: i64, month: i64, day: u32) -> Option<NaiveDate> {
    let absolute_month = year.checked_mul(12)?.checked_add(month - 1)?;
    let year = i32::try_from(absolute_month.div_euclid(12)).ok()?;
    let month = u32::try_from(absolute_month.rem_euclid(12) + 1).ok()?;
    NaiveDate::from_ymd_opt(year, month, 1)?.checked_add_signed(Duration::days(i64::from(day) - 1))
}
impl RetentionPolicy {
    /// Resolve an explicitly UTC declared duration using the same compliance fields
    /// and exclusive cutoff as [`Self::for_entity`].
    /// # Errors
    /// Rejects invalid metadata, duration or calendar bounds.
    pub fn for_duration_utc<T: ComplianceMetadata>(
        now: DateTime<Utc>,
        duration: RetentionDuration,
        action: RetentionAction,
    ) -> FrameworkResult<Self> {
        Self::for_entity::<T>(duration.subtract_utc(now)?.into(), action)
    }
}

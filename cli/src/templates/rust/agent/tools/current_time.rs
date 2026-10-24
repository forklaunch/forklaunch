//! Example read-only tool: the current UTC time. Delete freely.
use forklaunch_core::agent::{Tool, ToolError};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

#[must_use]
pub fn tool() -> Tool {
    Tool::new(
        "current_time",
        "Current time",
        "The current date and time in UTC. Call it when the user asks what time or day it is.",
        json!({"type": "object", "properties": {}}),
    )
    .read_only()
    .handler(|_, _| async {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ToolError::new("The system clock is before 1970."))?
            .as_secs();
        Ok(json!({"utc": utc(seconds), "unixSeconds": seconds}))
    })
}

/// `YYYY-MM-DDTHH:MM:SSZ` from Unix seconds (proleptic Gregorian calendar).
fn utc(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let time = seconds % 86_400;
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time / 3600,
        time % 3600 / 60,
        time % 60
    )
}

#[cfg(test)]
mod tests {
    use super::utc;

    #[test]
    fn formats_utc() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(1_792_454_400), "2026-10-20T00:00:00Z");
    }
}

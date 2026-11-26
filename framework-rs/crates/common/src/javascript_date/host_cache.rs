use super::{DateError, LocalTimeZone, timezone};
use std::sync::{Arc, Mutex};

struct Entry {
    environment: Option<String>,
    timezone: Result<Arc<LocalTimeZone>, DateError>,
}
#[derive(Default)]
struct Cache(Option<Entry>);
impl Cache {
    fn resolve(&mut self, environment: Option<String>) -> Result<Arc<LocalTimeZone>, DateError> {
        if let Some(entry) = &self.0
            && entry.environment == environment
        {
            return entry.timezone.clone();
        }
        let timezone = timezone::discover(environment.as_deref())
            .map(LocalTimeZone)
            .map(Arc::new)
            .map_err(|_| DateError::TimeZoneUnavailable);
        self.0 = Some(Entry {
            environment,
            timezone: timezone.clone(),
        });
        timezone
    }
}
static HOST: Mutex<Cache> = Mutex::new(Cache(None));

pub(super) fn environment() -> Result<Option<String>, DateError> {
    match std::env::var("TZ") {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(DateError::TimeZoneUnavailable),
    }
}

pub(super) fn timezone() -> Result<Arc<LocalTimeZone>, DateError> {
    // Only trusted process configuration selects this single bounded entry.
    // The guard is destroyed here, before calendar parsing/resolution begins.
    HOST.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .resolve(environment()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_tz_replaces_one_entry_without_changing_existing_snapshots() {
        let mut cache = Cache::default();
        let utc = cache.resolve(Some("UTC".into())).unwrap();
        assert!(Arc::ptr_eq(
            &utc,
            &cache.resolve(Some("UTC".into())).unwrap()
        ));
        let los_angeles = cache.resolve(Some("America/Los_Angeles".into())).unwrap();
        assert!(!Arc::ptr_eq(&utc, &los_angeles));
        let input = "2026-07-01T12:00".encode_utf16().collect::<Vec<_>>();
        assert_eq!(
            super::super::parse_utf16(&input, &los_angeles).unwrap()
                - super::super::parse_utf16(&input, &utc).unwrap(),
            7 * 3_600_000
        );
        let utc_again = cache.resolve(Some("UTC".into())).unwrap();
        assert!(!Arc::ptr_eq(&utc, &utc_again));
        assert_eq!(
            super::super::parse_utf16(&input, &utc_again),
            super::super::parse_utf16(&input, &utc)
        );
        assert_eq!(
            cache.0.as_ref().unwrap().environment.as_deref(),
            Some("UTC")
        );
    }
}

use crate::encrypted::ProcessorError;
use std::time::Duration;

/// Bounded retries; queue payload metadata cannot override this policy.
#[derive(Clone, Copy, Debug)]
pub struct RetrySettings {
    pub(crate) retries: usize,
    pub(crate) minimum: Duration,
    pub(crate) maximum: Duration,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            retries: 7,
            minimum: Duration::from_secs(1),
            maximum: Duration::from_secs(30),
        }
    }
}

impl RetrySettings {
    /// # Errors
    /// Rejects more than 16 retries, zero minimum delay, a maximum below the
    /// minimum, or a maximum exceeding five minutes.
    pub fn new(
        retries: usize,
        minimum: Duration,
        maximum: Duration,
    ) -> Result<Self, ProcessorError> {
        if retries > 16
            || minimum.is_zero()
            || maximum < minimum
            || maximum > Duration::from_secs(300)
        {
            return Err(ProcessorError::Configuration);
        }
        Ok(Self {
            retries,
            minimum,
            maximum,
        })
    }

    #[must_use]
    pub fn retries(self) -> usize {
        self.retries
    }
    #[must_use]
    pub fn minimum(self) -> Duration {
        self.minimum
    }
    #[must_use]
    pub fn maximum(self) -> Duration {
        self.maximum
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unbounded_or_invalid_backoff() {
        let second = Duration::from_secs(1);
        assert!(RetrySettings::new(17, second, second).is_err());
        assert!(RetrySettings::new(1, Duration::ZERO, second).is_err());
        assert!(RetrySettings::new(1, second, Duration::ZERO).is_err());
        assert!(RetrySettings::new(1, second, Duration::from_secs(301)).is_err());
        assert_eq!(RetrySettings::new(0, second, second).unwrap().retries(), 0);
        assert_eq!(RetrySettings::default().retries(), 7);
    }
}

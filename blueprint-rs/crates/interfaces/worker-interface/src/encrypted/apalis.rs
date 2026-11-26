//! Native Apalis adapters: bounded jittered backoff and permanent-error rejection.
use crate::encrypted::{ProcessorError, RetrySettings};
use ::apalis::layers::retry::{
    BackoffRetryPolicy, HasherRng, RetryIfPolicy, RetryPolicy,
    backoff::{ExponentialBackoff, ExponentialBackoffMaker, MakeBackoff},
};
use apalis_core::error::{AbortError, BoxDynError};

pub type SecureRetryPolicy =
    RetryIfPolicy<BackoffRetryPolicy<ExponentialBackoff>, fn(&BoxDynError) -> bool>;

/// Filter boxed errors explicitly: the native policy's `Any` test alone does
/// not recognize an `AbortError` contained inside `BoxDynError`.
///
/// # Errors
///
/// Returns [`ProcessorError::Configuration`] when the backoff bounds are invalid.
pub fn retry_policy(settings: RetrySettings) -> Result<SecureRetryPolicy, ProcessorError> {
    let mut backoff = ExponentialBackoffMaker::new(
        settings.minimum,
        settings.maximum,
        0.1,
        HasherRng::default(),
    )
    .map_err(|_| ProcessorError::Configuration)?;
    Ok(RetryPolicy::retries(settings.retries)
        .with_backoff(backoff.make_backoff())
        .retry_if(retryable as fn(&BoxDynError) -> bool))
}

fn retryable(error: &BoxDynError) -> bool {
    if error.downcast_ref::<AbortError>().is_some() {
        return false;
    }
    error
        .downcast_ref::<ProcessorError>()
        .is_some_and(|error| error.retryable())
}

/// Preserve only a fixed framework error at the queue boundary.
#[must_use]
pub fn queue_error(error: ProcessorError) -> BoxDynError {
    if error.retryable() {
        Box::new(error)
    } else {
        Box::new(AbortError::new(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::apalis::layers::retry::Policy;
    use apalis_core::{task::builder::TaskBuilder, worker::context::WorkerContext};
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn native_policy_stops_at_budget_and_filters_abort() {
        let settings =
            RetrySettings::new(2, Duration::from_secs(1), Duration::from_secs(2)).unwrap();
        let mut policy = retry_policy(settings).unwrap();
        for attempt in 1..=4 {
            let mut task = TaskBuilder::new(())
                .attempt(attempt)
                .data(WorkerContext::new("retry-test"))
                .build();
            let mut result: Result<(), BoxDynError> = Err(queue_error(ProcessorError::Failed));
            let retry = policy.retry(&mut task, &mut result);
            assert_eq!(retry.is_some(), attempt <= 2);
            if let Some(delay) = retry {
                let start = tokio::time::Instant::now();
                delay.await;
                assert!(start.elapsed() <= Duration::from_millis(2200));
            }
            let mut permanent: Result<(), BoxDynError> =
                Err(queue_error(ProcessorError::InvalidEnvelope));
            assert!(policy.retry(&mut task, &mut permanent).is_none());
        }
    }

    #[test]
    fn rejects_permanent_and_unknown_boxed_errors() {
        assert!(!retryable(&queue_error(ProcessorError::InvalidEnvelope)));
        assert!(!retryable(&queue_error(ProcessorError::Configuration)));
        assert!(retryable(&queue_error(ProcessorError::Timeout)));
        assert!(retryable(&queue_error(ProcessorError::Failed)));
        let unknown: BoxDynError = Box::new(std::io::Error::other("private response body"));
        assert!(!retryable(&unknown));
    }
}

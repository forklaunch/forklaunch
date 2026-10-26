use crate::encrypted::{JobContext, JobPayload, ProcessorError, decode_job};
use forklaunch_core::encryption::EnvelopeKey;
use serde::de::DeserializeOwned;
use std::{future::Future, time::Duration};
use zeroize::Zeroizing;

/// Decode, bind and run one attempt, persisting only unit on success. Application
/// errors are erased to a fixed classification; no arbitrary error source leaks
/// into queue storage. The handler must publish idempotently before returning.
///
/// Timeout drops the handler future; it cannot undo a remote commit. Retried
/// publication must use `context.operation_id` and verify its acknowledgement.
/// # Errors
/// Returns an envelope error for rejected input, a configuration error for
/// an invalid timeout, a fixed handler failure, or a timeout.
pub async fn process<T, F, Fut, E>(
    key: &EnvelopeKey,
    context: JobContext,
    purpose: &str,
    ciphertext: &str,
    timeout: Duration,
    handler: F,
) -> Result<(), ProcessorError>
where
    T: JobPayload + DeserializeOwned,
    F: FnOnce(JobContext, Zeroizing<T>) -> Fut,
    Fut: Future<Output = Result<(), E>>,
{
    process_attempt(timeout, async move {
        let payload = decode_job(key, context, purpose, ciphertext)?;
        handler(context, payload)
            .await
            .map_err(|_| ProcessorError::Failed)
    })
    .await
}

/// Apply the same unit-result and bounded timeout boundary when the application
/// already uses `decode_job` in a domain service. Errors must be classified into
/// the fixed framework enum before crossing this boundary. Unlike `process`,
/// this preserves permanent application validation failures without decoding twice.
///
/// With the `telemetry` feature, attempts emit fixed-name spans and bounded
/// outcome/duration/active metrics through the application-owned global providers.
/// Cancellation closes the measurement without logging job identity or content.
///
/// Futures must cooperate with the async runtime: a timeout cannot preempt
/// blocking CPU work. Cancellation cannot undo an already committed publication.
/// # Errors
/// Rejects zero or greater-than-five-minute timeouts, reports elapsed
/// timeouts, and preserves the attempt's classified error.
pub async fn process_attempt<Fut>(timeout: Duration, attempt: Fut) -> Result<(), ProcessorError>
where
    Fut: Future<Output = Result<(), ProcessorError>>,
{
    let run = async move {
        if timeout.is_zero() || timeout > Duration::from_secs(300) {
            return Err(ProcessorError::Configuration);
        }
        tokio::time::timeout(timeout, attempt)
            .await
            .map_err(|_| ProcessorError::Timeout)?
    };
    #[cfg(feature = "telemetry")]
    {
        crate::encrypted::telemetry::observe(run).await
    }
    #[cfg(not(feature = "telemetry"))]
    {
        run.await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn classified_attempt_preserves_permanent_errors_and_bounds_time() {
        let timeout = Duration::from_secs(1);
        let rejected =
            process_attempt(timeout, async { Err(ProcessorError::InvalidEnvelope) }).await;
        assert_eq!(rejected, Err(ProcessorError::InvalidEnvelope));
        assert!(!rejected.unwrap_err().retryable());
        assert_eq!(
            process_attempt(timeout, std::future::pending()).await,
            Err(ProcessorError::Timeout)
        );
        assert_eq!(
            process_attempt(Duration::ZERO, async {
                panic!("invalid configuration must not poll the attempt")
            })
            .await,
            Err(ProcessorError::Configuration)
        );
    }
}

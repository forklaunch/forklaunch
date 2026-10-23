//! Bounded operations for long-lived SSE/WebSocket sessions. Application policy
//! still supplies verified identity and tenant selection. Never allow a stalled
//! write or authorization lookup to indefinitely prevent revocation checks.
use super::Shutdown;
use std::{fmt, future::Future, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamClosed;
impl fmt::Display for StreamClosed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("stream closed")
    }
}
impl std::error::Error for StreamClosed {}

#[derive(Clone, Debug)]
pub struct StreamLifecycle {
    shutdown: Shutdown,
    operation_timeout: Duration,
}
impl StreamLifecycle {
    /// # Errors
    /// Rejects zero timeouts and operation limits greater than 30 seconds.
    pub fn new(shutdown: Shutdown, operation_timeout: Duration) -> Result<Self, StreamClosed> {
        if operation_timeout.is_zero() || operation_timeout > Duration::from_secs(30) {
            return Err(StreamClosed);
        }
        Ok(Self {
            shutdown,
            operation_timeout,
        })
    }

    /// A timeout, shutdown, or application failure closes the session without
    /// forwarding arbitrary error text. Dropping work retains the application's
    /// usual transaction/idempotency responsibilities for external side effects.
    /// # Errors
    /// Closes on shutdown, timeout, or application operation failure.
    pub async fn operation<T, E>(
        &self,
        work: impl Future<Output = Result<T, E>>,
    ) -> Result<T, StreamClosed> {
        match self
            .shutdown
            .run_until_cancelled(tokio::time::timeout(self.operation_timeout, work))
            .await
        {
            Some(Ok(Ok(value))) => Ok(value),
            _ => Err(StreamClosed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn stalled_operations_and_application_errors_close_without_leaking() {
        let stream = StreamLifecycle::new(Shutdown::new(), Duration::from_millis(5)).unwrap();
        assert_eq!(
            stream
                .operation(std::future::pending::<Result<(), &str>>())
                .await,
            Err(StreamClosed)
        );
        let error = stream
            .operation(async { Err::<(), _>("sensitive message") })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "stream closed");
        assert_eq!(stream.operation(async { Ok::<_, ()>(42) }).await, Ok(42));
    }
    #[tokio::test]
    async fn shutdown_prevents_new_stream_work() {
        let shutdown = Shutdown::new();
        let stream = StreamLifecycle::new(shutdown.clone(), Duration::from_secs(5)).unwrap();
        shutdown.trigger();
        assert_eq!(
            stream
                .operation(async {
                    panic!("cancelled work polled");
                    #[allow(unreachable_code)]
                    Ok::<(), ()>(())
                })
                .await,
            Err(StreamClosed)
        );
        assert!(StreamLifecycle::new(shutdown, Duration::ZERO).is_err());
    }
}

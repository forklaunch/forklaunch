//! Credential-free, fixed-cardinality authentication outcomes.
use forklaunch_core::auth::AuthError;
use opentelemetry::{
    Context, KeyValue,
    metrics::{Counter, Meter},
    trace::{FutureExt, Status, TraceContextExt, Tracer},
};
use std::future::Future;

struct Attempt {
    context: Context,
    counter: Counter<u64>,
    scheme: &'static str,
    outcome: &'static str,
}
impl Attempt {
    fn new<T: Tracer>(meter: &Meter, tracer: &T, scheme: &'static str) -> Self
    where
        T::Span: Send + Sync + 'static,
    {
        let span = tracer.start("auth.authenticate");
        Self {
            context: Context::current_with_span(span),
            counter: meter.u64_counter("forklaunch.auth.attempts").build(),
            scheme,
            outcome: "cancelled",
        }
    }
    async fn run<T>(
        mut self,
        future: impl Future<Output = Result<T, AuthError>>,
    ) -> Result<T, AuthError> {
        let result = future.with_context(self.context.clone()).await;
        self.outcome = match &result {
            Ok(_) => "authenticated",
            Err(AuthError::MissingCredentials) => "missing",
            Err(AuthError::InvalidCredentials) => "invalid",
            Err(AuthError::Forbidden(_)) => "forbidden",
        };
        result
    }
}
impl Drop for Attempt {
    fn drop(&mut self) {
        let attributes = [
            KeyValue::new("auth.scheme", self.scheme),
            KeyValue::new("auth.outcome", self.outcome),
        ];
        self.counter.add(1, &attributes);
        let span = self.context.span();
        span.set_attributes(attributes);
        if self.outcome != "authenticated" {
            span.set_status(Status::error(self.outcome));
        }
        span.end();
    }
}
pub(super) async fn observe<T>(
    scheme: &'static str,
    future: impl Future<Output = Result<T, AuthError>>,
) -> Result<T, AuthError> {
    Attempt::new(
        &opentelemetry::global::meter("forklaunch.auth"),
        &opentelemetry::global::tracer("forklaunch.auth"),
        scheme,
    )
    .run(future)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::{metrics::MeterProvider, trace::TracerProvider};
    use opentelemetry_sdk::{
        metrics::SdkMeterProvider,
        trace::{InMemorySpanExporter, SdkTracerProvider},
    };
    #[tokio::test]
    async fn fixed_outcomes_inherit_parent_without_credential_claim_or_error_text() {
        let exporter = InMemorySpanExporter::default();
        let traces = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let meters = SdkMeterProvider::default();
        let parent = traces.tracer("test").start("http.server");
        let context = Context::current_with_span(parent);
        let parent_id = context.span().span_context().span_id();
        async {
            for result in [
                Ok(()),
                Err(AuthError::MissingCredentials),
                Err(AuthError::InvalidCredentials),
                Err(AuthError::Forbidden("private-token-claims".into())),
            ] {
                let original = result.clone();
                assert_eq!(
                    Attempt::new(&meters.meter("test"), &traces.tracer("test"), "jwt")
                        .run(async { result })
                        .await,
                    original
                );
            }
            let mut cancelled = Box::pin(
                Attempt::new(&meters.meter("test"), &traces.tracer("test"), "hmac")
                    .run(std::future::pending::<Result<(), AuthError>>()),
            );
            std::future::poll_fn(|cx| {
                assert!(cancelled.as_mut().poll(cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
            drop(cancelled);
        }
        .with_context(context.clone())
        .await;
        traces.force_flush().unwrap();
        let spans = exporter.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 5);
        for span in spans {
            assert_eq!(span.parent_span_id, parent_id);
            assert_eq!(span.attributes.len(), 2);
            assert!(!format!("{span:?}").contains("private-token-claims"));
        }
    }
}

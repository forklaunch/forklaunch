//! Fixed, low-cardinality attempt telemetry. Never records payloads, tenant/job
//! identifiers, purpose strings, handler errors or transport credentials.
use crate::encrypted::ProcessorError;
use opentelemetry::{
    Context, KeyValue,
    metrics::{Counter, Histogram, Meter, UpDownCounter},
    trace::{FutureExt, SpanKind, Status, TraceContextExt, Tracer},
};
use std::{future::Future, time::Instant};

struct Attempt {
    context: Context,
    started: Instant,
    active: UpDownCounter<i64>,
    count: Counter<u64>,
    duration: Histogram<f64>,
    outcome: &'static str,
}
impl Attempt {
    fn start<T: Tracer>(meter: &Meter, tracer: &T) -> Self
    where
        T::Span: Send + Sync + 'static,
    {
        Self::start_with_parent(meter, tracer, &Context::current())
    }
    fn start_with_parent<T: Tracer>(meter: &Meter, tracer: &T, parent: &Context) -> Self
    where
        T::Span: Send + Sync + 'static,
    {
        let span = tracer
            .span_builder("worker.process")
            .with_kind(SpanKind::Consumer)
            .start_with_context(tracer, parent);
        let context = parent.with_span(span);
        let active = meter
            .i64_up_down_counter("forklaunch.worker.active_attempts")
            .build();
        active.add(1, &[]);
        Self {
            context,
            started: Instant::now(),
            active,
            count: meter.u64_counter("forklaunch.worker.attempts").build(),
            duration: meter
                .f64_histogram("forklaunch.worker.duration")
                .with_unit("s")
                .build(),
            outcome: "cancelled",
        }
    }
    async fn observe(
        mut self,
        future: impl Future<Output = Result<(), ProcessorError>>,
    ) -> Result<(), ProcessorError> {
        let result = future.with_context(self.context.clone()).await;
        self.outcome = match result {
            Ok(()) => "success",
            Err(ProcessorError::Configuration) => "configuration",
            Err(ProcessorError::InvalidEnvelope) => "invalid_envelope",
            Err(ProcessorError::Failed) => "failed",
            Err(ProcessorError::Timeout) => "timeout",
        };
        result
    }
}
impl Drop for Attempt {
    fn drop(&mut self) {
        let attributes = [KeyValue::new("worker.outcome", self.outcome)];
        self.count.add(1, &attributes);
        self.duration
            .record(self.started.elapsed().as_secs_f64(), &attributes);
        self.active.add(-1, &[]);
        let span = self.context.span();
        span.set_attribute(attributes[0].clone());
        if self.outcome != "success" {
            span.set_status(Status::error(self.outcome));
        }
        span.end();
    }
}
pub(crate) async fn observe(
    future: impl Future<Output = Result<(), ProcessorError>>,
) -> Result<(), ProcessorError> {
    Attempt::start(
        &opentelemetry::global::meter("forklaunch.worker"),
        &opentelemetry::global::tracer("forklaunch.worker"),
    )
    .observe(future)
    .await
}

pub(crate) async fn observe_with_parent(
    parent: Context,
    future: impl Future<Output = Result<(), ProcessorError>>,
) -> Result<(), ProcessorError> {
    Attempt::start_with_parent(
        &opentelemetry::global::meter("forklaunch.worker"),
        &opentelemetry::global::tracer("forklaunch.worker"),
        &parent,
    )
    .observe(future)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::{metrics::MeterProvider, trace::TracerProvider};
    use opentelemetry_sdk::{
        metrics::{
            InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
            data::{AggregatedMetrics, MetricData},
        },
        trace::{InMemorySpanExporter, SdkTracerProvider},
    };
    #[tokio::test]
    async fn completion_failure_and_cancellation_emit_only_fixed_outcomes_and_balance_active() {
        let metrics = InMemoryMetricExporter::default();
        let meters = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(metrics.clone()).build())
            .build();
        let spans = InMemorySpanExporter::default();
        let traces = SdkTracerProvider::builder()
            .with_simple_exporter(spans.clone())
            .build();
        for result in [
            Ok(()),
            Err(ProcessorError::Failed),
            Err(ProcessorError::InvalidEnvelope),
            Err(ProcessorError::Timeout),
            Err(ProcessorError::Configuration),
        ] {
            let attempt = Attempt::start(&meters.meter("test"), &traces.tracer("test"));
            assert_eq!(attempt.observe(async { result }).await, result);
        }
        let attempt = Attempt::start(&meters.meter("test"), &traces.tracer("test"));
        let mut future = Box::pin(attempt.observe(std::future::pending()));
        std::future::poll_fn(|cx| {
            assert!(future.as_mut().poll(cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        drop(future);
        meters.force_flush().unwrap();
        traces.force_flush().unwrap();
        let spans = spans.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 6);
        let outcomes: std::collections::BTreeSet<_> = spans
            .iter()
            .map(|span| {
                assert_eq!(span.name, "worker.process");
                assert_eq!(span.attributes.len(), 1);
                assert_eq!(span.attributes[0].key.as_str(), "worker.outcome");
                span.attributes[0].value.to_string()
            })
            .collect();
        assert_eq!(
            outcomes,
            [
                "success",
                "failed",
                "invalid_envelope",
                "timeout",
                "configuration",
                "cancelled"
            ]
            .map(str::to_owned)
            .into_iter()
            .collect()
        );
        let exported = metrics.get_finished_metrics().unwrap();
        let mut count = 0;
        let mut active = None;
        let mut duration = 0;
        for resource in exported {
            for scope in resource.scope_metrics() {
                for metric in scope.metrics() {
                    match (metric.name(), metric.data()) {
                        (
                            "forklaunch.worker.attempts",
                            AggregatedMetrics::U64(MetricData::Sum(sum)),
                        ) => {
                            count += sum
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                .sum::<u64>();
                        }
                        (
                            "forklaunch.worker.active_attempts",
                            AggregatedMetrics::I64(MetricData::Sum(sum)),
                        ) => {
                            active = Some(
                                sum.data_points()
                                    .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                                    .sum::<i64>(),
                            );
                        }
                        (
                            "forklaunch.worker.duration",
                            AggregatedMetrics::F64(MetricData::Histogram(hist)),
                        ) => {
                            duration += hist
                                .data_points()
                                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::count)
                                .sum::<u64>();
                        }
                        _ => {}
                    }
                }
            }
        }
        assert_eq!(count, 6);
        assert_eq!(duration, 6);
        assert_eq!(active, Some(0));
    }
}

//! Structured logging and OpenTelemetry instruments. Providers and exporters are
//! application-owned; the framework never replaces a process-global provider.
use crate::common::{FrameworkError, FrameworkResult};
use opentelemetry::{
    Context,
    global::BoxedTracer,
    trace::{SpanKind, Status, TraceContextExt, Tracer},
};
use opentelemetry::{
    KeyValue,
    metrics::{Counter, Gauge, Histogram, Meter, UpDownCounter},
};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

#[derive(Debug, Clone, Copy)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    UpDownCounter,
}

#[derive(Clone)]
enum Instrument {
    Counter(Counter<f64>),
    Gauge(Gauge<f64>),
    Histogram(Histogram<f64>),
    UpDownCounter(UpDownCounter<f64>),
}

/// Collector scoped to a service. Clone handles instead of recreating instruments.
#[derive(Clone)]
pub struct OpenTelemetryCollector {
    service: String,
    meter: Meter,
    tracer: Arc<BoxedTracer>,
    response_size: Histogram<u64>,
    metrics: BTreeMap<String, Instrument>,
    requests: Counter<u64>,
    duration: Histogram<f64>,
    active: UpDownCounter<i64>,
    auth_failures: Counter<u64>,
    errors: Counter<u64>,
}

impl OpenTelemetryCollector {
    #[must_use]
    pub fn new(service: impl Into<String>) -> Self {
        let service = service.into();
        Self::with_meter(service.clone(), opentelemetry::global::meter("forklaunch"))
    }

    #[must_use]
    pub fn with_meter(service: impl Into<String>, meter: Meter) -> Self {
        Self {
            service: service.into(),
            tracer: Arc::new(opentelemetry::global::tracer("forklaunch")),
            response_size: meter
                .u64_histogram("http.server.response.body.size")
                .with_unit("By")
                .build(),
            requests: meter.u64_counter("http.server.request.count").build(),
            duration: meter
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .build(),
            active: meter
                .i64_up_down_counter("http.server.active_requests")
                .build(),
            auth_failures: meter.u64_counter("forklaunch.auth.failures").build(),
            errors: meter.u64_counter("forklaunch.http.errors").build(),
            meter,
            metrics: BTreeMap::new(),
        }
    }

    /// Configures a tracer without replacing the process-global provider.
    #[must_use]
    pub fn with_tracer<T>(mut self, tracer: T) -> Self
    where
        T: Tracer + Send + Sync + 'static,
        T::Span: Send + Sync + 'static,
    {
        self.tracer = Arc::new(BoxedTracer::new(Box::new(tracer)));
        self
    }

    /// The service meter also supports observable counters/gauges with callbacks.
    #[must_use]
    pub fn meter(&self) -> &Meter {
        &self.meter
    }

    /// Registers named application instruments once at startup.
    ///
    /// # Errors
    /// Rejects duplicate names to prevent conflicting instrument definitions.
    pub fn define_metric(
        &mut self,
        name: impl Into<String>,
        kind: MetricType,
    ) -> FrameworkResult<()> {
        let name = name.into();
        if name.is_empty() || self.metrics.contains_key(&name) {
            return Err(FrameworkError::new(
                "telemetry_invalid_metric",
                "Metric names must be nonempty and unique",
            ));
        }
        let instrument = match kind {
            MetricType::Counter => {
                Instrument::Counter(self.meter.f64_counter(name.clone()).build())
            }
            MetricType::Gauge => Instrument::Gauge(self.meter.f64_gauge(name.clone()).build()),
            MetricType::Histogram => {
                Instrument::Histogram(self.meter.f64_histogram(name.clone()).build())
            }
            MetricType::UpDownCounter => {
                Instrument::UpDownCounter(self.meter.f64_up_down_counter(name.clone()).build())
            }
        };
        self.metrics.insert(name, instrument);
        Ok(())
    }

    /// # Errors
    /// Rejects unknown metrics, nonfinite values, and negative counter increments.
    pub fn record_metric(
        &self,
        name: &str,
        value: f64,
        attributes: &[KeyValue],
    ) -> FrameworkResult<()> {
        if !value.is_finite() {
            return Err(FrameworkError::new(
                "telemetry_invalid_value",
                "Metric values must be finite",
            ));
        }
        match self.metrics.get(name) {
            Some(Instrument::Counter(counter)) if value >= 0.0 => counter.add(value, attributes),
            Some(Instrument::Gauge(gauge)) => gauge.record(value, attributes),
            Some(Instrument::Histogram(histogram)) => histogram.record(value, attributes),
            Some(Instrument::UpDownCounter(counter)) => counter.add(value, attributes),
            _ => {
                return Err(FrameworkError::new(
                    "telemetry_invalid_metric",
                    "Unknown metric or invalid counter increment",
                ));
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn start_request(&self, method: &str, route: &str) -> RequestMeasurement {
        self.start_request_with_context(method, route, &Context::current())
    }

    #[must_use]
    pub fn start_request_with_context(
        &self,
        method: &str,
        route: &str,
        parent: &Context,
    ) -> RequestMeasurement {
        let attributes = vec![
            KeyValue::new("http.request.method", method.to_owned()),
            KeyValue::new("http.route", route.to_owned()),
        ];
        self.active.add(1, &attributes);
        let span = self
            .tracer
            .span_builder(format!("{method} {route}"))
            .with_kind(SpanKind::Server)
            .with_attributes(attributes.clone())
            .start_with_context(self.tracer.as_ref(), parent);
        let context = parent.with_span(span);
        RequestMeasurement {
            collector: self.clone(),
            attributes,
            started: Instant::now(),
            finished: false,
            context,
        }
    }

    pub fn auth_failure(&self, operation: &str, correlation_id: &str) {
        self.auth_failures.add(
            1,
            &[KeyValue::new("forklaunch.operation", operation.to_owned())],
        );
        self.warn("authentication or authorization failed", correlation_id);
    }

    pub fn trace(&self, message: &str, correlation_id: &str) {
        tracing::trace!(service = %self.service, correlation_id, message);
    }
    pub fn debug(&self, message: &str, correlation_id: &str) {
        tracing::debug!(service = %self.service, correlation_id, message);
    }
    pub fn info(&self, message: &str, correlation_id: &str) {
        tracing::info!(service = %self.service, correlation_id, message);
    }
    pub fn warn(&self, message: &str, correlation_id: &str) {
        tracing::warn!(service = %self.service, correlation_id, message);
    }
    pub fn error(&self, message: &str, correlation_id: &str) {
        tracing::error!(service = %self.service, correlation_id, message);
    }
    pub fn fatal(&self, message: &str, correlation_id: &str) {
        tracing::error!(service = %self.service, severity = "fatal", correlation_id, message);
    }
}

/// Drops always decrement active requests, even when the handler future is cancelled.
pub struct RequestMeasurement {
    collector: OpenTelemetryCollector,
    attributes: Vec<KeyValue>,
    started: Instant,
    finished: bool,
    context: Context,
}

impl RequestMeasurement {
    #[must_use]
    pub fn context(&self) -> Context {
        self.context.clone()
    }

    pub fn finish(self, status: u16, response_bytes: Option<u64>, correlation_id: &str) {
        self.complete(status, response_bytes, correlation_id, None);
    }

    /// Records a failed response stream while preserving the already-sent HTTP status.
    /// Use a bounded, non-sensitive error category, never raw backend error details.
    pub fn finish_failed(
        self,
        status: u16,
        response_bytes: Option<u64>,
        correlation_id: &str,
        category: &'static str,
    ) {
        self.complete(status, response_bytes, correlation_id, Some(category));
    }

    fn complete(
        mut self,
        status: u16,
        response_bytes: Option<u64>,
        correlation_id: &str,
        failure: Option<&'static str>,
    ) {
        self.attributes.push(KeyValue::new(
            "http.response.status_code",
            i64::from(status),
        ));
        self.context.span().set_attribute(KeyValue::new(
            "http.response.status_code",
            i64::from(status),
        ));
        self.context.span().set_attribute(KeyValue::new(
            "forklaunch.correlation_id",
            correlation_id.to_owned(),
        ));
        if let Some(category) = failure {
            self.context.span().set_status(Status::error(category));
        } else if status >= 500 {
            self.context
                .span()
                .set_status(Status::error("server error"));
        }
        self.context.span().end();
        self.collector.requests.add(1, &self.attributes);
        self.collector
            .duration
            .record(self.started.elapsed().as_secs_f64(), &self.attributes);
        if status >= 400 || failure.is_some() {
            self.collector.errors.add(1, &self.attributes);
            self.collector
                .debug("request returned an error response", correlation_id);
        }
        if let Some(size) = response_bytes {
            self.collector.response_size.record(size, &self.attributes);
        }
        self.finished = true;
    }
}

impl Drop for RequestMeasurement {
    fn drop(&mut self) {
        // Status is absent from active-request attributes on both increment/decrement.
        self.collector.active.add(-1, &self.attributes[..2]);
        if !self.finished {
            self.context
                .span()
                .set_status(Status::error("request cancelled"));
            self.context.span().end();
            self.collector.warn("request future cancelled", "");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::{
        metrics::MeterProvider,
        trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState, TracerProvider},
    };
    use opentelemetry_sdk::{
        metrics::{
            InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
            data::{AggregatedMetrics, MetricData},
        },
        trace::{InMemorySpanExporter, SdkTracerProvider},
    };

    #[test]
    fn exported_metrics_and_spans_include_status_and_correlation_without_metric_cardinality() {
        let metrics = InMemoryMetricExporter::default();
        let meters = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(metrics.clone()).build())
            .build();
        let spans = InMemorySpanExporter::default();
        let traces = SdkTracerProvider::builder()
            .with_simple_exporter(spans.clone())
            .build();
        let mut collector = OpenTelemetryCollector::with_meter("test", meters.meter("test"))
            .with_tracer(traces.tracer("test"));
        for (name, kind, value) in [
            ("jobs", MetricType::Counter, 2.0),
            ("queue", MetricType::Gauge, 3.0),
            ("latency", MetricType::Histogram, 0.5),
            ("connections", MetricType::UpDownCounter, -1.0),
        ] {
            collector.define_metric(name, kind).unwrap();
            collector.record_metric(name, value, &[]).unwrap();
        }
        assert!(collector.record_metric("jobs", -1.0, &[]).is_err());
        assert!(collector.record_metric("jobs", f64::NAN, &[]).is_err());
        assert!(collector.record_metric("unknown", 1.0, &[]).is_err());
        assert!(collector.define_metric("jobs", MetricType::Gauge).is_err());
        let remote = SpanContext::new(
            TraceId::from(42),
            SpanId::from(7),
            TraceFlags::SAMPLED,
            true,
            TraceState::default(),
        );
        let parent = Context::new().with_remote_span_context(remote);
        collector
            .start_request_with_context("GET", "/users/{id}", &parent)
            .finish(500, Some(12), "correlation-123");
        collector.auth_failure("getUser", "correlation-123");
        // Cancellation balances active gauges and closes a failed span.
        drop(collector.start_request("GET", "/users/{id}"));
        meters.force_flush().unwrap();
        traces.force_flush().unwrap();
        let exported = metrics.get_finished_metrics().unwrap();
        let instruments: Vec<_> = exported
            .iter()
            .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
            .collect();
        assert!(instruments.iter().any(|metric| metric.name() == "jobs"));
        let requests = instruments
            .iter()
            .find(|metric| metric.name() == "http.server.request.count")
            .unwrap();
        let AggregatedMetrics::U64(MetricData::Sum(sum)) = requests.data() else {
            panic!("expected request counter");
        };
        let point = sum.data_points().next().unwrap();
        assert_eq!(point.value(), 1);
        let attributes: Vec<_> = point.attributes().cloned().collect();
        assert!(attributes.contains(&KeyValue::new("http.route", "/users/{id}")));
        assert!(attributes.contains(&KeyValue::new("http.response.status_code", 500_i64)));
        assert!(
            !attributes
                .iter()
                .any(|attribute| attribute.key.as_str().contains("correlation"))
        );
        let active = instruments
            .iter()
            .find(|metric| metric.name() == "http.server.active_requests")
            .unwrap();
        let AggregatedMetrics::I64(MetricData::Sum(sum)) = active.data() else {
            panic!("expected active up/down counter");
        };
        assert_eq!(sum.data_points().next().unwrap().value(), 0);
        let exported = spans.get_finished_spans().unwrap();
        assert_eq!(exported.len(), 2);
        assert_eq!(exported[0].span_context.trace_id(), TraceId::from(42));
        assert_eq!(exported[0].parent_span_id, SpanId::from(7));
        assert!(matches!(exported[0].status, Status::Error { .. }));
        assert!(exported[0].attributes.contains(&KeyValue::new(
            "forklaunch.correlation_id",
            "correlation-123"
        )));
        assert!(matches!(exported[1].status, Status::Error { .. }));
        meters.shutdown().unwrap();
        traces.shutdown().unwrap();
    }
}

/// Application-owned OTLP providers and lifecycle.
#[cfg(feature = "otlp")]
pub mod otlp;

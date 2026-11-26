use super::super::{NativeHttpPolicy, secure};
use super::*;
use axum::{
    body::{Body, to_bytes},
    routing::get,
};
use opentelemetry::{metrics::MeterProvider, trace::TracerProvider};
use opentelemetry_sdk::{
    metrics::{
        InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
        data::{AggregatedMetrics, MetricData},
    },
    propagation::TraceContextPropagator,
    trace::{InMemorySpanExporter, SdkTracerProvider},
};
use tower::ServiceExt;

#[tokio::test]
#[allow(clippy::too_many_lines)] // Keep the request/stream lifecycle assertions together.
async fn native_boundary_preserves_w3c_parent_and_counts_complete_and_cancelled_bodies() {
    let _exclusive = super::super::TRACE_TEST.lock().await;
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    let metrics = InMemoryMetricExporter::default();
    let meters = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(metrics.clone()).build())
        .build();
    let spans = InMemorySpanExporter::default();
    let traces = SdkTracerProvider::builder()
        .with_simple_exporter(spans.clone())
        .build();
    let collector = OpenTelemetryCollector::with_meter("native", meters.meter("native"))
        .with_tracer(traces.tracer("native"));
    let app = with_telemetry(
        secure(
            Router::new().route("/notes/{id}", get(|| async { "private-response" })),
            NativeHttpPolicy::new("https://notes.test").unwrap(),
        ),
        collector,
    );
    for (method, origin, expected, consume) in [
        ("GET", "https://notes.test", 200, true),
        ("POST", "https://notes.test", 405, true),
        ("GET", "https://evil.test", 403, true),
        ("GET", "https://notes.test", 200, false),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/notes/private-id?private-query=1")
                    .header("origin", origin)
                    .header(
                        "traceparent",
                        "00-1234567890abcdef1234567890abcdef-0123456789abcdef-01",
                    )
                    .header("x-correlation-id", "bounded-correlation")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), expected);
        assert_eq!(
            response.headers()["x-correlation-id"],
            "bounded-correlation"
        );
        assert!(
            response.headers()["traceparent"]
                .to_str()
                .unwrap()
                .starts_with("00-1234567890abcdef1234567890abcdef-")
        );
        if consume {
            to_bytes(response.into_body(), 1024).await.unwrap();
        } else {
            drop(response);
        }
    }
    meters.force_flush().unwrap();
    traces.force_flush().unwrap();
    let exported = spans.get_finished_spans().unwrap();
    assert_eq!(exported.len(), 4);
    for span in exported {
        assert_eq!(
            span.span_context.trace_id().to_string(),
            "1234567890abcdef1234567890abcdef"
        );
        assert_eq!(span.parent_span_id.to_string(), "0123456789abcdef");
        assert!(!span.name.contains("private-id"));
        assert!(span.attributes.contains(&opentelemetry::KeyValue::new(
            "forklaunch.correlation_id",
            "bounded-correlation"
        )));
    }
    let exported = metrics.get_finished_metrics().unwrap();
    let instruments: Vec<_> = exported
        .iter()
        .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
        .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
        .collect();
    let count = instruments
        .iter()
        .find(|m| m.name() == "http.server.request.count")
        .unwrap();
    let AggregatedMetrics::U64(MetricData::Sum(count)) = count.data() else {
        panic!("request counter");
    };
    assert_eq!(
        count
            .data_points()
            .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
            .sum::<u64>(),
        4
    );
    for point in count.data_points() {
        assert!(
            point
                .attributes()
                .all(|attribute| !attribute.key.as_str().contains("correlation")
                    && !attribute.value.to_string().contains("private-"))
        );
    }
    let active = instruments
        .iter()
        .find(|m| m.name() == "http.server.active_requests")
        .unwrap();
    let AggregatedMetrics::I64(MetricData::Sum(active)) = active.data() else {
        panic!("active counter");
    };
    assert!(active.data_points().all(|point| point.value() == 0));
    let sizes = instruments
        .iter()
        .find(|m| m.name() == "http.server.response.body.size")
        .unwrap();
    let AggregatedMetrics::U64(MetricData::Histogram(sizes)) = sizes.data() else {
        panic!("body histogram");
    };
    assert!(sizes.data_points().any(|point| point.sum() > 0));
    assert!(
        instruments
            .iter()
            .any(|m| m.name() == "http.server.request.duration")
    );
    meters.shutdown().unwrap();
    traces.shutdown().unwrap();
}

#[test]
fn correlation_rejects_unbounded_duplicate_and_sensitive_header_values() {
    for value in ["secret payload / with spaces".to_owned(), "x".repeat(129)] {
        let mut headers = HeaderMap::new();
        headers.insert("x-correlation-id", value.parse().unwrap());
        assert_ne!(correlation(&headers), value);
    }
    let mut headers = HeaderMap::new();
    headers.append("x-correlation-id", "one".parse().unwrap());
    headers.append("x-correlation-id", "two".parse().unwrap());
    assert_ne!(correlation(&headers), "one");
}

#[tokio::test]
async fn forwarded_context_and_correlation_match_the_exported_server_span() {
    let _exclusive = super::super::TRACE_TEST.lock().await;
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());
    let exported = InMemorySpanExporter::default();
    let traces = SdkTracerProvider::builder()
        .with_simple_exporter(exported.clone())
        .build();
    let collector = OpenTelemetryCollector::new("context").with_tracer(traces.tracer("context"));
    let app = with_telemetry(secure(Router::new().route("/context", get(|headers: HeaderMap| async move {
        let active = opentelemetry::Context::current();
        let span = active.span().span_context().clone();
        let expected = format!("00-{}-{}-{:02x}", span.trace_id(), span.span_id(), span.trace_flags().to_u8());
        assert_eq!(headers["traceparent"], expected);
        axum::Json(serde_json::json!({"parent":expected,"correlation":headers["x-correlation-id"].to_str().unwrap()}))
    })), NativeHttpPolicy::new("https://notes.test").unwrap()), collector);
    for incoming in [
        None,
        Some("00-1234567890abcdef1234567890abcdef-0123456789abcdef-01"),
    ] {
        let mut request = Request::builder()
            .uri("/context")
            .header("x-correlation-id", "invalid secret correlation");
        if let Some(parent) = incoming {
            request = request.header("traceparent", parent);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let parent = response.headers()["traceparent"]
            .to_str()
            .unwrap()
            .to_owned();
        let correlation = response.headers()["x-correlation-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["parent"], parent);
        assert_eq!(body["correlation"], correlation);
        assert_ne!(correlation, "invalid secret correlation");
        traces.force_flush().unwrap();
        let spans = exported.get_finished_spans().unwrap();
        let span = spans.last().unwrap();
        assert!(parent.contains(&span.span_context.trace_id().to_string()));
        assert!(parent.contains(&span.span_context.span_id().to_string()));
        if incoming.is_none() {
            assert_eq!(span.parent_span_id, opentelemetry::trace::SpanId::INVALID);
        }
    }
}

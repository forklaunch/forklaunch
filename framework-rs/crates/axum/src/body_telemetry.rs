//! Observes frames without buffering or changing response backpressure.
use axum::{body::Body, response::Response};
use bytes::Bytes;
use forklaunch_core::telemetry::RequestMeasurement;
use http_body::{Body as HttpBody, Frame, SizeHint};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub(crate) fn observe_response(
    response: Response,
    measurement: RequestMeasurement,
    correlation_id: String,
    head: bool,
) -> Response {
    let status = response.status().as_u16();
    response.map(|body| {
        if head || status == 204 || status == 304 || body.is_end_stream() {
            measurement.finish(status, Some(0), &correlation_id);
            body
        } else {
            Body::new(ObservedBody {
                inner: Box::pin(body),
                measurement: Some(measurement),
                bytes: 0,
                status,
                correlation_id,
            })
        }
    })
}

struct ObservedBody {
    inner: Pin<Box<Body>>,
    measurement: Option<RequestMeasurement>,
    bytes: u64,
    status: u16,
    correlation_id: String,
}

impl ObservedBody {
    fn finish(&mut self, failure: Option<&'static str>) {
        if let Some(measurement) = self.measurement.take() {
            if let Some(category) = failure {
                measurement.finish_failed(
                    self.status,
                    Some(self.bytes),
                    &self.correlation_id,
                    category,
                );
            } else {
                measurement.finish(self.status, Some(self.bytes), &self.correlation_id);
            }
        }
    }
}

impl HttpBody for ObservedBody {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        // Body producers execute inside the same server span as the handler.
        let _context = self
            .measurement
            .as_ref()
            .map(|measurement| measurement.context().attach());
        let result = self.inner.as_mut().poll_frame(cx);
        match &result {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    self.bytes = self.bytes.saturating_add(data.len() as u64);
                }
                if self.inner.is_end_stream() {
                    self.finish(None);
                }
            }
            Poll::Ready(Some(Err(_))) => self.finish(Some("response body failed")),
            Poll::Ready(None) => self.finish(None),
            Poll::Pending => {}
        }
        result
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for ObservedBody {
    fn drop(&mut self) {
        self.finish(Some("response body cancelled"));
    }
}

#[cfg(all(test, feature = "adapter"))]
mod tests {
    use super::*;
    use crate::{ForkLaunchRouter, RouterConfig};
    use axum::{body::to_bytes, http::Request};
    use forklaunch_core::contract::RouteContract;
    use forklaunch_core::telemetry::OpenTelemetryCollector;
    use opentelemetry::{
        metrics::MeterProvider,
        trace::{Status, TracerProvider},
    };
    use opentelemetry_sdk::{
        metrics::{
            InMemoryMetricExporter, PeriodicReader, SdkMeterProvider,
            data::{AggregatedMetrics, MetricData},
        },
        trace::{InMemorySpanExporter, SdkTracerProvider},
    };
    use std::{
        future::Future,
        sync::{Arc, Mutex},
    };
    use tokio::sync::oneshot;
    use tower::ServiceExt;

    // A response whose first byte is unavailable until the test permits it.
    // Unknown length exercises real streaming rather than Content-Length accounting.
    struct GateBody {
        receiver: oneshot::Receiver<Bytes>,
        ended: bool,
    }
    impl HttpBody for GateBody {
        type Data = Bytes;
        type Error = std::io::Error;
        fn poll_frame(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
            if self.ended {
                return Poll::Ready(None);
            }
            match Pin::new(&mut self.receiver).poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(result) => {
                    self.ended = true;
                    Poll::Ready(Some(
                        result
                            .map(Frame::data)
                            .map_err(|_| std::io::Error::other("test body error")),
                    ))
                }
            }
        }
        fn is_end_stream(&self) -> bool {
            self.ended
        }
    }

    #[tokio::test]
    async fn streaming_tracks_completion_cancellation_and_body_errors_without_buffering() {
        let metrics = InMemoryMetricExporter::default();
        let meters = SdkMeterProvider::builder()
            .with_reader(PeriodicReader::builder(metrics.clone()).build())
            .build();
        let spans = InMemorySpanExporter::default();
        let traces = SdkTracerProvider::builder()
            .with_simple_exporter(spans.clone())
            .build();
        let collector = OpenTelemetryCollector::with_meter("stream", meters.meter("stream"))
            .with_tracer(traces.tracer("stream"));
        for outcome in ["complete", "cancel", "error", "head"] {
            let (sender, receiver) = oneshot::channel();
            let body = Arc::new(Mutex::new(Some(Body::new(GateBody {
                receiver,
                ended: false,
            }))));
            let router = ForkLaunchRouter::anonymous()
                .get(
                    RouteContract::get("Stream", "Stream", "/stream").with_response(200, None),
                    move || {
                        let body = body.lock().unwrap().take().unwrap();
                        async move { body }
                    },
                )
                .with_config(RouterConfig {
                    telemetry: Some(collector.clone()),
                    ..RouterConfig::default()
                })
                .into_router();
            let response = router
                .oneshot(
                    Request::builder()
                        .method(if outcome == "head" { "HEAD" } else { "GET" })
                        .uri("/stream")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let before = spans.get_finished_spans().unwrap().len();
            if outcome == "head" {
                assert_eq!(before, 4);
                assert!(
                    to_bytes(response.into_body(), 1024)
                        .await
                        .unwrap()
                        .is_empty()
                );
            } else {
                // Headers arrive before the producer is ready; the span stays open.
                assert_eq!(
                    before,
                    match outcome {
                        "complete" => 0,
                        "cancel" => 1,
                        _ => 2,
                    }
                );
                match outcome {
                    "complete" => {
                        sender.send(Bytes::from_static(b"streamed bytes")).unwrap();
                        assert_eq!(
                            to_bytes(response.into_body(), 1024).await.unwrap(),
                            "streamed bytes"
                        );
                    }
                    "cancel" => {
                        drop(response);
                        drop(sender);
                    }
                    _ => {
                        drop(sender);
                        assert!(to_bytes(response.into_body(), 1024).await.is_err());
                    }
                }
            }
        }
        traces.force_flush().unwrap();
        meters.force_flush().unwrap();
        let exported = spans.get_finished_spans().unwrap();
        assert_eq!(exported.len(), 4);
        assert_eq!(exported[0].status, Status::Unset);
        assert!(
            matches!(&exported[1].status, Status::Error { description } if description == "response body cancelled")
        );
        assert!(
            matches!(&exported[2].status, Status::Error { description } if description == "response body failed")
        );
        assert_eq!(exported[3].status, Status::Unset);
        assert_exported_metrics(&metrics);
        meters.shutdown().unwrap();
        traces.shutdown().unwrap();
    }
    fn assert_exported_metrics(metrics: &InMemoryMetricExporter) {
        let exported = metrics.get_finished_metrics().unwrap();
        let instruments: Vec<_> = exported
            .iter()
            .flat_map(opentelemetry_sdk::metrics::data::ResourceMetrics::scope_metrics)
            .flat_map(opentelemetry_sdk::metrics::data::ScopeMetrics::metrics)
            .collect();
        let sizes = instruments
            .iter()
            .find(|metric| metric.name() == "http.server.response.body.size")
            .unwrap();
        let AggregatedMetrics::U64(MetricData::Histogram(histogram)) = sizes.data() else {
            panic!("expected size histogram");
        };
        assert_eq!(
            histogram
                .data_points()
                .map(opentelemetry_sdk::metrics::data::HistogramDataPoint::sum)
                .sum::<u64>(),
            14
        );
        let active = instruments
            .iter()
            .find(|metric| metric.name() == "http.server.active_requests")
            .unwrap();
        let AggregatedMetrics::I64(MetricData::Sum(sum)) = active.data() else {
            panic!("expected active counter");
        };
        assert!(sum.data_points().all(|point| point.value() == 0));
        let requests = instruments
            .iter()
            .find(|metric| metric.name() == "http.server.request.count")
            .unwrap();
        let AggregatedMetrics::U64(MetricData::Sum(sum)) = requests.data() else {
            panic!("expected request count");
        };
        assert_eq!(
            sum.data_points()
                .map(opentelemetry_sdk::metrics::data::SumDataPoint::value)
                .sum::<u64>(),
            4
        );
    }
}

//! Native routers share the framework's streaming-aware `OpenTelemetry` collector.
use axum::{
    Router,
    extract::{MatchedPath, Request, State},
    http::{HeaderMap, HeaderValue, Method},
    middleware::{self, Next},
    response::Response,
};
use forklaunch_core::telemetry::OpenTelemetryCollector;
use opentelemetry::{
    propagation::Extractor,
    trace::{FutureExt, TraceContextExt},
};
use uuid::Uuid;

struct Headers<'a>(&'a HeaderMap);
impl Extractor for Headers<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        let mut values = self.0.get_all(key).iter();
        let value = values.next()?;
        if values.next().is_some() {
            return None;
        }
        value.to_str().ok()
    }
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(axum::http::HeaderName::as_str).collect()
    }
}

fn correlation(headers: &HeaderMap) -> String {
    Headers(headers)
        .get("x-correlation-id")
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
        .map_or_else(|| Uuid::new_v4().to_string(), str::to_owned)
}

async fn observe(
    State(collector): State<OpenTelemetryCollector>,
    mut request: Request,
    next: Next,
) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str);
    let method = match *request.method() {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        Method::CONNECT => "CONNECT",
        Method::TRACE => "TRACE",
        _ => "OTHER",
    };
    let correlation = correlation(request.headers());
    let parent = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&Headers(request.headers()))
    });
    let measurement = collector.start_request_with_context(method, route, &parent);
    request.headers_mut().insert(
        "x-correlation-id",
        HeaderValue::from_str(&correlation).expect("bounded ASCII correlation"),
    );
    let context = measurement.context();
    let span = context.span().span_context().clone();
    let head = request.method() == Method::HEAD;
    let mut response = next.run(request).with_context(context).await;
    if span.is_valid() {
        let traceparent = format!(
            "00-{}-{}-{:02x}",
            span.trace_id(),
            span.span_id(),
            span.trace_flags().to_u8()
        );
        if let Ok(value) = HeaderValue::from_str(&traceparent) {
            response.headers_mut().insert("traceparent", value);
        }
    }
    response.headers_mut().insert(
        "x-correlation-id",
        HeaderValue::from_str(&correlation).expect("bounded ASCII correlation"),
    );
    crate::body_telemetry::observe_response(response, measurement, correlation, head)
}

/// Add native OTLP spans and request metrics around the completed secure router.
///
/// Apply after `secure` so the original W3C parent is captured before tracing
/// headers are assigned and middleware rejections are measured too. Response
/// frames are observed without buffering; active requests end on body completion
/// or cancellation. Only bounded method, route template and status enter metric
/// dimensions. The application owns the telemetry runtime and must flush it after
/// graceful HTTP shutdown.
#[must_use = "use the returned router to install telemetry"]
pub fn with_telemetry<S: Clone + Send + Sync + 'static>(
    router: Router<S>,
    collector: OpenTelemetryCollector,
) -> Router<S> {
    router.layer(middleware::from_fn_with_state(collector, observe))
}

#[cfg(test)]
#[path = "telemetry_tests.rs"]
mod tests;

//! Request tracing only: no request bodies, cookies, query strings, or tenant identifiers.
use axum::{
    Router,
    body::Body,
    extract::{MatchedPath, Request},
    http::{HeaderValue, Response},
    middleware::{self, Next},
};
use opentelemetry::trace::TraceContextExt;
use std::time::Duration;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

fn trace_id(value: &str) -> Option<&str> {
    let b = value.as_bytes();
    if b.len() != 55 || &b[..3] != b"00-" || b[35] != b'-' || b[52] != b'-' {
        return None;
    }
    if !b
        .iter()
        .enumerate()
        .all(|(i, c)| matches!(i, 2 | 35 | 52) || c.is_ascii_digit() || (b'a'..=b'f').contains(c))
        || b[3..35].iter().all(|c| *c == b'0')
        || b[36..52].iter().all(|c| *c == b'0')
    {
        return None;
    }
    Some(&value[3..35])
}

async fn context(mut request: Request, next: Next) -> Response<Body> {
    // The managed telemetry layer already owns the server span. Reuse its
    // identity so logs and forwarded headers describe the exported span.
    let active = opentelemetry::Context::current();
    let current = active.span().span_context().clone();
    let parent = if current.is_valid() {
        HeaderValue::from_str(&format!(
            "00-{}-{}-{:02x}",
            current.trace_id(),
            current.span_id(),
            current.trace_flags().to_u8()
        ))
        .expect("OpenTelemetry hexadecimal context")
    } else {
        let incoming = request
            .headers()
            .get("traceparent")
            .and_then(|v| v.to_str().ok());
        let id = incoming
            .and_then(trace_id)
            .map_or_else(|| Uuid::new_v4().simple().to_string(), str::to_owned);
        let span = Uuid::new_v4().simple().to_string();
        HeaderValue::from_str(&format!("00-{id}-{}-01", &span[..16]))
            .expect("generated hexadecimal context")
    };
    request.headers_mut().insert("traceparent", parent.clone());
    let mut response = next.run(request).await;
    response.headers_mut().insert("traceparent", parent);
    response
}

/// Attach request tracing to an ordinary Axum router. Only method, matched route
/// template, validated trace ID, response status and elapsed time are recorded.
#[must_use = "use the returned router to install instrumentation"]
pub fn instrument<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router.layer(TraceLayer::new_for_http()
        .make_span_with(|request: &Request| {
            let route = request.extensions().get::<MatchedPath>().map_or("unmatched", MatchedPath::as_str);
            let id = request.headers().get("traceparent").and_then(|v| v.to_str().ok()).and_then(trace_id).unwrap_or("unavailable");
            ::tracing::info_span!("http.request", method = %request.method(), route, trace_id = id)
        })
        .on_request(())
        .on_response(|response: &Response<Body>, latency: Duration, span: &::tracing::Span| {
            span.in_scope(|| ::tracing::info!(status = response.status().as_u16(), elapsed_us = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX), "http.response"));
        })
        .on_failure(()))
        .layer(middleware::from_fn(context))
}

#[cfg(test)]
mod tests {
    use super::super::TRACE_TEST;
    use super::*;
    use tower::ServiceExt;
    #[tokio::test]
    async fn context_preserves_valid_trace_and_replaces_invalid() {
        let _exclusive = TRACE_TEST.lock().await;
        let app = instrument(Router::new().route("/health", axum::routing::get(|| async { "ok" })));
        for (parent, expected) in [
            (
                "00-11111111111111111111111111111111-2222222222222222-00",
                Some("11111111111111111111111111111111"),
            ),
            ("secret-not-a-trace", None),
            (
                "00-00000000000000000000000000000000-2222222222222222-01",
                None,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .header("traceparent", parent)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let value = response.headers()["traceparent"].to_str().unwrap();
            let actual = trace_id(value).unwrap();
            if let Some(expected) = expected {
                assert_eq!(actual, expected);
            }
            assert_ne!(value, parent);
        }
    }
    #[tokio::test]
    async fn traces_record_templates_and_never_sensitive_request_data() {
        use std::sync::{Arc, Mutex};
        #[derive(Clone)]
        struct Capture(Arc<Mutex<String>>);
        impl ::tracing::field::Visit for Capture {
            fn record_debug(
                &mut self,
                field: &::tracing::field::Field,
                value: &dyn std::fmt::Debug,
            ) {
                use std::fmt::Write;
                writeln!(self.0.lock().unwrap(), "{}={value:?}", field.name()).unwrap();
            }
        }
        impl ::tracing::Subscriber for Capture {
            fn enabled(&self, _: &::tracing::Metadata<'_>) -> bool {
                true
            }
            fn new_span(&self, attrs: &::tracing::span::Attributes<'_>) -> ::tracing::span::Id {
                attrs.record(&mut self.clone());
                ::tracing::span::Id::from_u64(1)
            }
            fn record(&self, _: &::tracing::span::Id, values: &::tracing::span::Record<'_>) {
                values.record(&mut self.clone());
            }
            fn record_follows_from(&self, _: &::tracing::span::Id, _: &::tracing::span::Id) {}
            fn event(&self, event: &::tracing::Event<'_>) {
                event.record(&mut self.clone());
            }
            fn enter(&self, _: &::tracing::span::Id) {}
            fn exit(&self, _: &::tracing::span::Id) {}
        }
        let _exclusive = TRACE_TEST.lock().await;
        let capture = Capture(Arc::new(Mutex::new(String::new())));
        let _subscriber = ::tracing::subscriber::set_default(capture.clone());
        let app =
            instrument(Router::new().route("/notes/{id}", axum::routing::post(|| async { "ok" })));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/notes/secret-tenant?secret-query=true")
                    .header("cookie", "secret-cookie")
                    .body(Body::from("secret-note-content"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let recorded = capture.0.lock().unwrap();
        assert!(recorded.contains("/notes/{id}"));
        assert!(recorded.contains("status=200"));
        assert!(
            !recorded.contains("secret-"),
            "sensitive input reached trace fields"
        );
    }
}

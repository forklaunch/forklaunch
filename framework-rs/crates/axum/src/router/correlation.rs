//! Opt-in TypeScript request correlation, independent of native request IDs.
use axum::{
    Router,
    extract::Request,
    http::{Extensions, HeaderMap, HeaderValue},
    middleware::{self, Next},
    response::Response,
};

/// Selection and propagation of the framework request correlation ID.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CorrelationIdFormat {
    /// Preserve native context fallback and independent x-request-id behavior.
    #[default]
    Native,
    /// Join supplied correlation fields using Node's Latin-1 semantics, or generate
    /// a `UUIDv4` for an absent/empty value. Propagate unless the application supplies
    /// a response header. Original request headers remain unchanged for validation.
    /// With the HTML origin profile, reproduce finalhandler's non-HEAD UTF-8
    /// header serialization; other responses retain the original Latin-1 bytes.
    TypeScript,
}

#[derive(Clone)]
struct SelectedCorrelation {
    context: String,
    wire: HeaderValue,
}

pub(super) fn resolve<'a>(
    extensions: &'a Extensions,
    headers: &'a HeaderMap,
    fallback: &'a str,
) -> &'a str {
    extensions.get::<SelectedCorrelation>().map_or_else(
        || {
            headers
                .get("x-correlation-id")
                .or_else(|| headers.get("x-request-id"))
                .and_then(|value| value.to_str().ok())
                .unwrap_or(fallback)
        },
        |selected| selected.context.as_str(),
    )
}

pub(super) fn wrap(router: Router, format: CorrelationIdFormat) -> Router {
    if format == CorrelationIdFormat::TypeScript {
        // Outside CORS, limits and router dispatch, including unknown paths.
        Router::new()
            .fallback_service(router)
            .layer(middleware::from_fn(propagate))
    } else {
        router
    }
}

fn joined_header(headers: &HeaderMap) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (index, value) in headers.get_all("x-correlation-id").iter().enumerate() {
        if index > 0 {
            bytes.extend_from_slice(b", ");
        }
        // Node's HTTP parser strips only optional space/tab around field values.
        let mut value = value.as_bytes();
        while value
            .first()
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            value = &value[1..];
        }
        while value
            .last()
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            value = &value[..value.len() - 1];
        }
        bytes.extend_from_slice(value);
    }
    bytes
}

pub(super) fn supplied_header(extensions: &Extensions, headers: &HeaderMap) -> Option<String> {
    (extensions.get::<SelectedCorrelation>().is_some() && headers.contains_key("x-correlation-id"))
        .then(|| joined_header(headers).into_iter().map(char::from).collect())
}

fn select(headers: &HeaderMap) -> SelectedCorrelation {
    let mut bytes = joined_header(headers);
    if bytes.is_empty() {
        bytes = uuid::Uuid::new_v4().to_string().into_bytes();
    }
    let context = bytes.iter().map(|byte| char::from(*byte)).collect();
    // Input HeaderValues and the ASCII join delimiter are already valid.
    let wire = HeaderValue::from_bytes(&bytes).expect("joined valid correlation header values");
    SelectedCorrelation { context, wire }
}

async fn propagate(mut request: Request, next: Next) -> Response {
    let selected = select(request.headers());
    request.extensions_mut().insert(selected.clone());
    let mut response = next.run(request).await;
    let html_utf8 = response
        .extensions_mut()
        .remove::<super::origin::HtmlUtf8Headers>()
        .is_some();
    if !response.headers().contains_key("x-correlation-id") {
        let header = if html_utf8 {
            HeaderValue::from_bytes(selected.context.as_bytes())
                .expect("Latin-1 context encoded as UTF-8 remains a valid header")
        } else {
            selected.wire
        };
        response.headers_mut().insert("x-correlation-id", header);
    }
    response
}

#[cfg(test)]
mod tests;

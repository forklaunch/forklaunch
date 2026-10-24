//! Finite transport collection for selected default error responses.
use super::{Response, framework_response_to_axum};
use axum::body::{Body, to_bytes};
use forklaunch_core::contract::RouteContract;
use forklaunch_core::http::{
    Body as FrameworkBody, DefaultErrorResponses, FrameworkRequest, FrameworkResponse,
};

pub(super) fn validate_configuration(
    config: &super::RouterConfig,
    routes: &[RouteContract],
) -> Result<(), String> {
    for route in routes {
        let selected = |status| {
            config
                .default_error_responses
                .limits(route, status)
                .is_some()
        };
        let requires_auth = route.auth.is_some()
            || route.rbac.is_some()
            || route.access != forklaunch_core::contract::AccessLevel::Public;
        if (selected(401) || selected(403) || selected(400) && requires_auth)
            && config.auth_error_format != super::AuthErrorFormat::TypeScriptText
        {
            return Err("Selected authentication error statuses require the TypeScript authentication error profile".into());
        }
        if selected(400) && config.request_validators.contains(route) {
            return Err("Request validators must declare their own rejection response instead of selecting a default 400".into());
        }
        config
            .default_error_responses
            .validate_execution(route, config.parser_error_format)?;
    }
    Ok(())
}
pub(super) async fn normalize(
    profiles: &DefaultErrorResponses,
    route: &RouteContract,
    request: &FrameworkRequest,
    response: Response,
) -> Response {
    let Some((max_bytes, timeout)) = profiles.limits(route, response.status().as_u16()) else {
        return response;
    };
    let (parts, body) = response.into_parts();
    let Ok(Ok(bytes)) = tokio::time::timeout(timeout, to_bytes(body, max_bytes)).await else {
        return replace(parts, safe_failure(request));
    };
    if bytes.is_empty() {
        let mut parts = parts;
        if parts
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case("text/plain")
            })
        {
            parts.headers.insert(
                "content-type",
                axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
            );
        }
        return Response::from_parts(parts, Body::from(bytes));
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return replace(parts, safe_failure(request));
    };
    let candidate = FrameworkResponse {
        status_code: parts.status.as_u16(),
        headers: parts
            .headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.to_string(), value.to_owned()))
            })
            .collect(),
        body: if parts
            .headers
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .eq_ignore_ascii_case("application/octet-stream")
            }) {
            FrameworkBody::Binary(bytes.to_vec())
        } else {
            FrameworkBody::Text(text.to_owned())
        },
    };
    let normalized = match profiles.normalize(route, candidate, &request.context.correlation_id) {
        Ok(response) | Err(response) => response,
    };
    replace(parts, normalized)
}

fn safe_failure(request: &FrameworkRequest) -> FrameworkResponse {
    forklaunch_core::http::typescript_parser_error_response(500, &request.context.correlation_id)
}
fn replace(mut parts: axum::http::response::Parts, replacement: FrameworkResponse) -> Response {
    let (replacement, body) = framework_response_to_axum(replacement).into_parts();
    parts.status = replacement.status;
    for name in [
        "content-type",
        "content-length",
        "content-encoding",
        "content-range",
        "transfer-encoding",
        "etag",
        "digest",
        "content-digest",
        "repr-digest",
        "content-md5",
        "trailer",
        "last-modified",
    ] {
        parts.headers.remove(name);
    }
    // Preserve repeated application headers and extensions; replace only values
    // explicitly changed by normalization, not the flattened candidate's copies.
    for (name, value) in &replacement.headers {
        if !parts.headers.contains_key(name) || name == "content-type" {
            parts.headers.insert(name.clone(), value.clone());
        }
    }
    Response::from_parts(parts, body)
}

#[cfg(test)]
#[path = "default_errors_tests.rs"]
mod tests;

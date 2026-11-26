//! Formatting for responses produced by router fallbacks, never handler responses.
use super::{ParserErrorFormat, RouterConfig, framework_response_to_axum};
use axum::{
    Router,
    extract::{MatchedPath, Request},
    http::{Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use forklaunch_core::http::{match_path_checked, typescript_parser_error_response};
use std::fmt::Write;

/// Wire policy for framework-owned unknown-path and unsupported-method fallbacks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RouterOriginFormat {
    /// Axum's empty 404/405 responses and native Allow header.
    #[default]
    Native,
    /// Express finalhandler's escaped HTML 404, including HEAD representation headers,
    /// and automatic OPTIONS responses derived from registered methods.
    /// Existing application fallbacks, documentation and internal routes remain owned
    /// by their routers. A root internal mount retains ownership of the root fallback.
    /// CORS and correlation propagation are independent policies.
    TypeScriptHtml,
}

pub(super) fn extended(
    router: axum::routing::MethodRouter,
    methods: std::collections::HashMap<String, axum::routing::MethodRouter>,
    allow: String,
    config: &RouterConfig,
) -> axum::routing::MethodRouter {
    use tower::ServiceExt;
    let origin_format = config.router_origin_format;
    let parser_format = config.parser_error_format;
    router.fallback(move |request: Request| {
        let handler = methods.get(request.method().as_str()).cloned();
        let allow = allow.clone();
        async move {
            match handler {
                Some(handler) => match handler.oneshot(request).await {
                    Ok(response) => response,
                    Err(never) => match never {},
                },
                None if origin_format == RouterOriginFormat::TypeScriptHtml => {
                    fallback(&request, parser_format, Some(&allow))
                }
                None => (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, allow)]).into_response(),
            }
        }
    })
}

pub(super) fn selected_options(
    router: axum::routing::MethodRouter,
    path: &str,
    registry: &forklaunch_core::contract::ContractRegistry,
    config: &RouterConfig,
) -> axum::routing::MethodRouter {
    if !config.typescript_options_paths.contains(path) {
        return router;
    }
    let mut methods: Vec<_> = registry
        .routes()
        .iter()
        .filter(|route| route.openapi_path == path)
        .map(|route| route.method.as_str())
        .collect();
    if methods.contains(&"OPTIONS") {
        return router;
    }
    if methods.contains(&"GET") {
        methods.push("HEAD");
    }
    methods.sort_unstable();
    methods.dedup();
    let allow = methods.join(", ");
    let parser_format = config.parser_error_format;
    router.options(move |request: Request| {
        let allow = allow.clone();
        async move {
            let allow = request
                .extensions()
                .get::<super::case_routes::OptionsAllow>()
                .map_or(allow.as_str(), |value| value.0.as_str());
            fallback(&request, parser_format, Some(allow))
        }
    })
}

pub(super) fn install(
    mut router: Router,
    config: &RouterConfig,
    root_owned: bool,
    registry: &forklaunch_core::contract::ContractRegistry,
) -> Router {
    if config.router_origin_format == RouterOriginFormat::TypeScriptHtml {
        let parser_format = config.parser_error_format;
        let mut allowed = std::collections::HashMap::<String, Vec<&str>>::new();
        for route in registry.routes() {
            let methods = allowed.entry(route.openapi_path.clone()).or_default();
            methods.push(route.method.as_str());
            if route.method == forklaunch_core::http::HttpMethod::Get {
                methods.push("HEAD");
            }
        }
        let allowed = std::sync::Arc::new(
            allowed
                .into_iter()
                .map(|(path, mut methods)| {
                    methods.sort_unstable();
                    methods.dedup();
                    (path, methods.join(", "))
                })
                .collect::<std::collections::HashMap<_, _>>(),
        );
        router = router.method_not_allowed_fallback(move |request: Request| {
            let allowed = allowed.clone();
            async move {
                let allow = request
                    .extensions()
                    .get::<MatchedPath>()
                    .and_then(|path| allowed.get(path.as_str()))
                    .map(String::as_str);
                fallback(&request, parser_format, allow)
            }
        });
        if !root_owned {
            router = router.fallback(move |request: Request| async move {
                fallback(&request, parser_format, None)
            });
        }
    }
    router
}

pub(super) fn wrap(router: Router, format: RouterOriginFormat) -> Router {
    // MethodRouter appends Allow after its route layers. Wrap the completed
    // router so only marked framework fallback responses lose that header.
    if format == RouterOriginFormat::TypeScriptHtml {
        Router::new()
            .fallback_service(router)
            .layer(middleware::from_fn(remove_allow))
    } else {
        router
    }
}

#[derive(Clone)]
struct OriginResponse;

/// Express finalhandler serializes headers with its non-HEAD UTF-8 HTML chunk.
#[derive(Clone)]
pub(super) struct HtmlUtf8Headers;

pub(super) async fn remove_allow(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    if response
        .extensions_mut()
        .remove::<OriginResponse>()
        .is_some()
    {
        response.headers_mut().remove(header::ALLOW);
    }
    response
}

pub(super) fn fallback(
    request: &Request,
    parser_format: ParserErrorFormat,
    allow: Option<&str>,
) -> Response {
    if parser_format == ParserErrorFormat::TypeScriptText
        && let Some(pattern) = request.extensions().get::<MatchedPath>()
        && match_path_checked(pattern.as_str(), request.uri().path()).is_err()
    {
        let mut response =
            framework_response_to_axum(typescript_parser_error_response(400, "No correlation ID"));
        response.extensions_mut().insert(OriginResponse);
        return response;
    }
    if request.method() == Method::OPTIONS
        && let Some(allow) = allow
    {
        return (
            StatusCode::OK,
            [
                (header::ALLOW, allow.to_owned()),
                (header::CONTENT_TYPE, "text/plain".to_owned()),
                (header::CONTENT_LENGTH, allow.len().to_string()),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            ],
            allow.to_owned(),
        )
            .into_response();
    }
    let path = encode_path(request.uri().path());
    let message = format!("Cannot {} {path}", request.method());
    let escaped = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;");
    let body = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Error</title>\n</head>\n<body>\n<pre>{escaped}</pre>\n</body>\n</html>\n"
    );
    let length = body.len().to_string();
    let body = if request.method() == Method::HEAD {
        String::new()
    } else {
        body
    };
    let mut response = (
        StatusCode::NOT_FOUND,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_LENGTH, length.as_str()),
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        body,
    )
        .into_response();
    response.extensions_mut().insert(OriginResponse);
    if request.method() != Method::HEAD {
        response.extensions_mut().insert(HtmlUtf8Headers);
    }
    response
}

// encodeurl's ASCII URL admission, preserving valid percent escapes. HTTP Uri
// contains UTF-8 bytes, not arbitrary JavaScript strings/unpaired surrogates.
fn encode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut result = String::with_capacity(path.len());
    let mut index = 0;
    while index < bytes.len() {
        let start = index;
        while let Some(length) = encoding_token(&bytes[index..]) {
            index += length;
        }
        if start == index {
            result.push(char::from(bytes[index]));
            index += 1;
        } else {
            // encodeURI is applied to the entire matched run, including any
            // characters consumed by an invalid-percent token.
            for byte in &bytes[start..index] {
                if byte.is_ascii_alphanumeric() || b";/?:@&=+$,#-_.!~*'()".contains(byte) {
                    result.push(char::from(*byte));
                } else {
                    let _ = write!(result, "%{byte:02X}");
                }
            }
        }
    }
    result
}

fn encoding_token(bytes: &[u8]) -> Option<usize> {
    let byte = *bytes.first()?;
    if !matches!(byte, b'!' | b'#'..=b';' | b'=' | b'?'..=b'_' | b'a'..=b'z' | b'|' | b'~') {
        return Some(1);
    }
    if byte == b'%' {
        return match bytes.get(1) {
            None => Some(1),
            Some(next) if !next.is_ascii_hexdigit() => Some(2),
            Some(_) if bytes.get(2).is_some_and(|next| !next.is_ascii_hexdigit()) => Some(3),
            // encodeurl preserves the peculiar trailing single-hex form `%A`.
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests;

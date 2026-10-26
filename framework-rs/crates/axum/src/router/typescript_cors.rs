//! Opt-in compatibility with the TypeScript application's cors middleware.
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use tower::{Layer, ServiceExt};

mod delegate;
pub use delegate::{
    TypeScriptCorsOriginDecision, TypeScriptCorsOriginDelegate, TypeScriptCorsOriginError,
};

/// Origin policies supported by the TypeScript CORS profile.
#[derive(Debug, Clone, Default)]
pub enum TypeScriptCorsOrigin {
    /// Emit `*`, including requests without Origin.
    #[default]
    Any,
    /// Emit the configured origin even if the request origin differs.
    Fixed(HeaderValue),
    /// Reflect a supplied Origin and vary on Origin.
    Reflect,
    /// Reflect an exact matching origin; denied origins still get other CORS headers.
    AllowList(Vec<HeaderValue>),
    /// Bypass CORS entirely, including OPTIONS interception.
    Disabled,
    /// Await a trusted origin callback before CORS and route dispatch.
    Delegate(TypeScriptCorsOriginDelegate),
}

/// TypeScript cors options. Trusted origin delegates are supported; regular
/// expressions and arbitrary JavaScript coercion are separate capabilities.
#[derive(Debug, Clone)]
pub struct TypeScriptCors {
    pub origin: TypeScriptCorsOrigin,
    pub methods: HeaderValue,
    /// None reflects requested headers and varies on that request field.
    /// An explicitly empty value models an empty allowedHeaders array.
    pub allowed_headers: Option<HeaderValue>,
    pub exposed_headers: Option<HeaderValue>,
    pub credentials: bool,
    /// Nonnegative JavaScript-safe integer seconds (at most 2^53 - 1).
    pub max_age: Option<u64>,
    /// Full JavaScript Number `maxAge`, including fractions, negatives and infinities.
    /// When supplied, takes precedence over the legacy safe-integer `max_age` field.
    pub max_age_number: Option<f64>,
    /// Continue OPTIONS through routing, including explicit OPTIONS handlers.
    pub preflight_continue: bool,
    /// Must be a final HTTP status (200–599); default204 matches TypeScript.
    /// Redirect and error statuses end preflight with an empty body as configured.
    /// Hyper omits zero Content-Length on204/304; Express cors sends it explicitly.
    pub options_success_status: StatusCode,
}

impl Default for TypeScriptCors {
    fn default() -> Self {
        Self {
            origin: TypeScriptCorsOrigin::Any,
            methods: HeaderValue::from_static("GET,HEAD,PUT,PATCH,POST,DELETE"),
            allowed_headers: None,
            exposed_headers: None,
            credentials: false,
            max_age: None,
            max_age_number: None,
            preflight_continue: false,
            options_success_status: StatusCode::NO_CONTENT,
        }
    }
}

/// CORS headers prepared before invoking the handler. A handler porting Express
/// incremental `res.vary` operations can seed its response from this map before
/// appending fields. Returned header values override the prepared defaults;
/// deleting a header from a fresh Axum response cannot represent Express removal.
#[derive(Debug, Clone)]
pub struct TypeScriptCorsHeaders(pub HeaderMap);

impl TypeScriptCors {
    fn disabled(&self) -> bool {
        matches!(&self.origin, TypeScriptCorsOrigin::Disabled)
            || matches!(&self.origin, TypeScriptCorsOrigin::Fixed(value) if value.is_empty())
    }

    fn headers(&self, request: &Request) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let mut vary = Vec::new();
        let origin = joined(request.headers(), header::ORIGIN);
        match &self.origin {
            TypeScriptCorsOrigin::Any => {
                headers.insert(
                    header::ACCESS_CONTROL_ALLOW_ORIGIN,
                    HeaderValue::from_static("*"),
                );
            }
            TypeScriptCorsOrigin::Fixed(value) => {
                headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value.clone());
                if value != "*" {
                    vary.push("Origin");
                }
            }
            TypeScriptCorsOrigin::Reflect | TypeScriptCorsOrigin::AllowList(_) => {
                let allowed = match &self.origin {
                    TypeScriptCorsOrigin::AllowList(values) => {
                        origin.as_ref().is_some_and(|value| values.contains(value))
                    }
                    _ => true,
                };
                if allowed
                    && let Some(origin) = origin
                    && !origin.is_empty()
                {
                    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
                }
                vary.push("Origin");
            }
            TypeScriptCorsOrigin::Disabled | TypeScriptCorsOrigin::Delegate(_) => return headers,
        }
        if self.credentials {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
                HeaderValue::from_static("true"),
            );
        }
        if let Some(value) = &self.exposed_headers
            && !value.is_empty()
        {
            headers.insert(header::ACCESS_CONTROL_EXPOSE_HEADERS, value.clone());
        }
        if request.method() == Method::OPTIONS {
            if !self.methods.is_empty() {
                headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, self.methods.clone());
            }
            let allowed = if let Some(value) = &self.allowed_headers {
                Some(value.clone())
            } else {
                vary.push("Access-Control-Request-Headers");
                joined(request.headers(), header::ACCESS_CONTROL_REQUEST_HEADERS)
            };
            if let Some(value) = allowed
                && !value.is_empty()
            {
                headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, value);
            }
            if let Some(age) = self.max_age_number {
                headers.insert(
                    header::ACCESS_CONTROL_MAX_AGE,
                    HeaderValue::from_str(&forklaunch_common::javascript_number::to_string(age))
                        .expect("JavaScript numeric text is a valid header"),
                );
            } else if let Some(age) = self.max_age {
                headers.insert(
                    header::ACCESS_CONTROL_MAX_AGE,
                    HeaderValue::from_str(&age.to_string()).expect("decimal age is a valid header"),
                );
            }
        }
        if !vary.is_empty() {
            headers.insert(
                header::VARY,
                HeaderValue::from_str(&vary.join(", ")).expect("static Vary names"),
            );
        }
        headers
    }
}

fn joined(headers: &HeaderMap, name: header::HeaderName) -> Option<HeaderValue> {
    let mut bytes = Vec::new();
    let mut present = false;
    for value in headers.get_all(name) {
        if present {
            bytes.extend_from_slice(b", ");
        }
        present = true;
        bytes.extend_from_slice(value.as_bytes());
    }
    present.then(|| HeaderValue::from_bytes(&bytes).expect("joining valid header values"))
}

pub(super) fn wrap(
    router: Router,
    native: Option<tower_http::cors::CorsLayer>,
    typescript: Option<TypeScriptCors>,
) -> Router {
    assert!(
        native.is_none() || typescript.is_none(),
        "native and TypeScript CORS profiles are mutually exclusive"
    );
    if let Some(config) = typescript {
        return wrap_typescript(router, config);
    }
    let cors = native.unwrap_or_else(tower_http::cors::CorsLayer::permissive);
    router.layer(middleware::from_fn(move |request: Request, next: Next| {
        let cors = cors.clone();
        async move {
            // Only a genuine CORS preflight may bypass an explicit OPTIONS handler.
            let preflight = request.headers().contains_key(header::ORIGIN)
                && request
                    .headers()
                    .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);
            if request.method() == Method::OPTIONS && !preflight {
                return next.run(request).await;
            }
            let service = tower::service_fn(move |request| {
                let next = next.clone();
                async move { Ok::<_, std::convert::Infallible>(next.run(request).await) }
            });
            match cors.layer(service).oneshot(request).await {
                Ok(response) => response,
                Err(never) => match never {},
            }
        }
    }))
}

fn wrap_typescript(router: Router, config: TypeScriptCors) -> Router {
    assert!(
        config.max_age_number.is_some()
            || config
                .max_age
                .is_none_or(|age| age <= 9_007_199_254_740_991),
        "TypeScript CORS max age must be a JavaScript-safe integer"
    );
    assert!(
        (200..=599).contains(&config.options_success_status.as_u16()),
        "TypeScript CORS options success status must be a final HTTP status (200–599)"
    );
    Router::new()
        .fallback_service(router)
        .layer(middleware::from_fn_with_state(config, apply))
}

async fn apply(
    State(mut config): State<TypeScriptCors>,
    mut request: Request,
    next: Next,
) -> Response {
    if let TypeScriptCorsOrigin::Delegate(callback) = &config.origin {
        config.origin = match callback
            .resolve(joined(request.headers(), header::ORIGIN))
            .await
        {
            Ok(origin) => origin,
            Err(_) => {
                return super::framework_response_to_axum(super::local_handler_failure_response(
                    super::HandlerErrorFormat::TypeScriptText,
                    "No correlation ID",
                ));
            }
        };
    }
    if config.disabled() {
        return next.run(request).await;
    }
    let headers = config.headers(&request);
    if request.method() == Method::OPTIONS && !config.preflight_continue {
        let mut response = config.options_success_status.into_response();
        response.headers_mut().extend(headers);
        response
            .headers_mut()
            .insert(header::CONTENT_LENGTH, HeaderValue::from_static("0"));
        return response;
    }
    request
        .extensions_mut()
        .insert(TypeScriptCorsHeaders(headers.clone()));
    let mut response = next.run(request).await;
    for (name, value) in &headers {
        if !response.headers().contains_key(name) {
            response.headers_mut().insert(name.clone(), value.clone());
        }
    }
    response
}

#[cfg(test)]
mod tests;

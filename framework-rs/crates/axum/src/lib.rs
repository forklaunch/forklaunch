#![forbid(unsafe_code)]

#[cfg(feature = "native")]
pub mod native;

/// Agent routes served from the native contract router.
#[cfg(feature = "agent")]
pub mod agent;

#[cfg(any(feature = "adapter", feature = "native"))]
mod body_telemetry;
#[cfg(feature = "adapter")]
mod parsed_fields;
#[cfg(feature = "adapter")]
pub use parsed_fields::{ParsedHeaders, ParsedParams, ParsedQuery};
#[cfg(feature = "adapter")]
mod versioned_fields;
#[cfg(feature = "adapter")]
pub use versioned_fields::{VersionedHeaders, VersionedParams, VersionedQuery};
#[cfg(feature = "adapter")]
mod versioned_body;
#[cfg(feature = "adapter")]
pub use versioned_body::{OptionalVersionedBody, VersionedBody};
#[cfg(feature = "cookie-session")]
pub mod cookie_session;
#[cfg(any(feature = "adapter", feature = "native"))]
mod docs;
#[cfg(any(feature = "adapter", feature = "native"))]
pub use docs::{ScalarDocument, ScalarSource};
#[cfg(feature = "mcp")]
mod mcp_runtime;
#[cfg(feature = "multipart")]
pub mod multipart;
#[cfg(feature = "adapter")]
pub mod router;
#[cfg(any(feature = "adapter", feature = "native"))]
pub mod session;
#[cfg(feature = "sse")]
pub mod sse;
#[cfg(feature = "websocket")]
pub mod websocket;

#[cfg(feature = "adapter")]
pub use router::{
    AuthErrorFormat, CorrelationIdFormat, DEFAULT_BODY_LIMIT_BYTES, ForkLaunchRouter,
    HandlerErrorFormat, JsonTransformFailure, RequestValidationStage, RequestValidators,
    ResponseValidators, RouterConfig, RouterOriginFormat, TypeScriptCors, TypeScriptCorsHeaders,
    TypeScriptCorsOrigin, TypeScriptCorsOriginDecision, TypeScriptCorsOriginDelegate,
    TypeScriptCorsOriginError,
};

pub use forklaunch_core::http::{
    DefaultErrorResponses, JsonRequestParsers, ParsedJsonBody, ParsedRequestFields,
    ParserErrorFormat, RequestContext, RequestFieldValues, RequestTransformOutput,
    ResponseStatusErrorFormat,
};

use forklaunch_core::http::{
    Body, ContractValidationError, FrameworkRequest, FrameworkResponse, Headers, HttpApplication,
    HttpMethod, validate_framework_request,
};

#[derive(Debug, Clone)]
pub struct AxumApplication {
    pub app: HttpApplication,
    pub version_body_hooks: forklaunch_core::http::VersionBodyHooksRegistry,
}

impl AxumApplication {
    #[must_use]
    pub fn new(app: HttpApplication) -> Self {
        Self {
            app,
            version_body_hooks: forklaunch_core::http::VersionBodyHooksRegistry::default(),
        }
    }

    #[must_use]
    pub fn service_name(&self) -> &str {
        &self.app.service.name
    }

    #[must_use]
    pub fn route_summaries(&self) -> Vec<AxumRouteSummary> {
        self.app
            .registry
            .routes()
            .iter()
            .map(|route| AxumRouteSummary {
                method: route.method.as_str().to_owned(),
                path: route.path.clone(),
                operation_name: route.operation_name.clone(),
            })
            .collect()
    }

    #[must_use]
    pub fn into_inner(self) -> HttpApplication {
        self.app
    }

    #[must_use]
    pub fn prepare_request(&self, parts: AxumRequestParts) -> AxumPreparedRequest {
        let mut request = parts.into_framework_request();
        let explicit_head = request.method == HttpMethod::Head
            && self.app.registry.routes().iter().any(|route| {
                route.method == HttpMethod::Head
                    && !matches!(
                        forklaunch_core::http::match_path_checked(&route.path, &request.path),
                        Ok(None)
                    )
            });
        for route in self.app.registry.routes() {
            if route.method != request.method
                && !(request.method == HttpMethod::Head
                    && !explicit_head
                    && route.method == HttpMethod::Get)
            {
                continue;
            }

            let params = match forklaunch_core::http::match_path_checked(&route.path, &request.path)
            {
                Ok(params) => params,
                Err(error) => {
                    return AxumPreparedRequest {
                        operation_name: Some(route.operation_name.clone()),
                        request,
                        validation_errors: vec![error],
                    };
                }
            };
            if let Some(params) = params {
                request.params = params;
                forklaunch_core::http::normalize_framework_request_body(route, &mut request);
                // This metadata adapter permits callers to mutate its public registry.
                // Live Axum/LocalRouter transports cache plans at registration instead.
                let selection = self
                    .version_body_hooks
                    .validate_routes(self.app.registry.routes())
                    .and_then(|()| self.version_body_hooks.compile(route))
                    .map_err(|_| {
                        vec![ContractValidationError::new(
                            forklaunch_core::http::ContractValidationLocation::BodyParserConfiguration,
                            "invalid versioned route schema",
                        )]
                    })
                    .and_then(|plan| plan.select(&request));
                let selected = match selection {
                    Ok(Some((selected, version))) => {
                        request.version = Some(version);
                        Some(selected)
                    }
                    Err(validation_errors)
                        if route.options.request_validation
                            == forklaunch_core::contract::ValidationMode::Error
                            || validation_errors
                                .iter()
                                .any(ContractValidationError::is_parser_error) =>
                    {
                        return AxumPreparedRequest {
                            operation_name: Some(route.operation_name.clone()),
                            request,
                            validation_errors,
                        };
                    }
                    Ok(None) | Err(_) => None,
                };
                let route = selected.as_ref().unwrap_or(route);

                let validation_errors = validate_framework_request(route, &request);
                return AxumPreparedRequest {
                    operation_name: Some(route.operation_name.clone()),
                    request,
                    validation_errors,
                };
            }
        }

        AxumPreparedRequest {
            operation_name: None,
            request,
            validation_errors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxumRouteSummary {
    pub method: String,
    pub path: String,
    pub operation_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxumRequestParts {
    pub method: HttpMethod,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Headers,
    pub body: Vec<u8>,
    pub correlation_id: Option<String>,
}

impl AxumRequestParts {
    #[must_use]
    pub fn new(method: HttpMethod, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: Vec::new(),
            headers: Headers::new(),
            body: Vec::new(),
            correlation_id: None,
        }
    }

    #[must_use]
    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers
            .insert(key.into().to_ascii_lowercase(), value.into());
        self
    }

    #[must_use]
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    #[must_use]
    pub fn into_framework_request(self) -> FrameworkRequest {
        let correlation_id = self
            .correlation_id
            .or_else(|| self.headers.get("x-correlation-id").cloned())
            .unwrap_or_else(|| "forklaunch-request".to_owned());
        let mut request =
            FrameworkRequest::new(self.method, self.path, RequestContext::new(correlation_id));

        for (key, value) in self.query {
            request = request.with_query(key, value);
        }

        for (key, value) in self.headers {
            request = request.with_header(key, value);
        }

        if !self.body.is_empty() {
            request = match String::from_utf8(self.body) {
                Ok(text) => request.with_body(Body::Text(text)),
                Err(error) => request.with_body(Body::Binary(error.into_bytes())),
            };
        }

        request
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxumPreparedRequest {
    pub operation_name: Option<String>,
    pub request: FrameworkRequest,
    pub validation_errors: Vec<ContractValidationError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxumResponseParts {
    pub status_code: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[must_use]
pub fn response_parts(response: &FrameworkResponse) -> AxumResponseParts {
    let body = match &response.body {
        Body::Empty => Vec::new(),
        Body::Text(text) => text.as_bytes().to_vec(),
        Body::Binary(bytes) => bytes.clone(),
    };

    AxumResponseParts {
        status_code: response.status_code,
        headers: response
            .headers
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        body,
    }
}

#[cfg(test)]
mod tests {
    use forklaunch_core::ServiceInfo;
    use forklaunch_core::contract::{FieldContract, RouteContract};

    use super::*;

    #[test]
    fn axum_application_exposes_route_summaries() {
        let mut app = HttpApplication::new(ServiceInfo::new("accounts", "0.1.0"));
        app.register_route(RouteContract::get("List Users", "List users", "/users"));

        let adapter = AxumApplication::new(app);
        let routes = adapter.route_summaries();

        assert_eq!(adapter.service_name(), "accounts");
        assert_eq!(routes[0].method, "GET");
        assert_eq!(routes[0].path, "/users");
        assert_eq!(routes[0].operation_name, "listUsers");
    }

    #[test]
    fn response_parts_preserve_status_headers_and_body() {
        let response = FrameworkResponse::text(201, "created");

        let parts = response_parts(&response);

        assert_eq!(parts.status_code, 201);
        assert!(parts.headers.iter().any(|(key, _)| key == "content-type"));
        assert_eq!(parts.body, b"created");
    }

    #[test]
    fn request_parts_convert_into_framework_request() {
        let request = AxumRequestParts::new(HttpMethod::Post, "/users")
            .with_query("active", "true")
            .with_header("X-Correlation-Id", "corr-1")
            .with_header("Content-Type", "application/json")
            .with_body(br#"{"name":"Test"}"#.to_vec())
            .into_framework_request();

        assert_eq!(request.method, HttpMethod::Post);
        assert_eq!(request.path, "/users");
        assert_eq!(request.context.correlation_id, "corr-1");
        assert_eq!(
            request.query.get("active").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            request.headers.get("content-type").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(request.body.as_text(), Some(r#"{"name":"Test"}"#));
    }

    #[test]
    fn facade_keeps_declared_file_and_binary_bodies_as_bytes() {
        use forklaunch_core::contract::{BodyContract, BodyKind, SchemaRef};
        for kind in [BodyKind::File, BodyKind::Binary, BodyKind::Multipart] {
            let mut app = HttpApplication::new(ServiceInfo::new("upload", "1"));
            app.register_route(
                RouteContract::post("Upload", "Upload", "/upload").with_request_body(
                    BodyContract {
                        kind,
                        schema: SchemaRef::named("Bytes"),
                        content_type: Some("application/octet-stream".into()),
                    },
                ),
            );
            let adapter = AxumApplication::new(app);
            for bytes in [b"ASCII".to_vec(), vec![]] {
                let prepared = adapter.prepare_request(
                    AxumRequestParts::new(HttpMethod::Post, "/upload")
                        .with_header("content-type", "application/octet-stream")
                        .with_body(bytes.clone()),
                );
                assert!(prepared.validation_errors.is_empty());
                assert_eq!(prepared.request.body, Body::Binary(bytes));
            }
        }
    }

    #[test]
    fn facade_preserves_empty_declared_text_and_form_bodies() {
        use forklaunch_core::contract::{BodyContract, BodyKind, SchemaRef};
        for (kind, content_type) in [
            (BodyKind::Text, "text/plain"),
            (BodyKind::UrlEncoded, "application/x-www-form-urlencoded"),
        ] {
            let mut app = HttpApplication::new(ServiceInfo::new("empty", "1"));
            app.register_route(
                RouteContract::post("Upload", "Upload", "/upload").with_request_body(
                    BodyContract {
                        kind,
                        schema: SchemaRef::named("Body"),
                        content_type: Some(content_type.into()),
                    },
                ),
            );
            let adapter = AxumApplication::new(app);
            let prepared = adapter.prepare_request(
                AxumRequestParts::new(HttpMethod::Post, "/upload")
                    .with_header("content-type", content_type),
            );
            assert!(prepared.validation_errors.is_empty());
            assert_eq!(prepared.request.body, Body::Text(String::new()));
            let missing_type =
                adapter.prepare_request(AxumRequestParts::new(HttpMethod::Post, "/upload"));
            assert!(!missing_type.validation_errors.is_empty());
        }
    }

    #[test]
    fn application_prepares_request_with_path_params_and_validation() {
        let mut app = HttpApplication::new(ServiceInfo::new("accounts", "0.1.0"));
        app.register_route(
            RouteContract::get("Get User", "Get user", "/users/:id")
                .with_param(FieldContract::required("id", "String"))
                .with_query(FieldContract::required("includePosts", "Boolean"))
                .with_response(200, None),
        );
        let adapter = AxumApplication::new(app);

        let prepared =
            adapter.prepare_request(AxumRequestParts::new(HttpMethod::Get, "/users/123"));

        assert_eq!(prepared.operation_name.as_deref(), Some("getUser"));
        assert_eq!(
            prepared.request.params.get("id").map(String::as_str),
            Some("123")
        );
        assert_eq!(prepared.validation_errors.len(), 1);
        assert_eq!(
            prepared.validation_errors[0].field.as_deref(),
            Some("includePosts")
        );
    }
}

#[cfg(feature = "remote-jwks")]
pub mod module_auth;

//! Real `axum`-backed router (`ForkLaunchRouter`).
//!
//! Wraps [`axum::Router`] while keeping a [`ContractRegistry`] in sync so that
//! contract-driven request validation, `OpenAPI` generation, and documentation
//! endpoints all flow from the same source of truth as the rest of the
//! framework. Enabled by the default `adapter` feature.

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::session::{AuthenticatedSession, Authenticator, Authorizer, ResponseAuthenticator};
use axum::{
    Router,
    body::{Body as AxumBody, to_bytes},
    extract::{MatchedPath, Request, State},
    handler::Handler,
    http::{HeaderValue, Method, StatusCode, header, request::Parts},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::MethodRouter,
};
use forklaunch_common::{join_base_path, normalize_route_path};
use forklaunch_core::auth::{AuthError, authorize};
use forklaunch_core::contract::{
    AccessLevel, AuthConfig, BodyKind, ContractRegistry, HttpMethod, RouteContract, ValidationMode,
};
use forklaunch_core::http::{
    Body as FrameworkBody, DefaultErrorResponses, ErrorKind, ErrorResponse, FrameworkRequest,
    FrameworkResponse, LocalRouter, ParserErrorFormat, RequestContext, ResponseStatusErrorFormat,
    match_path_checked, typescript_parser_error_response, validate_framework_request,
    validation_error_response, validation_error_response_with_parser_format,
};
use forklaunch_core::openapi::{
    OpenApiOptions, OpenApiServer, generate_openapi_json_with_options,
    generate_portable_openapi_json_with_options,
};
use forklaunch_core::{ServiceInfo, telemetry::OpenTelemetryCollector};
use opentelemetry::trace::FutureExt;
#[cfg(test)]
use tower::ServiceExt;
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::TraceLayer,
};

mod case_routes;
mod correlation;
mod typescript_cors;
pub use typescript_cors::{
    TypeScriptCors, TypeScriptCorsHeaders, TypeScriptCorsOrigin, TypeScriptCorsOriginDecision,
    TypeScriptCorsOriginDelegate, TypeScriptCorsOriginError,
};
mod default_errors;
mod origin;
pub use correlation::CorrelationIdFormat;
pub use origin::RouterOriginFormat;
mod response_validation;
pub use response_validation::ResponseValidators;
mod request_validation;
pub use request_validation::{JsonTransformFailure, RequestValidationStage, RequestValidators};

/// Default maximum request body size accepted before a `400` is returned (2 MiB).
pub const DEFAULT_BODY_LIMIT_BYTES: usize = 2 * 1024 * 1024;

/// Wire representation for authentication and authorization rejections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AuthErrorFormat {
    /// Native structured errors with code, message and correlation ID.
    #[default]
    Json,
    /// TypeScript middleware's plain-text public message and UTF-8 media type.
    /// Use a compatible verifier/policy to also match statuses and messages.
    TypeScriptText,
}

/// Wire representation for failures returned by a `LocalRouter` handler.
/// This does not catch panics or rewrite responses returned by Axum handlers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HandlerErrorFormat {
    /// Native structured, redacted internal error (the default).
    #[default]
    Json,
    /// TypeScript's generic exception message with the request correlation ID.
    TypeScriptText,
}

/// Configuration for cross-cutting router behavior (docs, limits, CORS).
#[derive(Clone)]
pub struct RouterConfig {
    /// Application validators bound to declared unversioned request schemas.
    pub request_validators: RequestValidators,
    /// Opt-in unversioned JSON parsers, run before authentication and schema callbacks.
    pub json_request_parsers: forklaunch_core::http::JsonRequestParsers,
    /// Opt-in bounded validators for finite response bodies.
    pub response_validators: ResponseValidators,
    /// Selected missing default error-string responses with finite collection bounds.
    pub default_error_responses: DefaultErrorResponses,
    /// Native callbacks run around each version's body schema validation.
    pub version_body_hooks: forklaunch_core::http::VersionBodyHooksRegistry,
    /// Optional server-side browser sessions and CSRF policy.
    #[cfg(feature = "cookie-session")]
    pub cookie_sessions: Option<crate::cookie_session::CookieSessions>,
    /// Whether the generated `OpenAPI` document is served.
    pub openapi_enabled: bool,
    /// Path the checked `OpenAPI` JSON document is served from.
    /// Its `<path>-hash` endpoint supports SDK discovery and schema refresh.
    /// Runtime-only contracts retain standard documentation; their hash endpoint
    /// returns 503 when checked SDK metadata cannot be compiled.
    pub openapi_path: String,
    /// Whether the Scalar documentation UI is served.
    pub docs_enabled: bool,
    /// Path the documentation UI is served from.
    pub docs_path: String,
    /// Title rendered in the documentation UI.
    pub docs_title: String,
    /// Additional Scalar documents, shown after the generated application API.
    pub docs_sources: Vec<crate::ScalarSource>,
    /// Whether the Swagger UI is also served.
    pub swagger_enabled: bool,
    /// Path the Swagger UI is served from.
    pub swagger_path: String,
    /// Maximum accepted request body size in bytes.
    pub body_limit: usize,
    /// Optional `OpenAPI` `info.description`.
    pub description: Option<String>,
    /// `OpenAPI` servers advertised in the generated document.
    pub servers: Vec<OpenApiServer>,
    /// CORS policy; defaults to a permissive policy when unset.
    pub cors: Option<CorsLayer>,
    /// TypeScript CORS profile; mutually exclusive with `cors`.
    pub typescript_cors: Option<TypeScriptCors>,
    /// Credential verifier for routes requiring authentication. Missing verifiers fail closed.
    pub authenticator: Option<Authenticator>,
    /// Optional verifier with explicit safe HTTP errors; takes precedence when set.
    pub response_authenticator: Option<ResponseAuthenticator>,
    /// Optional replacement for native RBAC, invoked only after authentication.
    pub authorizer: Option<Authorizer>,
    /// Format only credential/policy errors; validation/handler errors are separate.
    pub auth_error_format: AuthErrorFormat,
    /// Per-operation override keyed by method and operation name, matching request
    /// validator identity. Unselected routes retain `auth_error_format`.
    /// Rendering only: callers must declare matching response schemas for exports.
    pub auth_error_formats: HashMap<(String, String), AuthErrorFormat>,
    /// Whole canonical paths whose registered methods also accept one final slash.
    /// Empty by default. This does not rewrite the URI, add contracts or select auth.
    pub optional_trailing_slash_paths: HashSet<String>,
    /// Paths with implicit Express-style OPTIONS, only absent an explicit handler.
    /// Uses exact-pattern methods by default; case-insensitive dispatch unions matching patterns.
    /// Empty by default; selection neither creates a contract nor bypasses explicit auth.
    pub typescript_options_paths: HashSet<String>,
    /// Static-segment ECMAScript non-/u case-insensitive routing (optional feature).
    /// Matching selected intersections use registry operation order; unselected paths remain strict.
    pub typescript_case_insensitive_paths: HashSet<String>,
    /// Format for errors returned by framework local handlers.
    pub handler_error_format: HandlerErrorFormat,
    /// Final wire representation for request-parser and transport-limit failures.
    pub parser_error_format: ParserErrorFormat,
    /// Opt-in formatting for framework-owned router fallbacks only.
    pub router_origin_format: RouterOriginFormat,
    /// Optional TypeScript correlation selection and response propagation.
    pub correlation_id_format: CorrelationIdFormat,
    /// Wire format for framework-detected undocumented response statuses.
    pub response_status_error_format: ResponseStatusErrorFormat,
    /// Inject a configured collector to export HTTP traces and metrics.
    pub telemetry: Option<OpenTelemetryCollector>,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            version_body_hooks: forklaunch_core::http::VersionBodyHooksRegistry::default(),
            request_validators: RequestValidators::default(),
            json_request_parsers: forklaunch_core::http::JsonRequestParsers::default(),
            response_validators: ResponseValidators::default(),
            default_error_responses: DefaultErrorResponses::default(),
            #[cfg(feature = "cookie-session")]
            cookie_sessions: None,
            openapi_enabled: true,
            openapi_path: "/openapi.json".to_owned(),
            docs_enabled: true,
            docs_path: "/docs".to_owned(),
            docs_title: "API Reference".to_owned(),
            docs_sources: Vec::new(),
            swagger_enabled: false,
            swagger_path: "/swagger".to_owned(),
            body_limit: DEFAULT_BODY_LIMIT_BYTES,
            description: None,
            servers: Vec::new(),
            cors: None,
            typescript_cors: None,
            authenticator: None,
            response_authenticator: None,
            authorizer: None,
            auth_error_format: AuthErrorFormat::Json,
            auth_error_formats: HashMap::new(),
            optional_trailing_slash_paths: HashSet::new(),
            typescript_options_paths: HashSet::new(),
            typescript_case_insensitive_paths: HashSet::new(),
            handler_error_format: HandlerErrorFormat::Json,
            parser_error_format: ParserErrorFormat::Json,
            router_origin_format: RouterOriginFormat::Native,
            correlation_id_format: CorrelationIdFormat::Native,
            response_status_error_format: ResponseStatusErrorFormat::Json,
            telemetry: None,
        }
    }
}

impl RouterConfig {
    /// Configures credential verification and optional session enrichment.
    /// The verifier receives the raw request and declared contract, including auth
    /// method/header settings. It must verify credentials before returning a session.
    #[must_use]
    pub fn with_authenticator<F, Fut>(mut self, verifier: F) -> Self
    where
        F: Fn(FrameworkRequest, RouteContract) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<AuthenticatedSession, AuthError>> + Send + 'static,
    {
        self.response_authenticator = None;
        self.authenticator = Some(Arc::new(move |request, contract| {
            Box::pin(verifier(request, contract))
        }));
        self
    }

    /// Configures trusted credential verification with explicit public errors.
    /// Successful sessions still pass normal authorization and cannot be supplied
    /// by client headers. The callback must keep secrets out of error messages.
    /// Replaces a verifier set by `with_authenticator`; cookie sessions retain
    /// their dedicated verifier. Responses use the native structured error format.
    #[must_use]
    pub fn with_response_authenticator<F, Fut>(mut self, verifier: F) -> Self
    where
        F: Fn(FrameworkRequest, RouteContract) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<AuthenticatedSession, ErrorResponse>>
            + Send
            + 'static,
    {
        self.authenticator = None;
        self.response_authenticator = Some(Arc::new(move |request, contract| {
            Box::pin(verifier(request, contract))
        }));
        self
    }

    /// Accept exactly one trailing slash for all methods registered at this path.
    /// The canonical root has no alternate spelling. Wildcard paths are unsupported.
    /// Selection is path-wide, not an authentication or per-method policy.
    #[must_use]
    pub fn with_optional_trailing_slash_for_path(mut self, path: impl AsRef<str>) -> Self {
        self.optional_trailing_slash_paths
            .insert(forklaunch_common::route_open_api_path(
                &normalize_route_path(path.as_ref()),
            ));
        self
    }

    /// Answer undeclared OPTIONS with sorted registered verbs and implicit HEAD.
    /// The selection covers one canonical path pattern and all its registered methods.
    /// Unlike Express traversal, overlapping patterns are not unioned. Explicit OPTIONS
    /// retains its own handler and authentication; other method failures are unchanged.
    #[must_use]
    pub fn with_typescript_options_for_path(mut self, path: impl AsRef<str>) -> Self {
        self.typescript_options_paths
            .insert(forklaunch_common::route_open_api_path(
                &normalize_route_path(path.as_ref()),
            ));
        self
    }

    /// Select pinned ECMAScript `/i` matching for static path segments only.
    /// Requires `typescript-routing`; original URI, parameters, query and signatures
    /// remain intact. Selected intersections use registry order, including HEAD fallback.
    #[must_use]
    pub fn with_typescript_case_insensitive_path(mut self, path: impl AsRef<str>) -> Self {
        self.typescript_case_insensitive_paths
            .insert(forklaunch_common::route_open_api_path(
                &normalize_route_path(path.as_ref()),
            ));
        self
    }

    /// Replace native RBAC checks with an application-owned policy for verified
    /// sessions. Credential verification still runs first and fails closed.
    /// Resolve any async grants in the authenticator before returning a session.
    /// This callback owns authorization for every authenticated route, including
    /// mounted routes; it must enforce the declared contract's access policy.
    #[must_use]
    pub fn with_authorizer(
        mut self,
        authorizer: impl Fn(&AuthenticatedSession, &RouteContract) -> Result<(), ErrorResponse>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        self.authorizer = Some(Arc::new(authorizer));
        self
    }

    /// Select public authentication-error rendering for one trusted route.
    /// This does not alter credential verification or authorization.
    #[must_use]
    pub fn with_auth_error_format_for_route(
        mut self,
        route: &RouteContract,
        format: AuthErrorFormat,
    ) -> Self {
        self.auth_error_formats.insert(
            (
                route.method.as_str().to_owned(),
                route.operation_name.clone(),
            ),
            format,
        );
        self
    }

    fn openapi_options(&self, registry: &ContractRegistry) -> OpenApiOptions {
        let mut options = OpenApiOptions::from_registry(registry);
        if self.auth_error_format == AuthErrorFormat::TypeScriptText {
            options = options.with_typescript_auth_errors();
        }
        if self.parser_error_format == ParserErrorFormat::TypeScriptText {
            options = options.with_typescript_path_errors();
        }
        if let Some(description) = &self.description {
            options = options.with_description(description.clone());
        }
        for server in &self.servers {
            options = options.with_server(server.clone());
        }
        options
    }
}

#[cfg(test)]
mod documentation_discovery_tests;
mod internal;
#[cfg(test)]
mod response_status_tests;
#[cfg(test)]
mod versioning_tests;

/// A contract-aware wrapper around [`axum::Router`].
pub struct ForkLaunchRouter {
    base_path: String,
    registry: ContractRegistry,
    projected_registry: Option<ContractRegistry>,
    routes: HashMap<String, MethodRouter>,
    extended_routes: HashMap<String, HashMap<String, MethodRouter>>,
    config: RouterConfig,
    internal_mounts: Vec<internal::InternalMount>,
    local_validation: HashSet<String>,
    local_version_plans: HashMap<String, forklaunch_core::http::VersionedRoutePlan>,
}

macro_rules! typed_route_methods {
    ($($method:ident => $variant:ident),* $(,)?) => {
        $(
            #[doc = concat!("Registers a `", stringify!($variant), "` route with its contract and handler.")]
            ///
            /// # Panics
            /// Panics if the contract method differs or duplicates an existing route.
            #[must_use]
            pub fn $method<H, T>(self, contract: RouteContract, handler: H) -> Self
            where
                H: Handler<T, ()>,
                T: 'static,
            {
                assert_eq!(contract.method, HttpMethod::$variant,
                    "route method must match its contract");
                self.route(contract, handler)
            }
        )*
    };
}

impl ForkLaunchRouter {
    /// Creates a router that tags generated documentation with the given service.
    #[must_use]
    pub fn new(service: ServiceInfo) -> Self {
        Self {
            base_path: "/".to_owned(),
            registry: ContractRegistry::for_service(service.contract()),
            projected_registry: None,
            routes: HashMap::new(),
            extended_routes: HashMap::new(),
            config: RouterConfig::default(),
            internal_mounts: Vec::new(),
            local_validation: HashSet::new(),
            local_version_plans: HashMap::new(),
        }
    }

    /// Serves synchronous framework handlers through Axum's HTTP transport.
    /// Contract registration, validation, docs, and wire handling share one registry.
    #[must_use]
    pub fn from_local_router(
        service: ServiceInfo,
        contracts: impl IntoIterator<Item = RouteContract>,
        handlers: LocalRouter,
    ) -> Self {
        let handlers = Arc::new(handlers);
        let mut router = Self::new(service);
        for contract in contracts {
            if let Some(plan) = handlers.version_plan_for(&contract) {
                router
                    .local_version_plans
                    .insert(route_key(contract.method, &contract.openapi_path), plan);
            }
            router
                .local_validation
                .insert(route_key(contract.method, &contract.openapi_path));
            let handlers = Arc::clone(&handlers);
            let handler_contract = contract.clone();
            let handler = move |request: Request| {
                let handlers = Arc::clone(&handlers);
                let contract = handler_contract.clone();
                async move {
                    let (parts, body) = request.into_parts();
                    let Ok(bytes) = to_bytes(body, usize::MAX).await else {
                        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
                    };
                    let mut request = match build_framework_request(&contract, &parts, &bytes) {
                        Ok(request) => request,
                        Err(response) => return framework_response_to_axum(response),
                    };
                    // Flattened nesting changes the transport path, while the local
                    // router retains its original route paths and parameter matcher.
                    if let Some(prefix) =
                        parts.extensions.get::<MatchedPath>().and_then(|matched| {
                            let path = matched.as_str();
                            let canonical = path.strip_suffix('/').unwrap_or(path);
                            if contract.openapi_path == "/" {
                                Some(canonical)
                            } else {
                                canonical.strip_suffix(&contract.openapi_path)
                            }
                        })
                    {
                        if let Some(dispatch) = request.transport_route_path.as_mut() {
                            if dispatch.starts_with(prefix) {
                                dispatch.drain(..prefix.len());
                                request.path = case_routes::local_path(&request.path, prefix);
                            }
                        } else if request.path.starts_with(prefix) {
                            request.path.drain(..prefix.len());
                        }
                    }
                    let correlation_id = request.context.correlation_id.clone();
                    let error_format = parts
                        .extensions
                        .get::<HandlerErrorFormat>()
                        .copied()
                        .unwrap_or_default();
                    let versioned = parts
                        .extensions
                        .get::<response_validation::VersionedResponseValidation>()
                        .is_some();
                    let result = version_stage::dispatch_local(&handlers, request, &parts);
                    match result {
                        Ok((response, matched)) => {
                            response_validation::attach_local_metadata(response, matched)
                        }
                        Err(error) => {
                            tracing::error!(error = %error, correlation_id, "local request handler failed");
                            let response =
                                local_handler_failure_response(error_format, &correlation_id);
                            response_validation::attach_local_metadata(
                                response,
                                versioned
                                    .then(forklaunch_core::http::VersionedResponseOutcome::default),
                            )
                        }
                    }
                }
            };
            router = router.route(contract, handler);
        }
        router
    }

    /// Creates a router without any associated service metadata.
    #[must_use]
    pub fn anonymous() -> Self {
        Self {
            base_path: "/".to_owned(),
            registry: ContractRegistry::new(),
            projected_registry: None,
            routes: HashMap::new(),
            extended_routes: HashMap::new(),
            config: RouterConfig::default(),
            internal_mounts: Vec::new(),
            local_validation: HashSet::new(),
            local_version_plans: HashMap::new(),
        }
    }

    /// Sets a base path prepended to every route registered afterwards.
    #[must_use]
    pub fn with_base_path(mut self, base_path: impl AsRef<str>) -> Self {
        self.base_path = normalize_route_path(base_path.as_ref());
        self
    }

    /// Overrides the cross-cutting router configuration.
    /// # Panics
    /// Panics if explicit validator rejection metadata conflicts with a route.
    #[must_use]
    pub fn with_config(mut self, config: RouterConfig) -> Self {
        self.config = config;
        self.refresh_rejection_contracts();
        self
    }

    fn refresh_rejection_contracts(&mut self) {
        let request_registry = self
            .config
            .request_validators
            .project(&self.registry)
            .expect("invalid request validator response declaration");
        self.projected_registry = self
            .config
            .default_error_responses
            .project(request_registry.as_ref().unwrap_or(&self.registry))
            .expect("invalid default error response declaration")
            .or(request_registry);
    }

    /// Install native version body callbacks without replacing other router options.
    #[must_use]
    pub fn with_version_body_hooks(
        mut self,
        hooks: forklaunch_core::http::VersionBodyHooksRegistry,
    ) -> Self {
        self.config.version_body_hooks = hooks;
        self
    }

    /// Adds an `OpenAPI` server entry advertised in the generated document.
    #[must_use]
    pub fn with_server(mut self, server: OpenApiServer) -> Self {
        self.config.servers.push(server);
        self
    }

    /// Sets the `OpenAPI` `info.description`.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.config.description = Some(description.into());
        self
    }

    /// Registers any supported method using the contract as the dispatch source of truth.
    ///
    /// # Panics
    /// Panics when the same method and path are registered twice.
    #[must_use]
    pub fn route<H, T>(mut self, contract: RouteContract, handler: H) -> Self
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        let contract = self.rebase_contract(contract);
        let path = contract.openapi_path.clone();
        let method = contract.method;
        assert!(
            !self
                .registry
                .routes()
                .iter()
                .any(|registered| registered.openapi_path == path && registered.method == method),
            "duplicate route method and path"
        );
        self.registry.register(contract);
        self.refresh_rejection_contracts();
        if let Ok(filter) = axum::routing::MethodFilter::try_from(
            Method::from_bytes(method.as_str().as_bytes()).expect("valid method"),
        ) {
            self.add_method_router(path, axum::routing::on(filter, handler));
        } else {
            self.routes.entry(path.clone()).or_default();
            self.extended_routes
                .entry(path)
                .or_default()
                .insert(method.as_str().to_owned(), axum::routing::any(handler));
        }
        self
    }

    typed_route_methods!(
        get => Get,
        post => Post,
        put => Put,
        patch => Patch,
        delete => Delete,
        head => Head,
        options => Options,
        trace => Trace,
        checkout => Checkout,
        copy => Copy,
        link => Link,
        lock => Lock,
        m_search => MSearch,
        merge => Merge,
        mkactivity => MkActivity,
        mkcol => MkCol,
        move_resource => Move,
        notify => Notify,
        propfind => PropFind,
        proppatch => PropPatch,
        purge => Purge,
        report => Report,
        search => Search,
        subscribe => Subscribe,
        unlink => Unlink,
        unlock => Unlock,
        unsubscribe => Unsubscribe,

    );

    /// Nests another router under `prefix`, flattening its contracts and routes
    /// so a single validation layer continues to cover every endpoint.
    ///
    /// # Panics
    /// Panics on overlapping method/path registrations.
    #[must_use]
    pub fn nest(mut self, prefix: impl AsRef<str>, child: ForkLaunchRouter) -> Self {
        let prefix = join_base_path(&self.base_path, prefix.as_ref());

        let ForkLaunchRouter {
            registry,
            routes,
            extended_routes,
            internal_mounts,
            local_validation,
            local_version_plans,
            config,
            ..
        } = child;
        self.config
            .auth_error_formats
            .extend(config.auth_error_formats);
        self.config.typescript_case_insensitive_paths.extend(
            config
                .typescript_case_insensitive_paths
                .into_iter()
                .map(|path| join_base_path(&prefix, &path)),
        );
        self.config.typescript_options_paths.extend(
            config
                .typescript_options_paths
                .into_iter()
                .map(|path| join_base_path(&prefix, &path)),
        );
        self.config.optional_trailing_slash_paths.extend(
            config
                .optional_trailing_slash_paths
                .into_iter()
                .map(|path| join_base_path(&prefix, &path)),
        );
        self.config
            .json_request_parsers
            .extend(config.json_request_parsers)
            .expect("duplicate JSON parser registration");
        self.config
            .version_body_hooks
            .extend(config.version_body_hooks);
        self.config
            .request_validators
            .extend(config.request_validators);
        self.config
            .response_validators
            .extend(config.response_validators);
        self.config
            .default_error_responses
            .extend(config.default_error_responses)
            .expect("duplicate default error profile operation");

        for mut mount in internal_mounts {
            mount.prefix = join_base_path(&prefix, &mount.prefix);
            for namespace in &mut mount.reserved_prefixes {
                *namespace = join_base_path(&prefix, namespace);
            }
            self.internal_mounts.push(mount);
        }

        for mut route in registry.into_artifact().routes {
            if let Some(plan) =
                local_version_plans.get(&route_key(route.method, &route.openapi_path))
            {
                self.local_version_plans.insert(
                    route_key(route.method, &join_base_path(&prefix, &route.openapi_path)),
                    plan.clone(),
                );
            }
            if local_validation.contains(&route_key(route.method, &route.openapi_path)) {
                self.local_validation.insert(route_key(
                    route.method,
                    &join_base_path(&prefix, &route.openapi_path),
                ));
            }
            route.path = join_base_path(&prefix, &route.path);
            route.openapi_path = join_base_path(&prefix, &route.openapi_path);
            self.registry.register(route);
        }

        for (path, method_router) in routes {
            let nested_path = join_base_path(&prefix, &path);
            self.add_method_router(nested_path, method_router);
        }

        for (path, methods) in extended_routes {
            let target = self
                .extended_routes
                .entry(join_base_path(&prefix, &path))
                .or_default();
            for (method, handler) in methods {
                assert!(
                    target.insert(method, handler).is_none(),
                    "duplicate nested route"
                );
            }
        }

        self.refresh_rejection_contracts();
        self
    }

    /// Decorates handlers at a registered router-relative path without changing
    /// their contracts. The transform covers standard and extended methods,
    /// survives nesting, and is used by HTTP and MCP dispatch alike. Use
    /// `MethodRouter::layer` to include extended-method fallback handlers.
    /// # Errors
    /// Rejects an unregistered path.
    pub fn map_route(
        mut self,
        path: &str,
        mut transform: impl FnMut(MethodRouter) -> MethodRouter,
    ) -> Result<Self, String> {
        let path = forklaunch_common::route_open_api_path(&join_base_path(&self.base_path, path));
        if !self.routes.contains_key(&path) {
            return Err("Cannot decorate an unregistered route".into());
        }
        let has_standard = self.registry.routes().iter().any(|route| {
            route.openapi_path == path
                && Method::from_bytes(route.method.as_str().as_bytes())
                    .is_ok_and(|method| axum::routing::MethodFilter::try_from(method).is_ok())
        });
        if has_standard {
            let Some(route) = self.routes.remove(&path) else {
                return Err("Cannot decorate an unregistered route".into());
            };
            self.routes.insert(path.clone(), transform(route));
        }
        if let Some(methods) = self.extended_routes.get_mut(&path) {
            for handler in methods.values_mut() {
                *handler = transform(std::mem::take(handler));
            }
        }
        Ok(self)
    }

    /// Borrows the effective registry, including explicit validator rejection
    /// declarations, for consistent contract, SDK and `OpenAPI` export.
    /// # Panics
    /// Panics if a default error profile lacks its original matching route.
    #[must_use]
    pub fn registry(&self) -> &ContractRegistry {
        self.config
            .default_error_responses
            .validate_routes(self.registry.routes())
            .expect("invalid default error profile export");
        default_errors::validate_configuration(&self.config, self.registry.routes())
            .expect("incompatible default error response profile");
        match &self.projected_registry {
            Some(registry) => registry,
            None => &self.registry,
        }
    }

    /// Builds the final [`axum::Router`] with validation, docs, and middleware.
    ///
    /// # Panics
    /// Panics on invalid or overlapping paths, including documentation endpoints.
    pub fn into_router(self) -> Router {
        let Self {
            registry: declared_registry,
            projected_registry,
            routes,
            extended_routes,
            config,
            internal_mounts,
            local_validation,
            local_version_plans,
            ..
        } = self;

        let registry = projected_registry.unwrap_or_else(|| declared_registry.clone());

        internal::validate(&internal_mounts, &registry, &config)
            .expect("invalid internal HTTP mount");
        let (mut router, isolated) =
            build_transport_routes(routes, extended_routes, &registry, &config);

        router = origin::install(
            router,
            &config,
            internal_mounts.iter().any(|mount| mount.prefix == "/"),
            &registry,
        );

        let mut protected = internal_mounts
            .iter()
            .flat_map(|mount| mount.reserved_prefixes.clone())
            .collect::<Vec<_>>();
        if config.openapi_enabled {
            protected.push(config.openapi_path.clone());
            protected.push(format!("{}-hash", config.openapi_path));
        }
        if config.docs_enabled {
            protected.push(config.docs_path.clone());
        }
        if config.swagger_enabled {
            protected.push(config.swagger_path.clone());
        }
        for mount in internal_mounts {
            router = if mount.prefix == "/" {
                router.merge(mount.router)
            } else {
                router.nest(&mount.prefix, mount.router)
            };
        }

        router = mount_documentation(router, &config, &registry);

        let telemetry = config
            .telemetry
            .clone()
            .unwrap_or_else(|| OpenTelemetryCollector::new("forklaunch-http"));
        let state = Arc::new(ValidationState::from_registry(
            &declared_registry,
            &registry,
            &config,
            telemetry.clone(),
            local_validation,
            &local_version_plans,
        ));
        let ids = CounterMakeRequestId::default();
        let router = instrument_router(
            router,
            Arc::clone(&state),
            telemetry.clone(),
            &config,
            ids.clone(),
        );
        let isolated = isolated
            .into_iter()
            .map(|(path, router)| {
                let router = origin::install(router, &config, true, &registry);
                (
                    path,
                    instrument_router(
                        router,
                        Arc::clone(&state),
                        telemetry.clone(),
                        &config,
                        ids.clone(),
                    ),
                )
            })
            .collect();
        let router = case_routes::wrap(router, isolated, &registry, &config, protected);
        let router = typescript_cors::wrap(router, config.cors, config.typescript_cors);
        let router = origin::wrap(router, config.router_origin_format);
        #[cfg(feature = "cookie-session")]
        if let Some(sessions) = config.cookie_sessions {
            return correlation::wrap(sessions.wrap(router), config.correlation_id_format);
        }
        correlation::wrap(router, config.correlation_id_format)
    }

    /// Mounts an MCP transport whose tools dispatch through this HTTP router.
    /// # Errors
    /// Rejects invalid MCP configuration and conflicting tool names or paths.
    #[cfg(feature = "mcp")]
    pub fn into_router_with_mcp(
        self,
        options: forklaunch_core::mcp::server::McpServerOptions,
    ) -> Result<Router, String> {
        let (http, transport) = self.into_routers_with_mcp(options)?;
        Ok(http.merge(transport))
    }

    /// Builds separate HTTP and MCP-only routers; MCP still invokes the same
    /// authenticated HTTP handlers internally.
    /// # Errors
    /// Rejects invalid MCP options, names and endpoint collisions.
    #[cfg(feature = "mcp")]
    pub fn into_routers_with_mcp(
        self,
        options: forklaunch_core::mcp::server::McpServerOptions,
    ) -> Result<(Router, Router), String> {
        if self.internal_mounts.iter().any(|mount| {
            mount
                .reserved_prefixes
                .iter()
                .any(|prefix| internal::contains_path(prefix, &options.path))
        }) {
            return Err("MCP endpoint conflicts with an internal HTTP namespace".into());
        }
        if (self.config.openapi_enabled
            && (options.path == self.config.openapi_path
                || options.path == format!("{}-hash", self.config.openapi_path)))
            || (self.config.openapi_enabled
                && self.config.docs_enabled
                && options.path == self.config.docs_path)
            || (self.config.openapi_enabled
                && self.config.swagger_enabled
                && options.path == self.config.swagger_path)
        {
            return Err("MCP endpoint conflicts with documentation".into());
        }
        let registry = self.registry().clone();
        let http = self.into_router();
        let transport =
            forklaunch_core::mcp::server::http_transport(http.clone(), &registry, options)?;
        Ok((http, transport))
    }

    /// Enables `/mcp` on generated services. Server-owned environment variables
    /// configure `MCP_ENABLED`, `MCP_PATH`, `MCP_ALLOWED_HOSTS` and
    /// `MCP_ALLOWED_ORIGINS`; route authentication remains in effect.
    /// # Errors
    /// Rejects invalid environment settings and MCP configuration.
    #[cfg(feature = "mcp")]
    pub fn into_router_with_default_mcp(self) -> Result<Router, String> {
        match self.default_mcp_options()? {
            Some(options) => self.into_router_with_mcp(options),
            None => Ok(self.into_router()),
        }
    }

    #[cfg(feature = "mcp")]
    pub(super) fn default_mcp_options(
        &self,
    ) -> Result<Option<forklaunch_core::mcp::server::McpServerOptions>, String> {
        let enabled: String =
            forklaunch_core::config::required("MCP_ENABLED").map_err(|error| error.to_string())?;
        match enabled.as_str() {
            "false" | "0" => return Ok(None),
            "true" | "1" => {}
            _ => return Err("MCP_ENABLED must be true, false, 1 or 0".into()),
        }
        let service = self.registry.service();
        let mut options = forklaunch_core::mcp::server::McpServerOptions::new(
            service.map_or("forklaunch-service", |s| s.name.as_str()),
            service.map_or("0.1.0", |s| s.version.as_str()),
        );
        if let Ok(path) = std::env::var("MCP_PATH") {
            options.path = path;
        }
        for (variable, hosts) in [("MCP_ALLOWED_HOSTS", true), ("MCP_ALLOWED_ORIGINS", false)] {
            if let Ok(value) = std::env::var(variable) {
                let values: Vec<_> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
                    .collect();
                if values.is_empty() {
                    return Err(format!("{variable} must not be empty"));
                }
                if hosts {
                    options.transport = options.transport.with_allowed_hosts(values);
                } else {
                    options.transport = options.transport.with_allowed_origins(values);
                }
            }
        }
        Ok(Some(options))
    }

    /// Serves HTTP and optional MCP with coordinated graceful shutdown. Use
    /// `MCP_LISTENER=separate` (HTTP port + 2000), `MCP_PORT` or `MCP_HOST` for a
    /// separate listener; the default keeps MCP on the HTTP listener.
    /// # Errors
    /// Returns invalid configuration, bind, server and bounded drain errors.
    #[cfg(feature = "mcp")]
    pub async fn serve_with_default_mcp<F>(
        self,
        listener: tokio::net::TcpListener,
        shutdown: F,
    ) -> std::io::Result<()>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        crate::mcp_runtime::serve(self, listener, shutdown).await
    }

    /// Binds to `addr` and serves until shutdown.
    ///
    /// # Errors
    ///
    /// Returns an error if the listener cannot bind or the server task fails.
    pub async fn serve(self, addr: SocketAddr) -> std::io::Result<()> {
        let router = self.into_router();
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, router.into_make_service()).await
    }

    fn rebase_contract(&self, mut contract: RouteContract) -> RouteContract {
        if self.base_path != "/" {
            contract.path = join_base_path(&self.base_path, &contract.path);
            contract.openapi_path = join_base_path(&self.base_path, &contract.openapi_path);
        }
        contract
    }

    fn add_method_router(&mut self, path: String, method_router: MethodRouter) {
        let merged = match self.routes.remove(&path) {
            Some(existing) => existing.merge(method_router),
            None => method_router,
        };
        self.routes.insert(path, merged);
    }
}

fn openapi_handler(document: String) -> MethodRouter {
    axum::routing::get(move || {
        let document = document.clone();
        async move { ([(header::CONTENT_TYPE, "application/json")], document) }
    })
}

fn docs_handler(html: String) -> MethodRouter {
    axum::routing::get(move || {
        let html = html.clone();
        async move { Html(html) }
    })
}

/// Indexed contracts used by the validation middleware.
struct ValidationState {
    response_validators: ResponseValidators,
    default_error_responses: Arc<DefaultErrorResponses>,
    request_validators: RequestValidators,
    json_request_parsers: forklaunch_core::http::JsonRequestParsers,
    routes: HashMap<String, RouteContract>,
    versions: HashMap<String, forklaunch_core::http::VersionedRoutePlan>,
    local_validation: HashSet<String>,
    pre_auth_versions: HashSet<String>,
    body_limit: usize,
    authenticator: Option<Authenticator>,
    response_authenticator: Option<ResponseAuthenticator>,
    authorizer: Option<Authorizer>,
    auth_error_format: AuthErrorFormat,
    auth_error_formats: HashMap<(String, String), AuthErrorFormat>,
    handler_error_format: HandlerErrorFormat,
    parser_error_format: ParserErrorFormat,
    response_status_error_format: ResponseStatusErrorFormat,
    telemetry: OpenTelemetryCollector,
}

impl ValidationState {
    fn contract(&self, method: HttpMethod, path: &str) -> Option<RouteContract> {
        // Axum serves HEAD with GET handlers unless an explicit HEAD contract exists.
        self.routes
            .get(&route_key(method, path))
            .or_else(|| {
                (method == HttpMethod::Head)
                    .then(|| self.routes.get(&route_key(HttpMethod::Get, path)))
                    .flatten()
            })
            .cloned()
    }

    fn build_request(
        &self,
        contract: &RouteContract,
        parts: &Parts,
        bytes: &[u8],
    ) -> Result<FrameworkRequest, FrameworkResponse> {
        // The only fallible construction step is path parameter decoding. Express
        // rejects malformed URI components before establishing a request context.
        build_framework_request(contract, parts, bytes).map_err(|response| {
            if self.parser_error_format == ParserErrorFormat::TypeScriptText {
                typescript_parser_error_response(400, "No correlation ID")
            } else {
                response
            }
        })
    }

    fn from_registry(
        declared_registry: &ContractRegistry,
        registry: &ContractRegistry,
        config: &RouterConfig,
        telemetry: OpenTelemetryCollector,
        local_validation: HashSet<String>,
        local_version_plans: &HashMap<String, forklaunch_core::http::VersionedRoutePlan>,
    ) -> Self {
        config
            .json_request_parsers
            .validate_routes(declared_registry.routes())
            .expect("invalid JSON parser registration");
        config
            .request_validators
            .validate_routes(declared_registry.routes())
            .expect("invalid request validator registration");
        config
            .request_validators
            .validate_json_parsers(declared_registry.routes(), &config.json_request_parsers)
            .expect("missing JSON transform parser registration");
        config
            .response_validators
            .validate_routes(declared_registry.routes())
            .expect("invalid response validator registration");
        config
            .default_error_responses
            .validate_routes(declared_registry.routes())
            .expect("invalid default error profile registration");
        default_errors::validate_configuration(config, declared_registry.routes())
            .expect("incompatible default error response profile");
        let hooks = &config.version_body_hooks;
        hooks
            .validate_routes(declared_registry.routes())
            .expect("invalid body hook registration");
        let mut routes: HashMap<_, _> = registry
            .routes()
            .iter()
            .map(|route| (route_key(route.method, &route.openapi_path), route.clone()))
            .collect();
        for route in registry.routes() {
            if route.openapi_path != "/"
                && config
                    .optional_trailing_slash_paths
                    .contains(&route.openapi_path)
            {
                routes.insert(
                    route_key(route.method, &format!("{}/", route.openapi_path)),
                    route.clone(),
                );
            }
        }
        // Runtime selection and callback schemas retain original declarations.
        // Effective response exports may add opt-in default error statuses.
        let versions = declared_registry
            .routes()
            .iter()
            .map(|route| {
                (
                    route_key(route.method, &route.openapi_path),
                    {
                        let configured=hooks.compile(route).expect("invalid versioned route schema or hooks");
                        let key=route_key(route.method,&route.openapi_path);
                        if hooks.selection_before_authentication(route) && local_validation.contains(&key) {
                            local_version_plans.get(&key).expect("Pre-auth local version selection requires an exact registered local contract").clone()
                        } else { configured }
                    },
                )
            })
            .collect();
        Self {
            request_validators: config.request_validators.clone(),
            json_request_parsers: config.json_request_parsers.clone(),
            response_validators: config.response_validators.clone(),
            default_error_responses: Arc::new(config.default_error_responses.clone()),
            routes,
            versions,
            pre_auth_versions: declared_registry
                .routes()
                .iter()
                .filter(|route| hooks.selection_before_authentication(route))
                .map(|route| route_key(route.method, &route.openapi_path))
                .collect(),
            local_validation,
            body_limit: config.body_limit,
            authenticator: config.authenticator.clone(),
            response_authenticator: config.response_authenticator.clone(),
            authorizer: config.authorizer.clone(),
            auth_error_format: config.auth_error_format,
            auth_error_formats: config.auth_error_formats.clone(),
            handler_error_format: config.handler_error_format,
            parser_error_format: config.parser_error_format,
            response_status_error_format: config.response_status_error_format,
            telemetry,
        }
    }
}

struct HeaderExtractor<'a>(&'a axum::http::HeaderMap);
impl opentelemetry::propagation::Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(axum::http::HeaderName::as_str).collect()
    }
}

async fn observe_request(
    State(collector): State<OpenTelemetryCollector>,
    request: Request,
    next: Next,
) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("unmatched", MatchedPath::as_str);
    let method = method_to_contract(request.method()).map_or("OTHER", HttpMethod::as_str);
    let correlation_id =
        correlation::resolve(request.extensions(), request.headers(), "").to_owned();
    let parent = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(request.headers()))
    });
    let measurement = collector.start_request_with_context(method, route, &parent);
    let context = measurement.context();
    let head = request.method() == Method::HEAD;
    let response = next.run(request).with_context(context).await;
    crate::body_telemetry::observe_response(response, measurement, correlation_id, head)
}

fn route_key(method: HttpMethod, path: &str) -> String {
    format!("{} {}", method.as_str(), path)
}

async fn authenticate_contract(
    state: &ValidationState,
    contract: &RouteContract,
    parts: &Parts,
    framework_request: &FrameworkRequest,
) -> Result<AuthenticatedSession, ErrorResponse> {
    if let Some(AuthConfig::Cookie { name }) = &contract.auth {
        #[cfg(feature = "cookie-session")]
        let result = crate::cookie_session::authenticate(parts, name).await;
        #[cfg(not(feature = "cookie-session"))]
        let result = {
            let _ = (parts, name);
            Err(AuthError::MissingCredentials)
        };
        result.map_err(|error| authentication_error(contract, &error))
    } else if let Some(verifier) = &state.response_authenticator {
        verifier(framework_request.clone(), contract.clone()).await
    } else {
        match &state.authenticator {
            Some(verifier) => verifier(framework_request.clone(), contract.clone()).await,
            None => Err(AuthError::MissingCredentials),
        }
        .map_err(|error| authentication_error(contract, &error))
    }
}

fn authentication_error(contract: &RouteContract, error: &AuthError) -> ErrorResponse {
    // Presented but rejected JWTs match TypeScript's 403; opaque sessions keep 401.
    if *error == AuthError::InvalidCredentials && matches!(contract.auth, Some(AuthConfig::Jwt(_)))
    {
        ErrorResponse::new(ErrorKind::Forbidden, "invalid credentials")
    } else {
        error.to_error_response()
    }
}

async fn validate_contract(
    State(state): State<Arc<ValidationState>>,
    request: Request,
    next: Next,
) -> Response {
    let Some(method) = case_routes::selected_method(request.extensions())
        .or_else(|| method_to_contract(request.method()))
    else {
        return next.run(request).await;
    };
    let Some(matched) = request
        .extensions()
        .get::<MatchedPath>()
        .map(|matched| matched.as_str().to_owned())
    else {
        return next.run(request).await;
    };

    let Some(contract) = state.contract(method, &matched) else {
        return next.run(request).await;
    };

    let selected = [400, 401, 403, 404, 500].iter().any(|status| {
        state
            .default_error_responses
            .limits(&contract, *status)
            .is_some()
    });
    if !selected {
        return validate_matched_contract(&state, contract, request, next).await;
    }
    let correlation_id = correlation::resolve(
        request.extensions(),
        request.headers(),
        "forklaunch-request",
    );
    let context_request = FrameworkRequest::new(
        method,
        request.uri().path(),
        RequestContext::new(correlation_id),
    );
    let response = validate_matched_contract(&state, contract.clone(), request, next).await;
    default_errors::normalize(
        &state.default_error_responses,
        &contract,
        &context_request,
        response,
    )
    .await
}

fn attach_parsed_request(parts: &mut Parts, framework_request: &FrameworkRequest) {
    if let Some(parsed) = &framework_request.parsed_json {
        parts.extensions.insert(parsed.clone());
    }
    if let Some(fields) = &framework_request.parsed_fields {
        parts.extensions.insert(fields.clone());
    }
}

fn authentication_error_format(
    state: &ValidationState,
    contract: &RouteContract,
) -> AuthErrorFormat {
    state
        .auth_error_formats
        .get(&(
            contract.method.as_str().to_owned(),
            contract.operation_name.clone(),
        ))
        .copied()
        .unwrap_or(state.auth_error_format)
}

async fn validate_matched_contract(
    state: &Arc<ValidationState>,
    contract: RouteContract,
    request: Request,
    next: Next,
) -> Response {
    let (mut parts, body) = request.into_parts();
    let bytes = match to_bytes(body, state.body_limit).await {
        Ok(bytes) => bytes,
        Err(error) => return formatted_body_read_error(&error, state.parser_error_format, &parts),
    };

    let mut framework_request = match state.build_request(&contract, &parts, &bytes) {
        Ok(request) => request,
        Err(response) => return framework_response_to_axum(response),
    };
    match state
        .json_request_parsers
        .parse(&contract, &framework_request)
    {
        Ok(Some(parsed)) => {
            parts.extensions.insert(parsed.clone());
            framework_request.parsed_json = Some(parsed);
        }
        Ok(None) => {}
        Err(error) => {
            return framework_response_to_axum(validation_error_response_with_parser_format(
                &[error],
                &framework_request.context.correlation_id,
                state.parser_error_format,
            ));
        }
    }
    if let Some(response) = state.request_validators.transform_request(
        &contract,
        &mut framework_request,
        parts.uri.query(),
    ) {
        return framework_response_to_axum(response);
    }
    attach_parsed_request(&mut parts, &framework_request);
    if let Some(response) = state.request_validators.reject_at(
        RequestValidationStage::BeforeAuthentication,
        &contract,
        &framework_request,
        parts.uri.query(),
    ) {
        return framework_response_to_axum(response);
    }
    if let Some(response) = version_stage::prepare_before_authentication(
        state,
        &contract,
        &mut parts,
        &mut framework_request,
    ) {
        return response;
    }
    let requires_auth = contract.auth.is_some()
        || contract.rbac.is_some()
        || contract.access != AccessLevel::Public;
    if requires_auth {
        let authenticated =
            authenticate_contract(state, &contract, &parts, &framework_request).await;
        let result = authenticated.and_then(|session| {
            if let Some(authorizer) = &state.authorizer {
                authorizer(&session, &contract)?;
            } else if let Some(rbac) = &contract.rbac {
                authorize(&session.principal, rbac).map_err(|error| error.to_error_response())?;
            }
            Ok(session)
        });
        match result {
            Ok(session) => {
                parts.extensions.insert(session);
            }
            Err(error) => {
                state.telemetry.auth_failure(
                    &contract.operation_name,
                    &framework_request.context.correlation_id,
                );
                tracing::warn!(operation = %contract.operation_name, correlation_id = %framework_request.context.correlation_id, "request authentication or authorization failed");
                let format = authentication_error_format(state, &contract);
                let response = match format {
                    AuthErrorFormat::Json => error
                        .with_correlation_id(framework_request.context.correlation_id)
                        .to_response(),
                    AuthErrorFormat::TypeScriptText => {
                        FrameworkResponse::text(error.kind.status_code(), error.message)
                            .with_header("content-type", "text/plain; charset=utf-8")
                    }
                };
                return framework_response_to_axum(response);
            }
        }
    }
    if let Some(response) = state.request_validators.reject_at(
        RequestValidationStage::AfterAuthentication,
        &contract,
        &framework_request,
        parts.uri.query(),
    ) {
        return framework_response_to_axum(response);
    }
    execute_validated(state, contract, parts, bytes, framework_request, next).await
}

async fn execute_validated(
    state: &ValidationState,
    contract: RouteContract,
    mut parts: Parts,
    bytes: bytes::Bytes,
    mut framework_request: FrameworkRequest,
    next: Next,
) -> Response {
    // Handlers use the same context that authentication and validation observed.
    parts.extensions.insert(framework_request.context.clone());
    parts.extensions.insert(state.handler_error_format);
    parts.extensions.insert(state.parser_error_format);
    parts.extensions.insert(state.response_status_error_format);
    parts
        .extensions
        .insert(state.default_error_responses.clone());
    let union_responses = state.response_validators.is_versioned(&contract);
    if union_responses {
        parts
            .extensions
            .insert(response_validation::VersionedResponseValidation);
    }
    // LocalRouter owns selection and response validation for its handlers.
    // Authentication and the transport body limit still run above, but runtime
    // callbacks must execute exactly once and may change the selected schema.
    if state
        .local_validation
        .contains(&route_key(contract.method, &contract.openapi_path))
    {
        let response = next
            .run(Request::from_parts(parts, AxumBody::from(bytes)))
            .await;
        return finish_response(state, &contract, &framework_request, response).await;
    }
    let version_plan = &state.versions[&route_key(contract.method, &contract.openapi_path)];
    let contract =
        match select_contract_version(version_plan, contract, &mut parts, &mut framework_request) {
            Ok(contract) => contract,
            Err(errors) => {
                return framework_response_to_axum(validation_error_response_with_parser_format(
                    &errors,
                    &framework_request.context.correlation_id,
                    state.parser_error_format,
                ));
            }
        };
    let errors = validate_framework_request(&contract, &framework_request);
    if !errors.is_empty() {
        let correlation_id = framework_request.context.correlation_id.clone();
        return framework_response_to_axum(validation_error_response(&errors, &correlation_id));
    }

    let matched = union_responses
        .then(|| {
            framework_request
                .version
                .as_ref()
                .and_then(|selected| version_plan.matched_response_versions(selected))
        })
        .flatten();
    let response_contract = matched.as_ref().map_or_else(
        || contract.clone(),
        |matched| matched.status_contract(&contract),
    );
    let correlation_id = framework_request.context.correlation_id.clone();
    let mut response = next
        .run(Request::from_parts(parts, AxumBody::from(bytes)))
        .await;
    if let Some(matched) = &matched {
        response.extensions_mut().insert(matched.clone());
    }
    let original_metadata = matched
        .as_ref()
        .map(|_| (response.headers().clone(), response.extensions().clone()));
    let mut response = if state
        .default_error_responses
        .limits(&contract, response.status().as_u16())
        .is_some()
    {
        default_errors::normalize(
            &state.default_error_responses,
            &contract,
            &framework_request,
            response,
        )
        .await
    } else {
        validate_response_status(
            &response_contract,
            response,
            &correlation_id,
            state.response_status_error_format,
        )
    };
    if let Some((headers, extensions)) = original_metadata {
        response_validation::preserve_metadata(headers, extensions, &mut response);
    }
    if let Some(matched) = matched {
        response.extensions_mut().insert(matched);
    }
    finish_response(state, &contract, &framework_request, response).await
}

async fn finish_response(
    state: &ValidationState,
    contract: &RouteContract,
    request: &FrameworkRequest,
    response: Response,
) -> Response {
    state
        .response_validators
        .validate(contract, request, response)
        .await
}

fn with_body_limit(router: Router, limit: usize, format: ParserErrorFormat) -> Router {
    if format == ParserErrorFormat::TypeScriptText {
        router.layer(middleware::from_fn_with_state(limit, typescript_body_limit))
    } else {
        router.layer(RequestBodyLimitLayer::new(limit))
    }
}

fn formatted_body_read_error(
    error: &axum::Error,
    format: ParserErrorFormat,
    parts: &Parts,
) -> Response {
    let response = body_read_error_response(error);
    if format == ParserErrorFormat::TypeScriptText {
        framework_response_to_axum(typescript_parser_error_response(
            response.status().as_u16(),
            &request_correlation_id(parts),
        ))
    } else {
        response
    }
}

fn request_correlation_id(parts: &Parts) -> String {
    correlation::resolve(&parts.extensions, &parts.headers, "forklaunch-request").to_owned()
}

async fn typescript_body_limit(
    State(limit): State<usize>,
    request: Request,
    next: Next,
) -> Response {
    let (parts, body) = request.into_parts();
    if parts
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size > limit as u64)
    {
        return framework_response_to_axum(typescript_parser_error_response(
            413,
            &request_correlation_id(&parts),
        ));
    }
    // Keep streaming bodies bounded even when no Content-Length was supplied.
    let body = AxumBody::new(http_body_util::Limited::new(body, limit));
    next.run(Request::from_parts(parts, body)).await
}

fn body_read_error_response(error: &axum::Error) -> Response {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(error) = source {
        if error.is::<http_body_util::LengthLimitError>() {
            return framework_response_to_axum(
                ErrorResponse::new(ErrorKind::PayloadTooLarge, "request body exceeds limit")
                    .to_response(),
            );
        }
        source = error.source();
    }
    framework_response_to_axum(
        ErrorResponse::new(ErrorKind::Validation, "invalid request body").to_response(),
    )
}

fn select_contract_version(
    plan: &forklaunch_core::http::VersionedRoutePlan,
    contract: RouteContract,
    parts: &mut Parts,
    request: &mut FrameworkRequest,
) -> Result<RouteContract, Vec<forklaunch_core::http::ContractValidationError>> {
    let limits = parts
        .extensions
        .get::<forklaunch_core::http::MultipartLimits>()
        .copied()
        .unwrap_or_default();
    let selected = if let Some(prepared) = parts
        .extensions
        .get::<forklaunch_core::http::PreparedVersionSelection>()
    {
        plan.apply_prepared(prepared, request).map_err(|_| {
            vec![forklaunch_core::http::ContractValidationError::new(
                forklaunch_core::http::ContractValidationLocation::Body,
                "invalid prepared request selection",
            )]
        })?
    } else {
        plan.select_into_request(request, limits)
    };
    parts
        .extensions
        .remove::<forklaunch_core::http::VersionParsedBody>();
    if let Some(parsed) = &request.version_parsed_body {
        parts.extensions.insert(parsed.clone());
    }
    if parts
        .extensions
        .get::<forklaunch_core::http::PreparedVersionSelection>()
        .is_some()
    {
        request.version = None;
        parts
            .extensions
            .remove::<forklaunch_core::http::SelectedVersion>();
    }
    match selected {
        Ok(Some((selected, version))) => {
            request.version = Some(version.clone());
            parts.extensions.insert(version);
            Ok(selected)
        }
        Err(errors)
            if contract.options.request_validation == ValidationMode::Error
                || errors
                    .iter()
                    .any(forklaunch_core::http::ContractValidationError::is_parser_error) =>
        {
            Err(errors)
        }
        Ok(None) | Err(_) => Ok(contract),
    }
}

/// Enforces that handlers only return status codes documented in the contract.
fn local_handler_failure_response(
    error_format: HandlerErrorFormat,
    correlation_id: &str,
) -> FrameworkResponse {
    match error_format {
        HandlerErrorFormat::Json => {
            ErrorResponse::new(ErrorKind::Internal, "internal server error").to_response()
        }
        HandlerErrorFormat::TypeScriptText => FrameworkResponse::text(
            500,
            format!("Internal server error:\n\nCorrelation id: {correlation_id}"),
        )
        .with_header("content-type", "text/plain; charset=utf-8"),
    }
}

fn validate_response_status(
    contract: &RouteContract,
    response: Response,
    correlation_id: &str,
    format: ResponseStatusErrorFormat,
) -> Response {
    if contract.options.response_validation != ValidationMode::Error
        || contract.responses.is_empty()
    {
        return response;
    }

    let status = response.status().as_u16();
    if contract
        .responses
        .iter()
        .any(|documented| documented.status_code == status)
    {
        return response;
    }

    tracing::error!(status, operation = %contract.operation_name, correlation_id, "handler returned undocumented status");
    if format == ResponseStatusErrorFormat::TypeScriptText {
        return framework_response_to_axum(typescript_parser_error_response(
            status,
            correlation_id,
        ));
    }
    framework_response_to_axum(
        ErrorResponse::new(ErrorKind::Internal, "internal server error")
            .with_correlation_id(correlation_id.to_owned())
            .to_response(),
    )
}

fn build_framework_request(
    contract: &RouteContract,
    parts: &Parts,
    bytes: &[u8],
) -> Result<FrameworkRequest, FrameworkResponse> {
    let path = parts.uri.path();
    let correlation_id = request_correlation_id(parts);

    let mut request = FrameworkRequest::new(
        method_to_contract(&parts.method).unwrap_or(contract.method),
        path,
        RequestContext::new(correlation_id),
    );
    request.raw_path = Some(path.to_owned());
    request.transport_route_path = case_routes::dispatch_path(parts).map(str::to_owned);
    request.transport_route_method = case_routes::selected_method(&parts.extensions);
    request.params = match_path_checked(
        &contract.openapi_path,
        request.transport_route_path.as_deref().unwrap_or(path),
    )
    .map_err(|error| validation_error_response(&[error], &request.context.correlation_id))?
    .unwrap_or_default();

    request.raw_query = parts.uri.query().map(str::to_owned);
    request.parsed_fields = parts
        .extensions
        .get::<forklaunch_core::http::ParsedRequestFields>()
        .cloned();
    for (key, value) in parse_query(parts.uri.query().unwrap_or_default()) {
        request = request.with_query(key, value);
    }

    for (name, value) in &parts.headers {
        if let Ok(value) = value.to_str() {
            request = request.with_header(name.as_str(), value);
        }
    }
    if let Some(value) = correlation::supplied_header(&parts.extensions, &parts.headers) {
        request = request.with_header("x-correlation-id", value);
    }

    if contract.parsing_body().is_some_and(|body| {
        matches!(
            body.kind,
            BodyKind::Binary | BodyKind::File | BodyKind::Multipart
        )
    }) {
        request = request.with_body(FrameworkBody::Binary(bytes.to_vec()));
    } else if !bytes.is_empty()
        || contract
            .parsing_body()
            .is_some_and(|body| matches!(body.kind, BodyKind::Text | BodyKind::UrlEncoded))
    {
        request = match std::str::from_utf8(bytes) {
            Ok(text) => request.with_body(FrameworkBody::Text(text.to_owned())),
            Err(_) => request.with_body(FrameworkBody::Binary(bytes.to_vec())),
        };
    }

    request.parsed_json = parts
        .extensions
        .get::<forklaunch_core::http::ParsedJsonBody>()
        .cloned();
    request.version = parts
        .extensions
        .get::<forklaunch_core::http::SelectedVersion>()
        .cloned();
    request.session = parts
        .extensions
        .get::<AuthenticatedSession>()
        .map(|session| session.claims.clone());
    Ok(request)
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

fn method_to_contract(method: &Method) -> Option<HttpMethod> {
    HttpMethod::parse(method.as_str())
}

/// Converts a framework response, preserving its status, body, and headers.
#[must_use]
pub fn framework_response_to_axum(response: FrameworkResponse) -> Response {
    let status =
        StatusCode::from_u16(response.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = match response.body {
        FrameworkBody::Empty => AxumBody::empty(),
        FrameworkBody::Text(text) => AxumBody::from(text),
        FrameworkBody::Binary(bytes) => AxumBody::from(bytes),
    };

    let mut builder = Response::builder().status(status);
    for (key, value) in response.headers {
        builder = builder.header(key, value);
    }
    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn mount_documentation(
    mut router: Router,
    config: &RouterConfig,
    registry: &ContractRegistry,
) -> Router {
    if config.openapi_enabled {
        let hash_path = format!("{}-hash", config.openapi_path);
        let paths = [
            Some(config.openapi_path.as_str()),
            Some(hash_path.as_str()),
            config.docs_enabled.then_some(config.docs_path.as_str()),
            config
                .swagger_enabled
                .then_some(config.swagger_path.as_str()),
        ];
        assert!(
            !registry.routes().iter().any(|route| {
                matches!(route.method, HttpMethod::Get | HttpMethod::Head)
                    && paths
                        .iter()
                        .flatten()
                        .any(|path| *path == route.openapi_path)
            }),
            "documentation path collides with a declared operation"
        );
        let options = config.openapi_options(registry);
        let (document, fingerprint) =
            match generate_portable_openapi_json_with_options(registry, &options) {
                Ok(document) => {
                    let fingerprint = crate::docs::document_hash(&document);
                    (document, Some(fingerprint))
                }
                Err(error) => {
                    // The adapter supports runtime-only declarations as well as
                    // checked SDK contracts. Keep their existing HTTP/docs behavior,
                    // but reject SDK discovery rather than publishing partial types.
                    tracing::warn!(%error, "checked SDK documentation is unavailable");
                    (generate_openapi_json_with_options(registry, &options), None)
                }
            };
        router = router
            .route(&config.openapi_path, openapi_handler(document))
            .route(
                &format!("{}-hash", config.openapi_path),
                axum::routing::get(move || {
                    let fingerprint = fingerprint.clone();
                    async move {
                        fingerprint.map_or_else(
                            || {
                                (
                                    StatusCode::SERVICE_UNAVAILABLE,
                                    "Contract cannot produce checked SDK documentation",
                                )
                                    .into_response()
                            },
                            IntoResponse::into_response,
                        )
                    }
                }),
            );

        if config.docs_enabled {
            let html = crate::docs::scalar_html(
                &config.docs_title,
                &config.openapi_path,
                &config.docs_sources,
            );
            router = router.route(&config.docs_path, docs_handler(html));
        }

        if config.swagger_enabled {
            let html = crate::docs::swagger_html(&config.docs_title, &config.openapi_path);
            router = router.route(&config.swagger_path, docs_handler(html));
        }
    }

    router
}

fn build_transport_routes(
    handlers: HashMap<String, MethodRouter>,
    mut extended_routes: HashMap<String, HashMap<String, MethodRouter>>,
    registry: &ContractRegistry,
    config: &RouterConfig,
) -> (Router, HashMap<String, Router>) {
    for path in &config.optional_trailing_slash_paths {
        assert!(
            handlers.contains_key(path) && !path.contains("{*"),
            "invalid optional trailing-slash path selection"
        );
    }
    for path in &config.typescript_options_paths {
        assert!(
            handlers.contains_key(path) && !path.contains("{*"),
            "invalid automatic OPTIONS path selection"
        );
    }
    let mut isolated = HashMap::new();
    let mut router = Router::new();
    for (path, mut method_router) in handlers {
        if let Some(methods) = extended_routes.remove(&path) {
            let mut allowed: Vec<_> = registry
                .routes()
                .iter()
                .filter(|route| route.openapi_path == path)
                .map(|route| route.method.as_str())
                .collect();
            if allowed.contains(&"GET") && !allowed.contains(&"HEAD") {
                allowed.push("HEAD");
            }
            allowed.sort_unstable();
            allowed.dedup();
            let allow = allowed.join(", ");
            method_router = origin::extended(method_router, methods, allow, config);
        }
        method_router = origin::selected_options(method_router, &path, registry, config);
        if !config.typescript_case_insensitive_paths.is_empty() {
            let mut single = Router::new().route(&path, method_router.clone());
            if path != "/" && config.optional_trailing_slash_paths.contains(&path) {
                single = single.route(&format!("{path}/"), method_router.clone());
            }
            isolated.insert(path.clone(), single);
        }
        if !config.typescript_case_insensitive_paths.contains(&path) {
            if path != "/" && config.optional_trailing_slash_paths.contains(&path) {
                router = router.route(&format!("{path}/"), method_router.clone());
            }
            router = router.route(&path, method_router);
        }
    }

    (router, isolated)
}

fn instrument_router(
    router: Router,
    state: Arc<ValidationState>,
    telemetry: OpenTelemetryCollector,
    config: &RouterConfig,
    ids: CounterMakeRequestId,
) -> Router {
    let router = router.layer(middleware::from_fn_with_state(state, validate_contract));
    with_body_limit(router, config.body_limit, config.parser_error_format)
        .layer(middleware::from_fn_with_state(telemetry, observe_request))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(ids))
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(case_routes::restore))
}

/// A dependency-free request-id generator backed by a monotonic counter.
#[derive(Clone, Default)]
struct CounterMakeRequestId {
    counter: Arc<AtomicU64>,
}

impl MakeRequestId for CounterMakeRequestId {
    fn make_request_id<B>(&mut self, _request: &axum::http::Request<B>) -> Option<RequestId> {
        let id = self.counter.fetch_add(1, Ordering::Relaxed);
        HeaderValue::from_str(&format!("forklaunch-{id}"))
            .ok()
            .map(RequestId::new)
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::Body as AxumBody, http::Request};
    use forklaunch_core::ServiceInfo;
    use forklaunch_core::contract::{BodyContract, FieldContract, RouteContract};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn local_handler_failures_use_opt_in_safe_typescript_text() {
        for format in [HandlerErrorFormat::Json, HandlerErrorFormat::TypeScriptText] {
            let contract = RouteContract::get("Failure", "Failure", "/failure");
            let mut handlers = LocalRouter::new();
            handlers.register(contract.clone(), |_: FrameworkRequest| {
                Err(forklaunch_common::FrameworkError::new(
                    "secret_code",
                    "secret detail",
                ))
            });
            let app = ForkLaunchRouter::from_local_router(
                ServiceInfo::new("failure", "1"),
                [contract],
                handlers,
            )
            .with_config(RouterConfig {
                handler_error_format: format,
                ..RouterConfig::default()
            })
            .into_router();
            let response = app
                .oneshot(
                    Request::builder()
                        .uri("/failure")
                        .header("x-correlation-id", "failure-correlation")
                        .body(AxumBody::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let media_type = response.headers()["content-type"]
                .to_str()
                .unwrap()
                .to_owned();
            let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(!text.contains("secret"));
            match format {
                HandlerErrorFormat::Json => {
                    assert_eq!(media_type, "application/json");
                    assert_eq!(
                        text,
                        r#"{"code":"internal_error","message":"internal server error"}"#
                    );
                }
                HandlerErrorFormat::TypeScriptText => {
                    assert_eq!(media_type, "text/plain; charset=utf-8");
                    assert_eq!(
                        text,
                        "Internal server error:\n\nCorrelation id: failure-correlation"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn local_handlers_preserve_wire_input_and_binary_output() {
        let contract = RouteContract::post("Echo", "Echo input", "/echo/:id")
            .with_param(FieldContract::required("id", "String"))
            .with_query(FieldContract::required("label", "String"))
            .with_response(201, None);
        let mut handlers = LocalRouter::new();
        handlers.register(contract.clone(), |request: FrameworkRequest| {
            assert_eq!(request.params.get("id").unwrap(), "42");
            assert_eq!(request.query.get("label").unwrap(), "hello world&more");
            assert_eq!(request.headers.get("x-custom").unwrap(), "yes");
            assert_eq!(request.context.correlation_id, "test-correlation");
            assert_eq!(request.body, FrameworkBody::Binary(vec![0, 255, 1]));
            Ok(FrameworkResponse::new(201).with_body(FrameworkBody::Binary(vec![255, 0, 128])))
        });
        let app = ForkLaunchRouter::from_local_router(
            ServiceInfo::new("echo", "0.1.0"),
            [contract],
            handlers,
        )
        .into_router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/echo/42?label=hello+world%26more")
                    .header("x-custom", "yes")
                    .header("x-correlation-id", "test-correlation")
                    .body(AxumBody::from(vec![0, 255, 1]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(
            to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
            &[255, 0, 128]
        );
    }

    #[tokio::test]
    async fn declared_binary_and_file_bodies_preserve_utf8_and_empty_bytes() {
        use forklaunch_core::contract::SchemaRef;
        for kind in [BodyKind::Binary, BodyKind::File, BodyKind::Multipart] {
            for bytes in [
                b"plain ASCII".to_vec(),
                "hello 水".as_bytes().to_vec(),
                vec![0, 255],
                vec![],
            ] {
                let contract = RouteContract::post("Upload", "Upload bytes", "/upload")
                    .with_request_body(BodyContract {
                        kind,
                        schema: SchemaRef::named("Bytes"),
                        content_type: Some("application/octet-stream".into()),
                    })
                    .with_response(200, None);
                let expected = bytes.clone();
                let mut handlers = LocalRouter::new();
                handlers.register(contract.clone(), move |request: FrameworkRequest| {
                    assert_eq!(request.body, FrameworkBody::Binary(expected.clone()));
                    Ok(FrameworkResponse::new(200).with_body(request.body))
                });
                let app = ForkLaunchRouter::from_local_router(
                    ServiceInfo::new("upload", "1"),
                    [contract],
                    handlers,
                )
                .into_router();
                let response = app
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/upload")
                            .header("content-type", "application/octet-stream")
                            .body(AxumBody::from(bytes.clone()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(
                    to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
                    bytes
                );
            }
        }
    }

    #[tokio::test]
    async fn declared_text_media_with_charset_passes_transport_validation() {
        use forklaunch_core::contract::SchemaRef;
        for (kind, mime) in [
            (BodyKind::Text, "text/plain; charset=utf-8"),
            (
                BodyKind::UrlEncoded,
                "application/x-www-form-urlencoded; charset=utf-8",
            ),
        ] {
            let contract = RouteContract::post("Echo Text", "Echo Text", "/text")
                .with_request_body(BodyContract {
                    kind,
                    schema: SchemaRef::named("String"),
                    content_type: Some(mime.into()),
                })
                .with_response(200, None);
            let mut handlers = LocalRouter::new();
            handlers.register(contract.clone(), |request: FrameworkRequest| {
                assert!(request.body.as_text().is_some());
                Ok(FrameworkResponse::new(200).with_body(request.body))
            });
            let app = ForkLaunchRouter::from_local_router(
                ServiceInfo::new("echo", "1"),
                [contract],
                handlers,
            )
            .into_router();
            for text in ["hello 水", ""] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/text")
                            .header("content-type", mime.to_ascii_uppercase())
                            .body(AxumBody::from(text))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(
                    to_bytes(response.into_body(), 100).await.unwrap().as_ref(),
                    text.as_bytes()
                );
            }
            let missing_type = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/text")
                        .body(AxumBody::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(missing_type.status(), StatusCode::BAD_REQUEST);
        }
    }

    fn ok_handler() -> impl Handler<((),), ()> {
        || async { "ok" }
    }

    #[tokio::test]
    async fn registers_and_serves_a_route() {
        let app = ForkLaunchRouter::new(ServiceInfo::new("users", "0.1.0"))
            .get(
                RouteContract::get("Get User", "Get a user", "/users/:id")
                    .with_param(FieldContract::required("id", "String"))
                    .with_response(200, None),
                ok_handler(),
            )
            .into_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users/123")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_requests_failing_contract_validation() {
        let app = ForkLaunchRouter::new(ServiceInfo::new("users", "0.1.0"))
            .get(
                RouteContract::get("List Users", "List users", "/users")
                    .with_query(FieldContract::required("limit", "Integer"))
                    .with_response(200, None),
                ok_handler(),
            )
            .into_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn supports_multiple_methods_on_the_same_path() {
        let app = ForkLaunchRouter::new(ServiceInfo::new("users", "0.1.0"))
            .get(
                RouteContract::get("List Users", "List users", "/users").with_response(200, None),
                ok_handler(),
            )
            .post(
                RouteContract::post("Create User", "Create a user", "/users")
                    .with_request_body(BodyContract::json("CreateUserBody"))
                    .with_response(201, None),
                || async { (StatusCode::CREATED, "created") },
            )
            .into_router();

        let listed = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/users")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(listed.status(), StatusCode::OK);

        let created = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/users")
                    .header("content-type", "application/json")
                    .body(AxumBody::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn decorating_handlers_preserves_contracts_nesting_and_extended_methods() {
        assert!(
            ForkLaunchRouter::anonymous()
                .map_route("/missing", |route| route)
                .is_err()
        );
        let mut child = ForkLaunchRouter::anonymous();
        for method in [HttpMethod::Post, HttpMethod::Subscribe] {
            child = child.route(
                RouteContract::new(
                    format!("Dispatch {}", method.as_str()),
                    "Event dispatcher",
                    method,
                    "/events/:feature",
                )
                .with_param(FieldContract::required("feature", "String"))
                .with_query(FieldContract::required("limit", "Integer"))
                .with_response(200, None)
                .with_response(404, None),
                || async { (StatusCode::NOT_FOUND, "original") },
            );
        }
        let before = child.registry().clone().into_artifact().to_json();
        let child = child
            .map_route("/events/:feature", |route| {
                route.layer(middleware::from_fn(
                    |request: axum::extract::Request, next: Next| async move {
                        if request
                            .headers()
                            .get("x-dispatch")
                            .is_some_and(|value| value == "new")
                        {
                            (StatusCode::OK, "decorated").into_response()
                        } else {
                            next.run(request).await
                        }
                    },
                ))
            })
            .unwrap();
        assert_eq!(child.registry().clone().into_artifact().to_json(), before);
        let app = ForkLaunchRouter::new(ServiceInfo::new("events", "0.1.0"))
            .nest("/api", child)
            .into_router();
        for method in ["POST", "SUBSCRIBE"] {
            for (query, dispatch, expected, body) in [
                ("?limit=1", "new", StatusCode::OK, Some("decorated")),
                ("?limit=1", "old", StatusCode::NOT_FOUND, Some("original")),
                ("", "new", StatusCode::BAD_REQUEST, None),
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(format!("/api/events/email{query}"))
                            .header("x-dispatch", dispatch)
                            .body(AxumBody::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), expected);
                if let Some(body) = body {
                    assert_eq!(to_bytes(response.into_body(), 1024).await.unwrap(), body);
                }
            }
        }
    }

    #[tokio::test]
    async fn nested_routers_share_validation_and_base_paths() {
        let child = ForkLaunchRouter::anonymous().get(
            RouteContract::get("Get Org", "Get an org", "/:id")
                .with_param(FieldContract::required("id", "String"))
                .with_response(200, None),
            ok_handler(),
        );
        let app = ForkLaunchRouter::new(ServiceInfo::new("orgs", "0.1.0"))
            .nest("/orgs", child)
            .into_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/orgs/42")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_undocumented_response_status() {
        let app = ForkLaunchRouter::new(ServiceInfo::new("users", "0.1.0"))
            .get(
                RouteContract::get("List Users", "List users", "/users").with_response(200, None),
                || async { (StatusCode::IM_A_TEAPOT, "nope") },
            )
            .into_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/users")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(response.into_body(), DEFAULT_BODY_LIMIT_BYTES)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(body.contains("internal server error"));
        assert!(!body.contains("418"));
    }

    #[tokio::test]
    async fn serves_swagger_ui_when_enabled() {
        let config = RouterConfig {
            swagger_enabled: true,
            ..RouterConfig::default()
        };
        let app = ForkLaunchRouter::new(ServiceInfo::new("users", "0.1.0"))
            .with_config(config)
            .get(
                RouteContract::get("List Users", "List users", "/users").with_response(200, None),
                ok_handler(),
            )
            .into_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/swagger")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), DEFAULT_BODY_LIMIT_BYTES)
            .await
            .unwrap();
        let html = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(html.contains("swagger-ui"));
        assert!(html.contains("/openapi.json"));
    }

    #[tokio::test]
    async fn serves_generated_openapi_document() {
        let app = ForkLaunchRouter::new(ServiceInfo::new("users", "1.0.0"))
            .get(
                RouteContract::get("List Users", "List users", "/users").with_response(200, None),
                ok_handler(),
            )
            .into_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/openapi.json")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), DEFAULT_BODY_LIMIT_BYTES)
            .await
            .unwrap();
        let document = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(document.contains("\"openapi\":\"3.1.0\""));
        assert!(document.contains("listUsers"));
    }
}

#[cfg(test)]
mod extended_method_tests {
    use super::*;
    use forklaunch_core::auth::Principal;
    use forklaunch_core::contract::FieldContract;

    #[tokio::test]
    async fn every_exported_method_dispatches_validates_and_authenticates_after_nesting() {
        let mut child = ForkLaunchRouter::anonymous();
        for method in HttpMethod::ALL {
            let contract =
                RouteContract::new(method.as_str(), "Method probe", method, "/probe/:id")
                    .with_access(AccessLevel::Authenticated)
                    .with_param(FieldContract::required("id", "String"))
                    .with_response(200, None);
            child = child.route(contract, move |request: Request| async move {
                assert_eq!(request.method().as_str(), method.as_str());
                assert!(request.extensions().get::<AuthenticatedSession>().is_some());
                (StatusCode::OK, [("x-method", method.as_str())])
            });
        }
        let config = RouterConfig::default().with_authenticator(|request, contract| async move {
            assert_eq!(request.method, contract.method);
            assert_eq!(request.params.get("id").map(String::as_str), Some("42"));
            if request.headers.get("authorization").map(String::as_str) != Some("verified") {
                return Err(AuthError::MissingCredentials);
            }
            Ok(AuthenticatedSession::new(
                Principal::new("tester"),
                serde_json::json!({"sub":"tester"}),
            ))
        });
        let app = ForkLaunchRouter::anonymous()
            .nest("/module", child)
            .with_config(config)
            .into_router();
        for method in HttpMethod::ALL {
            for authorized in [false, true] {
                let mut request = Request::builder()
                    .method(method.as_str())
                    .uri("/module/probe/42");
                if authorized {
                    request = request.header("authorization", "verified");
                }
                let response = app
                    .clone()
                    .oneshot(request.body(AxumBody::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    if authorized {
                        StatusCode::OK
                    } else {
                        StatusCode::UNAUTHORIZED
                    },
                    "{}",
                    method.as_str()
                );
                if authorized {
                    assert_eq!(response.headers()["x-method"], method.as_str());
                }
            }
        }
        let preflight = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/module/probe/42")
                    .header(header::ORIGIN, "https://browser.example")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(preflight.status(), StatusCode::OK);
        assert!(
            preflight
                .headers()
                .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("BREW")
                    .uri("/module/probe/42")
                    .body(AxumBody::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let allow = response.headers()[header::ALLOW].to_str().unwrap();
        for method in HttpMethod::ALL {
            assert!(
                allow.split(", ").any(|value| value == method.as_str()),
                "{allow}"
            );
        }
    }

    #[tokio::test]
    async fn local_handlers_preserve_extended_methods() {
        let mut local = LocalRouter::new();
        let contracts: Vec<_> = HttpMethod::ALL
            .into_iter()
            .filter(|method| !method.is_openapi_standard())
            .map(|method| {
                let contract =
                    RouteContract::new(method.as_str(), "Local method", method, "/probe")
                        .with_response(200, None);
                local.register(contract.clone(), move |request: FrameworkRequest| {
                    assert_eq!(request.method, method);
                    Ok(FrameworkResponse::text(200, method.as_str()))
                });
                contract
            })
            .collect();
        let app = ForkLaunchRouter::from_local_router(
            ServiceInfo::new("methods", "0.1.0"),
            contracts,
            local,
        )
        .into_router();
        for method in HttpMethod::ALL
            .into_iter()
            .filter(|method| !method.is_openapi_standard())
        {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method.as_str())
                        .uri("/probe")
                        .body(AxumBody::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                to_bytes(response.into_body(), 1024).await.unwrap(),
                method.as_str()
            );
        }
    }

    #[test]
    #[should_panic(expected = "route method must match its contract")]
    fn mismatched_handler_method_cannot_bypass_contract_security() {
        let _ = ForkLaunchRouter::anonymous()
            .get(RouteContract::post("wrong", "wrong", "/wrong"), || async {
                StatusCode::OK
            });
    }
}

#[cfg(test)]
mod request_context_tests;

#[cfg(test)]
mod json_request_tests;

#[cfg(test)]
mod request_order_tests;

#[cfg(test)]
mod request_transform_tests;

#[cfg(test)]
mod request_fields_tests;

#[cfg(test)]
mod version_stage_tests;

mod version_stage;

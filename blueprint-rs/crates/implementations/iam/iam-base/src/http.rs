//! Contract-bound HTTP support for an asynchronous, tenant-bound IAM backend.
//!
//! Generated services own their controllers: session controllers call
//! [`IamHttp::sessions`], directory controllers call the audited services from
//! [`IamHttp::directory`] and [`IamHttp::user_directory`], and [`IamRoutes`]
//! binds them to the interface's contracts. Storage, authentication and audit
//! stay here. The packaged [`IamHttp::handler`] remains for services generated
//! before 0.2.5.
#[cfg(feature = "browser-sessions")]
pub mod browser;
pub mod directory;
use axum::{
    Extension, Json,
    extract::{
        Path, Query,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{MethodRouter, any},
};
use forklaunch_axum::{
    ForkLaunchRouter, RouterConfig, module_auth::HmacJwksAuth, session::AuthenticatedSession,
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::auth::{AuthError, Principal, parse_bearer};
use forklaunch_core::contract::RouteContract;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::PageRequest;
use forklaunch_core::http::ErrorKind;
use forklaunch_core::http::ErrorResponse;
use forklaunch_iam_interface::{AsyncIamSessions, Credentials, module_contracts};
use std::{collections::HashMap, sync::Arc};

/// The permission that lets a session read users other than itself and list
/// users/organizations. Matches the TypeScript IAM directory read permission.
pub const ADMIN_READ_PERMISSION: &str = "platform:read";

use crate::directory::{compliance::DirectoryCompliance, postgres::PostgresDirectoryStore};
pub use directory::DirectoryAuthorizationMode;

/// One operation's request handler. Its contract supplies the method and path.
pub type Handler = MethodRouter;

/// Opaque session operations, in their mounted order.
const SESSION_OPERATIONS: [&str; 7] = [
    "getHealth",
    "authenticate",
    "logout",
    "getSessionUser",
    "listUsers",
    "listUserPermissions",
    "listOrganizations",
];

/// Session, directory and compliance handlers over one trusted tenant. The
/// stores must already be bound to that tenant; HTTP input never selects one.
pub struct IamHttp<S> {
    sessions: Arc<S>,
    directory: Arc<PostgresDirectoryStore>,
    compliance: Arc<DirectoryCompliance>,
    auth: Arc<HmacJwksAuth>,
    mode: DirectoryAuthorizationMode,
    service: ServiceInfo,
    jwks: Option<Arc<serde_json::Value>>,
    cors_origins: Vec<axum::http::HeaderValue>,
}

impl<S> Clone for IamHttp<S> {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            directory: Arc::clone(&self.directory),
            compliance: Arc::clone(&self.compliance),
            auth: Arc::clone(&self.auth),
            mode: self.mode,
            service: self.service.clone(),
            jwks: self.jwks.clone(),
            cors_origins: self.cors_origins.clone(),
        }
    }
}

impl<S: AsyncIamSessions + 'static> IamHttp<S> {
    /// Directory reads use JWKS JWTs and internal operations use request HMAC
    /// signatures. Stored-session mode verifies the JWT, then replaces identity
    /// and grants with the persisted session before framework RBAC runs.
    #[must_use]
    pub fn new(
        sessions: Arc<S>,
        directory: PostgresDirectoryStore,
        auth: HmacJwksAuth,
        mode: DirectoryAuthorizationMode,
        service: ServiceInfo,
    ) -> Self {
        let auth = if mode == DirectoryAuthorizationMode::StoredSessions {
            auth.with_role_resolver(|_| async { Ok(Vec::new()) })
        } else {
            auth
        };
        Self {
            sessions,
            directory: Arc::new(directory),
            compliance: Arc::new(DirectoryCompliance::default()),
            auth: Arc::new(auth),
            mode,
            service,
            jwks: None,
            cors_origins: Vec::new(),
        }
    }

    /// Publish this public JWK set at `GET /api/auth/jwks`, the URL every
    /// generated `JWKS_PUBLIC_KEY_URL` points at. Pass the session signer's
    /// [`RsaJwtSessionIssuer::jwks`](crate::jwt_sessions::RsaJwtSessionIssuer::jwks);
    /// without one no key endpoint is mounted.
    #[must_use]
    pub fn with_jwks(mut self, jwks: serde_json::Value) -> Self {
        self.jwks = Some(Arc::new(jwks));
        self
    }

    /// Exact browser origins allowed to call IAM cross-origin, with
    /// credentials (TypeScript `CORS_ORIGINS`). Responses echo the origin and
    /// never `*`; with no origins no CORS headers are sent, so browsers on
    /// other origins are refused.
    ///
    /// # Errors
    /// Rejects `*`, empty values and origins with a path, query or credentials.
    pub fn with_cors_origins<I, O>(mut self, origins: I) -> Result<Self, BlueprintError>
    where
        I: IntoIterator<Item = O>,
        O: AsRef<str>,
    {
        for origin in origins {
            let origin = origin.as_ref().trim();
            let valid = (origin.starts_with("https://") || origin.starts_with("http://"))
                && !origin.contains('*')
                && !origin.contains('@')
                && origin
                    .split("://")
                    .nth(1)
                    .is_some_and(|rest| !rest.is_empty() && !rest.contains(['/', '?', '#']));
            let value = axum::http::HeaderValue::from_str(origin)
                .ok()
                .filter(|_| valid)
                .ok_or_else(|| {
                    BlueprintError::new(
                        "iam_cors_origin_invalid",
                        "CORS_ORIGINS entries must be exact http(s) origins",
                    )
                })?;
            if !self.cors_origins.contains(&value) {
                self.cors_origins.push(value);
            }
        }
        Ok(self)
    }

    /// Use application-owned compliance classifications for export and erasure.
    /// Incomplete storage operations return an error rather than a successful erasure.
    #[must_use]
    pub fn with_compliance(mut self, compliance: DirectoryCompliance) -> Self {
        self.compliance = Arc::new(compliance);
        self
    }

    /// The tenant-bound session service that session controllers call.
    #[must_use]
    pub fn sessions(&self) -> &Arc<S> {
        &self.sessions
    }

    /// The packaged handler for one session, directory or compliance operation.
    ///
    /// Deprecated since 0.2.5: generated services now own their controllers.
    /// It stays available, unchanged, for services generated earlier.
    ///
    /// # Panics
    /// Panics if `operation` is not an IAM operation.
    pub fn handler(&self, operation: &str) -> Handler {
        if SESSION_OPERATIONS.contains(&operation) {
            self.session_handler(operation)
        } else {
            directory::handler(self, &contract(operation))
        }
    }

    /// Collects handlers for the service's IAM routes.
    #[must_use]
    pub fn routes(&self) -> IamRoutes<S> {
        IamRoutes {
            http: self.clone(),
            handlers: HashMap::new(),
        }
    }

    fn session_handler(&self, operation: &str) -> Handler {
        let service = Arc::clone(&self.sessions);
        match operation {
            "getHealth" => any(|| async { Json(serde_json::json!({"status":"ok"})) }),
            "authenticate" => any(
                move |headers: HeaderMap, credentials: Result<Json<Credentials>, JsonRejection>| {
                    login_handler(Arc::clone(&service), headers, credentials)
                },
            ),
            "logout" => {
                any(move |headers: HeaderMap| logout_handler(Arc::clone(&service), headers))
            }
            "getSessionUser" => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      headers: HeaderMap,
                      Path(id): Path<String>| {
                    let service = Arc::clone(&service);
                    async move {
                        if !may_read_user(&session, &id) {
                            return forbidden(&headers);
                        }
                        user_handler(service, headers, id).await
                    }
                },
            ),
            "listUsers" => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      headers: HeaderMap,
                      query: Result<Query<PageQuery>, QueryRejection>| {
                    let service = Arc::clone(&service);
                    async move {
                        if !is_admin_reader(&session) {
                            return forbidden(&headers);
                        }
                        users_handler(service, headers, query).await
                    }
                },
            ),
            "listUserPermissions" => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      headers: HeaderMap,
                      Path(id): Path<String>| {
                    let service = Arc::clone(&service);
                    async move {
                        if !may_read_user(&session, &id) {
                            return forbidden(&headers);
                        }
                        permissions_handler(service, headers, id).await
                    }
                },
            ),
            _ => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      headers: HeaderMap,
                      query: Result<Query<PageQuery>, QueryRejection>| {
                    let service = Arc::clone(&service);
                    async move {
                        if !is_admin_reader(&session) {
                            return forbidden(&headers);
                        }
                        organizations_handler(service, headers, query).await
                    }
                },
            ),
        }
    }
}

/// The service's IAM routes. Each operation's contract comes from the IAM
/// interface, so routes, SDKs, `OpenAPI` and MCP tools share one declaration.
pub struct IamRoutes<S> {
    http: IamHttp<S>,
    handlers: HashMap<String, Handler>,
}

impl<S: AsyncIamSessions + 'static> IamRoutes<S> {
    /// Serve `operation` with `handler`. Operations without a handler are not mounted.
    ///
    /// # Panics
    /// Panics if `operation` is not an IAM operation or is already routed.
    #[must_use]
    pub fn route(mut self, operation: &str, handler: Handler) -> Self {
        let _ = contract(operation);
        assert!(
            self.handlers
                .insert(operation.to_owned(), handler)
                .is_none(),
            "IAM operation {operation} is already routed"
        );
        self
    }

    /// Mount the routed operations in contract order with IAM authentication.
    #[must_use]
    pub fn finish(mut self) -> ForkLaunchRouter {
        let mut router = ForkLaunchRouter::new(self.http.service.clone());
        let directory = module_contracts::registry();
        let operations = SESSION_OPERATIONS
            .iter()
            .map(|operation| (*operation).to_owned())
            .chain(
                directory
                    .routes()
                    .iter()
                    .map(|route| route.operation_name.clone()),
            );
        for operation in operations {
            if let Some(handler) = self.handlers.remove(&operation) {
                router = router.route(contract(&operation), handler);
            }
        }
        let mut config = directory::router_config(&self.http);
        config.cors = Some(cors_layer(&self.http.cors_origins));
        let mut router = router.with_config(config);
        if let Some(jwks) = self.http.jwks.clone() {
            router = router.nest_internal(
                "/api/auth",
                axum::Router::new().route(
                    "/jwks",
                    axum::routing::get(move || {
                        let jwks = Arc::clone(&jwks);
                        async move {
                            (
                                [(axum::http::header::CACHE_CONTROL, "public, max-age=300")],
                                Json(jwks.as_ref().clone()),
                            )
                        }
                    }),
                ),
            );
        }
        router
    }
}

/// Allow-listed origins only, with credentials; no origins sends no CORS headers.
fn cors_layer(origins: &[axum::http::HeaderValue]) -> tower_http::cors::CorsLayer {
    use axum::http::Method;
    use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};
    if origins.is_empty() {
        return CorsLayer::new();
    }
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins.iter().cloned()))
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(AllowHeaders::mirror_request())
}

/// Whether the session holds [`ADMIN_READ_PERMISSION`].
#[must_use]
pub fn is_admin_reader(session: &AuthenticatedSession) -> bool {
    session
        .principal
        .permissions
        .iter()
        .any(|permission| permission == ADMIN_READ_PERMISSION)
}

/// A session reads itself; reading anyone else needs [`ADMIN_READ_PERMISSION`].
#[must_use]
pub fn may_read_user(session: &AuthenticatedSession, id: &str) -> bool {
    session.principal.subject == id || is_admin_reader(session)
}

fn forbidden(headers: &HeaderMap) -> Response {
    response_error(ErrorKind::Forbidden, "forbidden", headers)
}

/// The combined session and directory contract for `operation`.
fn contract(operation: &str) -> RouteContract {
    module_contracts::combined_registry()
        .routes()
        .iter()
        .find(|route| route.operation_name == operation)
        .cloned()
        .unwrap_or_else(|| panic!("IAM operation {operation} has no contract"))
}

fn session_config<S: AsyncIamSessions + 'static>(service: &Arc<S>) -> RouterConfig {
    let verifier = Arc::clone(service);
    RouterConfig::default().with_authenticator(move |request, _| {
        let service = Arc::clone(&verifier);
        async move {
            let token = request
                .headers
                .get("authorization")
                .and_then(|header| parse_bearer(header))
                .filter(|token| token.len() <= 4096)
                .ok_or(AuthError::MissingCredentials)?;
            authenticate_stored_session(service.as_ref(), token).await
        }
    })
}

async fn authenticate_stored_session<S: AsyncIamSessions>(
    service: &S,
    token: &str,
) -> Result<AuthenticatedSession, AuthError> {
    if token.is_empty() || token.len() > 4096 {
        return Err(AuthError::InvalidCredentials);
    }
    let session = service
        .validate_session(token)
        .await
        .map_err(|error| auth_backend_error(&error))?
        .ok_or(AuthError::InvalidCredentials)?;
    let user = service
        .get_user(&session.user_id)
        .await
        .map_err(|error| auth_backend_error(&error))?
        .filter(|user| !user.disabled)
        .ok_or(AuthError::InvalidCredentials)?;
    let permissions = service
        .permissions_for_user(&user.id)
        .await
        .map_err(|error| auth_backend_error(&error))?;
    let mut principal = Principal::new(&user.id);
    principal.roles = user.roles;
    principal.permissions = permissions
        .into_iter()
        .map(|p| format!("{}:{}", p.resource, p.action))
        .collect();
    let claims = serde_json::json!({"sub":principal.subject, "roles":principal.roles,
                "permissions":principal.permissions, "exp":session.expires_at_unix_seconds});
    Ok(AuthenticatedSession::new(principal, claims))
}

async fn login_handler<S: AsyncIamSessions>(
    service: Arc<S>,
    headers: HeaderMap,
    credentials: Result<Json<Credentials>, JsonRejection>,
) -> Response {
    let Ok(Json(credentials)) = credentials else {
        return invalid_input(&headers);
    };
    match service.authenticate(credentials).await {
        Ok(session) => {
            let mut response = Json(session).into_response();
            response.headers_mut().insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-store"),
            );
            response
        }
        Err(error) if error.code == "iam_invalid_credentials" => {
            response_error(ErrorKind::Unauthorized, "invalid credentials", &headers)
        }
        Err(error) => backend_error(&error, &headers),
    }
}

async fn logout_handler<S: AsyncIamSessions>(service: Arc<S>, headers: HeaderMap) -> Response {
    let Some(token) = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_bearer)
    else {
        return response_error(ErrorKind::Unauthorized, "missing credentials", &headers);
    };
    match service.revoke_session(token).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => backend_error(&error, &headers),
    }
}

async fn user_handler<S: AsyncIamSessions>(
    service: Arc<S>,
    headers: HeaderMap,
    id: String,
) -> Response {
    match service.get_user(&id).await {
        Ok(Some(user)) => Json(user).into_response(),
        Ok(None) => response_error(ErrorKind::NotFound, "user not found", &headers),
        Err(error) => backend_error(&error, &headers),
    }
}

async fn users_handler<S: AsyncIamSessions>(
    service: Arc<S>,
    headers: HeaderMap,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Response {
    let Ok(request) = page_request(query) else {
        return invalid_input(&headers);
    };
    match service.list_users(request).await {
        Ok(page) => Json(page).into_response(),
        Err(error) => backend_error(&error, &headers),
    }
}

async fn permissions_handler<S: AsyncIamSessions>(
    service: Arc<S>,
    headers: HeaderMap,
    id: String,
) -> Response {
    match service.permissions_for_user(&id).await {
        Ok(permissions) => Json(permissions).into_response(),
        Err(error) => backend_error(&error, &headers),
    }
}

async fn organizations_handler<S: AsyncIamSessions>(
    service: Arc<S>,
    headers: HeaderMap,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Response {
    let Ok(request) = page_request(query) else {
        return invalid_input(&headers);
    };
    match service.list_organizations(request).await {
        Ok(page) => Json(page).into_response(),
        Err(error) => backend_error(&error, &headers),
    }
}

/// The `limit` and `cursor` query of a session list route.
#[derive(serde::Deserialize)]
pub struct PageQuery {
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

/// The page a list query asks for: `limit` defaults to 100 and must be 1 to
/// 1000. `None` for a malformed query or an out-of-range limit.
#[must_use]
pub fn list_page(query: Result<Query<PageQuery>, QueryRejection>) -> Option<PageRequest> {
    page_request(query).ok()
}

fn page_request(query: Result<Query<PageQuery>, QueryRejection>) -> Result<PageRequest, ()> {
    let Query(query) = query.map_err(|_| ())?;
    let limit = query.limit.unwrap_or(100);
    if !(1..=1000).contains(&limit) {
        return Err(());
    }
    Ok(PageRequest {
        limit,
        cursor: query.cursor,
    })
}

fn invalid_input(headers: &HeaderMap) -> Response {
    response_error(ErrorKind::Validation, "invalid request", headers)
}

/// A session route error: the framework's JSON error body with the request's
/// `x-correlation-id` (or `x-request-id`).
#[must_use]
pub fn session_error(kind: ErrorKind, message: &str, headers: &HeaderMap) -> Response {
    response_error(kind, message, headers)
}

/// A session backend failure: logs the error code and returns a generic 500.
#[must_use]
pub fn session_backend_error(error: &BlueprintError, headers: &HeaderMap) -> Response {
    backend_error(error, headers)
}

fn response_error(kind: ErrorKind, message: &str, headers: &HeaderMap) -> Response {
    let correlation = headers
        .get("x-correlation-id")
        .or_else(|| headers.get("x-request-id"))
        .and_then(|header| header.to_str().ok())
        .unwrap_or("");
    forklaunch_axum::router::framework_response_to_axum(
        ErrorResponse::new(kind, message)
            .with_correlation_id(correlation)
            .to_response(),
    )
}

fn backend_error(error: &BlueprintError, headers: &HeaderMap) -> Response {
    tracing::error!(code = %error.code, "IAM backend operation failed");
    response_error(ErrorKind::Internal, "internal server error", headers)
}

fn auth_backend_error(error: &BlueprintError) -> AuthError {
    tracing::error!(code = %error.code, "IAM session backend operation failed");
    AuthError::InvalidCredentials
}

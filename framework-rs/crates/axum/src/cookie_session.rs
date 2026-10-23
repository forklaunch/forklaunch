//! Optional server-side browser sessions. Applications verify login credentials and
//! resolve current authorization from their own persistence on every request.
// Rejections are axum responses, returned as handlers return them; boxing would
// only add an allocation per request.
#![allow(clippy::result_large_err)]
use crate::session::{AuthenticatedSession, AuthenticationFuture};
use axum::{
    Extension, Router,
    extract::FromRequestParts,
    http::{Method, StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use forklaunch_core::auth::AuthError;
use std::{future::Future, sync::Arc};
use subtle::ConstantTimeEq;
use tower_sessions::{Expiry, Session, SessionManagerLayer, SessionStore};

const SUBJECT: &str = "forklaunch.subject";
const CSRF: &str = "forklaunch.csrf";
type Resolver = Arc<dyn Fn(String) -> AuthenticationFuture + Send + Sync>;
type Wrap = Arc<dyn Fn(Router) -> Router + Send + Sync>;

/// Secure, `HttpOnly`, `SameSite=Strict` cookies with a thirty-minute expiry refreshed by session writes.
/// Use a durable shared store in production. The resolver must reject revoked or
/// disabled identities; session-map writes are not an atomic revocation ledger.
#[derive(Clone)]
pub struct CookieSessions {
    name: String,
    origin: String,
    resolver: Resolver,
    layer: Wrap,
}
impl CookieSessions {
    /// `origin` must be an exact HTTPS origin (no path, credentials or query).
    ///
    /// # Errors
    /// Rejects invalid cookie names and noncanonical or non-HTTPS origins.
    pub fn new<S, F, Fut>(
        name: &str,
        origin: &str,
        store: S,
        resolver: F,
    ) -> Result<Self, &'static str>
    where
        S: SessionStore + Clone,
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<AuthenticatedSession, AuthError>> + Send + 'static,
    {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
        {
            return Err("invalid session cookie name");
        }
        let url = url::Url::parse(origin).map_err(|_| "invalid trusted origin")?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || url.origin().ascii_serialization() != origin
        {
            return Err("trusted origin must be an exact HTTPS origin");
        }
        let layer = SessionManagerLayer::new(store)
            .with_name(name.to_owned())
            .with_expiry(Expiry::OnInactivity(
                tower_sessions::cookie::time::Duration::minutes(30),
            ));
        Ok(Self {
            name: name.to_owned(),
            origin: origin.to_owned(),
            resolver: Arc::new(move |subject| Box::pin(resolver(subject))),
            layer: Arc::new(move |router| router.layer(layer.clone())),
        })
    }
    /// Apply this policy to plain Axum login, logout and CSRF bootstrap routes.
    pub fn wrap(&self, router: Router) -> Router {
        (self.layer)(router.layer(Extension(self.clone())))
    }
}

/// Extract in login/logout handlers as well as CSRF bootstrap handlers. Unsafe
/// requests require an exact Origin and the server-issued `x-csrf-token` header.
pub struct CookieSession {
    session: Session,
    policy: CookieSessions,
    unsafe_method: bool,
}
fn rejected() -> AuthError {
    AuthError::Forbidden("invalid CSRF proof".to_owned())
}
#[allow(clippy::needless_pass_by_value)] // Used directly as a Result mapper.
fn failure(error: AuthError) -> Response {
    let status = if matches!(error, AuthError::Forbidden(_)) {
        StatusCode::FORBIDDEN
    } else {
        StatusCode::UNAUTHORIZED
    };
    (status, error.to_string()).into_response()
}
async fn extract(parts: &Parts) -> Result<CookieSession, AuthError> {
    let policy = parts
        .extensions
        .get::<CookieSessions>()
        .cloned()
        .ok_or(AuthError::MissingCredentials)?;
    let session = parts
        .extensions
        .get::<Session>()
        .cloned()
        .ok_or(AuthError::MissingCredentials)?;
    let unsafe_method = !matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);
    if unsafe_method {
        let mut origins = parts.headers.get_all("origin").iter();
        if origins.next().and_then(|v| v.to_str().ok()) != Some(policy.origin.as_str())
            || origins.next().is_some()
        {
            return Err(rejected());
        }
        let expected: String = session
            .get(CSRF)
            .await
            .map_err(|_| AuthError::InvalidCredentials)?
            .ok_or_else(rejected)?;
        let mut tokens = parts.headers.get_all("x-csrf-token").iter();
        let token = tokens.next().ok_or_else(rejected)?;
        if tokens.next().is_some() || !bool::from(expected.as_bytes().ct_eq(token.as_bytes())) {
            return Err(rejected());
        }
    }
    Ok(CookieSession {
        session,
        policy,
        unsafe_method,
    })
}
impl<S: Send + Sync> FromRequestParts<S> for CookieSession {
    type Rejection = Response;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Response> {
        extract(parts).await.map_err(failure)
    }
}
impl CookieSession {
    /// Opaque server-side browser binding for encrypted single-use challenges.
    /// This is secret cookie identity, not authentication. Never expose or log it.
    /// Bootstrap CSRF first so a persisted browser session ID is available.
    /// # Errors
    /// Rejects an absent cookie session.
    pub fn browser_binding(&self) -> Result<String, AuthError> {
        self.session
            .id()
            .map(|id| id.to_string())
            .ok_or(AuthError::MissingCredentials)
    }

    /// Reads the application-owned server-side identity binding. This is not an
    /// authentication result: use `authenticated` to apply the current resolver.
    /// Never return a secret credential binding to a browser or log it.
    ///
    /// # Errors
    /// Returns a generic storage error without exposing stored data.
    pub async fn identity(&self) -> Result<Option<String>, Response> {
        self.session.get(SUBJECT).await.map_err(store_failure)
    }

    /// Resolves the stored identity against current application authorization.
    ///
    /// # Errors
    /// Rejects missing, revoked or disabled identities and resolver failures.
    pub async fn authenticated(&self) -> Result<AuthenticatedSession, Response> {
        let identity = self
            .identity()
            .await?
            .ok_or_else(|| failure(AuthError::MissingCredentials))?;
        (self.policy.resolver)(identity).await.map_err(failure)
    }

    /// Bootstrap a synchronizer token. Return only to the trusted browser origin.
    ///
    /// # Errors
    /// Returns a generic 500 response when session storage fails.
    pub async fn csrf_token(&self) -> Result<String, Response> {
        if let Some(token) = self
            .session
            .get::<String>(CSRF)
            .await
            .map_err(store_failure)?
        {
            return Ok(token);
        }
        self.rotate_csrf().await
    }
    async fn rotate_csrf(&self) -> Result<String, Response> {
        use std::fmt::Write;
        let bytes: [u8; 32] = rand::random();
        let mut token = String::with_capacity(64);
        for byte in bytes {
            let _ = write!(token, "{byte:02x}");
        }
        self.session
            .insert(CSRF, &token)
            .await
            .map_err(store_failure)?;
        Ok(token)
    }
    /// Call only after verifying credentials. Rotates the session ID and CSRF
    /// token; returns the new token. Safe HTTP methods cannot establish a login.
    ///
    /// # Errors
    /// Rejects safe methods, rejected identities, and storage failures.
    pub async fn login(&self, subject: &str) -> Result<String, Response> {
        if !self.unsafe_method {
            return Err(failure(rejected()));
        }
        (self.policy.resolver)(subject.to_owned())
            .await
            .map_err(failure)?;
        self.session.cycle_id().await.map_err(store_failure)?;
        self.session
            .insert(SUBJECT, subject)
            .await
            .map_err(store_failure)?;
        self.rotate_csrf().await
    }
    /// Clears this browser session. Global revocation belongs in the resolver.
    ///
    /// # Errors
    /// Rejects safe methods and storage failures.
    pub async fn logout(&self) -> Result<(), Response> {
        if !self.unsafe_method {
            return Err(failure(rejected()));
        }
        self.session.flush().await.map_err(store_failure)
    }
}
fn store_failure(_: tower_sessions::session::Error) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "session storage unavailable",
    )
        .into_response()
}
pub(crate) async fn authenticate(
    parts: &Parts,
    name: &str,
) -> Result<AuthenticatedSession, AuthError> {
    let session = extract(parts).await?;
    if session.policy.name != name {
        return Err(AuthError::MissingCredentials);
    }
    let subject = session
        .session
        .get::<String>(SUBJECT)
        .await
        .map_err(|_| AuthError::InvalidCredentials)?
        .ok_or(AuthError::MissingCredentials)?;
    (session.policy.resolver)(subject).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForkLaunchRouter, RouterConfig};
    use axum::{
        body::{Body, to_bytes},
        http::Request,
        routing::{get, post},
    };
    use forklaunch_core::auth::Principal;
    use forklaunch_core::contract::{AuthConfig, RouteContract};
    use std::sync::atomic::{AtomicBool, Ordering};
    use tower::ServiceExt;
    use tower_sessions::MemoryStore;

    fn app(disabled: Arc<AtomicBool>) -> Router {
        let policy = CookieSessions::new(
            "__Host-session",
            "https://example.test",
            MemoryStore::default(),
            move |subject| {
                let disabled = disabled.clone();
                async move {
                    if disabled.load(Ordering::SeqCst) {
                        Err(AuthError::InvalidCredentials)
                    } else {
                        Ok(AuthenticatedSession::new(
                            Principal::new(subject),
                            serde_json::json!({}),
                        ))
                    }
                }
            },
        )
        .unwrap();
        let router = ForkLaunchRouter::anonymous()
            .get(
                RouteContract::get("profile", "profile", "/profile")
                    .with_auth(AuthConfig::cookie("__Host-session"))
                    .with_response(200, None),
                || async { "ok" },
            )
            .get(
                RouteContract::get("jwt", "jwt", "/jwt")
                    .with_auth(AuthConfig::Bearer)
                    .with_response(200, None),
                || async { "ok" },
            )
            .with_config(RouterConfig {
                cookie_sessions: Some(policy.clone()),
                ..RouterConfig::default()
            });
        #[cfg(feature = "mcp")]
        let router = router
            .into_router_with_mcp(forklaunch_core::mcp::server::McpServerOptions::new(
                "cookie-test",
                "1",
            ))
            .unwrap();
        #[cfg(not(feature = "mcp"))]
        let router = router.into_router();
        // Login/bootstrap handlers also use the same session layer.
        router.merge(
            policy.wrap(
                Router::new()
                    .route(
                        "/csrf",
                        get(|session: CookieSession| async move { session.csrf_token().await }),
                    )
                    .route(
                        "/login",
                        post(|session: CookieSession| async move { session.login("user").await }),
                    )
                    .route(
                        "/logout",
                        post(|session: CookieSession| async move {
                            session.logout().await.map(|()| "ok")
                        }),
                    ),
            ),
        )
    }
    async fn request(
        app: &Router,
        method: &str,
        path: &str,
        cookie: &str,
        origin: &str,
        token: &str,
    ) -> Response {
        let mut request = Request::builder().method(method).uri(path);
        if !cookie.is_empty() {
            request = request.header("cookie", cookie);
        }
        if !origin.is_empty() {
            request = request.header("origin", origin);
        }
        if !token.is_empty() {
            request = request.header("x-csrf-token", token);
        }
        app.clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }
    fn cookie(response: &Response) -> String {
        let value = response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            value.contains("HttpOnly")
                && value.contains("Secure")
                && value.contains("SameSite=Strict")
        );
        value.split(';').next().unwrap().to_owned()
    }
    async fn body(response: Response) -> String {
        String::from_utf8(to_bytes(response.into_body(), 4096).await.unwrap().to_vec()).unwrap()
    }

    #[tokio::test]
    async fn rotation_csrf_logout_and_live_revocation() {
        let disabled = Arc::new(AtomicBool::new(false));
        let app = app(disabled.clone());
        assert_eq!(
            request(&app, "GET", "/profile", "", "", "").await.status(),
            StatusCode::UNAUTHORIZED
        );
        let bootstrap = request(&app, "GET", "/csrf", "", "", "").await;
        let before = cookie(&bootstrap);
        let token = body(bootstrap).await;
        for (origin, proof) in [
            ("", token.as_str()),
            ("https://evil.test", token.as_str()),
            ("https://example.test", "wrong"),
        ] {
            assert_eq!(
                request(&app, "POST", "/login", &before, origin, proof)
                    .await
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        let login = request(
            &app,
            "POST",
            "/login",
            &before,
            "https://example.test",
            &token,
        )
        .await;
        assert_eq!(login.status(), StatusCode::OK);
        let after = cookie(&login);
        let new_token = body(login).await;
        assert_ne!(before, after);
        assert_ne!(token, new_token);
        assert_eq!(
            request(&app, "GET", "/profile", &before, "", "")
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(&app, "GET", "/profile", &after, "", "")
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            request(&app, "GET", "/jwt", &after, "", "").await.status(),
            StatusCode::UNAUTHORIZED
        );
        disabled.store(true, Ordering::SeqCst);
        assert_eq!(
            request(&app, "GET", "/profile", &after, "", "")
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        disabled.store(false, Ordering::SeqCst);
        assert_eq!(
            request(
                &app,
                "POST",
                "/logout",
                &after,
                "https://example.test",
                &token
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        let logout = request(
            &app,
            "POST",
            "/logout",
            &after,
            "https://example.test",
            &new_token,
        )
        .await;
        assert_eq!(logout.status(), StatusCode::OK);
        assert!(
            logout
                .headers()
                .get("set-cookie")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("Max-Age=0")
        );
        assert_eq!(
            request(&app, "GET", "/profile", &after, "", "")
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    #[cfg(feature = "mcp")]
    #[tokio::test]
    async fn live_http_and_mcp_share_cookie_authentication() {
        let app = app(Arc::new(AtomicBool::new(false)));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = reqwest::Client::new();
        let bootstrap = client.get(format!("{url}/csrf")).send().await.unwrap();
        let before = bootstrap.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let token = bootstrap.text().await.unwrap();
        let login = client
            .post(format!("{url}/login"))
            .header("cookie", before)
            .header("origin", "https://example.test")
            .header("x-csrf-token", token)
            .send()
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::OK);
        let cookie = login.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        assert_eq!(
            client
                .get(format!("{url}/profile"))
                .header("cookie", &cookie)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        for (credential, success) in [(cookie.as_str(), true), ("__Host-session=invalid", false)] {
            let response: serde_json::Value = client.post(format!("{url}/mcp"))
                .header("accept", "application/json, text/event-stream")
                .header("mcp-protocol-version", "2025-11-25")
                .header("cookie", credential)
                .json(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"profile","arguments":{}}}))
                .send().await.unwrap().json().await.unwrap();
            assert_eq!(
                response["result"]["isError"].as_bool().unwrap_or(false),
                !success,
                "{response}"
            );
            assert!(response.get("error").is_none(), "{response}");
        }
        server.abort();
        let _ = server.await;
    }
    #[test]
    fn invalid_configuration_is_rejected() {
        for (name, origin) in [
            ("", "https://example.test"),
            ("bad;name", "https://example.test"),
            ("session", "http://example.test"),
            ("session", "https://example.test/path"),
            ("session", "https://user@example.test"),
            ("session", "null"),
        ] {
            assert!(
                CookieSessions::new(name, origin, MemoryStore::default(), |_| async {
                    Err(AuthError::InvalidCredentials)
                })
                .is_err()
            );
        }
    }
}

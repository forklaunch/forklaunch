//! Optional browser login backed by the ordinary persistent IAM session ledger.
//! Applications supply a trusted, durable cookie store and an exact HTTPS origin.
#[cfg(feature = "oidc-sessions")]
mod oidc;

use axum::{
    Json, Router,
    extract::rejection::JsonRejection,
    http::{HeaderMap, HeaderValue, StatusCode, header::CACHE_CONTROL},
    middleware::map_response,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use forklaunch_axum::cookie_session::{CookieSession, CookieSessions};
use forklaunch_iam_interface::{AsyncIamSessions, Credentials};
use std::sync::Arc;
use tower_sessions::SessionStore;

/// Browser-only routes and reusable cookie authentication for application routes.
/// The cookie store contains a server-side IAM bearer credential: protect access
/// to it like other authentication secrets, and use a durable shared store in
/// production. The browser receives only an opaque cookie ID and CSRF proof.
/// These routes do not expose credentials through responses, SDKs or MCP tools.
#[derive(Clone)]
pub struct BrowserIam {
    policy: CookieSessions,
    router: Router,
    #[cfg(feature = "oidc-sessions")]
    origin: String,
}

impl BrowserIam {
    /// Creates `/auth/browser/csrf`, `/login`, `/logout` and `/session` routes.
    /// Supply a dedicated store namespace per trusted IAM tenant/application.
    /// Cookies are `Secure`, `HttpOnly` and `SameSite=Strict`; TLS termination is owned
    /// by the application. Current session/user/grants are checked on each use.
    ///
    /// # Errors
    /// Rejects invalid cookie names and origins using the shared cookie policy.
    pub fn new<S: AsyncIamSessions + 'static, T: SessionStore + Clone>(
        service: Arc<S>,
        store: T,
        cookie_name: &str,
        origin: &str,
    ) -> Result<Self, &'static str> {
        let verifier = Arc::clone(&service);
        let policy = CookieSessions::new(cookie_name, origin, store, move |token| {
            let service = Arc::clone(&verifier);
            async move { super::authenticate_stored_session(service.as_ref(), &token).await }
        })?;
        let logout_service = Arc::clone(&service);
        let endpoints = Router::new()
            .route("/auth/browser/csrf", get(csrf))
            .route("/auth/browser/session", get(current))
            .route("/auth/browser/login", post(move |session: CookieSession, headers: HeaderMap, credentials: Result<Json<Credentials>, JsonRejection>| {
                login(Arc::clone(&service), session, headers, credentials)
            }))
            .route("/auth/browser/logout", post(move |session: CookieSession, headers: HeaderMap| {
                logout(Arc::clone(&logout_service), session, headers)
            }));
        let router = policy
            .wrap(endpoints)
            .layer(map_response(|mut response: Response| async {
                response
                    .headers_mut()
                    .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
                response
            }));
        Ok(Self {
            policy,
            router,
            #[cfg(feature = "oidc-sessions")]
            origin: origin.to_owned(),
        })
    }

    /// Set this policy in `RouterConfig::cookie_sessions` for application-owned
    /// contract routes using the same `AuthConfig::Cookie` name.
    #[must_use]
    pub fn cookie_sessions(&self) -> CookieSessions {
        self.policy.clone()
    }

    /// Reserves the browser HTTP namespace without exporting login or CSRF tools.
    /// Shared HTTP limits, CORS and telemetry continue to apply.
    ///
    /// # Panics
    /// Router construction rejects conflicting application routes or MCP paths.
    #[must_use]
    pub fn mount(
        self,
        router: forklaunch_axum::ForkLaunchRouter,
    ) -> forklaunch_axum::ForkLaunchRouter {
        router.merge_internal(self.router, ["/auth/browser"])
    }

    /// Mount the browser endpoints alongside the application's HTTP router.
    pub fn into_router(self) -> Router {
        self.router
    }
}

async fn csrf(session: CookieSession) -> Response {
    match session.csrf_token().await {
        Ok(token) => Json(serde_json::json!({"csrfToken": token})).into_response(),
        Err(error) => error,
    }
}

async fn current(session: CookieSession) -> Response {
    match session.authenticated().await {
        Ok(value) => Json(value.claims).into_response(),
        Err(error) => error,
    }
}

async fn login<S: AsyncIamSessions>(
    service: Arc<S>,
    session: CookieSession,
    headers: HeaderMap,
    credentials: Result<Json<Credentials>, JsonRejection>,
) -> Response {
    let Ok(Json(credentials)) = credentials else {
        return super::invalid_input(&headers);
    };
    let previous = match session.identity().await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let issued = match service.authenticate(credentials).await {
        Ok(value) => value,
        Err(error) if error.code == "iam_invalid_credentials" => {
            return super::response_error(
                super::ErrorKind::Unauthorized,
                "invalid credentials",
                &headers,
            );
        }
        Err(error) => return super::backend_error(&error, &headers),
    };
    // Replace the previous credential before establishing the new browser login.
    // On any failure, the newly issued credential is never returned to the client.
    if let Some(previous) = previous
        && let Err(error) = service.revoke_session(&previous).await
    {
        let _ = service.revoke_session(&issued.id).await;
        return super::backend_error(&error, &headers);
    }
    match session.login(&issued.id).await {
        Ok(token) => Json(serde_json::json!({"csrfToken": token})).into_response(),
        Err(error) => {
            let _ = service.revoke_session(&issued.id).await;
            error
        }
    }
}

async fn logout<S: AsyncIamSessions>(
    service: Arc<S>,
    session: CookieSession,
    headers: HeaderMap,
) -> Response {
    match session.identity().await {
        Ok(Some(token)) => {
            if let Err(error) = service.revoke_session(&token).await {
                return super::backend_error(&error, &headers);
            }
        }
        Ok(None) => {}
        Err(error) => return error,
    }
    match session.logout().await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error,
    }
}

use super::BrowserIam;
use crate::{browser_sessions::PostgresCookieStore, postgres::PostgresIamService};
use axum::{
    Json, Router,
    extract::rejection::JsonRejection,
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::map_response,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use forklaunch_axum::cookie_session::CookieSession;
use forklaunch_core::auth::oidc::OidcProvider;
use serde::Deserialize;
use std::sync::Arc;

const CALLBACK: &str = "/auth/browser/oidc/callback";
const RELAY: &str = include_str!("oidc_relay.html");
const CSP: &str = include_str!("oidc_relay.csp");

impl BrowserIam {
    /// Add a single operator-configured provider. Start is a CSRF-protected POST
    /// returning its authorization URL. The fixed callback serves a static relay
    /// that removes URL secrets and performs a same-origin CSRF-protected POST.
    /// Pending state is encrypted, cookie-bound and atomically consumed in the
    /// supplied `PostgreSQL` store, which MUST be the store passed to `new`.
    /// Existing local users must have explicit issuer/subject bindings.
    /// # Errors
    /// Rejects a callback URI outside this exact browser origin/fixed callback.
    pub fn with_oidc(
        mut self,
        service: Arc<PostgresIamService>,
        store: PostgresCookieStore,
        provider: Arc<OidcProvider>,
        scopes: Vec<String>,
    ) -> Result<Self, &'static str> {
        if provider.redirect_uri() != format!("{}{CALLBACK}", self.origin) {
            return Err("OIDC callback must match the browser origin and fixed callback path");
        }
        provider.begin(&scopes).map_err(|_| "invalid OIDC scopes")?;
        let start_provider = Arc::clone(&provider);
        let start_store = store.clone();
        let endpoints = Router::new()
            .route(
                "/auth/browser/oidc/start",
                post(move |session: CookieSession| {
                    start(
                        session,
                        start_store.clone(),
                        Arc::clone(&start_provider),
                        scopes.clone(),
                    )
                }),
            )
            .route(
                CALLBACK,
                get(relay).post(
                    move |session: CookieSession,
                          headers: HeaderMap,
                          body: Result<Json<Callback>, JsonRejection>| {
                        callback(
                            session,
                            headers,
                            body,
                            Arc::clone(&service),
                            store.clone(),
                            Arc::clone(&provider),
                        )
                    },
                ),
            );
        self.router = self
            .router
            .merge(self.policy.wrap(endpoints).layer(map_response(
                |mut response: Response| async {
                    response
                        .headers_mut()
                        .insert("cache-control", HeaderValue::from_static("no-store"));
                    response
                        .headers_mut()
                        .insert("referrer-policy", HeaderValue::from_static("no-referrer"));
                    response
                },
            )));
        Ok(self)
    }
}
async fn relay() -> Response {
    let mut response = Html(RELAY).into_response();
    response
        .headers_mut()
        .insert("content-security-policy", HeaderValue::from_static(CSP));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}
fn denied() -> Response {
    (StatusCode::UNAUTHORIZED, "OIDC authentication failed").into_response()
}
async fn start(
    session: CookieSession,
    store: PostgresCookieStore,
    provider: Arc<OidcProvider>,
    scopes: Vec<String>,
) -> Response {
    let Ok(binding) = session.browser_binding() else {
        return denied();
    };
    let Ok(pending) = provider.begin(&scopes) else {
        return denied();
    };
    let authorization_url = pending.authorization_url().to_string();
    if store.save_oidc_pending(&binding, pending).await.is_err() {
        return denied();
    }
    Json(serde_json::json!({"authorizationUrl":authorization_url})).into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Callback {
    code: String,
    state: String,
}
async fn callback(
    session: CookieSession,
    headers: HeaderMap,
    body: Result<Json<Callback>, JsonRejection>,
    service: Arc<PostgresIamService>,
    store: PostgresCookieStore,
    provider: Arc<OidcProvider>,
) -> Response {
    let Ok(Json(body)) = body else {
        return denied();
    };
    if body.code.is_empty()
        || body.code.len() > 16 * 1024
        || body.state.is_empty()
        || body.state.len() > 1024
    {
        return denied();
    }
    let Ok(binding) = session.browser_binding() else {
        return denied();
    };
    let previous = match session.identity().await {
        Ok(value) => value,
        Err(error) => return error,
    };
    let Ok(pending) = store.consume_oidc_pending(&binding, &body.state).await else {
        return denied();
    };
    let Ok(issued) = service
        .authenticate_oidc(&provider, pending, &body.state, &body.code)
        .await
    else {
        return denied();
    };
    if let Some(previous) = previous
        && let Err(error) = service.revoke_session(&previous).await
    {
        let _ = service.revoke_session(&issued.id).await;
        return super::super::backend_error(&error, &headers);
    }
    match session.login(&issued.id).await {
        Ok(token) => Json(serde_json::json!({"csrfToken":token})).into_response(),
        Err(error) => {
            let _ = service.revoke_session(&issued.id).await;
            error
        }
    }
}

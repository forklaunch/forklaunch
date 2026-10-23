//! Credential login, opaque session and session-scoped lookup. Errors use the
//! framework's JSON error body with the request's correlation ID.
use crate::registrations::{AppState, Iam};
use axum::{
    Extension, Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
    routing::any,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_core::{auth::parse_bearer, http::ErrorKind};
use forklaunch_iam_base::http::{
    Handler, PageQuery, is_admin_reader, list_page, may_read_user, session_backend_error, session_error,
};
use forklaunch_iam_interface::Credentials;

/// Exchanges credentials for a session; the response is never cached.
pub fn authenticate(state: &AppState) -> Handler {
    any(login).with_state(state.iam.clone())
}

async fn login(
    State(iam): State<Iam>,
    headers: HeaderMap,
    credentials: Result<Json<Credentials>, JsonRejection>,
) -> Response {
    let Ok(Json(credentials)) = credentials else {
        return session_error(ErrorKind::Validation, "invalid request", &headers);
    };
    match iam.sessions().authenticate(credentials).await {
        Ok(session) => {
            let mut response = Json(session).into_response();
            response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) if error.code == "iam_invalid_credentials" => {
            session_error(ErrorKind::Unauthorized, "invalid credentials", &headers)
        }
        Err(error) => session_backend_error(&error, &headers),
    }
}

/// Revokes the bearer session.
pub fn logout(state: &AppState) -> Handler {
    any(revoke).with_state(state.iam.clone())
}

async fn revoke(State(iam): State<Iam>, headers: HeaderMap) -> Response {
    let Some(token) = headers.get("authorization").and_then(|value| value.to_str().ok()).and_then(parse_bearer) else {
        return session_error(ErrorKind::Unauthorized, "missing credentials", &headers);
    };
    match iam.sessions().revoke_session(token).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => session_backend_error(&error, &headers),
    }
}

/// Reads a session user; reading anyone but yourself needs `platform:read`.
pub fn get_session_user(state: &AppState) -> Handler {
    any(user).with_state(state.iam.clone())
}

async fn user(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !may_read_user(&session, &id) {
        return session_error(ErrorKind::Forbidden, "forbidden", &headers);
    }
    match iam.sessions().get_user(&id).await {
        Ok(Some(user)) => Json(user).into_response(),
        Ok(None) => session_error(ErrorKind::NotFound, "user not found", &headers),
        Err(error) => session_backend_error(&error, &headers),
    }
}

/// Pages session users; needs `platform:read`.
pub fn list_users(state: &AppState) -> Handler {
    any(users).with_state(state.iam.clone())
}

async fn users(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Response {
    if !is_admin_reader(&session) {
        return session_error(ErrorKind::Forbidden, "forbidden", &headers);
    }
    let Some(page) = list_page(query) else {
        return session_error(ErrorKind::Validation, "invalid request", &headers);
    };
    match iam.sessions().list_users(page).await {
        Ok(page) => Json(page).into_response(),
        Err(error) => session_backend_error(&error, &headers),
    }
}

/// Lists a session user's permissions; reading anyone but yourself needs `platform:read`.
pub fn list_user_permissions(state: &AppState) -> Handler {
    any(permissions).with_state(state.iam.clone())
}

async fn permissions(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !may_read_user(&session, &id) {
        return session_error(ErrorKind::Forbidden, "forbidden", &headers);
    }
    match iam.sessions().permissions_for_user(&id).await {
        Ok(permissions) => Json(permissions).into_response(),
        Err(error) => session_backend_error(&error, &headers),
    }
}

/// Pages organizations; needs `platform:read`.
pub fn list_organizations(state: &AppState) -> Handler {
    any(organizations).with_state(state.iam.clone())
}

async fn organizations(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
    query: Result<Query<PageQuery>, QueryRejection>,
) -> Response {
    if !is_admin_reader(&session) {
        return session_error(ErrorKind::Forbidden, "forbidden", &headers);
    }
    let Some(page) = list_page(query) else {
        return session_error(ErrorKind::Validation, "invalid request", &headers);
    };
    match iam.sessions().list_organizations(page).await {
        Ok(page) => Json(page).into_response(),
        Err(error) => session_backend_error(&error, &headers),
    }
}

/// Reports the configured sign-in methods; none are advertised.
pub fn get_auth_methods(_state: &AppState) -> Handler {
    any(auth_methods)
}

async fn auth_methods() -> StatusCode {
    StatusCode::NO_CONTENT
}

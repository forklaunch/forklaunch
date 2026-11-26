//! Credential login, opaque session and session-scoped lookup routes; the IAM
//! interface declares each operation's contract.
use crate::{api::controllers::session, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("authenticate", session::authenticate(state))
        .route("logout", session::logout(state))
        .route("getSessionUser", session::get_session_user(state))
        .route("listUsers", session::list_users(state))
        .route("listUserPermissions", session::list_user_permissions(state))
        .route("listOrganizations", session::list_organizations(state))
        .route("getauthmethods", session::get_auth_methods(state))
}

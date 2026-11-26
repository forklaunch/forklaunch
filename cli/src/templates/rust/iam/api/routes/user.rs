//! Directory users and their surfaced grants routes; the IAM
//! interface declares each operation's contract.
use crate::{api::controllers::user, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createUser", user::create_user(state))
        .route("getUser", user::get_user(state))
        .route("updateUser", user::update_user(state))
        .route("deleteUser", user::delete_user(state))
        .route("createBatchUsers", user::create_batch_users(state))
        .route("updateBatchUsers", user::update_batch_users(state))
        .route("deleteBatchUsers", user::delete_batch_users(state))
        .route("getBatchUsers", user::get_batch_users(state))
        .route("surfaceUserRoles", user::surface_user_roles(state))
        .route("surfaceUserPermissions", user::surface_user_permissions(state))
}

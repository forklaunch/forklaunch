//! Directory roles routes; the IAM
//! interface declares each operation's contract.
use crate::{api::controllers::role, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createRole", role::create_role(state))
        .route("getRole", role::get_role(state))
        .route("updateRole", role::update_role(state))
        .route("deleteRole", role::delete_role(state))
        .route("createBatchRoles", role::create_batch_roles(state))
        .route("updateBatchRoles", role::update_batch_roles(state))
        .route("deleteBatchRoles", role::delete_batch_roles(state))
        .route("getBatchRoles", role::get_batch_roles(state))
}

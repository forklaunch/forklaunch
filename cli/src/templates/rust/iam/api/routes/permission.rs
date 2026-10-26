//! Directory permissions routes; the IAM
//! interface declares each operation's contract.
use crate::{api::controllers::permission, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createPermission", permission::create_permission(state))
        .route("getPermission", permission::get_permission(state))
        .route("updatePermission", permission::update_permission(state))
        .route("deletePermission", permission::delete_permission(state))
        .route("createBatchPermissions", permission::create_batch_permissions(state))
        .route("updateBatchPermissions", permission::update_batch_permissions(state))
        .route("deleteBatchPermissions", permission::delete_batch_permissions(state))
        .route("getBatchPermissions", permission::get_batch_permissions(state))
}

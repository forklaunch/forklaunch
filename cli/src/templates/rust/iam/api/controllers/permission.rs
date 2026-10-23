//! Directory permissions. Each handler calls the audited, tenant-bound
//! directory service as the authenticated caller.
use crate::registrations::{AppState, Iam};
use axum::{
    Extension, Json,
    extract::{Path, RawQuery, State},
    http::StatusCode,
    routing::any,
};
use forklaunch_axum::{RequestContext, session::AuthenticatedSession};
use forklaunch_iam_base::http::{
    Handler,
    directory::{DirectoryFailure, batch_ids, decode},
};
use forklaunch_iam_interface::services::{PermissionDto, PermissionService};
use serde_json::Value;

/// Creates a permission.
pub fn create_permission(state: &AppState) -> Handler {
    any(create).with_state(state.iam.clone())
}

async fn create(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.create_permission(decode(input)?).await?;
    Ok((StatusCode::CREATED, "Permission created successfully"))
}

/// Reads a permission.
pub fn get_permission(state: &AppState) -> Handler {
    any(get).with_state(state.iam.clone())
}

async fn get(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<PermissionDto>, DirectoryFailure> {
    Ok(Json(iam.directory(&session, &context)?.get_permission(&id).await?))
}

/// Updates a permission.
pub fn update_permission(state: &AppState) -> Handler {
    any(update).with_state(state.iam.clone())
}

async fn update(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.update_permission(decode(input)?).await?;
    Ok((StatusCode::OK, "Permission updated successfully"))
}

/// Deletes a permission.
pub fn delete_permission(state: &AppState) -> Handler {
    any(delete).with_state(state.iam.clone())
}

async fn delete(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.delete_permission(&id).await?;
    Ok((StatusCode::OK, "Permission deleted successfully"))
}

/// Creates several permissions.
pub fn create_batch_permissions(state: &AppState) -> Handler {
    any(create_batch).with_state(state.iam.clone())
}

async fn create_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.create_batch_permissions(decode(input)?).await?;
    Ok((StatusCode::CREATED, "Batch permissions created successfully"))
}

/// Updates several permissions.
pub fn update_batch_permissions(state: &AppState) -> Handler {
    any(update_batch).with_state(state.iam.clone())
}

async fn update_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.update_batch_permissions(decode(input)?).await?;
    Ok((StatusCode::OK, "Batch permissions updated successfully"))
}

/// Deletes the permissions in the `ids` query.
pub fn delete_batch_permissions(state: &AppState) -> Handler {
    any(delete_batch).with_state(state.iam.clone())
}

async fn delete_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    RawQuery(query): RawQuery,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    let ids = batch_ids(query.as_deref())?;
    iam.directory(&session, &context)?.delete_batch_permissions(&ids).await?;
    Ok((StatusCode::OK, "Batch permissions deleted successfully"))
}

/// Reads the permissions in the `ids` query.
pub fn get_batch_permissions(state: &AppState) -> Handler {
    any(get_batch).with_state(state.iam.clone())
}

async fn get_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    RawQuery(query): RawQuery,
) -> Result<Json<Vec<PermissionDto>>, DirectoryFailure> {
    let ids = batch_ids(query.as_deref())?;
    Ok(Json(iam.directory(&session, &context)?.get_batch_permissions(&ids).await?))
}

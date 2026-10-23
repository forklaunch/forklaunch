//! Directory roles. Each handler calls the audited, tenant-bound directory
//! service as the authenticated caller.
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
use forklaunch_iam_interface::services::{RoleDto, RoleService};
use serde_json::Value;

/// Creates a role.
pub fn create_role(state: &AppState) -> Handler {
    any(create).with_state(state.iam.clone())
}

async fn create(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.create_role(decode(input)?).await?;
    Ok((StatusCode::CREATED, "Role created successfully"))
}

/// Reads a role.
pub fn get_role(state: &AppState) -> Handler {
    any(get).with_state(state.iam.clone())
}

async fn get(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<RoleDto>, DirectoryFailure> {
    Ok(Json(iam.directory(&session, &context)?.get_role(&id).await?))
}

/// Updates a role.
pub fn update_role(state: &AppState) -> Handler {
    any(update).with_state(state.iam.clone())
}

async fn update(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.update_role(decode(input)?).await?;
    Ok((StatusCode::OK, "Role updated successfully"))
}

/// Deletes a role.
pub fn delete_role(state: &AppState) -> Handler {
    any(delete).with_state(state.iam.clone())
}

async fn delete(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.delete_role(&id).await?;
    Ok((StatusCode::OK, "Role deleted successfully"))
}

/// Creates several roles.
pub fn create_batch_roles(state: &AppState) -> Handler {
    any(create_batch).with_state(state.iam.clone())
}

async fn create_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.create_batch_roles(decode(input)?).await?;
    Ok((StatusCode::CREATED, "Batch roles created successfully"))
}

/// Updates several roles.
pub fn update_batch_roles(state: &AppState) -> Handler {
    any(update_batch).with_state(state.iam.clone())
}

async fn update_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.update_batch_roles(decode(input)?).await?;
    Ok((StatusCode::OK, "Batch roles updated successfully"))
}

/// Deletes the roles in the `ids` query.
pub fn delete_batch_roles(state: &AppState) -> Handler {
    any(delete_batch).with_state(state.iam.clone())
}

async fn delete_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    RawQuery(query): RawQuery,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    let ids = batch_ids(query.as_deref())?;
    iam.directory(&session, &context)?.delete_batch_roles(&ids).await?;
    Ok((StatusCode::OK, "Batch roles deleted successfully"))
}

/// Reads the roles in the `ids` query.
pub fn get_batch_roles(state: &AppState) -> Handler {
    any(get_batch).with_state(state.iam.clone())
}

async fn get_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    RawQuery(query): RawQuery,
) -> Result<Json<Vec<RoleDto>>, DirectoryFailure> {
    let ids = batch_ids(query.as_deref())?;
    Ok(Json(iam.directory(&session, &context)?.get_batch_roles(&ids).await?))
}

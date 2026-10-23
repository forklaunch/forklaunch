//! Directory users and their surfaced grants. Each handler calls the audited,
//! tenant-bound directory service as the authenticated caller; the
//! JWT-protected routes are scoped to the caller's organization.
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
use forklaunch_iam_interface::services::{PermissionDto, RoleDto, UserDto, UserService};
use serde_json::Value;

/// Creates a user.
pub fn create_user(state: &AppState) -> Handler {
    any(create).with_state(state.iam.clone())
}

async fn create(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.create_user(decode(input)?).await?;
    Ok((StatusCode::CREATED, "User created successfully"))
}

/// Reads a user.
pub fn get_user(state: &AppState) -> Handler {
    any(get).with_state(state.iam.clone())
}

async fn get(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<UserDto>, DirectoryFailure> {
    Ok(Json(iam.user_directory(&session, &context).await?.get_user(&id).await?))
}

/// Updates a user.
pub fn update_user(state: &AppState) -> Handler {
    any(update).with_state(state.iam.clone())
}

async fn update(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.user_directory(&session, &context).await?.update_user(decode(input)?).await?;
    Ok((StatusCode::OK, "User updated successfully"))
}

/// Deletes a user.
pub fn delete_user(state: &AppState) -> Handler {
    any(delete).with_state(state.iam.clone())
}

async fn delete(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.user_directory(&session, &context).await?.delete_user(&id, None).await?;
    Ok((StatusCode::OK, "User deleted successfully"))
}

/// Creates several users.
pub fn create_batch_users(state: &AppState) -> Handler {
    any(create_batch).with_state(state.iam.clone())
}

async fn create_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.create_batch_users(decode(input)?).await?;
    Ok((StatusCode::CREATED, "Batch users created successfully"))
}

/// Updates several users.
pub fn update_batch_users(state: &AppState) -> Handler {
    any(update_batch).with_state(state.iam.clone())
}

async fn update_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.user_directory(&session, &context).await?.update_batch_users(decode(input)?).await?;
    Ok((StatusCode::OK, "Batch users updated successfully"))
}

/// Deletes the users in the `ids` query.
pub fn delete_batch_users(state: &AppState) -> Handler {
    any(delete_batch).with_state(state.iam.clone())
}

async fn delete_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    RawQuery(query): RawQuery,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    let ids = batch_ids(query.as_deref())?;
    iam.user_directory(&session, &context).await?.delete_batch_users(&ids, None).await?;
    Ok((StatusCode::OK, "Batch users deleted successfully"))
}

/// Reads the users in the `ids` query.
pub fn get_batch_users(state: &AppState) -> Handler {
    any(get_batch).with_state(state.iam.clone())
}

async fn get_batch(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    RawQuery(query): RawQuery,
) -> Result<Json<Vec<UserDto>>, DirectoryFailure> {
    let ids = batch_ids(query.as_deref())?;
    Ok(Json(iam.user_directory(&session, &context).await?.get_batch_users(&ids, None).await?))
}

/// Lists a user's roles for other services.
pub fn surface_user_roles(state: &AppState) -> Handler {
    any(surface_roles).with_state(state.iam.clone())
}

async fn surface_roles(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<RoleDto>>, DirectoryFailure> {
    Ok(Json(iam.directory(&session, &context)?.surface_roles(&id).await?))
}

/// Lists a user's permissions for other services.
pub fn surface_user_permissions(state: &AppState) -> Handler {
    any(surface_permissions).with_state(state.iam.clone())
}

async fn surface_permissions(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<PermissionDto>>, DirectoryFailure> {
    Ok(Json(iam.directory(&session, &context)?.surface_permissions(&id).await?))
}

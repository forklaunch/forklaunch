//! Directory organizations. Each handler calls the audited, tenant-bound
//! directory service as the authenticated caller.
use crate::registrations::{AppState, Iam};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    routing::any,
};
use forklaunch_axum::{RequestContext, session::AuthenticatedSession};
use forklaunch_iam_base::http::{
    Handler,
    directory::{DirectoryFailure, decode},
};
use forklaunch_iam_interface::services::{OrganizationDto, OrganizationService};
use serde_json::Value;

/// Creates an organization.
pub fn create_organization(state: &AppState) -> Handler {
    any(create).with_state(state.iam.clone())
}

async fn create(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, Json<OrganizationDto>), DirectoryFailure> {
    Ok((StatusCode::CREATED, Json(iam.directory(&session, &context)?.create_organization(decode(input)?).await?)))
}

/// Reads an organization.
pub fn get_organization(state: &AppState) -> Handler {
    any(get).with_state(state.iam.clone())
}

async fn get(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<Json<OrganizationDto>, DirectoryFailure> {
    Ok(Json(iam.directory(&session, &context)?.get_organization(&id).await?))
}

/// Updates an organization.
pub fn update_organization(state: &AppState) -> Handler {
    any(update).with_state(state.iam.clone())
}

async fn update(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Json(input): Json<Value>,
) -> Result<(StatusCode, Json<OrganizationDto>), DirectoryFailure> {
    Ok((StatusCode::OK, Json(iam.directory(&session, &context)?.update_organization(decode(input)?).await?)))
}

/// Deletes an organization.
pub fn delete_organization(state: &AppState) -> Handler {
    any(delete).with_state(state.iam.clone())
}

async fn delete(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Result<(StatusCode, &'static str), DirectoryFailure> {
    iam.directory(&session, &context)?.delete_organization(&id).await?;
    Ok((StatusCode::OK, "Organization deleted successfully"))
}

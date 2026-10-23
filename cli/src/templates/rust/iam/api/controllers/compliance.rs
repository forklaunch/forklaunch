//! Subject data export and erasure under the configured compliance
//! classifications. Incomplete storage operations are errors, never a
//! successful erasure.
use crate::registrations::{AppState, Iam};
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::any,
};
use forklaunch_axum::{RequestContext, session::AuthenticatedSession};
use forklaunch_core::domain::BlueprintError;
use forklaunch_iam_base::http::{Handler, directory::DirectoryFailure};

/// Exports a subject's classified directory data.
pub fn export_user_data(state: &AppState) -> Handler {
    any(export).with_state(state.iam.clone())
}

async fn export(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(subject): Path<String>,
) -> Result<Response, DirectoryFailure> {
    let result = iam.compliance(&session, &context)?.export(&subject).await?;
    if !result.failures.is_empty() {
        return Err(incomplete());
    }
    if result.entities.is_empty() {
        return Ok((StatusCode::NOT_FOUND, "User not found or no PII data to export").into_response());
    }
    Ok(Json(result).into_response())
}

/// Erases a subject's classified directory data.
pub fn erase_user_data(state: &AppState) -> Handler {
    any(erase).with_state(state.iam.clone())
}

async fn erase(
    State(iam): State<Iam>,
    Extension(session): Extension<AuthenticatedSession>,
    Extension(context): Extension<RequestContext>,
    Path(subject): Path<String>,
) -> Result<Response, DirectoryFailure> {
    let result = iam.compliance(&session, &context)?.erase(&subject).await?;
    if !result.failures.is_empty() {
        return Err(incomplete());
    }
    if result.records_deleted == 0 {
        return Ok((StatusCode::NOT_FOUND, "User not found or no PII data to erase").into_response());
    }
    Ok(Json(result).into_response())
}

fn incomplete() -> DirectoryFailure {
    BlueprintError::new("iam_directory_storage_error", "IAM directory compliance operation incomplete").into()
}

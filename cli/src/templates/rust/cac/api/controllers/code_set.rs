//! Code-set description and procedure-code lookup over the configured provider.
use crate::registrations::{AppState, Cac};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::any,
};
use forklaunch_cac_base::http::Handler;
use forklaunch_cac_interface::{CodeSetDescriptorDto, CodeSetProvider, ProcedureCodeLookupDto};

/// Reports the provider's code-set descriptor.
pub fn describe_code_set(state: &AppState) -> Handler {
    any(describe).with_state(state.cac.clone())
}

async fn describe(State(cac): State<Cac>) -> Json<CodeSetDescriptorDto> {
    Json(cac.provider().describe())
}

/// Looks up one procedure code; an unknown code is a text 404.
pub fn lookup_procedure_code(state: &AppState) -> Handler {
    any(lookup).with_state(state.cac.clone())
}

async fn lookup(State(cac): State<Cac>, Path(code): Path<String>) -> Response {
    let request = ProcedureCodeLookupDto { code: code.clone() };
    match cac.provider().lookup_procedure_code(request).await {
        Ok(Some(result)) => Json(result).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, format!("Procedure code '{code}' not found")).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Code-set provider unavailable").into_response(),
    }
}

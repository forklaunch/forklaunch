//! Subject data export and erasure. Billing records carry no classified
//! subject data yet, so authorized requests report that nothing was found.
use crate::registrations::AppState;
use axum::{http::StatusCode, routing::any};
use forklaunch_billing_stripe::http::Handler;

/// Exports a subject's classified data.
pub fn export_user_data(_state: &AppState) -> Handler {
    any(export)
}

async fn export() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "User not found or no PII data to export")
}

/// Erases a subject's classified data.
pub fn erase_user_data(_state: &AppState) -> Handler {
    any(erase)
}

async fn erase() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "User not found or no PII data to erase")
}

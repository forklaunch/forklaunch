//! Health check.
use crate::registrations::AppState;
use axum::{Json, routing::any};
use forklaunch_iam_base::http::Handler;
use serde_json::{Value, json};

/// Reports that the service is up.
pub fn get_health(_state: &AppState) -> Handler {
    any(health)
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok"}))
}

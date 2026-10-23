//! Catalog imports by external ID.
use crate::registrations::AppState;
use axum::{Extension, Json, extract::State, routing::any};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_ecommerce_interface::import::{CatalogImport, CatalogImportResult};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Failure, Handler, JsonBody};

/// Imports products and variants by external ID and seeds missing inventory.
pub fn import_catalog(state: &AppState) -> Handler {
    any(import).with_state(state.ecommerce.clone())
}

async fn import(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<CatalogImport>,
) -> Result<Json<CatalogImportResult>, Failure> {
    Ok(Json(ecommerce.service(&session)?.import_catalog(input).await?))
}

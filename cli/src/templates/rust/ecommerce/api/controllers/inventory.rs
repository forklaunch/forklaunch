//! Inventory and stock.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, State},
    routing::any,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_ecommerce_interface::{
    AdjustStockDto, CreateInventoryDto, InventoryDto, InventoryService, StockCheckDto, StockCheckResultDto,
};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Failure, Handler, JsonBody};

/// Creates a variant's inventory record.
pub fn create_inventory(state: &AppState) -> Handler {
    any(create).with_state(state.ecommerce.clone())
}

async fn create(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<CreateInventoryDto>,
) -> Result<Json<InventoryDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.create_inventory(input).await?))
}

/// Reads a variant's inventory.
pub fn get_inventory(state: &AppState) -> Handler {
    any(get).with_state(state.ecommerce.clone())
}

async fn get(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(variant_id): Path<String>,
) -> Result<Json<InventoryDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.get_inventory(&variant_id).await?))
}

/// Adds to or removes from a variant's stock.
pub fn adjust_stock(state: &AppState) -> Handler {
    any(adjust).with_state(state.ecommerce.clone())
}

async fn adjust(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<AdjustStockDto>,
) -> Result<Json<InventoryDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.adjust_stock(input).await?))
}

/// Reports whether a quantity of a variant is in stock.
pub fn check_stock(state: &AppState) -> Handler {
    any(check).with_state(state.ecommerce.clone())
}

async fn check(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<StockCheckDto>,
) -> Result<Json<StockCheckResultDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.check_stock(input).await?))
}

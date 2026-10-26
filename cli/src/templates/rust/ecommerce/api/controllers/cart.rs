//! Carts. Each handler calls the tenant-bound ecommerce service as the
//! authenticated caller; errors map through `Failure`.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, State},
    routing::any,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_ecommerce_interface::{AddCartItemDto, CartDto, CartService, CreateCartDto, RemoveCartItemDto};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Failure, Handler, JsonBody};

/// Creates a cart.
pub fn create_cart(state: &AppState) -> Handler {
    any(create).with_state(state.ecommerce.clone())
}

async fn create(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<CreateCartDto>,
) -> Result<Json<CartDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.create_cart(input).await?))
}

/// Reads a cart with its items.
pub fn get_cart(state: &AppState) -> Handler {
    any(get).with_state(state.ecommerce.clone())
}

async fn get(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Result<Json<CartDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.get_cart(&id).await?))
}

/// Adds a variant to a cart; stock is checked at checkout, not here.
pub fn add_cart_item(state: &AppState) -> Handler {
    any(add_item).with_state(state.ecommerce.clone())
}

async fn add_item(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<AddCartItemDto>,
) -> Result<Json<CartDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.add_item(input).await?))
}

/// Removes a variant from a cart.
pub fn remove_cart_item(state: &AppState) -> Handler {
    any(remove_item).with_state(state.ecommerce.clone())
}

async fn remove_item(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path((cart_id, variant_id)): Path<(String, String)>,
) -> Result<Json<CartDto>, Failure> {
    let request = RemoveCartItemDto { cart_id, variant_id };
    Ok(Json(ecommerce.service(&session)?.remove_item(request).await?))
}

/// Empties a cart.
pub fn clear_cart(state: &AppState) -> Handler {
    any(clear).with_state(state.ecommerce.clone())
}

async fn clear(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Result<&'static str, Failure> {
    ecommerce.service(&session)?.clear_cart(&id).await?;
    Ok("Cleared cart")
}

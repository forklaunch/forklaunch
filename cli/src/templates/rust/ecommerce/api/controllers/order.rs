//! Orders and their transitions.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, RawQuery, State},
    routing::any,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_ecommerce_interface::{CreateOrderDto, OrderDto, OrderService, module::OrderTransition};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Failure, Handler, JsonBody, query};

/// Creates an order.
pub fn create_order(state: &AppState) -> Handler {
    any(create).with_state(state.ecommerce.clone())
}

async fn create(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<CreateOrderDto>,
) -> Result<Json<OrderDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.create_order(input).await?))
}

/// Lists orders, optionally by a JSON `ids` array.
pub fn list_orders(state: &AppState) -> Handler {
    any(list).with_state(state.ecommerce.clone())
}

async fn list(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    RawQuery(raw): RawQuery,
) -> Result<Json<Vec<OrderDto>>, Failure> {
    let ids = query::ids(raw.as_deref())?;
    Ok(Json(ecommerce.service(&session)?.list_orders(ids).await?))
}

/// Reads an order.
pub fn get_order(state: &AppState) -> Handler {
    any(get).with_state(state.ecommerce.clone())
}

async fn get(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Result<Json<OrderDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.get_order(&id).await?))
}

/// Moves an order to a new status and enqueues its inventory event.
pub fn transition_order(state: &AppState) -> Handler {
    any(transition).with_state(state.ecommerce.clone())
}

async fn transition(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    JsonBody(input): JsonBody<OrderTransition>,
) -> Result<Json<OrderDto>, Failure> {
    Ok(Json(ecommerce.transition_order(&session, id, input).await?))
}

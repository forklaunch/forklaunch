//! Checkout with tax and shipping. Prices come from the cart's variants, never
//! from the request.
use crate::registrations::AppState;
use axum::{Extension, Json, extract::State, routing::any};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_ecommerce_interface::module::{Checkout, CheckoutResult};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Failure, Handler, JsonBody};

/// Prices the cart, creates or resumes its order and starts the payment.
pub fn checkout(state: &AppState) -> Handler {
    any(check_out).with_state(state.ecommerce.clone())
}

async fn check_out(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<Checkout>,
) -> Result<Json<CheckoutResult>, Failure> {
    Ok(Json(ecommerce.checkout(&session, input).await?))
}

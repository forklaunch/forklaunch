//! Provider payments.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, State},
    routing::any,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_ecommerce_interface::{
    PaymentDto, PaymentService,
    module::{CreateProviderPayment, CreatedPayment},
};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Failure, Handler, JsonBody};

/// Creates a Stripe or `PayPal` payment for an order.
pub fn create_payment(state: &AppState) -> Handler {
    any(create).with_state(state.ecommerce.clone())
}

async fn create(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<CreateProviderPayment>,
) -> Result<Json<CreatedPayment>, Failure> {
    Ok(Json(ecommerce.create_payment(&session, input).await?))
}

/// Reads a payment.
pub fn get_payment(state: &AppState) -> Handler {
    any(get).with_state(state.ecommerce.clone())
}

async fn get(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Result<Json<PaymentDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.get_payment(&id).await?))
}

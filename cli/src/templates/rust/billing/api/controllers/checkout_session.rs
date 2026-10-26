//! Checkout sessions. Each handler maps its request, calls the tenant-bound billing
//! service as the authenticated caller and replies in the module's wire format.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, State},
    response::Response,
    routing::any,
};
use forklaunch_axum::{RequestContext, session::AuthenticatedSession};
use forklaunch_billing_interface::services::CheckoutSessionService;
use forklaunch_billing_stripe::http::{BillingOperation, Handler, decode};
use serde_json::Value;

/// Creates a checkout session.
pub fn create_checkout_session(state: &AppState) -> Handler {
    any(create).with_state(state.billing.operation("createCheckoutSession"))
}

async fn create(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(input): Json<Value>,
) -> Response {
    let result = async {
        let input = operation.request(input)?;
        operation.service(&session)?.create_checkout_session(decode(input)?).await
    };
    operation.reply(result.await)
}

/// Reads a checkout session.
pub fn get_checkout_session(state: &AppState) -> Handler {
    any(get).with_state(state.billing.operation("getCheckoutSession"))
}

async fn get(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.get_checkout_session(&id).await };
    operation.reply_in(result.await, &context)
}

/// Expires a checkout session.
pub fn expire_checkout_session(state: &AppState) -> Handler {
    any(expire).with_state(state.billing.operation("expireCheckoutSession"))
}

async fn expire(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.expire_checkout_session(&id).await };
    operation.reply_text_in(result.await.map(|()| format!("Expired checkout session {id}")), &context)
}

/// Records a successful checkout.
pub fn handle_checkout_success(state: &AppState) -> Handler {
    any(succeed).with_state(state.billing.operation("handleCheckoutSuccess"))
}

async fn succeed(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.handle_checkout_success(&id).await };
    operation.reply_text_in(result.await.map(|()| format!("Handled checkout success for session {id}")), &context)
}

/// Records a failed checkout.
pub fn handle_checkout_failure(state: &AppState) -> Handler {
    any(fail).with_state(state.billing.operation("handleCheckoutFailure"))
}

async fn fail(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.handle_checkout_failure(&id).await };
    operation.reply_text_in(result.await.map(|()| format!("Handled checkout failure for session {id}")), &context)
}

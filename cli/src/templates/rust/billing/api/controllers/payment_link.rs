//! Payment links. Each handler maps its request, calls the tenant-bound billing
//! service as the authenticated caller and replies in the module's wire format.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, RawQuery, State},
    response::Response,
    routing::any,
};
use forklaunch_axum::{RequestContext, session::AuthenticatedSession};
use forklaunch_billing_interface::services::PaymentLinkService;
use forklaunch_billing_stripe::http::{BillingOperation, Handler, decode, list_ids, with_path_id};
use forklaunch_core::http::ParsedJsonBody;
use serde_json::Value;

/// Creates a payment link.
pub fn create_payment_link(state: &AppState) -> Handler {
    any(create).with_state(state.billing.operation("createPaymentLink"))
}

async fn create(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(input): Json<Value>,
) -> Response {
    let result = async {
        let input = operation.request(input)?;
        operation.service(&session)?.create_payment_link(decode(input)?).await
    };
    operation.reply(result.await)
}

/// Reads a payment link.
pub fn get_payment_link(state: &AppState) -> Handler {
    any(get).with_state(state.billing.operation("getPaymentLink"))
}

async fn get(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.get_payment_link(&id).await };
    operation.reply_in(result.await, &context)
}

/// Updates a payment link.
pub fn update_payment_link(state: &AppState) -> Handler {
    any(update).with_state(state.billing.operation("updatePaymentLink"))
}

async fn update(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
    parsed: Option<Extension<ParsedJsonBody>>,
    Json(input): Json<Value>,
) -> Response {
    let input = match with_path_id(&id, parsed.map(|Extension(parsed)| parsed), input) {
        Ok(input) => input,
        Err(error) => return operation.failure(&error),
    };
    let result = async {
        let input = operation.request(input)?;
        operation.service(&session)?.update_payment_link(decode(input)?).await
    };
    operation.reply_in(result.await, &context)
}

/// Expires a payment link.
pub fn expire_payment_link(state: &AppState) -> Handler {
    any(expire).with_state(state.billing.operation("expirePaymentLink"))
}

async fn expire(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Response {
    let result = async { operation.service(&session)?.expire_payment_link(&id).await };
    operation.reply_text(result.await.map(|()| format!("Expired payment link {id}")))
}

/// Records a successful payment.
pub fn handle_payment_success(state: &AppState) -> Handler {
    any(succeed).with_state(state.billing.operation("handlePaymentSuccess"))
}

async fn succeed(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.handle_payment_success(&id).await };
    operation.reply_text_in(result.await.map(|()| format!("Handled payment success for {id}")), &context)
}

/// Records a failed payment.
pub fn handle_payment_failure(state: &AppState) -> Handler {
    any(fail).with_state(state.billing.operation("handlePaymentFailure"))
}

async fn fail(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.handle_payment_failure(&id).await };
    operation.reply_text_in(result.await.map(|()| format!("Handled payment failure for {id}")), &context)
}

/// Lists payment links, optionally by a JSON `ids` array.
pub fn list_payment_links(state: &AppState) -> Handler {
    any(list).with_state(state.billing.operation("listPaymentLinks"))
}

async fn list(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    RawQuery(query): RawQuery,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let ids = match list_ids(query.as_deref()) {
        Ok(ids) => ids,
        Err(error) => return operation.failure(&error),
    };
    let result = async { operation.service(&session)?.list_payment_links(ids.as_deref()).await };
    operation.reply_in(result.await, &context)
}

//! Billing portal sessions. Each handler maps its request, calls the tenant-bound billing
//! service as the authenticated caller and replies in the module's wire format.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, State},
    response::Response,
    routing::any,
};
use forklaunch_axum::{RequestContext, session::AuthenticatedSession};
use forklaunch_billing_interface::services::BillingPortalService;
use forklaunch_billing_stripe::http::{BillingOperation, Handler, decode, with_path_id};
use forklaunch_core::http::ParsedJsonBody;
use serde_json::Value;

/// Creates a billing portal session.
pub fn create_billing_portal_session(state: &AppState) -> Handler {
    any(create).with_state(state.billing.operation("createBillingPortalSession"))
}

async fn create(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(input): Json<Value>,
) -> Response {
    let result = async {
        let input = operation.request(input)?;
        operation.service(&session)?.create_billing_portal_session(decode(input)?).await
    };
    operation.reply(result.await)
}

/// Reads a billing portal session.
pub fn get_billing_portal_session(state: &AppState) -> Handler {
    any(get).with_state(state.billing.operation("getBillingPortalSession"))
}

async fn get(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.get_billing_portal_session(&id).await };
    operation.reply_in(result.await, &context)
}

/// Updates a billing portal session.
pub fn update_billing_portal_session(state: &AppState) -> Handler {
    any(update).with_state(state.billing.operation("updateBillingPortalSession"))
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
        operation.service(&session)?.update_billing_portal_session(decode(input)?).await
    };
    operation.reply_in(result.await, &context)
}

/// Expires a billing portal session.
pub fn expire_billing_portal_session(state: &AppState) -> Handler {
    any(expire).with_state(state.billing.operation("expireBillingPortalSession"))
}

async fn expire(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Response {
    let result = async { operation.service(&session)?.expire_billing_portal_session(&id).await };
    operation.reply_text(result.await.map(|()| format!("Expired billing portal session {id}")))
}

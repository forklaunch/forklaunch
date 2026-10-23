//! Subscriptions. Each handler maps its request, calls the tenant-bound billing
//! service as the authenticated caller and replies in the module's wire format.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, RawQuery, State},
    response::Response,
    routing::any,
};
use forklaunch_axum::{RequestContext, session::AuthenticatedSession};
use forklaunch_billing_interface::services::SubscriptionService;
use forklaunch_billing_stripe::http::{BillingOperation, Handler, decode, list_ids, with_path_id};
use forklaunch_core::http::ParsedJsonBody;
use serde_json::Value;

/// Creates a subscription.
pub fn create_subscription(state: &AppState) -> Handler {
    any(create).with_state(state.billing.operation("createSubscription"))
}

async fn create(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(input): Json<Value>,
) -> Response {
    let result = async {
        let input = operation.request(input)?;
        operation.service(&session)?.create_subscription(decode(input)?).await
    };
    operation.reply(result.await)
}

/// Lists subscriptions, optionally by a JSON `ids` array.
pub fn list_subscriptions(state: &AppState) -> Handler {
    any(list).with_state(state.billing.operation("listSubscriptions"))
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
    let result = async { operation.service(&session)?.list_subscriptions(ids.as_deref()).await };
    operation.reply_in(result.await, &context)
}

/// Reads a subscription.
pub fn get_subscription(state: &AppState) -> Handler {
    any(get).with_state(state.billing.operation("getSubscription"))
}

async fn get(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.get_subscription(&id).await };
    operation.reply_in(result.await, &context)
}

/// Reads a user's subscription.
pub fn get_user_subscription(state: &AppState) -> Handler {
    any(get_for_user).with_state(state.billing.operation("getUserSubscription"))
}

async fn get_for_user(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.get_user_subscription(&id).await };
    operation.reply_in(result.await, &context)
}

/// Reads an organization's subscription.
pub fn get_organization_subscription(state: &AppState) -> Handler {
    any(get_for_organization).with_state(state.billing.operation("getOrganizationSubscription"))
}

async fn get_for_organization(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.get_organization_subscription(&id).await };
    operation.reply_in(result.await, &context)
}

/// Updates a subscription.
pub fn update_subscription(state: &AppState) -> Handler {
    any(update).with_state(state.billing.operation("updateSubscription"))
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
        operation.service(&session)?.update_subscription(decode(input)?).await
    };
    operation.reply_in(result.await, &context)
}

/// Deletes a subscription.
pub fn delete_subscription(state: &AppState) -> Handler {
    any(delete).with_state(state.billing.operation("deleteSubscription"))
}

async fn delete(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Response {
    let result = async { operation.service(&session)?.delete_subscription(&id).await };
    operation.reply_text(result.await.map(|()| format!("Deleted subscription {id}")))
}

/// Cancels a subscription.
pub fn cancel_subscription(state: &AppState) -> Handler {
    any(cancel).with_state(state.billing.operation("cancelSubscription"))
}

async fn cancel(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.cancel_subscription(&id).await };
    operation.reply_text_in(result.await.map(|()| format!("Cancelled subscription {id}")), &context)
}

/// Resumes a cancelled subscription.
pub fn resume_subscription(state: &AppState) -> Handler {
    any(resume).with_state(state.billing.operation("resumeSubscription"))
}

async fn resume(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.resume_subscription(&id).await };
    operation.reply_text_in(result.await.map(|()| format!("Resumed subscription {id}")), &context)
}

//! Plans. Each handler maps its request, calls the tenant-bound billing
//! service as the authenticated caller and replies in the module's wire format.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, RawQuery, State},
    response::Response,
    routing::any,
};
use forklaunch_axum::{RequestContext, session::AuthenticatedSession};
use forklaunch_billing_interface::services::PlanService;
use forklaunch_billing_stripe::http::{BillingOperation, Handler, decode, list_ids};
use serde_json::Value;

/// Creates a plan; with Stripe, its product and price too.
pub fn create_plan(state: &AppState) -> Handler {
    any(create).with_state(state.billing.operation("createPlan"))
}

async fn create(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(input): Json<Value>,
) -> Response {
    let result = async {
        let input = operation.request(input)?;
        operation.service(&session)?.create_plan(decode(input)?).await
    };
    operation.reply(result.await)
}

/// Lists plans, optionally by a JSON `ids` array.
pub fn list_plans(state: &AppState) -> Handler {
    any(list).with_state(state.billing.operation("listPlans"))
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
    let result = async { operation.service(&session)?.list_plans(ids.as_deref()).await };
    operation.reply_in(result.await, &context)
}

/// Reads a plan.
pub fn get_plan(state: &AppState) -> Handler {
    any(get).with_state(state.billing.operation("getPlan"))
}

async fn get(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    let result = async { operation.service(&session)?.get_plan(&id).await };
    operation.reply_in(result.await, &context)
}

/// Updates a plan.
pub fn update_plan(state: &AppState) -> Handler {
    any(update).with_state(state.billing.operation("updatePlan"))
}

async fn update(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(input): Json<Value>,
) -> Response {
    let result = async {
        let input = operation.request(input)?;
        operation.service(&session)?.update_plan(decode(input)?).await
    };
    operation.reply(result.await)
}

/// Deletes a plan.
pub fn delete_plan(state: &AppState) -> Handler {
    any(delete).with_state(state.billing.operation("deletePlan"))
}

async fn delete(
    State(operation): State<BillingOperation>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Response {
    let result = async { operation.service(&session)?.delete_plan(&id).await };
    operation.reply_text(result.await.map(|()| format!("Deleted plan {id}")))
}

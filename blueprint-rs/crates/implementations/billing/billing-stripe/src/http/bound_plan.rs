//! Explicit direct creation capability, independent of the read-only selector.
mod contracts;
use super::{BillingBackend, bound_billing::configuration, failure, invalid};
use crate::{
    bound_plan_creation::{
        BoundPlanCreateRequest, BoundPlanCreationOutcome as Outcome, BoundPlanCreationProfile,
    },
    services::StripeBillingService,
};
use axum::{
    Extension, Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
pub use contracts::bound_plan_contracts;
use forklaunch_axum::{ForkLaunchRouter, session::AuthenticatedSession};
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Request {
    operation_id: String,
    name: String,
    description: Option<String>,
    features: Option<Vec<String>>,
    active: bool,
    #[serde(deserialize_with = "unit_amount")]
    unit_amount: u64,
    currency: String,
    cadence: String,
    product_id: String,
}
fn unit_amount<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = Value::deserialize(deserializer)?;
    let number = value
        .as_f64()
        .filter(|v| {
            v.is_finite() && v.fract() == 0.0 && (0.0..=9_007_199_254_740_991.0).contains(v)
        })
        .ok_or_else(|| serde::de::Error::custom("invalid unit amount"))?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(number as u64)
}
fn id(value: &str) -> BlueprintResult<Uuid> {
    Uuid::parse_str(value)
        .ok()
        .filter(|v| !v.is_nil())
        .ok_or_else(invalid)
}
impl BillingBackend {
    pub(super) async fn with_bound_plan(
        mut self,
        profile: BoundPlanCreationProfile,
    ) -> BlueprintResult<Self> {
        if self.backup || self.bound_plan.is_some() {
            return Err(configuration());
        }
        let service = StripeBillingService::new(
            BaseBillingService::new(Arc::new(self.store.clone())),
            self.client.as_ref().ok_or_else(configuration)?.clone(),
        );
        service.validate_bound_plan_creation_configuration().await?;
        self.bound_plan = Some(Arc::new(profile));
        Ok(self)
    }
}
fn response(outcome: Outcome, inspect: bool) -> (StatusCode, Value) {
    match outcome {
        Outcome::Completed {
            operation_id,
            record,
        } => (
            StatusCode::OK,
            json!({"state":"completed","operationId":operation_id.to_string(),"publicId":record.id}),
        ),
        Outcome::Unresolved { operation_id } => (
            if inspect {
                StatusCode::OK
            } else {
                StatusCode::ACCEPTED
            },
            json!({"state":if inspect {"pending"} else {"unresolved"},"operationId":operation_id.to_string()}),
        ),
        Outcome::RecordedEvidenceNoLongerCurrent { operation_id } => (
            StatusCode::ACCEPTED,
            json!({"state":"recordedEvidenceNoLongerCurrent","operationId":operation_id.to_string()}),
        ),
    }
}
async fn execute(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    input: Option<Value>,
    operation: Option<String>,
) -> BlueprintResult<(StatusCode, Value)> {
    if backend.backup {
        return Err(configuration());
    }
    let profile = backend.bound_plan.as_ref().ok_or_else(configuration)?;
    let store = backend.store.with_audit_context(AuditContext::actor(
        &session.principal.subject,
        Uuid::new_v4().to_string(),
    ))?;
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store)),
        backend.client.as_ref().ok_or_else(configuration)?.clone(),
    );
    if let Some(input) = input {
        let request: Request = serde_json::from_value(input).map_err(|_| invalid())?;
        let operation = id(&request.operation_id)?;
        let result = service
            .create_bound_plan(
                profile,
                operation,
                BoundPlanCreateRequest {
                    name: request.name,
                    description: request.description,
                    features: request.features,
                    active: request.active,
                    unit_amount: request.unit_amount,
                    currency: request.currency,
                    cadence: request.cadence,
                    product_id: request.product_id,
                },
            )
            .await?;
        Ok(response(result, false))
    } else {
        let operation = id(operation.as_deref().ok_or_else(invalid)?)?;
        match service.inspect_bound_plan_creation(operation).await? {
            Some(result) => Ok(response(result, true)),
            None => Ok((StatusCode::NOT_FOUND, json!(null))),
        }
    }
}
fn dispatch(result: BlueprintResult<(StatusCode, Value)>) -> Response {
    match result {
        Ok((StatusCode::NOT_FOUND, _)) => {
            (StatusCode::NOT_FOUND, "Bound plan operation not found").into_response()
        }
        Ok((status, value)) => (status, Json(value)).into_response(),
        Err(error) => match error.code.as_str() {
            "billing_bound_plan_invalid" => {
                (StatusCode::BAD_REQUEST, "Invalid bound plan creation").into_response()
            }
            "billing_provider_binding_conflict" => {
                (StatusCode::CONFLICT, "Bound plan operation does not match").into_response()
            }
            _ => failure(&error),
        },
    }
}
pub(super) fn register(
    mut router: ForkLaunchRouter,
    backend: &Arc<BillingBackend>,
) -> ForkLaunchRouter {
    if backend.bound_plan.is_none() {
        return router;
    }
    for contract in bound_plan_contracts() {
        let backend = Arc::clone(backend);
        if contract.operation_name == "createBoundPlan" {
            router = router.post(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Json(input): Json<Value>| {
                    let backend = Arc::clone(&backend);
                    async move { dispatch(execute(backend, session, Some(input), None).await) }
                },
            );
        } else {
            router = router.get(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(operation): Path<String>| {
                    let backend = Arc::clone(&backend);
                    async move { dispatch(execute(backend, session, None, Some(operation)).await) }
                },
            );
        }
    }
    router
}

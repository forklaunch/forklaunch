//! Explicit direct creation capability, independent of the read-only selector.
mod contracts;
use super::{BillingBackend, bound_billing::configuration, failure, invalid};
use crate::{
    bound_subscription_creation::{
        BoundSubscriptionCreateRequest, BoundSubscriptionCreationOutcome as Outcome,
        BoundSubscriptionCreationProfile,
    },
    services::StripeBillingService,
};
use axum::{
    Extension, Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
pub use contracts::{bound_subscription_contracts, bound_subscription_recovery_contracts};
use forklaunch_axum::{ForkLaunchRouter, session::AuthenticatedSession};
use forklaunch_billing_base::services::{
    BaseBillingService, postgres::TrustedSubscriptionPublisher,
};
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
    local_customer_id: String,
    local_plan_id: String,
    #[serde(deserialize_with = "quantity")]
    quantity: u64,
    description: Option<String>,
}
fn quantity<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = Value::deserialize(deserializer)?;
    let number = value
        .as_f64()
        .filter(|v| {
            v.is_finite() && v.fract() == 0.0 && (1.0..=9_007_199_254_740_991.0).contains(v)
        })
        .ok_or_else(|| serde::de::Error::custom("invalid quantity"))?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(number as u64)
}
pub(super) struct BoundSubscriptionV1 {
    pub(super) creation: Option<BoundSubscriptionCreationProfile>,
    pub(super) recovery: Option<BoundSubscriptionCreationProfile>,
    pub(super) publisher: TrustedSubscriptionPublisher,
}
fn id(value: &str) -> BlueprintResult<Uuid> {
    Uuid::parse_str(value)
        .ok()
        .filter(|v| !v.is_nil())
        .ok_or_else(invalid)
}
impl BillingBackend {
    pub(super) async fn with_bound_subscription(
        mut self,
        profile: BoundSubscriptionV1,
    ) -> BlueprintResult<Self> {
        if self.backup || self.bound_subscription.is_some() {
            return Err(configuration());
        }
        let audit = AuditContext::actor(
            "service:bound-subscription-startup",
            Uuid::new_v4().to_string(),
        );
        let store = self.store.with_audit_context(audit.clone())?;
        let publisher = profile.publisher.with_audit_context(audit)?;
        let service = StripeBillingService::new(
            BaseBillingService::new(Arc::new(store)),
            self.client.as_ref().ok_or_else(configuration)?.clone(),
        );
        if profile.creation.is_some() {
            service
                .validate_bound_subscription_configuration(&publisher)
                .await?;
        }
        if profile.recovery.is_some() {
            service
                .validate_bound_subscription_recovery_configuration(&publisher)
                .await?;
        }
        self.bound_subscription = Some(Arc::new(profile));
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
    let profile = backend
        .bound_subscription
        .as_ref()
        .ok_or_else(configuration)?;
    let audit = AuditContext::actor(&session.principal.subject, Uuid::new_v4().to_string());
    let store = backend.store.with_audit_context(audit.clone())?;
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store)),
        backend.client.as_ref().ok_or_else(configuration)?.clone(),
    );
    if let Some(input) = input {
        let request: Request = serde_json::from_value(input).map_err(|_| invalid())?;
        let operation = id(&request.operation_id)?;
        let publisher = profile.publisher.with_audit_context(audit)?;
        let result = service
            .create_bound_subscription(
                profile.creation.as_ref().ok_or_else(configuration)?,
                &publisher,
                operation,
                BoundSubscriptionCreateRequest {
                    local_customer_id: id(&request.local_customer_id)?,
                    local_plan_id: id(&request.local_plan_id)?,
                    quantity: request.quantity,
                    description: request.description,
                },
            )
            .await?;
        Ok(response(result, false))
    } else {
        let operation = id(operation.as_deref().ok_or_else(invalid)?)?;
        match service
            .inspect_bound_subscription_creation(operation)
            .await?
        {
            Some(result) => Ok(response(result, true)),
            None => Ok((StatusCode::NOT_FOUND, json!(null))),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecoveryRequest {
    event_id: String,
}
async fn recover(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    operation: String,
    input: Value,
) -> BlueprintResult<(StatusCode, Value)> {
    if backend.backup {
        return Err(configuration());
    }
    let selected = backend
        .bound_subscription
        .as_ref()
        .ok_or_else(configuration)?;
    let profile = selected.recovery.as_ref().ok_or_else(configuration)?;
    let request: RecoveryRequest = serde_json::from_value(input).map_err(|_| invalid())?;
    let operation = id(&operation)?;
    let audit = AuditContext::actor(&session.principal.subject, Uuid::new_v4().to_string());
    let store = backend.store.with_audit_context(audit.clone())?;
    let publisher = selected.publisher.with_audit_context(audit)?;
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store)),
        backend.client.as_ref().ok_or_else(configuration)?.clone(),
    );
    Ok(response(
        service
            .recover_bound_subscription(profile, &publisher, operation, &request.event_id)
            .await?,
        false,
    ))
}
fn dispatch(result: BlueprintResult<(StatusCode, Value)>) -> Response {
    match result {
        Ok((StatusCode::NOT_FOUND, _)) => (
            StatusCode::NOT_FOUND,
            "Bound subscription operation not found",
        )
            .into_response(),
        Ok((status, value)) => (status, Json(value)).into_response(),
        Err(error) => match error.code.as_str() {
            "billing_bound_subscription_invalid" => (
                StatusCode::BAD_REQUEST,
                "Invalid bound subscription creation",
            )
                .into_response(),
            "billing_provider_binding_conflict" => (
                StatusCode::CONFLICT,
                "Bound subscription operation does not match",
            )
                .into_response(),
            _ => failure(&error),
        },
    }
}
pub(super) fn register(
    mut router: ForkLaunchRouter,
    backend: &Arc<BillingBackend>,
) -> ForkLaunchRouter {
    let Some(selected) = backend.bound_subscription.as_ref() else {
        return router;
    };
    let mut contracts = if selected.creation.is_some() {
        bound_subscription_contracts()
    } else {
        vec![]
    };
    if selected.recovery.is_some() {
        contracts.extend(
            bound_subscription_recovery_contracts()
                .into_iter()
                .filter(|r| {
                    selected.creation.is_none()
                        || r.operation_name != "inspectBoundSubscriptionOperation"
                }),
        );
    }
    for contract in contracts {
        let backend = Arc::clone(backend);
        if contract.operation_name == "createBoundSubscription" {
            router = router.post(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Json(input): Json<Value>| {
                    let backend = Arc::clone(&backend);
                    async move { dispatch(execute(backend, session, Some(input), None).await) }
                },
            );
        } else if contract.operation_name == "recoverBoundSubscriptionOperation" {
            router = router.post(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(operation): Path<String>,
                      Json(input): Json<Value>| {
                    let backend = Arc::clone(&backend);
                    async move { dispatch(recover(backend, session, operation, input).await) }
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

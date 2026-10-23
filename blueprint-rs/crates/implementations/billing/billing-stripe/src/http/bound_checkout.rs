//! Explicit direct-only operation API; generic billing routes remain unchanged.
mod config;
mod contracts;
use super::{BillingBackend, failure, invalid, stripe_names};
use crate::{
    bound_checkout_creation::{BoundCheckoutCreateRequest, BoundCheckoutCreationOutcome},
    services::StripeBillingService,
};
use axum::{
    Extension, Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
pub use config::{BillingIdentityProfile, BoundCheckoutConfig};
pub use contracts::bound_checkout_contracts;
use forklaunch_axum::{ForkLaunchRouter, session::AuthenticatedSession};
use forklaunch_billing_base::services::{
    BaseBillingService,
    postgres::{CheckoutCreationStatus, TrustedCheckoutPublisher},
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

/// Server-owned, explicitly validated publisher and finite creation profile.
/// No Debug/Clone/Serde and no credentials or provider configuration in responses.
pub struct BoundCheckoutV1 {
    configuration: BoundCheckoutConfig,
    publisher: TrustedCheckoutPublisher,
}
impl BoundCheckoutV1 {
    #[must_use]
    pub const fn trusted(
        configuration: BoundCheckoutConfig,
        publisher: TrustedCheckoutPublisher,
    ) -> Self {
        Self {
            configuration,
            publisher,
        }
    }
}
impl BillingBackend {
    /// Select last, after database-backup configuration. Default routes stay legacy.
    /// Uses an explicit trusted publisher, never the ordinary runtime pool.
    /// # Errors
    /// Rejects base/managed clients, backup fallback and mismatched publisher roles/database.
    pub async fn with_bound_checkout(mut self, profile: BoundCheckoutV1) -> BlueprintResult<Self> {
        if self.backup {
            return Err(config::configuration());
        }
        let client = self.client.as_ref().ok_or_else(config::configuration)?;
        let audit =
            AuditContext::actor("service:bound-checkout-startup", Uuid::new_v4().to_string());
        let store = self.store.with_audit_context(audit.clone())?;
        let publisher = profile.publisher.with_audit_context(audit)?;
        let service =
            StripeBillingService::new(BaseBillingService::new(Arc::new(store)), client.clone());
        service
            .validate_bound_checkout_configuration(&publisher)
            .await?;
        self.bound_checkout = Some(Arc::new(profile));
        Ok(self)
    }
}
fn parse_id(id: &str) -> BlueprintResult<Uuid> {
    let id = Uuid::parse_str(id).map_err(|_| invalid())?;
    if id.is_nil() {
        return Err(invalid());
    }
    Ok(id)
}
fn request(value: &Value) -> BlueprintResult<BoundCheckoutCreateRequest> {
    let object = value.as_object().ok_or_else(invalid)?;
    if object.len() != 3
        || ["localCustomerId", "localPlanId", "quantity"]
            .iter()
            .any(|key| !object.contains_key(*key))
    {
        return Err(invalid());
    }
    let quantity = value["quantity"]
        .as_u64()
        .or_else(|| {
            let number = value["quantity"].as_f64()?;
            if number.is_finite()
                && number.fract() == 0.0
                && (1.0..=9_007_199_254_740_991.0).contains(&number)
            {
                format!("{number:.0}").parse().ok()
            } else {
                None
            }
        })
        .filter(|v| *v > 0 && *v <= 9_007_199_254_740_991)
        .ok_or_else(invalid)?;
    Ok(BoundCheckoutCreateRequest {
        local_customer_id: parse_id(value["localCustomerId"].as_str().ok_or_else(invalid)?)?,
        local_plan_id: parse_id(value["localPlanId"].as_str().ok_or_else(invalid)?)?,
        quantity,
    })
}
fn pending(operation: Uuid, state: &str) -> (StatusCode, Value) {
    (
        StatusCode::ACCEPTED,
        json!({"state":state,"operationId":operation.to_string()}),
    )
}
async fn execute(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    operation: &str,
    id: &str,
    input: Option<Value>,
) -> BlueprintResult<(StatusCode, Value)> {
    if backend.backup {
        return Err(config::configuration());
    }
    let profile = backend
        .bound_checkout
        .as_ref()
        .ok_or_else(config::configuration)?;
    let id = parse_id(id)?;
    let audit = AuditContext::actor(&session.principal.subject, Uuid::new_v4().to_string());
    let store = backend.store.with_audit_context(audit.clone())?;
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store)),
        backend
            .client
            .as_ref()
            .ok_or_else(config::configuration)?
            .clone(),
    );
    match operation {
        "createBoundCheckoutSession" => {
            let request = request(&input.ok_or_else(invalid)?)?;
            let publisher = profile.publisher.with_audit_context(audit)?;
            let outcome = service
                .create_bound_checkout_session(
                    &profile.configuration.profile,
                    &publisher,
                    id,
                    request,
                )
                .await;
            match outcome {
                Ok(BoundCheckoutCreationOutcome::Created {
                    operation_id,
                    record,
                }) => {
                    let public_id = record.id.clone();
                    let mut record = serde_json::to_value(record).map_err(|_| invalid())?;
                    stripe_names(&mut record);
                    Ok((
                        StatusCode::OK,
                        json!({"state":"created","operationId":operation_id.to_string(),"publicId":public_id,"record":record}),
                    ))
                }
                Ok(BoundCheckoutCreationOutcome::PreviouslyCompleted {
                    operation_id,
                    public_id,
                }) => Ok((
                    StatusCode::OK,
                    json!({"state":"completed","operationId":operation_id.to_string(),"publicId":public_id.to_string()}),
                )),
                Ok(BoundCheckoutCreationOutcome::Unresolved { operation_id }) => {
                    Ok(pending(operation_id, "pending"))
                }
                Ok(BoundCheckoutCreationOutcome::RecordedEvidenceNoLongerCurrent {
                    operation_id,
                }) => Ok(pending(operation_id, "recordedEvidenceNoLongerCurrent")),
                Err(error) if error.code == "billing_checkout_creation_commit_unknown" => {
                    Ok(pending(id, "pending"))
                }
                Err(error) => Err(error),
            }
        }
        "getBoundCreatedCheckoutSession" => {
            let record = service
                .get_bound_created_checkout_session(&profile.configuration.scope, id)
                .await?;
            let mut value = serde_json::to_value(record).map_err(|_| invalid())?;
            stripe_names(&mut value);
            Ok((StatusCode::OK, value))
        }
        "inspectBoundCheckoutOperation" => match service
            .base()
            .store()
            .inspect_bound_checkout_creation(id)
            .await?
        {
            Some(CheckoutCreationStatus::Pending(pending)) => Ok((
                StatusCode::OK,
                json!({"state":"pending","operationId":pending.operation_id.to_string(),"publicId":pending.public_id.to_string()}),
            )),
            Some(CheckoutCreationStatus::Completed {
                operation_id,
                public_id,
            }) => Ok((
                StatusCode::OK,
                json!({"state":"completed","operationId":operation_id.to_string(),"publicId":public_id.to_string()}),
            )),
            None => Err(BlueprintError::new(
                "billing_not_found",
                "Billing operation not found",
            )),
        },
        _ => Err(invalid()),
    }
}
async fn dispatch(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    operation: &str,
    id: &str,
    input: Option<Value>,
) -> Response {
    match execute(backend, session, operation, id, input).await {
        Ok((status, value)) => (status, Json(value)).into_response(),
        Err(error) => match error.code.as_str() {
            "billing_bound_checkout_invalid" => {
                (StatusCode::BAD_REQUEST, "Invalid bound Checkout request").into_response()
            }
            "billing_provider_binding_conflict" | "billing_bound_checkout_mismatch" => (
                StatusCode::CONFLICT,
                "Bound Checkout evidence does not match",
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
    if backend.bound_checkout.is_none() {
        return router;
    }
    for contract in bound_checkout_contracts() {
        let name = contract.operation_name.clone();
        let backend = Arc::clone(backend);
        if name == "createBoundCheckoutSession" {
            router = router.post(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(id): Path<String>,
                      Json(input): Json<Value>| {
                    let backend = Arc::clone(&backend);
                    let name = name.clone();
                    async move { dispatch(backend, session, &name, &id, Some(input)).await }
                },
            );
        } else {
            router = router.get(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(id): Path<String>| {
                    let backend = Arc::clone(&backend);
                    let name = name.clone();
                    async move { dispatch(backend, session, &name, &id, None).await }
                },
            );
        }
    }
    router
}

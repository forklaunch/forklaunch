//! Explicit direct creation capability, independent of the read-only selector.
mod contracts;
use super::{BillingBackend, bound_billing::configuration, failure, invalid};
use crate::{
    bound_creation::{BoundPaymentLinkCreationOutcome as Outcome, BoundPaymentLinkCreationProfile},
    services::StripeBillingService,
};
use axum::{
    Extension, Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
pub use contracts::bound_payment_link_contracts;
use forklaunch_axum::{ForkLaunchRouter, session::AuthenticatedSession};
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_billing_interface::services::CreatePaymentLinkDto;
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
    amount: f64,
    currency: String,
    payment_methods: Vec<String>,
    line_items: Vec<Item>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Item {
    price: String,
    #[serde(deserialize_with = "quantity")]
    quantity: u64,
}
fn quantity<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = Value::deserialize(deserializer)?;
    let number = value
        .as_f64()
        .filter(|v| v.is_finite() && v.fract() == 0.0 && (1.0..=999_999.0).contains(v))
        .ok_or_else(|| serde::de::Error::custom("invalid quantity"))?;
    // Bounded positive integer exactly representable by both JSON/ECMAScript and u64.
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
    pub(super) async fn with_bound_payment_link(
        mut self,
        profile: BoundPaymentLinkCreationProfile,
    ) -> BlueprintResult<Self> {
        if self.backup || self.bound_payment_link.is_some() {
            return Err(configuration());
        }
        let service = StripeBillingService::new(
            BaseBillingService::new(Arc::new(self.store.clone())),
            self.client.as_ref().ok_or_else(configuration)?.clone(),
        );
        service
            .validate_bound_payment_link_creation_configuration()
            .await?;
        self.bound_payment_link = Some(Arc::new(profile));
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
        .bound_payment_link
        .as_ref()
        .ok_or_else(configuration)?;
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
            .create_bound_payment_link(
                profile,
                operation,
                CreatePaymentLinkDto {
                    id: None,
                    amount: request.amount,
                    currency: request.currency,
                    payment_methods: request.payment_methods,
                    status: "PENDING".into(),
                    provider_fields: Some(json!({"line_items":request.line_items})),
                },
            )
            .await?;
        Ok(response(result, false))
    } else {
        let operation = id(operation.as_deref().ok_or_else(invalid)?)?;
        match service
            .inspect_bound_payment_link_creation(operation)
            .await?
        {
            Some(result) => Ok(response(result, true)),
            None => Ok((StatusCode::NOT_FOUND, json!(null))),
        }
    }
}
fn dispatch(result: BlueprintResult<(StatusCode, Value)>) -> Response {
    match result {
        Ok((StatusCode::NOT_FOUND, _)) => (
            StatusCode::NOT_FOUND,
            "Bound payment link operation not found",
        )
            .into_response(),
        Ok((status, value)) => (status, Json(value)).into_response(),
        Err(error) => match error.code.as_str() {
            "billing_bound_creation_invalid" => (
                StatusCode::BAD_REQUEST,
                "Invalid bound payment link creation",
            )
                .into_response(),
            "billing_provider_binding_conflict" => (
                StatusCode::CONFLICT,
                "Bound payment link operation does not match",
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
    if backend.bound_payment_link.is_none() {
        return router;
    }
    for contract in bound_payment_link_contracts() {
        let backend = Arc::clone(backend);
        if contract.operation_name == "createBoundPaymentLink" {
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

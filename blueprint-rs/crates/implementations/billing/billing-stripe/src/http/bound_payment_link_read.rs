//! Explicit provenance-aware read capability; grants no creation/publication API.
use super::{BillingBackend, bound_billing::configuration, failure, invalid, stripe_names};
use crate::{BoundPaymentLinkListLimit, services::StripeBillingService};
use axum::{
    Extension, Json,
    extract::{Path, RawQuery},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use forklaunch_axum::{ForkLaunchRouter, session::AuthenticatedSession};
use forklaunch_billing_base::services::BaseBillingService;
use forklaunch_core::contract::AccessLevel;
use forklaunch_core::contract::AuthConfig;
use forklaunch_core::contract::BodyContract;
use forklaunch_core::contract::BodyKind;
use forklaunch_core::contract::FieldContract;
use forklaunch_core::contract::HmacAuth;
use forklaunch_core::contract::RouteContract;
use forklaunch_core::contract::SchemaRef;
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

/// Explicit getter/list contracts shared by HTTP, MCP and generated client artifacts.
/// # Panics
/// Only malformed package-owned Payment link response metadata can panic.
#[must_use]
pub fn bound_payment_link_read_contracts() -> Vec<RouteContract> {
    contracts(100)
}
pub(super) fn contracts(max_records: usize) -> Vec<RouteContract> {
    let registry = forklaunch_billing_interface::module_contracts::module_registry(true);
    let mut body = registry
        .routes()
        .iter()
        .find(|r| r.operation_name == "getPaymentLink")
        .unwrap()
        .responses
        .iter()
        .find(|r| r.status_code == 200)
        .unwrap()
        .body
        .clone()
        .unwrap();
    let mut schema: Value =
        serde_json::from_str(body.schema.json_schema.as_ref().unwrap()).unwrap();
    schema["properties"]["id"]["format"] = json!("uuid");
    body.schema.json_schema = Some(schema.to_string());
    body.schema.name = "BoundCreatedPaymentLink".into();
    let mut parameter = FieldContract::required("publicId", "String");
    parameter.schema.json_schema = Some(json!({"type":"string","format":"uuid"}).to_string());
    let mut route=RouteContract::get("Get Bound Created Payment Link", "Actor-owned completed direct PaymentLink creation; verifies current account/mode, sealed original price profile and local record/binding. Refreshes PaymentLink provider fields only; local amount/status remain unchanged, including when current provider active is false. No payment or entitlement evidence.", "/bound-payment-links/:publicId")
        .with_param(parameter).with_access(AccessLevel::Internal)
        .with_auth(AuthConfig::Hmac(HmacAuth::new().with_header_name("authorization")))
        .with_response(200,Some(body.clone()));
    for status in [400, 409, 500, 502, 503] {
        route = route.with_response(
            status,
            Some(BodyContract {
                kind: BodyKind::Text,
                schema: SchemaRef::named("String"),
                content_type: Some("text/plain".into()),
            }),
        );
    }
    let mut ids = FieldContract::required("ids", "StringArray");
    ids.schema.json_schema=Some(json!({"type":"array","minItems":1,"maxItems":max_records,"items":{"type":"string","format":"uuid"}}).to_string());
    let item: Value = serde_json::from_str(body.schema.json_schema.as_ref().unwrap()).unwrap();
    body.schema.name = "BoundCreatedPaymentLinkList".into();
    body.schema.json_schema = Some(json!({"type":"array","items":item}).to_string());
    let mut list=RouteContract::get("List Bound Created Payment Links","Explicit nonempty UUID selection in one JSON-encoded ids query parameter (generated TypeScript client encodes its array input); repeated query keys unsupported; input order/duplicates preserved, boolean active=true filtering. No enumeration, provider pagination, mutation or entitlement. Whole local selection preflight; shared5s context and8MiB evidence/results bounds.","/bound-payment-links")
        .with_query(ids).with_access(AccessLevel::Internal).with_auth(AuthConfig::Hmac(HmacAuth::new().with_header_name("authorization"))).with_response(200,Some(body));
    for response in &route.responses {
        if response.status_code != 200 {
            list = list.with_response(response.status_code, response.body.clone());
        }
    }
    vec![route, list]
}
impl BillingBackend {
    pub(super) async fn with_bound_payment_link_read(
        mut self,
        profile: super::bound_billing::PaymentLinkReadConfig,
    ) -> BlueprintResult<Self> {
        if self.backup || self.bound_payment_link_read.is_some() {
            return Err(configuration());
        }
        let client = self.client.as_ref().ok_or_else(configuration)?;
        let service = StripeBillingService::new(
            BaseBillingService::new(Arc::new(self.store.clone())),
            client.clone(),
        );
        service
            .validate_bound_payment_link_read_configuration()
            .await?;
        self.bound_payment_link_read = Some(Arc::new(profile));
        Ok(self)
    }
}
async fn execute(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    id: &str,
) -> BlueprintResult<Value> {
    if backend.backup {
        return Err(configuration());
    }
    let profile = backend
        .bound_payment_link_read
        .as_ref()
        .ok_or_else(configuration)?;
    let id = Uuid::parse_str(id).map_err(|_| invalid())?;
    if id.is_nil() {
        return Err(invalid());
    }
    let store = backend.store.with_audit_context(AuditContext::actor(
        &session.principal.subject,
        Uuid::new_v4().to_string(),
    ))?;
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store)),
        backend.client.as_ref().ok_or_else(configuration)?.clone(),
    );
    let mut value = serde_json::to_value(
        service
            .get_bound_created_payment_link(&profile.profile, id)
            .await?,
    )
    .map_err(|_| invalid())?;
    stripe_names(&mut value);
    Ok(value)
}
async fn dispatch(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    id: &str,
) -> Response {
    outcome(execute(backend, session, id).await)
}
fn outcome(result: BlueprintResult<Value>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => match error.code.as_str() {
            "billing_bound_payment_link_read_mismatch"
            | "billing_provider_binding_conflict"
            | "billing_provider_context_mismatch"
            | "billing_bound_creation_invalid" => (
                StatusCode::CONFLICT,
                "Bound payment link evidence does not match",
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
    if backend.bound_payment_link_read.is_none() {
        return router;
    }
    let maximum = backend
        .bound_payment_link_read
        .as_ref()
        .unwrap()
        .max_records;
    for contract in contracts(maximum) {
        let backend = Arc::clone(backend);
        if contract.request.params.is_empty() {
            router = router.get(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      RawQuery(query): RawQuery| {
                    let backend = Arc::clone(&backend);
                    async move { outcome(execute_list(backend, session, query.as_deref()).await) }
                },
            );
        } else {
            router = router.get(
                contract,
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(id): Path<String>| {
                    let backend = Arc::clone(&backend);
                    async move { dispatch(backend, session, &id).await }
                },
            );
        }
    }
    router
}

async fn execute_list(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    query: Option<&str>,
) -> BlueprintResult<Value> {
    let profile = backend
        .bound_payment_link_read
        .as_ref()
        .ok_or_else(configuration)?;
    let mut ids = None;
    for (key, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        if key != "ids" || ids.is_some() {
            return Err(invalid());
        }
        let values: Vec<Uuid> = serde_json::from_str(&value).map_err(|_| invalid())?;
        if values.is_empty()
            || values.len() > profile.max_records
            || values.iter().any(Uuid::is_nil)
        {
            return Err(invalid());
        }
        ids = Some(values);
    }
    let ids = ids.ok_or_else(invalid)?;
    let store = backend.store.with_audit_context(AuditContext::actor(
        &session.principal.subject,
        Uuid::new_v4().to_string(),
    ))?;
    let service = StripeBillingService::new(
        BaseBillingService::new(Arc::new(store)),
        backend.client.as_ref().ok_or_else(configuration)?.clone(),
    );
    let records = service
        .list_bound_created_payment_links(
            &profile.profile,
            &ids,
            BoundPaymentLinkListLimit::trusted(profile.max_records)?,
        )
        .await?;
    let mut value = serde_json::to_value(records).map_err(|_| invalid())?;
    stripe_names(&mut value);
    Ok(value)
}

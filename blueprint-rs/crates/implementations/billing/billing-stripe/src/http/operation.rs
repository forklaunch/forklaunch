//! What generated billing controllers call: one operation's projected contract
//! with the configured backend, the audited service and the request/response
//! mappers that keep the wire format of the packaged handlers.
use super::{
    BillingBackend, BillingResponse, contract, dispatch_failure, failure, invalid, list_input,
    project, stripe_names, validate_input,
};
use crate::{client::StripeBillingClient, services::StripeBillingService};
use axum::{
    Json,
    response::{IntoResponse, Response},
};
use forklaunch_axum::{RequestContext, RouterConfig, session::AuthenticatedSession};
use forklaunch_billing_base::services::{BaseBillingService, postgres::PostgresBillingStore};
use forklaunch_billing_interface::services::{
    BillingPortalDto, BillingPortalService, CheckoutSessionDto, CheckoutSessionService,
    CreateBillingPortalDto, CreateCheckoutSessionDto, CreatePaymentLinkDto, CreatePlanDto,
    CreateSubscriptionDto, PaymentLinkDto, PaymentLinkService, PlanDto, PlanService,
    SubscriptionDto, SubscriptionService, UpdateBillingPortalDto, UpdatePaymentLinkDto,
    UpdatePlanDto, UpdateSubscriptionDto,
};
use forklaunch_core::contract::RouteContract;
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::http::ParsedJsonBody;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

/// One billing operation over the configured backend: its contract after the
/// backend's HMAC, request-date and `PaymentLink` projections. Generated
/// controllers take it as their state, from [`super::BillingHttp::operation`].
#[derive(Clone)]
pub struct BillingOperation {
    backend: Arc<BillingBackend>,
    contract: Arc<RouteContract>,
}

impl BillingOperation {
    pub(super) fn new(backend: &Arc<BillingBackend>, operation: &str) -> Self {
        let registry = forklaunch_billing_interface::module_contracts::module_registry(
            backend.client.is_some(),
        );
        let mut contract = contract(&registry, operation);
        project(
            backend,
            &registry,
            &mut RouterConfig::default(),
            &mut contract,
        );
        Self {
            backend: Arc::clone(backend),
            contract: Arc::new(contract),
        }
    }

    /// Maps a JSON request body to the service's input: normalizes the
    /// configured request dates, checks required fields, enums, UUIDs and
    /// object members against the contract (dropping undeclared members, as
    /// TypeScript parsing does) and, with Stripe, renames `stripeFields` to
    /// `providerFields` and drops client-chosen plan and subscription IDs.
    ///
    /// # Errors
    /// `billing_validation` (400) for input the contract rejects, and
    /// `billing_date_profile_unsupported` (400) for dates outside the profile.
    pub fn request(&self, mut input: Value) -> BlueprintResult<Value> {
        let contract = self.contract.as_ref();
        self.backend.request_dates.normalize(contract, &mut input)?;
        validate_input(contract, &mut input)?;
        if self.backend.client.is_some()
            && let Some(fields) = input
                .as_object_mut()
                .and_then(|fields| fields.remove("stripeFields"))
        {
            input["providerFields"] = fields;
        }
        if self.backend.client.is_some()
            && matches!(
                contract.operation_name.as_str(),
                "createPlan" | "createSubscription"
            )
        {
            input.as_object_mut().ok_or_else(invalid)?.remove("id");
        }
        Ok(input)
    }

    /// The tenant-bound billing service, audited as the session's subject with
    /// a fresh correlation ID: Stripe-backed when a client is configured,
    /// storage-only otherwise.
    ///
    /// # Errors
    /// Fails when the store rejects the audit context or the configured list
    /// limit is invalid.
    pub fn service(&self, session: &AuthenticatedSession) -> BlueprintResult<BillingService> {
        let backend = self.backend.as_ref();
        let store = backend.store.with_audit_context(AuditContext::actor(
            &session.principal.subject,
            uuid::Uuid::new_v4().to_string(),
        ))?;
        let service = BaseBillingService::new(Arc::new(store))
            .with_cache_ttl(Duration::from_secs(600))?
            .with_party_types("user".into(), "organization".into())?
            .with_database_backup(backend.backup);
        Ok(match &backend.client {
            Some(client) => {
                BillingService::Stripe(Box::new(stripe_service(backend, service, client)?))
            }
            None => BillingService::Base(service),
        })
    }

    /// A JSON result as the response: with Stripe, `providerFields` is renamed
    /// back to `stripeFields`. Errors map to their fixed status and text.
    #[must_use]
    pub fn reply<T: Serialize>(&self, result: BlueprintResult<T>) -> Response {
        match result.and_then(|value| self.json(value)) {
            Ok(response) => response.into_response(),
            Err(error) => failure(&error),
        }
    }

    /// [`Self::reply`] for routes whose configured TypeScript mappers report
    /// their failures in the TypeScript parser's error format with the
    /// request's correlation ID.
    #[must_use]
    pub fn reply_in<T: Serialize>(
        &self,
        result: BlueprintResult<T>,
        context: &RequestContext,
    ) -> Response {
        match result.and_then(|value| self.json(value)) {
            Ok(response) => response.into_response(),
            Err(error) => dispatch_failure(&self.backend, &self.contract, &error, context),
        }
    }

    /// A text result as the response; errors as in [`Self::reply`].
    #[must_use]
    #[allow(clippy::unused_self)] // Every reply goes through the operation.
    pub fn reply_text(&self, result: BlueprintResult<String>) -> Response {
        match result {
            Ok(message) => BillingResponse::Text(message).into_response(),
            Err(error) => failure(&error),
        }
    }

    /// A text result as the response; errors as in [`Self::reply_in`].
    #[must_use]
    pub fn reply_text_in(
        &self,
        result: BlueprintResult<String>,
        context: &RequestContext,
    ) -> Response {
        match result {
            Ok(message) => BillingResponse::Text(message).into_response(),
            Err(error) => dispatch_failure(&self.backend, &self.contract, &error, context),
        }
    }

    /// An error as its fixed status and text, for input rejected before the
    /// service runs.
    #[must_use]
    #[allow(clippy::unused_self)] // Every reply goes through the operation.
    pub fn failure(&self, error: &BlueprintError) -> Response {
        failure(error)
    }

    fn json<T: Serialize>(&self, value: T) -> BlueprintResult<Json<Value>> {
        let mut value = serde_json::to_value(value).map_err(|_| invalid())?;
        if self.backend.client.is_some() {
            stripe_names(&mut value);
        }
        Ok(Json(value))
    }
}

fn stripe_service(
    backend: &BillingBackend,
    service: BaseBillingService<PostgresBillingStore>,
    client: &StripeBillingClient,
) -> BlueprintResult<StripeBillingService<PostgresBillingStore>> {
    StripeBillingService::new(service, client.clone())
        .with_configured_subscription_updates(backend.configured_subscription_updates)
        .with_configured_payment_link_updates(backend.configured_payment_link_updates)
        .with_configured_cached_reads(backend.configured_cached_reads)
        .with_max_configured_list_records(backend.max_configured_list_records)
}

/// Decodes a mapped request (see [`BillingOperation::request`]) as `T`.
///
/// # Errors
/// `billing_validation` (400) when the input does not match `T`.
pub fn decode<T: DeserializeOwned>(input: Value) -> BlueprintResult<T> {
    serde_json::from_value(input).map_err(|_| invalid())
}

/// The optional JSON `ids` array of a list query.
///
/// # Errors
/// `billing_validation` (400) when `ids` is repeated or not a string array.
pub fn list_ids(query: Option<&str>) -> BlueprintResult<Option<Vec<String>>> {
    list_input(query)?
        .get("ids")
        .map(|ids| serde_json::from_value(ids.clone()))
        .transpose()
        .map_err(|_| invalid())
}

/// An update body with the path's `id`: the validated body the configured
/// parser produced, when there is one, replaces the raw body, and a body `id`
/// takes precedence over the path's.
///
/// # Errors
/// `billing_validation` (400) when the body is not a JSON object.
pub fn with_path_id(
    id: &str,
    parsed: Option<ParsedJsonBody>,
    mut input: Value,
) -> BlueprintResult<Value> {
    if let Some(value) = parsed.and_then(|parsed| parsed.value) {
        input = (*value).clone();
    }
    input
        .as_object_mut()
        .ok_or_else(invalid)?
        .entry("id")
        .or_insert(json!(id));
    Ok(input)
}

/// The configured billing service: storage-only, or Stripe-backed.
pub enum BillingService {
    Base(BaseBillingService<PostgresBillingStore>),
    Stripe(Box<StripeBillingService<PostgresBillingStore>>),
}

// Trait-qualified, so an inherent method of the same name is never chosen.
macro_rules! delegate {
    ($self:ident, $service:ident::$method:ident($($argument:expr),*)) => {
        match $self {
            Self::Base(service) => $service::$method(service, $($argument),*).await,
            Self::Stripe(service) => $service::$method(service.as_ref(), $($argument),*).await,
        }
    };
}

impl PlanService for BillingService {
    async fn create_plan(&self, request: CreatePlanDto) -> BlueprintResult<PlanDto> {
        delegate!(self, PlanService::create_plan(request))
    }
    async fn get_plan(&self, id: &str) -> BlueprintResult<PlanDto> {
        delegate!(self, PlanService::get_plan(id))
    }
    async fn update_plan(&self, request: UpdatePlanDto) -> BlueprintResult<PlanDto> {
        delegate!(self, PlanService::update_plan(request))
    }
    async fn delete_plan(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, PlanService::delete_plan(id))
    }
    async fn list_plans(&self, ids: Option<&[String]>) -> BlueprintResult<Vec<PlanDto>> {
        delegate!(self, PlanService::list_plans(ids))
    }
}

impl SubscriptionService for BillingService {
    async fn create_subscription(
        &self,
        request: CreateSubscriptionDto,
    ) -> BlueprintResult<SubscriptionDto> {
        delegate!(self, SubscriptionService::create_subscription(request))
    }
    async fn get_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        delegate!(self, SubscriptionService::get_subscription(id))
    }
    async fn get_user_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        delegate!(self, SubscriptionService::get_user_subscription(id))
    }
    async fn get_organization_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        delegate!(self, SubscriptionService::get_organization_subscription(id))
    }
    async fn find_subscription_id_by_external_id(
        &self,
        external_id: &str,
    ) -> BlueprintResult<Option<String>> {
        delegate!(
            self,
            SubscriptionService::find_subscription_id_by_external_id(external_id)
        )
    }
    async fn update_subscription(
        &self,
        request: UpdateSubscriptionDto,
    ) -> BlueprintResult<SubscriptionDto> {
        delegate!(self, SubscriptionService::update_subscription(request))
    }
    async fn delete_subscription(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, SubscriptionService::delete_subscription(id))
    }
    async fn list_subscriptions(
        &self,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<SubscriptionDto>> {
        delegate!(self, SubscriptionService::list_subscriptions(ids))
    }
    async fn cancel_subscription(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, SubscriptionService::cancel_subscription(id))
    }
    async fn resume_subscription(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, SubscriptionService::resume_subscription(id))
    }
}

impl CheckoutSessionService for BillingService {
    async fn create_checkout_session(
        &self,
        request: CreateCheckoutSessionDto,
    ) -> BlueprintResult<CheckoutSessionDto> {
        delegate!(
            self,
            CheckoutSessionService::create_checkout_session(request)
        )
    }
    async fn get_checkout_session(&self, id: &str) -> BlueprintResult<CheckoutSessionDto> {
        delegate!(self, CheckoutSessionService::get_checkout_session(id))
    }
    async fn expire_checkout_session(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, CheckoutSessionService::expire_checkout_session(id))
    }
    async fn handle_checkout_success(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, CheckoutSessionService::handle_checkout_success(id))
    }
    async fn handle_checkout_failure(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, CheckoutSessionService::handle_checkout_failure(id))
    }
}

impl PaymentLinkService for BillingService {
    async fn create_payment_link(
        &self,
        request: CreatePaymentLinkDto,
    ) -> BlueprintResult<PaymentLinkDto> {
        delegate!(self, PaymentLinkService::create_payment_link(request))
    }
    async fn update_payment_link(
        &self,
        request: UpdatePaymentLinkDto,
    ) -> BlueprintResult<PaymentLinkDto> {
        delegate!(self, PaymentLinkService::update_payment_link(request))
    }
    async fn get_payment_link(&self, id: &str) -> BlueprintResult<PaymentLinkDto> {
        delegate!(self, PaymentLinkService::get_payment_link(id))
    }
    async fn expire_payment_link(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, PaymentLinkService::expire_payment_link(id))
    }
    async fn handle_payment_success(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, PaymentLinkService::handle_payment_success(id))
    }
    async fn handle_payment_failure(&self, id: &str) -> BlueprintResult<()> {
        delegate!(self, PaymentLinkService::handle_payment_failure(id))
    }
    async fn list_payment_links(
        &self,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<PaymentLinkDto>> {
        delegate!(self, PaymentLinkService::list_payment_links(ids))
    }
}

impl BillingPortalService for BillingService {
    async fn create_billing_portal_session(
        &self,
        request: CreateBillingPortalDto,
    ) -> BlueprintResult<BillingPortalDto> {
        delegate!(
            self,
            BillingPortalService::create_billing_portal_session(request)
        )
    }
    async fn get_billing_portal_session(&self, id: &str) -> BlueprintResult<BillingPortalDto> {
        delegate!(self, BillingPortalService::get_billing_portal_session(id))
    }
    async fn update_billing_portal_session(
        &self,
        request: UpdateBillingPortalDto,
    ) -> BlueprintResult<BillingPortalDto> {
        delegate!(
            self,
            BillingPortalService::update_billing_portal_session(request)
        )
    }
    async fn expire_billing_portal_session(&self, id: &str) -> BlueprintResult<()> {
        delegate!(
            self,
            BillingPortalService::expire_billing_portal_session(id)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_ids_fill_a_missing_body_id_and_reject_non_objects() {
        assert_eq!(
            with_path_id("path", None, json!({"name": "Pro"})).unwrap(),
            json!({"name": "Pro", "id": "path"})
        );
        assert_eq!(
            with_path_id("path", None, json!({"id": "body"})).unwrap()["id"],
            "body"
        );
        assert_eq!(
            with_path_id("path", None, json!([1])).unwrap_err().code,
            "billing_validation"
        );
    }

    #[test]
    fn list_queries_take_an_optional_json_ids_array() {
        assert_eq!(list_ids(None).unwrap(), None);
        assert_eq!(
            list_ids(Some("ids=%5B%22a%22%5D")).unwrap(),
            Some(vec!["a".to_owned()])
        );
        for query in ["ids=a", "ids=%5B1%5D", "ids=%5B%5D&ids=%5B%5D"] {
            assert!(list_ids(Some(query)).is_err(), "{query}");
        }
    }
}

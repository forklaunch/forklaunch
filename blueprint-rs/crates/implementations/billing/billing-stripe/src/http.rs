//! Persistent billing HTTP support shared with authenticated MCP dispatch.
//!
//! Generated services own their controllers: each takes the
//! [`BillingOperation`] from [`BillingHttp::operation`], maps its request,
//! calls the audited [`BillingService`] and replies through the operation, and
//! [`BillingRoutes`] binds them to the interface's contracts. Webhooks, bound
//! capabilities and configured HMAC verification stay here. The packaged
//! [`BillingHttp::handler`] remains for services generated before 0.2.5.
mod configured_hmac;
mod operation;
pub use configured_hmac::project_configured_billing_hmac;
pub use operation::{BillingOperation, BillingService, decode, list_ids, with_path_id};
mod configured_payment_link_input;
pub use configured_payment_link_input::project_configured_payment_link_update;
#[cfg(feature = "bound-checkout")]
mod bound_billing;
#[cfg(feature = "bound-checkout")]
mod bound_checkout;
#[cfg(feature = "bound-checkout")]
mod bound_payment_link;
#[cfg(feature = "bound-checkout")]
mod bound_payment_link_expiration;
#[cfg(feature = "bound-checkout")]
mod bound_payment_link_read;
#[cfg(feature = "bound-checkout")]
mod bound_plan;
#[cfg(feature = "bound-checkout")]
mod bound_plan_read;
#[cfg(feature = "bound-checkout")]
mod bound_portal;
#[cfg(feature = "bound-checkout")]
mod bound_portal_expiration;
#[cfg(feature = "bound-checkout")]
mod bound_subscription;
#[cfg(feature = "bound-checkout")]
mod bound_subscription_read;
#[cfg(feature = "bound-checkout")]
pub mod profiles;
#[cfg(feature = "bound-checkout")]
pub use bound_billing::{BoundBillingConfig, BoundBillingV1, BoundPortalConfig};
#[cfg(feature = "bound-checkout")]
pub use bound_checkout::{
    BillingIdentityProfile, BoundCheckoutConfig, BoundCheckoutV1, bound_checkout_contracts,
};
#[cfg(feature = "bound-checkout")]
pub use bound_payment_link::bound_payment_link_contracts;
#[cfg(feature = "bound-checkout")]
pub use bound_payment_link_expiration::{
    bound_payment_link_expiration_contracts, bound_payment_link_expiration_recovery_contracts,
};
#[cfg(feature = "bound-checkout")]
pub use bound_payment_link_read::bound_payment_link_read_contracts;
#[cfg(feature = "bound-checkout")]
pub use bound_plan::bound_plan_contracts;
#[cfg(feature = "bound-checkout")]
pub use bound_plan_read::bound_plan_read_contracts;
#[cfg(feature = "bound-checkout")]
pub use bound_portal::{BoundPortalV1, bound_portal_contracts};
#[cfg(feature = "bound-checkout")]
pub use bound_portal_expiration::bound_portal_expiration_contracts;
#[cfg(feature = "bound-checkout")]
pub use bound_subscription::{bound_subscription_contracts, bound_subscription_recovery_contracts};
#[cfg(feature = "bound-checkout")]
pub use bound_subscription_read::bound_subscription_read_contracts;
mod request_dates;
mod webhook;
use crate::{client::StripeBillingClient, services::StripeBillingService};
use axum::{
    Extension, Json,
    extract::{Path, RawQuery},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{MethodRouter, any},
};
use forklaunch_axum::{
    ForkLaunchRouter, RouterConfig, module_auth::HmacJwksAuth, session::AuthenticatedSession,
};
use forklaunch_billing_base::services::{BaseBillingService, postgres::PostgresBillingStore};
use forklaunch_billing_interface::{
    module_contracts,
    services::{
        BillingPortalService, CheckoutSessionService, PaymentLinkService, PlanService,
        SubscriptionService,
    },
};
use forklaunch_core::ServiceInfo;
use forklaunch_core::contract::HttpMethod;
use forklaunch_core::contract::RouteContract;
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
pub use request_dates::BillingRequestDateProfile;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
// Independent backward-compatible mapper selections and backup policy; these
// are orthogonal switches, not states of one authorization workflow.
#[allow(clippy::struct_excessive_bools)]
pub struct BillingBackend {
    store: PostgresBillingStore,
    client: Option<StripeBillingClient>,
    backup: bool,
    #[cfg(feature = "bound-checkout")]
    bound_checkout: Option<Arc<BoundCheckoutV1>>,
    #[cfg(feature = "bound-checkout")]
    bound_portal: Option<Arc<BoundPortalV1>>,
    #[cfg(feature = "bound-checkout")]
    bound_portal_expiration: Option<Arc<crate::bound_portal_creation::BoundPortalCreationProfile>>,
    #[cfg(feature = "bound-checkout")]
    bound_plan_read: Option<Arc<crate::bound_plan_creation::BoundPlanCreationProfile>>,
    #[cfg(feature = "bound-checkout")]
    bound_subscription_read:
        Option<Arc<crate::bound_subscription_creation::BoundSubscriptionCreationProfile>>,
    #[cfg(feature = "bound-checkout")]
    bound_payment_link_read: Option<Arc<bound_billing::PaymentLinkReadConfig>>,
    #[cfg(feature = "bound-checkout")]
    bound_payment_link: Option<Arc<crate::bound_creation::BoundPaymentLinkCreationProfile>>,
    #[cfg(feature = "bound-checkout")]
    bound_plan: Option<Arc<crate::bound_plan_creation::BoundPlanCreationProfile>>,
    #[cfg(feature = "bound-checkout")]
    bound_subscription: Option<Arc<bound_subscription::BoundSubscriptionV1>>,
    #[cfg(feature = "bound-checkout")]
    bound_payment_link_expiration:
        Option<Arc<bound_payment_link_expiration::BoundPaymentLinkExpirationV1>>,
    configured_hmac: bool,
    mount_relative_hmac: Option<bool>,
    configured_subscription_updates: bool,
    configured_payment_link_updates: bool,
    configured_cached_reads: bool,
    max_configured_list_records: usize,
    request_dates: BillingRequestDateProfile,
    webhook: Option<crate::webhook::StripeWebhookVerifier>,
    platform_key: Option<zeroize::Zeroizing<String>>,
}
impl BillingBackend {
    #[must_use]
    pub const fn base(store: PostgresBillingStore) -> Self {
        Self {
            store,
            client: None,
            backup: false,
            #[cfg(feature = "bound-checkout")]
            bound_checkout: None,
            #[cfg(feature = "bound-checkout")]
            bound_portal: None,
            #[cfg(feature = "bound-checkout")]
            bound_portal_expiration: None,
            #[cfg(feature = "bound-checkout")]
            bound_plan_read: None,
            #[cfg(feature = "bound-checkout")]
            bound_subscription_read: None,
            #[cfg(feature = "bound-checkout")]
            bound_payment_link_read: None,
            #[cfg(feature = "bound-checkout")]
            bound_payment_link: None,
            #[cfg(feature = "bound-checkout")]
            bound_payment_link_expiration: None,
            #[cfg(feature = "bound-checkout")]
            bound_plan: None,
            #[cfg(feature = "bound-checkout")]
            bound_subscription: None,
            configured_hmac: false,
            mount_relative_hmac: None,
            configured_subscription_updates: false,
            configured_payment_link_updates: false,
            configured_cached_reads: false,
            max_configured_list_records: 100,
            request_dates: BillingRequestDateProfile::Legacy,
            webhook: None,
            platform_key: None,
        }
    }
    #[must_use]
    pub const fn stripe(store: PostgresBillingStore, client: StripeBillingClient) -> Self {
        Self {
            store,
            client: Some(client),
            backup: false,
            #[cfg(feature = "bound-checkout")]
            bound_checkout: None,
            #[cfg(feature = "bound-checkout")]
            bound_portal: None,
            #[cfg(feature = "bound-checkout")]
            bound_portal_expiration: None,
            #[cfg(feature = "bound-checkout")]
            bound_plan_read: None,
            #[cfg(feature = "bound-checkout")]
            bound_subscription_read: None,
            #[cfg(feature = "bound-checkout")]
            bound_payment_link_read: None,
            #[cfg(feature = "bound-checkout")]
            bound_payment_link: None,
            #[cfg(feature = "bound-checkout")]
            bound_payment_link_expiration: None,
            #[cfg(feature = "bound-checkout")]
            bound_plan: None,
            #[cfg(feature = "bound-checkout")]
            bound_subscription: None,
            configured_hmac: false,
            mount_relative_hmac: None,
            configured_subscription_updates: false,
            configured_payment_link_updates: false,
            configured_cached_reads: false,
            max_configured_list_records: 100,
            request_dates: BillingRequestDateProfile::Legacy,
            webhook: None,
            platform_key: None,
        }
    }
    /// Selects the generated TS application's Subscription owner-update preflight.
    /// Does not grant provider ownership or alter the generic default.
    #[must_use]
    pub const fn with_configured_subscription_updates(mut self, enabled: bool) -> Self {
        self.configured_subscription_updates = enabled;
        self
    }
    /// Deprecated compatibility: whether configured-HMAC signatures over the
    /// Express router-relative path (`/` for `POST /payment-link`) that older
    /// TypeScript callers produced are still accepted. On by default so existing
    /// callers keep working. A relative signature is not bound to its mount, so
    /// one signed for `POST /plan` also verifies as `POST /payment-link`; pass
    /// `false` (or set `HMAC_ACCEPT_MOUNT_RELATIVE_PATHS=false`) once every caller
    /// signs the full path. This builder setting wins over the environment.
    #[must_use]
    pub const fn with_mount_relative_hmac_paths(mut self, enabled: bool) -> Self {
        self.mount_relative_hmac = Some(enabled);
        self
    }
    /// Select actual configured internal billing HMAC parsing. The signed path is
    /// the full wire path (see [`Self::with_mount_relative_hmac_paths`]).
    /// Generic HMAC, Portal JWT and bound operation policies remain unchanged.
    /// ISO timestamp forms are supported; legacy/local-time JavaScript Date strings
    /// remain a distinct unsupported boundary in this implementation phase.
    #[must_use]
    pub const fn with_configured_hmac(mut self, enabled: bool) -> Self {
        self.configured_hmac = enabled;
        self
    }

    /// Selects configured `PaymentLink` cache/patch validation before provider updates.
    /// Read selectors do not imply this mutation behavior.
    #[must_use]
    pub const fn with_configured_payment_link_updates(mut self, enabled: bool) -> Self {
        self.configured_payment_link_updates = enabled;
        self
    }
    /// Selects configured cache-first Checkout and `PaymentLink` read mappers.
    #[must_use]
    pub const fn with_configured_cached_reads(mut self, enabled: bool) -> Self {
        self.configured_cached_reads = enabled;
        self
    }

    /// Trusted configured `PaymentLink` list limit; never taken from request data.
    ///
    /// # Errors
    /// Returns a configuration error unless the limit is between 1 and 1000.
    pub fn with_max_configured_list_records(mut self, limit: usize) -> BlueprintResult<Self> {
        if !(1..=1000).contains(&limit) {
            return Err(BlueprintError::new(
                "billing_configuration",
                "Invalid configured list limit",
            ));
        }
        self.max_configured_list_records = limit;
        Ok(self)
    }

    #[must_use]
    pub const fn with_database_backup(mut self, enabled: bool) -> Self {
        self.backup = enabled;
        self
    }
    #[must_use]
    pub const fn with_request_dates(mut self, profile: BillingRequestDateProfile) -> Self {
        self.request_dates = profile;
        self
    }
    /// # Errors
    /// Rejects invalid server-owned webhook secrets.
    pub fn with_webhook_secret(mut self, secret: String) -> BlueprintResult<Self> {
        self.webhook = Some(crate::webhook::StripeWebhookVerifier::new(secret)?);
        Ok(self)
    }
    /// # Errors
    /// Rejects empty or control-containing instance credentials.
    pub fn with_platform_event_key(mut self, secret: String) -> BlueprintResult<Self> {
        if secret.is_empty() || secret.len() > 4096 || secret.chars().any(char::is_control) {
            return Err(BlueprintError::new(
                "billing_configuration",
                "Invalid platform event key",
            ));
        }
        self.platform_key = Some(zeroize::Zeroizing::new(secret));
        Ok(self)
    }
    async fn call(
        &self,
        session: &AuthenticatedSession,
        contract: &RouteContract,
        mut input: Value,
    ) -> BlueprintResult<BillingResponse> {
        self.request_dates.normalize(contract, &mut input)?;
        validate_input(contract, &mut input)?;
        if self.client.is_some()
            && let Some(fields) = input
                .as_object_mut()
                .and_then(|fields| fields.remove("stripeFields"))
        {
            input["providerFields"] = fields;
        }
        if self.client.is_some()
            && matches!(
                contract.operation_name.as_str(),
                "createPlan" | "createSubscription"
            )
        {
            input.as_object_mut().ok_or_else(invalid)?.remove("id");
        }
        let store = self.store.with_audit_context(AuditContext::actor(
            &session.principal.subject,
            uuid::Uuid::new_v4().to_string(),
        ))?;
        let service = BaseBillingService::new(Arc::new(store))
            .with_cache_ttl(Duration::from_secs(600))?
            .with_party_types("user".into(), "organization".into())?
            .with_database_backup(self.backup);
        let mut response = match &self.client {
            Some(client) => {
                execute(
                    &StripeBillingService::new(service, client.clone())
                        .with_configured_subscription_updates(self.configured_subscription_updates)
                        .with_configured_payment_link_updates(self.configured_payment_link_updates)
                        .with_configured_cached_reads(self.configured_cached_reads)
                        .with_max_configured_list_records(self.max_configured_list_records)?,
                    &contract.operation_name,
                    input,
                )
                .await?
            }
            None => execute(&service, &contract.operation_name, input).await?,
        };
        if self.client.is_some()
            && let BillingResponse::Json(value) = &mut response
        {
            stripe_names(value);
        }
        Ok(response)
    }
}
enum BillingResponse {
    Json(Value),
    Text(String),
}
impl IntoResponse for BillingResponse {
    fn into_response(self) -> Response {
        match self {
            Self::Json(value) => Json(value).into_response(),
            Self::Text(value) => value.into_response(),
        }
    }
}
fn stripe_names(value: &mut Value) {
    if let Some(items) = value.as_array_mut() {
        for item in items {
            stripe_names(item);
        }
    } else if let Some(object) = value.as_object_mut()
        && let Some(fields) = object.remove("providerFields")
    {
        object.insert("stripeFields".into(), fields);
    }
}
fn validate_input(contract: &RouteContract, input: &mut Value) -> BlueprintResult<()> {
    let Some(body) = &contract.request.body else {
        return Ok(());
    };
    let schema: Value =
        serde_json::from_str(body.schema.json_schema.as_deref().ok_or_else(invalid)?)
            .map_err(|_| invalid())?;
    let object = input.as_object_mut().ok_or_else(invalid)?;
    if let Some(required) = schema["required"].as_array() {
        for field in required {
            if !object.contains_key(field.as_str().ok_or_else(invalid)?) {
                return Err(invalid());
            }
        }
    }
    let properties = schema["properties"].as_object().ok_or_else(invalid)?;
    // HTTP parsing in TS strips unknown object keys before mapper dispatch.
    object.retain(|field, _| properties.contains_key(field));
    for (field, value) in object {
        let Some(schema) = properties.get(field) else {
            continue;
        };
        if schema["type"] == "object" && !value.is_object() {
            return Err(invalid());
        }
        if schema.get("enum").is_some_and(|values| {
            values
                .as_array()
                .is_none_or(|values| !values.contains(value))
        }) {
            return Err(invalid());
        }
        if schema["format"] == "uuid"
            && value
                .as_str()
                .is_none_or(|value| uuid::Uuid::parse_str(value).is_err())
        {
            return Err(invalid());
        }
        if let Some(values) = schema["items"]["enum"].as_array()
            && value
                .as_array()
                .is_none_or(|items| items.iter().any(|item| !values.contains(item)))
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn invalid() -> BlueprintError {
    BlueprintError::new("billing_validation", "Invalid billing request")
}
fn failure(error: &BlueprintError) -> Response {
    let (status, message) = match error.code.as_str() {
        "billing_date_profile_unsupported" => (
            StatusCode::BAD_REQUEST,
            "Date is outside the configured billing date profile",
        ),
        "billing_not_found" => (StatusCode::NOT_FOUND, "Billing record not found"),
        "billing_validation"
        | "billing_invalid_webhook"
        | "billing_invalid_payment_link_options" => {
            (StatusCode::BAD_REQUEST, "Invalid billing request")
        }
        "billing_conflict" | "billing_event_conflict" => {
            (StatusCode::CONFLICT, "Billing record conflict")
        }
        "billing_webhook_reconciliation_required" => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Billing event requires reconciliation",
        ),
        "billing_webhook_busy" => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Billing event is being processed",
        ),
        "provider_request_failed" | "provider_transport_error" | "stripe_invalid_response" => {
            (StatusCode::BAD_GATEWAY, "Billing provider operation failed")
        }
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Billing operation failed",
        ),
    };
    (status, message).into_response()
}
async fn dispatch(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    contract: RouteContract,
    input: Value,
) -> Response {
    match backend.call(&session, &contract, input).await {
        Ok(result) => result.into_response(),
        Err(error) => failure(&error),
    }
}
async fn dispatch_with_context(
    backend: Arc<BillingBackend>,
    session: AuthenticatedSession,
    contract: RouteContract,
    input: Value,
    context: forklaunch_axum::RequestContext,
) -> Response {
    match backend.call(&session, &contract, input).await {
        Ok(result) => result.into_response(),
        Err(error) => dispatch_failure(&backend, &contract, &error, &context),
    }
}
/// The error response of routes whose configured TypeScript mappers report
/// failures in the TypeScript parser's format.
fn dispatch_failure(
    backend: &BillingBackend,
    contract: &RouteContract,
    error: &BlueprintError,
    context: &forklaunch_axum::RequestContext,
) -> Response {
    let configured_operation = (backend.configured_cached_reads
        && matches!(
            contract.operation_name.as_str(),
            "getCheckoutSession" | "getPaymentLink" | "listPaymentLinks"
        ))
        || (backend.configured_payment_link_updates
            && contract.operation_name == "updatePaymentLink");
    match error {
        error
            if error.code == "billing_configured_mapper_error"
                || (configured_operation && error.code != "billing_validation") =>
        {
            let status =
                if configured_operation && error.code == "billing_configured_provider_error" {
                    error
                        .details
                        .get("status")
                        .and_then(Value::as_u64)
                        .filter(|status| (400..600).contains(status))
                        .and_then(|status| u16::try_from(status).ok())
                        .unwrap_or(500)
                } else {
                    500
                };
            let response = forklaunch_core::http::typescript_parser_error_response(
                status,
                &context.correlation_id,
            );
            (
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                response.body.as_text().unwrap_or_default().to_owned(),
            )
                .into_response()
        }
        error => failure(error),
    }
}
/// One operation's request handler. Its contract supplies the method and path.
pub type Handler = MethodRouter;

/// Thirty core billing operations, provider callbacks, compliance routes and
/// health over one configured backend. Each data read audits the verified
/// request actor and uses the server's configured tenant.
#[derive(Clone)]
pub struct BillingHttp {
    backend: Arc<BillingBackend>,
    auth: Arc<HmacJwksAuth>,
    service: ServiceInfo,
}

impl BillingHttp {
    #[must_use]
    pub fn new(backend: BillingBackend, auth: HmacJwksAuth, service: ServiceInfo) -> Self {
        Self {
            backend: Arc::new(backend),
            auth: Arc::new(auth),
            service,
        }
    }

    /// One core billing operation with its configured contract projections,
    /// the state its generated controller takes.
    ///
    /// # Panics
    /// Panics if `operation` is not a billing operation of this backend.
    #[must_use]
    pub fn operation(&self, operation: &str) -> BillingOperation {
        BillingOperation::new(&self.backend, operation)
    }

    /// Verifies a raw Stripe event against the configured webhook secret, then
    /// records and synchronizes it once.
    pub async fn stripe_webhook(
        &self,
        headers: axum::http::HeaderMap,
        body: axum::body::Bytes,
    ) -> Response {
        webhook::stripe(Arc::clone(&self.backend), headers, body).await
    }

    /// Verifies a managed platform event for `feature` against the instance
    /// key, then records and synchronizes it once.
    pub async fn platform_event(
        &self,
        feature: String,
        headers: axum::http::HeaderMap,
        body: axum::body::Bytes,
    ) -> Response {
        webhook::platform(Arc::clone(&self.backend), feature, headers, body).await
    }

    /// The packaged handler for one core billing, callback, compliance or
    /// health operation.
    ///
    /// Deprecated since 0.2.5: generated services now own their controllers,
    /// which call [`Self::operation`]. It stays available, unchanged, for
    /// services generated earlier.
    ///
    /// # Panics
    /// Panics if `operation` is not a billing operation of this backend.
    pub fn handler(&self, operation: &str) -> Handler {
        if let Some(handler) = self.fixed_handler(operation) {
            return handler;
        }
        let registry = self.registry();
        let mut contract = contract(&registry, operation);
        project(
            &self.backend,
            &registry,
            &mut RouterConfig::default(),
            &mut contract,
        );
        let backend = Arc::clone(&self.backend);
        let handler_contract = contract.clone();
        match contract.method {
            HttpMethod::Post => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      Json(input): Json<Value>| {
                    dispatch(
                        Arc::clone(&backend),
                        session,
                        handler_contract.clone(),
                        input,
                    )
                },
            ),
            HttpMethod::Put if !contract.request.params.is_empty() => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(id): Path<String>,
                      Extension(context): Extension<forklaunch_axum::RequestContext>,
                      parsed: Option<Extension<forklaunch_core::http::ParsedJsonBody>>,
                      Json(mut input): Json<Value>| {
                    let backend = Arc::clone(&backend);
                    let contract = handler_contract.clone();
                    async move {
                        if let Some(Extension(parsed)) = parsed
                            && let Some(value) = parsed.value
                        {
                            input = (*value).clone();
                        }
                        let Some(object) = input.as_object_mut() else {
                            return failure(&invalid());
                        };
                        object.entry("id").or_insert(json!(id));
                        dispatch_with_context(backend, session, contract, input, context).await
                    }
                },
            ),
            HttpMethod::Put => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      Json(input): Json<Value>| {
                    dispatch(
                        Arc::clone(&backend),
                        session,
                        handler_contract.clone(),
                        input,
                    )
                },
            ),
            HttpMethod::Get if contract.request.params.is_empty() => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      RawQuery(query): RawQuery,
                      Extension(context): Extension<forklaunch_axum::RequestContext>| {
                    let backend = Arc::clone(&backend);
                    let contract = handler_contract.clone();
                    async move {
                        match list_input(query.as_deref()) {
                            Ok(input) => dispatch_with_context(backend, session, contract, input, context).await,
                            Err(error) => failure(&error),
                        }
                    }
                },
            ),
            HttpMethod::Get => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(id): Path<String>,
                      Extension(context): Extension<forklaunch_axum::RequestContext>| {
                    dispatch_with_context(
                        Arc::clone(&backend),
                        session,
                        handler_contract.clone(),
                        json!({"id":id}),
                        context,
                    )
                },
            ),
            HttpMethod::Delete => any(
                move |Extension(session): Extension<AuthenticatedSession>,
                      Path(id): Path<String>| {
                    dispatch(
                        Arc::clone(&backend),
                        session,
                        handler_contract.clone(),
                        json!({"id":id}),
                    )
                },
            ),
            _ => panic!("Unsupported package-owned billing HTTP method"),
        }
    }

    /// Health, provider callbacks and the compliance placeholders, which take no billing input.
    fn fixed_handler(&self, operation: &str) -> Option<Handler> {
        let backend = Arc::clone(&self.backend);
        match operation {
            "getHealth" => {
                let name = self.service.name.clone();
                Some(any(move || {
                    let name = name.clone();
                    async move { Json(json!({"service":name,"status":"ok"})) }
                }))
            }
            "handlewebhookevent" => Some(any(
                move |headers: axum::http::HeaderMap, body: axum::body::Bytes| {
                    webhook::stripe(Arc::clone(&backend), headers, body)
                },
            )),
            "receiveplatformevent" => Some(any(
                move |Path(feature): Path<String>,
                      headers: axum::http::HeaderMap,
                      body: axum::body::Bytes| {
                    webhook::platform(Arc::clone(&backend), feature, headers, body)
                },
            )),
            "exportuserdata" => Some(any(|| async {
                (
                    StatusCode::NOT_FOUND,
                    "User not found or no PII data to export",
                )
            })),
            "eraseuserdata" => Some(any(|| async {
                (
                    StatusCode::NOT_FOUND,
                    "User not found or no PII data to erase",
                )
            })),
            _ => None,
        }
    }

    /// Collects handlers for the service's billing routes.
    #[must_use]
    pub fn routes(&self) -> BillingRoutes {
        BillingRoutes {
            http: self.clone(),
            handlers: HashMap::new(),
            #[cfg(feature = "bound-checkout")]
            bound: false,
        }
    }

    /// Every billing operation with its packaged handler, plus the configured
    /// bound capabilities.
    #[must_use]
    pub fn router(&self) -> ForkLaunchRouter {
        let routes = self
            .registry()
            .routes()
            .iter()
            .fold(self.routes(), |routes, route| {
                routes.route(&route.operation_name, self.handler(&route.operation_name))
            });
        #[cfg(feature = "bound-checkout")]
        let routes = routes.bound();
        routes.finish()
    }

    fn registry(&self) -> forklaunch_core::contract::ContractRegistry {
        module_contracts::module_registry(self.backend.client.is_some())
    }
}

/// The service's billing routes. Each operation's contract comes from the
/// billing interface, so routes, SDKs, `OpenAPI` and MCP tools share one declaration.
pub struct BillingRoutes {
    http: BillingHttp,
    handlers: HashMap<String, Handler>,
    #[cfg(feature = "bound-checkout")]
    bound: bool,
}

impl BillingRoutes {
    /// Serve `operation` with `handler`. Operations without a handler are not mounted.
    ///
    /// # Panics
    /// Panics if `operation` is not a billing operation or is already routed.
    #[must_use]
    pub fn route(mut self, operation: &str, handler: Handler) -> Self {
        let _ = contract(&self.http.registry(), operation);
        assert!(
            self.handlers
                .insert(operation.to_owned(), handler)
                .is_none(),
            "billing operation {operation} is already routed"
        );
        self
    }

    /// Mount the bound Checkout, Portal, Plan, Subscription and `PaymentLink`
    /// capabilities that the backend's trusted configuration selected.
    #[cfg(feature = "bound-checkout")]
    #[must_use]
    pub fn bound(mut self) -> Self {
        self.bound = true;
        self
    }

    /// Mount the routed operations in contract order with billing authentication.
    #[must_use]
    pub fn finish(mut self) -> ForkLaunchRouter {
        let backend = Arc::clone(&self.http.backend);
        let registry = self.http.registry();
        let mut config = router_config(&backend, Arc::clone(&self.http.auth));
        let mut router = ForkLaunchRouter::new(self.http.service.clone());
        for route in registry
            .routes()
            .iter()
            .filter(|route| route.operation_name != "getHealth")
        {
            if let Some(handler) = self.handlers.remove(&route.operation_name) {
                let mut contract = route.clone();
                project(&backend, &registry, &mut config, &mut contract);
                router = router.route(contract, handler);
            }
        }
        #[cfg(feature = "bound-checkout")]
        if self.bound {
            router = bound_checkout::register(router, &backend);
            router = bound_portal::register(router, &backend);
            router = bound_portal_expiration::register(router, &backend);
            router = bound_plan_read::register(router, &backend);
            router = bound_subscription_read::register(router, &backend);
            router = bound_payment_link_read::register(router, &backend);
            router = bound_payment_link::register(router, &backend);
            router = bound_payment_link_expiration::register(router, &backend);
            router = bound_plan::register(router, &backend);
            router = bound_subscription::register(router, &backend);
        }
        if let Some(handler) = self.handlers.remove("getHealth") {
            router = router.route(module_contracts::health_contract(), handler);
        }
        router.with_config(config)
    }
}

/// Response formats and request authentication selected by the backend.
fn router_config(backend: &BillingBackend, auth: Arc<HmacJwksAuth>) -> RouterConfig {
    let configured_hmac = backend.configured_hmac;
    let mount_relative = backend
        .mount_relative_hmac
        .or_else(configured_hmac::mount_relative_from_env)
        .unwrap_or(true);
    RouterConfig {
        response_status_error_format: if backend.configured_cached_reads
            || backend.configured_payment_link_updates
        {
            forklaunch_axum::ResponseStatusErrorFormat::TypeScriptText
        } else {
            forklaunch_axum::ResponseStatusErrorFormat::Json
        },
        correlation_id_format: if backend.configured_hmac
            || backend.configured_subscription_updates
            || backend.configured_cached_reads
            || backend.configured_payment_link_updates
        {
            forklaunch_axum::CorrelationIdFormat::TypeScript
        } else {
            forklaunch_axum::CorrelationIdFormat::Native
        },
        ..RouterConfig::default()
    }
    .with_response_authenticator(move |request, contract| {
        let auth = Arc::clone(&auth);
        async move {
            if configured_hmac && configured_hmac::selected(&contract) {
                return configured_hmac::authenticate(&auth, &request, &contract, mount_relative);
            }
            let jwt = matches!(
                contract.auth,
                Some(forklaunch_core::contract::AuthConfig::Jwt(_))
            );
            auth.authenticate(request, contract).await.map_err(|error| {
                if jwt && error == forklaunch_core::auth::AuthError::InvalidCredentials {
                    forklaunch_core::http::ErrorResponse::new(
                        forklaunch_core::http::ErrorKind::Forbidden,
                        "invalid credentials",
                    )
                } else {
                    error.to_error_response()
                }
            })
        }
    })
}

/// Apply the backend's configured HMAC, request date and `PaymentLink` input
/// projections to one contract and the router options they require.
fn project(
    backend: &BillingBackend,
    registry: &forklaunch_core::contract::ContractRegistry,
    config: &mut RouterConfig,
    contract: &mut RouteContract,
) {
    if backend.configured_hmac && configured_hmac::selected(contract) {
        assert!(
            registry
                .routes()
                .iter()
                .filter(|other| other.openapi_path == contract.openapi_path)
                .all(configured_hmac::selected),
            "configured billing slash aliases require a fully selected path"
        );
        *config = std::mem::take(config)
            .with_optional_trailing_slash_for_path(&contract.openapi_path)
            .with_typescript_options_for_path(&contract.openapi_path)
            .with_typescript_case_insensitive_path(&contract.openapi_path);
        project_configured_billing_hmac(contract);
        *config = std::mem::take(config).with_auth_error_format_for_route(
            contract,
            forklaunch_axum::AuthErrorFormat::TypeScriptText,
        );
    }
    backend
        .request_dates
        .project(contract)
        .expect("package-owned billing request schema");
    if backend.configured_payment_link_updates && contract.operation_name == "updatePaymentLink" {
        configured_payment_link_input::register(config, contract);
    }
}

fn contract(
    registry: &forklaunch_core::contract::ContractRegistry,
    operation: &str,
) -> RouteContract {
    registry
        .routes()
        .iter()
        .find(|route| route.operation_name == operation)
        .cloned()
        .unwrap_or_else(|| panic!("billing operation {operation} has no contract"))
}
fn list_input(query: Option<&str>) -> BlueprintResult<Value> {
    let mut input = json!({});
    for (key, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        if key == "ids" {
            if input.get("ids").is_some() {
                return Err(invalid());
            }
            let ids: Vec<String> = serde_json::from_str(&value).map_err(|_| invalid())?;
            input["ids"] = json!(ids);
        }
    }
    Ok(input)
}
#[allow(clippy::too_many_lines)]
async fn execute<
    S: PlanService
        + SubscriptionService
        + CheckoutSessionService
        + PaymentLinkService
        + BillingPortalService,
>(
    service: &S,
    operation: &str,
    input: Value,
) -> BlueprintResult<BillingResponse> {
    let id = input["id"].as_str().unwrap_or("");
    let ids: Option<Vec<String>> = input
        .get("ids")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(|_| invalid())?;
    macro_rules! request {
        ($method:ident) => {
            serde_json::to_value(
                service
                    .$method(serde_json::from_value(input.clone()).map_err(|_| invalid())?)
                    .await?,
            )
            .map(BillingResponse::Json)
            .map_err(|_| invalid())
        };
    }
    macro_rules! get {
        ($method:ident) => {
            serde_json::to_value(service.$method(id).await?)
                .map(BillingResponse::Json)
                .map_err(|_| invalid())
        };
    }
    macro_rules! list {
        ($method:ident) => {
            serde_json::to_value(service.$method(ids.as_deref()).await?)
                .map(BillingResponse::Json)
                .map_err(|_| invalid())
        };
    }
    match operation {
        "createPlan" => request!(create_plan),
        "getPlan" => get!(get_plan),
        "updatePlan" => request!(update_plan),
        "deletePlan" => {
            service.delete_plan(id).await?;
            Ok(BillingResponse::Text(format!("Deleted plan {id}")))
        }
        "listPlans" => list!(list_plans),
        "createSubscription" => request!(create_subscription),
        "getSubscription" => get!(get_subscription),
        "updateSubscription" => request!(update_subscription),
        "deleteSubscription" => {
            service.delete_subscription(id).await?;
            Ok(BillingResponse::Text(format!("Deleted subscription {id}")))
        }
        "listSubscriptions" => list!(list_subscriptions),
        "getUserSubscription" => get!(get_user_subscription),
        "getOrganizationSubscription" => get!(get_organization_subscription),
        "cancelSubscription" => {
            service.cancel_subscription(id).await?;
            Ok(BillingResponse::Text(format!(
                "Cancelled subscription {id}"
            )))
        }
        "resumeSubscription" => {
            service.resume_subscription(id).await?;
            Ok(BillingResponse::Text(format!("Resumed subscription {id}")))
        }
        "createCheckoutSession" => request!(create_checkout_session),
        "getCheckoutSession" => get!(get_checkout_session),
        "expireCheckoutSession" => {
            service.expire_checkout_session(id).await?;
            Ok(BillingResponse::Text(format!(
                "Expired checkout session {id}"
            )))
        }
        "handleCheckoutSuccess" => {
            service.handle_checkout_success(id).await?;
            Ok(BillingResponse::Text(format!(
                "Handled checkout success for session {id}"
            )))
        }
        "handleCheckoutFailure" => {
            service.handle_checkout_failure(id).await?;
            Ok(BillingResponse::Text(format!(
                "Handled checkout failure for session {id}"
            )))
        }
        "createPaymentLink" => request!(create_payment_link),
        "getPaymentLink" => get!(get_payment_link),
        "updatePaymentLink" => request!(update_payment_link),
        "expirePaymentLink" => {
            service.expire_payment_link(id).await?;
            Ok(BillingResponse::Text(format!("Expired payment link {id}")))
        }
        "handlePaymentSuccess" => {
            service.handle_payment_success(id).await?;
            Ok(BillingResponse::Text(format!(
                "Handled payment success for {id}"
            )))
        }
        "handlePaymentFailure" => {
            service.handle_payment_failure(id).await?;
            Ok(BillingResponse::Text(format!(
                "Handled payment failure for {id}"
            )))
        }
        "listPaymentLinks" => list!(list_payment_links),
        "createBillingPortalSession" => request!(create_billing_portal_session),
        "getBillingPortalSession" => get!(get_billing_portal_session),
        "updateBillingPortalSession" => request!(update_billing_portal_session),
        "expireBillingPortalSession" => {
            service.expire_billing_portal_session(id).await?;
            Ok(BillingResponse::Text(format!(
                "Expired billing portal session {id}"
            )))
        }
        _ => Err(invalid()),
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;

    #[tokio::test]
    async fn invalid_payment_link_options_are_sanitized_client_errors() {
        let response = failure(&BlueprintError::new(
            "billing_invalid_payment_link_options",
            "secret provider payload must not escape",
        ));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(body.as_ref(), b"Invalid billing request");
        assert_eq!(
            failure(&BlueprintError::new("billing_unknown", "private")).status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}

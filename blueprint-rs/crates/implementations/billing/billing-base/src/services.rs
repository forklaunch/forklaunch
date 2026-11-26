//! Native asynchronous billing services and durable customer persistence.
use forklaunch_billing_interface::customers::{
    CreateCustomerDto, CustomerDto, CustomerService, UpdateCustomerDto,
};
use forklaunch_billing_interface::services::{
    BillingPortalDto, BillingPortalService, CheckoutSessionDto, CheckoutSessionService,
    CreateBillingPortalDto, CreateCheckoutSessionDto, CreatePaymentLinkDto, CreatePlanDto,
    CreateSubscriptionDto, PaymentLinkDto, PaymentLinkService, PlanDto, PlanService,
    SubscriptionDto, SubscriptionService, UpdateBillingPortalDto, UpdatePaymentLinkDto,
    UpdatePlanDto, UpdateSubscriptionDto,
};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{future::Future, sync::Arc, time::Duration};

mod cached_dates;
pub mod memory;
#[cfg(feature = "postgres")]
pub mod postgres;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BillingKind {
    Customer,
    Plan,
    Subscription,
    CheckoutSession,
    PaymentLink,
    BillingPortal,
}
impl BillingKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Customer => "customer",
            Self::Plan => "plan",
            Self::Subscription => "subscription",
            Self::CheckoutSession => "checkout_session",
            Self::PaymentLink => "payment_link",
            Self::BillingPortal => "billing_portal",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Durable,
    Cache,
}
impl Scope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Durable => "durable",
            Self::Cache => "cache",
        }
    }
}
/// Stores are tenant-bound. Cache expiry is independent of the DTO's provider expiry.
/// Cache failures propagate, matching the TypeScript billing services.
pub trait BillingStore: Send + Sync {
    fn insert(
        &self,
        scope: Scope,
        kind: BillingKind,
        data: Value,
        ttl: Duration,
    ) -> impl Future<Output = BlueprintResult<Value>> + Send;
    fn get(
        &self,
        scope: Scope,
        kind: BillingKind,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<Option<Value>>> + Send;
    /// Read the configured Checkout/`PaymentLink` cache DTO date subset without
    /// rewriting stored data. Generic reads remain unchanged. ISO/epoch values
    /// use JS millisecond truncation within chrono's representable range;
    /// locale/offsetless date strings are explicitly unsupported.
    fn get_configured_cached(
        &self,
        kind: BillingKind,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<Option<Value>>> + Send {
        async move {
            cached_dates::ensure_kind(kind)?;
            self.get(Scope::Cache, kind, id)
                .await?
                .map(|data| cached_dates::normalize(kind, data))
                .transpose()
        }
    }
    /// Configured cache selection. None/empty enumerates cache in store order;
    /// explicit IDs preserve order/duplicates and missing rows fail. Concrete
    /// stores bound key enumeration before decoding. This compatible fallback
    /// must first call a custom store's existing list implementation to enumerate.
    fn list_configured_cached(
        &self,
        ids: Option<&[String]>,
        limit: usize,
    ) -> impl Future<Output = BlueprintResult<Vec<(String, Value)>>> + Send {
        async move {
            cached_dates::selection(ids, limit)?;
            let selected = if let Some(ids) = ids.filter(|ids| !ids.is_empty()) {
                ids.to_vec()
            } else {
                let rows = self
                    .list(Scope::Cache, BillingKind::PaymentLink, None)
                    .await?;
                if rows.len() > limit {
                    return Err(cached_dates::list_invalid());
                }
                rows.iter()
                    .map(|row| {
                        row["id"]
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(cached_dates::list_invalid)
                    })
                    .collect::<BlueprintResult<Vec<_>>>()?
            };
            cached_dates::selection(Some(&selected), limit)?;
            let mut result = Vec::with_capacity(selected.len());
            for id in selected {
                let value = self
                    .get_configured_cached(BillingKind::PaymentLink, &id)
                    .await?
                    .ok_or_else(cached_dates::list_invalid)?;
                result.push((id, value));
            }
            Ok(result)
        }
    }
    fn patch(
        &self,
        scope: Scope,
        kind: BillingKind,
        id: &str,
        patch: Value,
        ttl: Duration,
    ) -> impl Future<Output = BlueprintResult<Value>> + Send;
    fn delete(
        &self,
        scope: Scope,
        kind: BillingKind,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<bool>> + Send;
    fn list(
        &self,
        scope: Scope,
        kind: BillingKind,
        ids: Option<&[String]>,
    ) -> impl Future<Output = BlueprintResult<Vec<Value>>> + Send;
    fn find_active_subscription(
        &self,
        party_id: &str,
        party_type: &str,
    ) -> impl Future<Output = BlueprintResult<Option<Value>>> + Send;
    fn find_subscription_id_by_external_id(
        &self,
        external_id: &str,
    ) -> impl Future<Output = BlueprintResult<Option<String>>> + Send;
}
#[derive(Debug, Clone)]
pub struct BaseBillingService<S> {
    store: Arc<S>,
    cache_ttl: Duration,
    database_backup: bool,
    user_party_type: String,
    organization_party_type: String,
}
impl<S: BillingStore> BaseBillingService<S> {
    #[must_use]
    pub fn new(store: Arc<S>) -> Self {
        Self {
            store,
            cache_ttl: Duration::from_secs(600),
            database_backup: false,
            user_party_type: "USER".into(),
            organization_party_type: "ORGANIZATION".into(),
        }
    }
    /// Configures application party enum values. Defaults retain the generic TS
    /// base service's uppercase values; generated modules use their lowercase enum.
    /// # Errors
    /// Rejects empty, control-containing or oversized enum values.
    pub fn with_party_types(mut self, user: String, organization: String) -> BlueprintResult<Self> {
        for value in [&user, &organization] {
            if value.is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
                return Err(invalid());
            }
        }
        self.user_party_type = user;
        self.organization_party_type = organization;
        Ok(self)
    }
    /// # Errors
    /// Rejects zero and unbounded cache TTLs.
    pub fn with_cache_ttl(mut self, ttl: Duration) -> BlueprintResult<Self> {
        if ttl.as_millis() == 0 || ttl > Duration::from_hours(8760) {
            return Err(invalid());
        }
        self.cache_ttl = ttl;
        Ok(self)
    }
    #[must_use]
    pub const fn database_backup_enabled(&self) -> bool {
        self.database_backup
    }

    #[must_use]
    pub const fn with_database_backup(mut self, enabled: bool) -> Self {
        self.database_backup = enabled;
        self
    }
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }
    async fn create<I: Serialize, O: DeserializeOwned>(
        &self,
        kind: BillingKind,
        request: I,
        cached: bool,
    ) -> BlueprintResult<O> {
        let mut data = serde_json::to_value(request).map_err(|_| invalid())?;
        let object = data.as_object_mut().ok_or_else(invalid)?;
        let id = object
            .entry("id")
            .or_insert_with(|| Value::String(uuid::Uuid::new_v4().to_string()))
            .as_str()
            .ok_or_else(invalid)?;
        validate_id(id)?;
        let now = timestamp();
        object.insert("createdAt".to_owned(), json!(now));
        object.insert("updatedAt".to_owned(), json!(now));
        let output = decode(data.clone())?;
        if !cached || self.database_backup {
            self.store
                .insert(Scope::Durable, kind, data.clone(), self.cache_ttl)
                .await?;
        }
        if cached {
            self.store
                .insert(Scope::Cache, kind, data, self.cache_ttl)
                .await?;
        }
        Ok(output)
    }
    async fn get<O: DeserializeOwned>(
        &self,
        kind: BillingKind,
        id: &str,
        cached: bool,
    ) -> BlueprintResult<O> {
        validate_id(id)?;
        decode(
            self.store
                .get(if cached { Scope::Cache } else { Scope::Durable }, kind, id)
                .await?
                .ok_or_else(not_found)?,
        )
    }
    async fn update<I: Serialize, O: DeserializeOwned>(
        &self,
        kind: BillingKind,
        request: I,
        cached: bool,
    ) -> BlueprintResult<O> {
        let mut patch = serde_json::to_value(request).map_err(|_| invalid())?;
        let id = patch["id"].as_str().ok_or_else(invalid)?.to_owned();
        validate_id(&id)?;
        patch
            .as_object_mut()
            .ok_or_else(invalid)?
            .insert("updatedAt".to_owned(), json!(timestamp()));
        if cached {
            // Check the primary cache before touching an optional database backup.
            let existing = self
                .store
                .get(Scope::Cache, kind, &id)
                .await?
                .ok_or_else(not_found)?;
            decode::<O>(merge(existing, &patch)?)?;
            if self.database_backup {
                self.store
                    .patch(Scope::Durable, kind, &id, patch.clone(), self.cache_ttl)
                    .await?;
            }
        }
        decode(
            self.store
                .patch(
                    if cached { Scope::Cache } else { Scope::Durable },
                    kind,
                    &id,
                    patch,
                    self.cache_ttl,
                )
                .await?,
        )
    }
    async fn list<O: DeserializeOwned>(
        &self,
        kind: BillingKind,
        ids: Option<&[String]>,
        cached: bool,
    ) -> BlueprintResult<Vec<O>> {
        if let Some(ids) = ids {
            for id in ids {
                validate_id(id)?;
            }
        }
        self.store
            .list(
                if cached { Scope::Cache } else { Scope::Durable },
                kind,
                ids,
            )
            .await?
            .into_iter()
            .map(decode)
            .collect()
    }
    async fn expire(&self, kind: BillingKind, id: &str) -> BlueprintResult<()> {
        validate_id(id)?;
        if !self.store.delete(Scope::Cache, kind, id).await? {
            return Err(not_found());
        }
        Ok(())
    }
    async fn finish_checkout(&self, id: &str, status: &str) -> BlueprintResult<()> {
        validate_id(id)?;
        if self.database_backup {
            self.store
                .patch(
                    Scope::Durable,
                    BillingKind::CheckoutSession,
                    id,
                    json!({"status":status,"updatedAt":timestamp()}),
                    self.cache_ttl,
                )
                .await?;
        }
        self.store
            .delete(Scope::Cache, BillingKind::CheckoutSession, id)
            .await?;
        Ok(())
    }
    async fn finish_link(&self, id: &str) -> BlueprintResult<()> {
        validate_id(id)?;
        if self.database_backup
            && !self
                .store
                .delete(Scope::Durable, BillingKind::PaymentLink, id)
                .await?
        {
            return Err(not_found());
        }
        self.store
            .delete(Scope::Cache, BillingKind::PaymentLink, id)
            .await?;
        Ok(())
    }
    async fn subscription_for_party(
        &self,
        id: &str,
        party_type: &str,
    ) -> BlueprintResult<SubscriptionDto> {
        validate_id(id)?;
        decode(
            self.store
                .find_active_subscription(id, party_type)
                .await?
                .ok_or_else(not_found)?,
        )
    }

    async fn set_subscription_active(&self, id: &str, active: bool) -> BlueprintResult<()> {
        validate_id(id)?;
        self.store
            .patch(
                Scope::Durable,
                BillingKind::Subscription,
                id,
                json!({"active":active,"updatedAt":timestamp()}),
                self.cache_ttl,
            )
            .await?;
        Ok(())
    }
}
impl<S: BillingStore> CustomerService for BaseBillingService<S> {
    async fn create_customer(&self, request: CreateCustomerDto) -> BlueprintResult<CustomerDto> {
        self.create(BillingKind::Customer, request, false).await
    }
    async fn get_customer(&self, id: &str) -> BlueprintResult<CustomerDto> {
        self.get(BillingKind::Customer, id, false).await
    }
    async fn update_customer(&self, request: UpdateCustomerDto) -> BlueprintResult<CustomerDto> {
        self.update(BillingKind::Customer, request, false).await
    }
    async fn delete_customer(&self, id: &str) -> BlueprintResult<()> {
        validate_id(id)?;
        self.store
            .delete(Scope::Durable, BillingKind::Customer, id)
            .await?;
        Ok(())
    }
    async fn list_customers(&self, ids: Option<&[String]>) -> BlueprintResult<Vec<CustomerDto>> {
        if ids.is_some_and(<[String]>::is_empty) {
            return Ok(Vec::new());
        }
        self.list(BillingKind::Customer, ids, false).await
    }
}
impl<S: BillingStore> PlanService for BaseBillingService<S> {
    async fn create_plan(&self, request: CreatePlanDto) -> BlueprintResult<PlanDto> {
        self.create(BillingKind::Plan, request, false).await
    }
    async fn get_plan(&self, id: &str) -> BlueprintResult<PlanDto> {
        self.get(BillingKind::Plan, id, false).await
    }
    async fn update_plan(&self, request: UpdatePlanDto) -> BlueprintResult<PlanDto> {
        self.update(BillingKind::Plan, request, false).await
    }
    async fn delete_plan(&self, id: &str) -> BlueprintResult<()> {
        validate_id(id)?;
        self.store
            .delete(Scope::Durable, BillingKind::Plan, id)
            .await?;
        Ok(())
    }
    async fn list_plans(&self, ids: Option<&[String]>) -> BlueprintResult<Vec<PlanDto>> {
        self.list(BillingKind::Plan, ids, false).await
    }
}
impl<S: BillingStore> SubscriptionService for BaseBillingService<S> {
    async fn create_subscription(
        &self,
        request: CreateSubscriptionDto,
    ) -> BlueprintResult<SubscriptionDto> {
        self.create(BillingKind::Subscription, request, false).await
    }
    async fn get_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        self.get(BillingKind::Subscription, id, false).await
    }
    async fn get_user_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        self.subscription_for_party(id, &self.user_party_type).await
    }
    async fn get_organization_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        self.subscription_for_party(id, &self.organization_party_type)
            .await
    }
    async fn find_subscription_id_by_external_id(
        &self,
        external_id: &str,
    ) -> BlueprintResult<Option<String>> {
        validate_id(external_id)?;
        self.store
            .find_subscription_id_by_external_id(external_id)
            .await
    }

    async fn update_subscription(
        &self,
        request: UpdateSubscriptionDto,
    ) -> BlueprintResult<SubscriptionDto> {
        self.update(BillingKind::Subscription, request, false).await
    }
    async fn delete_subscription(&self, id: &str) -> BlueprintResult<()> {
        validate_id(id)?;
        if !self
            .store
            .delete(Scope::Durable, BillingKind::Subscription, id)
            .await?
        {
            return Err(not_found());
        }
        Ok(())
    }
    async fn list_subscriptions(
        &self,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<SubscriptionDto>> {
        self.list(BillingKind::Subscription, ids, false).await
    }
    async fn cancel_subscription(&self, id: &str) -> BlueprintResult<()> {
        self.set_subscription_active(id, false).await
    }
    async fn resume_subscription(&self, id: &str) -> BlueprintResult<()> {
        self.set_subscription_active(id, true).await
    }
}
impl<S: BillingStore> CheckoutSessionService for BaseBillingService<S> {
    async fn create_checkout_session(
        &self,
        request: CreateCheckoutSessionDto,
    ) -> BlueprintResult<CheckoutSessionDto> {
        self.create(BillingKind::CheckoutSession, request, true)
            .await
    }
    async fn get_checkout_session(&self, id: &str) -> BlueprintResult<CheckoutSessionDto> {
        self.get(BillingKind::CheckoutSession, id, true).await
    }
    async fn expire_checkout_session(&self, id: &str) -> BlueprintResult<()> {
        self.expire(BillingKind::CheckoutSession, id).await
    }
    async fn handle_checkout_success(&self, id: &str) -> BlueprintResult<()> {
        self.finish_checkout(id, "SUCCESS").await
    }
    async fn handle_checkout_failure(&self, id: &str) -> BlueprintResult<()> {
        self.finish_checkout(id, "FAILED").await
    }
}
impl<S: BillingStore> PaymentLinkService for BaseBillingService<S> {
    async fn create_payment_link(
        &self,
        request: CreatePaymentLinkDto,
    ) -> BlueprintResult<PaymentLinkDto> {
        self.create(BillingKind::PaymentLink, request, true).await
    }
    async fn update_payment_link(
        &self,
        request: UpdatePaymentLinkDto,
    ) -> BlueprintResult<PaymentLinkDto> {
        self.update(BillingKind::PaymentLink, request, true).await
    }
    async fn get_payment_link(&self, id: &str) -> BlueprintResult<PaymentLinkDto> {
        self.get(BillingKind::PaymentLink, id, true).await
    }
    async fn expire_payment_link(&self, id: &str) -> BlueprintResult<()> {
        self.finish_link(id).await
    }
    async fn handle_payment_success(&self, id: &str) -> BlueprintResult<()> {
        self.finish_link(id).await
    }
    async fn handle_payment_failure(&self, id: &str) -> BlueprintResult<()> {
        self.finish_link(id).await
    }
    async fn list_payment_links(
        &self,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<PaymentLinkDto>> {
        self.list(BillingKind::PaymentLink, ids, true).await
    }
}
impl<S: BillingStore> BillingPortalService for BaseBillingService<S> {
    async fn create_billing_portal_session(
        &self,
        request: CreateBillingPortalDto,
    ) -> BlueprintResult<BillingPortalDto> {
        self.create(BillingKind::BillingPortal, request, true).await
    }
    async fn get_billing_portal_session(&self, id: &str) -> BlueprintResult<BillingPortalDto> {
        self.get(BillingKind::BillingPortal, id, true).await
    }
    async fn update_billing_portal_session(
        &self,
        request: UpdateBillingPortalDto,
    ) -> BlueprintResult<BillingPortalDto> {
        self.update(BillingKind::BillingPortal, request, true).await
    }
    async fn expire_billing_portal_session(&self, id: &str) -> BlueprintResult<()> {
        self.expire(BillingKind::BillingPortal, id).await
    }
}
fn timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn decode<T: DeserializeOwned>(data: Value) -> BlueprintResult<T> {
    serde_json::from_value(data).map_err(|_| invalid())
}
fn invalid() -> BlueprintError {
    BlueprintError::new("billing_validation", "Invalid billing input")
}
fn not_found() -> BlueprintError {
    BlueprintError::new("billing_not_found", "Billing record not found")
}
fn conflict() -> BlueprintError {
    BlueprintError::new("billing_conflict", "Billing record already exists")
}
fn storage_error() -> BlueprintError {
    BlueprintError::new("billing_storage", "Billing storage operation failed")
}
fn validate_id(id: &str) -> BlueprintResult<()> {
    if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
        Err(invalid())
    } else {
        Ok(())
    }
}
fn merge(mut data: Value, patch: &Value) -> BlueprintResult<Value> {
    let object = data.as_object_mut().ok_or_else(invalid)?;
    for (key, value) in patch.as_object().ok_or_else(invalid)? {
        object.insert(key.clone(), value.clone());
    }
    Ok(data)
}

fn validate_record(kind: BillingKind, data: &Value) -> BlueprintResult<()> {
    validate_id(data["id"].as_str().ok_or_else(invalid)?)?;
    match kind {
        BillingKind::Customer => {
            let customer: CustomerDto = decode(data.clone())?;
            if customer.email.trim().is_empty()
                || customer.email.len() > 320
                || customer.email.chars().any(char::is_control)
                || customer
                    .name
                    .as_ref()
                    .is_some_and(|name| name.len() > 1024 || name.chars().any(char::is_control))
            {
                return Err(invalid());
            }
        }
        BillingKind::Plan => {
            decode::<PlanDto>(data.clone())?;
        }
        BillingKind::Subscription => {
            decode::<SubscriptionDto>(data.clone())?;
        }
        BillingKind::CheckoutSession => {
            decode::<CheckoutSessionDto>(data.clone())?;
        }
        BillingKind::PaymentLink => {
            decode::<PaymentLinkDto>(data.clone())?;
        }
        BillingKind::BillingPortal => {
            decode::<BillingPortalDto>(data.clone())?;
        }
    }
    Ok(())
}

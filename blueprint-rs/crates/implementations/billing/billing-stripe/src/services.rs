//! Stripe provider orchestration over tenant-bound billing persistence.
//! `provider_fields` carries Stripe request fields on input and provider objects
//! on output. Generated Stripe contracts must expose the TS `stripeFields` name.
//! Provider mutations precede local persistence, matching the TS wrappers. A
//! failed local write can require reconciliation; external calls are not retried.
//! Payment-link and checkout mutations read the record first and call Stripe
//! with its stored Stripe id (`plink_`/`cs_`); public ids stay local.
use crate::client::{StripeBillingClient, StripeResource};
use chrono::{Duration, Utc};
use forklaunch_billing_base::services::{BaseBillingService, BillingStore};
use forklaunch_billing_interface::services::{
    BillingPortalDto, BillingPortalService, CheckoutSessionDto, CheckoutSessionService,
    CreateBillingPortalDto, CreateCheckoutSessionDto, CreatePaymentLinkDto, CreatePlanDto,
    CreateSubscriptionDto, PaymentLinkDto, PaymentLinkService, PlanDto, PlanService,
    SubscriptionDto, SubscriptionService, UpdateBillingPortalDto, UpdatePaymentLinkDto,
    UpdatePlanDto, UpdateSubscriptionDto,
};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Map, Value, json};

fn validate_configured_subscription_update(
    request: &UpdateSubscriptionDto,
    original: &SubscriptionDto,
) -> BlueprintResult<()> {
    external_id(&original.external_id)?;
    let blank = |value: &str| {
        value
            .trim_matches(|c| matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'))
            .is_empty()
    };
    if original.id != request.id
        || blank(&original.id)
        || blank(&original.party_id)
        || request
            .party_id
            .as_ref()
            .is_some_and(|id| id != &original.party_id)
        || request
            .party_type
            .as_ref()
            .is_some_and(|kind| kind != &original.party_type)
    {
        return Err(BlueprintError::new(
            "billing_configured_mapper_error",
            "Subscription ownership cannot change during an update",
        ));
    }
    if request
        .billing_provider
        .as_deref()
        .is_some_and(|value| value != "stripe")
        || request
            .party_type
            .as_deref()
            .is_some_and(|value| !matches!(value, "user" | "organization"))
    {
        return Err(BlueprintError::new(
            "billing_configured_mapper_error",
            "Invalid subscription update",
        ));
    }

    Ok(())
}

pub struct StripeBillingService<S> {
    base: BaseBillingService<S>,
    client: StripeBillingClient,
    configured_subscription_updates: bool,
    configured_payment_link_updates: bool,
    configured_cached_reads: bool,
    pub(crate) max_configured_list_records: usize,
}
impl<S: BillingStore> StripeBillingService<S> {
    #[must_use]
    pub const fn new(base: BaseBillingService<S>, client: StripeBillingClient) -> Self {
        Self {
            base,
            client,
            configured_subscription_updates: false,
            configured_payment_link_updates: false,
            configured_cached_reads: false,
            max_configured_list_records: 100,
        }
    }
    /// Applies the generated application's configured Subscription mapper owner preflight.
    /// Generic/custom service behavior remains unchanged unless explicitly selected.
    #[must_use]
    pub const fn with_configured_subscription_updates(mut self, enabled: bool) -> Self {
        self.configured_subscription_updates = enabled;
        self
    }
    /// Selects the configured application's `PaymentLink` cache/patch update preflight.
    /// This never resolves a retained provider reference into mutation authority.
    #[must_use]
    pub const fn with_configured_payment_link_updates(mut self, enabled: bool) -> Self {
        self.configured_payment_link_updates = enabled;
        self
    }
    /// Selects the configured application's cache-first Checkout GET and `PaymentLink` GET/list mappers.
    /// Cached references are read targets, not provider account or mutation authority.
    #[must_use]
    pub const fn with_configured_cached_reads(mut self, enabled: bool) -> Self {
        self.configured_cached_reads = enabled;
        self
    }
    /// Trusted configured-list bound, independent of caller DTO fields.
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
    pub const fn base(&self) -> &BaseBillingService<S> {
        &self.base
    }
    #[must_use]
    pub const fn client(&self) -> &StripeBillingClient {
        &self.client
    }
    async fn expanded_plan(&self, id: &str) -> BlueprintResult<Value> {
        self.client
            .retrieve(
                StripeResource::Plan,
                external_id(id)?,
                &json!({"expand":["product"]}),
            )
            .await
    }
    async fn read_plan(&self, id: &str) -> BlueprintResult<Value> {
        let provider = self.expanded_plan(id).await?;
        validate_read_identity(&provider, id, "plan")?;
        Ok(provider)
    }
    async fn enrich_subscription(
        &self,
        mut value: SubscriptionDto,
    ) -> BlueprintResult<SubscriptionDto> {
        let provider = self
            .client
            .retrieve(
                StripeResource::Subscription,
                external_id(&value.external_id)?,
                &json!({}),
            )
            .await?;
        validate_read_identity(&provider, &value.external_id, "subscription")?;
        value.provider_fields = Some(provider);
        Ok(value)
    }
    /// The Stripe id (`plink_...`) stored with a payment link at creation.
    async fn stored_payment_link_id(&self, id: &str) -> BlueprintResult<String> {
        let record = self.base.get_payment_link(id).await?;
        stored_stripe_id(&record.id, record.provider_fields.as_ref(), "plink_")
    }
    /// The Stripe id (`cs_...`) stored with a checkout session at creation.
    async fn stored_checkout_session_id(&self, id: &str) -> BlueprintResult<String> {
        let record = self.base.get_checkout_session(id).await?;
        stored_stripe_id(&record.id, record.provider_fields.as_ref(), "cs_")
    }
    async fn link_status(&self, id: &str, status: &str) -> BlueprintResult<()> {
        let stripe_id = self.stored_payment_link_id(id).await?;
        self.client
            .update(
                StripeResource::PaymentLink,
                &stripe_id,
                &json!({"metadata":{"status":status}}),
            )
            .await?;
        Ok(())
    }
    async fn checkout_status(&self, id: &str, status: &str) -> BlueprintResult<()> {
        let stripe_id = self.stored_checkout_session_id(id).await?;
        self.client
            .update(
                StripeResource::CheckoutSession,
                &stripe_id,
                &json!({"metadata":{"status":status}}),
            )
            .await?;
        Ok(())
    }
}
impl<S: BillingStore> PlanService for StripeBillingService<S> {
    async fn create_plan(&self, mut request: CreatePlanDto) -> BlueprintResult<PlanDto> {
        let fields = merge_fields(
            request.provider_fields.take(),
            json!({"amount":request.price,"interval":request.cadence,"product":request.name,"currency":request.currency}),
        )?;
        let created = self.client.create(StripeResource::Plan, &fields).await?;
        validate_creation_shape(&created, "plan")?;
        let created_id = provider_id(&created)?;
        let plan = self.expanded_plan(created_id).await?;
        validate_read_identity(&plan, created_id, "plan")?;
        provider_id(&plan)?.clone_into(&mut request.external_id);
        request.billing_provider = "stripe".into();
        request.provider_fields = Some(product(&plan)?);
        self.base.create_plan(request).await
    }
    async fn get_plan(&self, id: &str) -> BlueprintResult<PlanDto> {
        let mut value = self.base.get_plan(id).await?;
        value.provider_fields = Some(product(&self.read_plan(&value.external_id).await?)?);
        Ok(value)
    }
    async fn update_plan(&self, mut request: UpdatePlanDto) -> BlueprintResult<PlanDto> {
        let original = self.base.get_plan(&request.id).await?;
        let existing = self
            .client
            .retrieve(
                StripeResource::Plan,
                external_id(&original.external_id)?,
                &json!({}),
            )
            .await?;
        validate_read_identity(&existing, &original.external_id, "plan")?;
        let existing_product = product(&existing)?;
        let product_id = existing_product
            .as_str()
            .or_else(|| existing_product["id"].as_str())
            .ok_or_else(invalid_response)?;
        validate_provider_reference(product_id)?;
        let mut overrides = json!({"product":product_id,"interval":request.cadence.as_ref().map_or_else(||existing["interval"].clone(),|v|json!(v)),"currency":request.currency.as_ref().map_or_else(||existing["currency"].clone(),|v|json!(v))});
        if let Some(amount) = request
            .price
            .map(|v| json!(v))
            .or_else(|| existing.get("amount").filter(|v| !v.is_null()).cloned())
        {
            overrides["amount"] = amount;
        }
        let mut fields = merge_fields(request.provider_fields.take(), overrides.clone())?;
        if overrides.get("amount").is_none() {
            fields
                .as_object_mut()
                .ok_or_else(invalid_response)?
                .remove("amount");
        }
        self.client
            .delete(StripeResource::Plan, &original.external_id)
            .await?;
        let created = self.client.create(StripeResource::Plan, &fields).await?;
        validate_creation_shape(&created, "plan")?;
        let created_id = provider_id(&created)?;
        let plan = self.expanded_plan(created_id).await?;
        validate_read_identity(&plan, created_id, "plan")?;
        request.external_id = Some(provider_id(&plan)?.to_owned());
        request.billing_provider = Some("stripe".into());
        request.provider_fields = Some(product(&plan)?);
        self.base.update_plan(request).await
    }
    async fn delete_plan(&self, id: &str) -> BlueprintResult<()> {
        let value = self.base.get_plan(id).await?;
        self.client
            .delete(StripeResource::Plan, external_id(&value.external_id)?)
            .await?;
        self.base.delete_plan(id).await
    }
    async fn list_plans(&self, ids: Option<&[String]>) -> BlueprintResult<Vec<PlanDto>> {
        let mut result = Vec::new();
        for mut value in self.base.list_plans(ids).await? {
            if let Ok(provider) = self
                .read_plan(&value.external_id)
                .await
                .and_then(|v| product(&v))
            {
                value.provider_fields = Some(provider);
                result.push(value);
            }
        }
        Ok(result)
    }
}
impl<S: BillingStore> SubscriptionService for StripeBillingService<S> {
    async fn create_subscription(
        &self,
        mut request: CreateSubscriptionDto,
    ) -> BlueprintResult<SubscriptionDto> {
        let fields = merge_fields(
            request.provider_fields.take(),
            json!({"customer":request.party_id,"items":[{"plan":request.product_id}]}),
        )?;
        let provider = self
            .client
            .create(StripeResource::Subscription, &fields)
            .await?;
        validate_creation_shape(&provider, "subscription")?;
        provider_id(&provider)?.clone_into(&mut request.external_id);
        request.billing_provider = "stripe".into();
        request.provider_fields = Some(provider);
        self.base.create_subscription(request).await
    }
    async fn get_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        self.enrich_subscription(self.base.get_subscription(id).await?)
            .await
    }
    async fn get_user_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        self.enrich_subscription(self.base.get_user_subscription(id).await?)
            .await
    }
    async fn get_organization_subscription(&self, id: &str) -> BlueprintResult<SubscriptionDto> {
        self.enrich_subscription(self.base.get_organization_subscription(id).await?)
            .await
    }
    async fn find_subscription_id_by_external_id(
        &self,
        id: &str,
    ) -> BlueprintResult<Option<String>> {
        self.base.find_subscription_id_by_external_id(id).await
    }
    async fn update_subscription(
        &self,
        mut request: UpdateSubscriptionDto,
    ) -> BlueprintResult<SubscriptionDto> {
        let original = self.base.get_subscription(&request.id).await?;
        // Match the configured TS mapper before any provider read or mutation.
        // This does not establish provider ownership or resolve public aliases.
        if self.configured_subscription_updates {
            validate_configured_subscription_update(&request, &original)?;
        }
        let existing = self
            .client
            .retrieve(
                StripeResource::Subscription,
                external_id(&original.external_id)?,
                &json!({}),
            )
            .await?;
        validate_read_identity(&existing, &original.external_id, "subscription")?;
        let items = existing["items"]["data"]
            .as_array()
            .ok_or_else(invalid_response)?
            .iter()
            .map(|item| {
                let item_id = item["id"].as_str().ok_or_else(invalid_response)?;
                validate_provider_reference(item_id)?;
                let mut value = json!({"id":item_id});
                if let Some(plan) = request
                    .product_id
                    .as_ref()
                    .filter(|v| !v.is_empty())
                    .map(|v| json!(v))
                    .or_else(|| item.get("plan").and_then(|v| v.get("id")).cloned())
                {
                    // Only validate a provider fallback actually consumed by this update.
                    if request.product_id.as_ref().is_none_or(String::is_empty) {
                        validate_provider_reference(plan.as_str().ok_or_else(invalid_response)?)?;
                    }
                    value["plan"] = plan;
                }
                Ok(value)
            })
            .collect::<BlueprintResult<Vec<_>>>()?;
        let fields = merge_fields(request.provider_fields.take(), json!({"items":items}))?;
        let provider = self
            .client
            .update(StripeResource::Subscription, &original.external_id, &fields)
            .await?;
        validate_read_identity(&provider, &original.external_id, "subscription")?;
        request.external_id = Some(provider_id(&provider)?.into());
        request.billing_provider = Some("stripe".into());
        request.provider_fields = Some(provider);
        self.base.update_subscription(request).await
    }
    async fn delete_subscription(&self, id: &str) -> BlueprintResult<()> {
        let value = self.base.get_subscription(id).await?;
        self.client
            .delete(
                StripeResource::Subscription,
                external_id(&value.external_id)?,
            )
            .await?;
        self.base.delete_subscription(id).await
    }
    async fn list_subscriptions(
        &self,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<SubscriptionDto>> {
        let mut result = Vec::new();
        for value in self.base.list_subscriptions(ids).await? {
            if let Ok(value) = self.enrich_subscription(value).await {
                result.push(value);
            }
        }
        Ok(result)
    }
    async fn cancel_subscription(&self, id: &str) -> BlueprintResult<()> {
        let value = self.base.get_subscription(id).await?;
        self.client
            .delete(
                StripeResource::Subscription,
                external_id(&value.external_id)?,
            )
            .await?;
        self.base.cancel_subscription(id).await
    }
    async fn resume_subscription(&self, id: &str) -> BlueprintResult<()> {
        let value = self.base.get_subscription(id).await?;
        self.client
            .action(
                StripeResource::Subscription,
                external_id(&value.external_id)?,
                "resume",
                &json!({}),
            )
            .await?;
        self.base.resume_subscription(id).await
    }
}
impl<S: BillingStore> CheckoutSessionService for StripeBillingService<S> {
    async fn create_checkout_session(
        &self,
        mut request: CreateCheckoutSessionDto,
    ) -> BlueprintResult<CheckoutSessionDto> {
        let mut overrides = json!({"mode":"subscription","payment_method_types":request.payment_methods,"currency":request.currency});
        if let Some(value) = &request.success_redirect_uri {
            overrides["success_url"] = json!(value);
        }
        if let Some(value) = &request.cancel_redirect_uri {
            overrides["cancel_url"] = json!(value);
        }
        let mut fields = merge_fields(request.provider_fields.take(), overrides)?;
        for (key, missing) in [
            ("success_url", request.success_redirect_uri.is_none()),
            ("cancel_url", request.cancel_redirect_uri.is_none()),
        ] {
            if missing {
                fields
                    .as_object_mut()
                    .ok_or_else(invalid_response)?
                    .remove(key);
            }
        }
        let provider = self
            .client
            .create(StripeResource::CheckoutSession, &fields)
            .await?;
        validate_creation_shape(&provider, "checkout.session")?;
        request.id = Some(provider_id(&provider)?.into());
        request.uri = provider_uri(&provider)?;
        request.expires_at = Utc::now() + Duration::minutes(5);
        request.provider_fields = Some(provider);
        self.base.create_checkout_session(request).await
    }
    async fn get_checkout_session(&self, id: &str) -> BlueprintResult<CheckoutSessionDto> {
        if self.configured_cached_reads {
            return self.configured_get_checkout_session(id).await;
        }
        let provider = self
            .client
            .retrieve(StripeResource::CheckoutSession, id, &json!({}))
            .await?;
        let mut value = self.base.get_checkout_session(id).await?;
        value.provider_fields = Some(provider);
        Ok(value)
    }
    async fn expire_checkout_session(&self, id: &str) -> BlueprintResult<()> {
        let stripe_id = self.stored_checkout_session_id(id).await?;
        self.client
            .action(
                StripeResource::CheckoutSession,
                &stripe_id,
                "expire",
                &json!({}),
            )
            .await?;
        self.base.expire_checkout_session(id).await
    }
    async fn handle_checkout_success(&self, id: &str) -> BlueprintResult<()> {
        self.checkout_status(id, "SUCCESS").await?;
        self.base.handle_checkout_success(id).await
    }
    async fn handle_checkout_failure(&self, id: &str) -> BlueprintResult<()> {
        self.checkout_status(id, "FAILED").await?;
        self.base.handle_checkout_failure(id).await
    }
}
impl<S: BillingStore> PaymentLinkService for StripeBillingService<S> {
    async fn create_payment_link(
        &self,
        mut request: CreatePaymentLinkDto,
    ) -> BlueprintResult<PaymentLinkDto> {
        if !request
            .provider_fields
            .as_ref()
            .and_then(Value::as_object)
            .and_then(|fields| fields.get("line_items"))
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().all(Value::is_object))
        {
            return Err(BlueprintError::new(
                "billing_invalid_payment_link_options",
                "Payment link provider options require a line_items array of objects",
            ));
        }
        let fields = merge_fields(
            request.provider_fields.take(),
            json!({"payment_method_types":request.payment_methods,"currency":request.currency}),
        )?;
        let provider = self
            .client
            .create(StripeResource::PaymentLink, &fields)
            .await?;
        validate_creation_shape(&provider, "payment_link")?;
        request.id = Some(provider_id(&provider)?.into());
        request.provider_fields = Some(provider);
        self.base.create_payment_link(request).await
    }
    async fn update_payment_link(
        &self,
        mut request: UpdatePaymentLinkDto,
    ) -> BlueprintResult<PaymentLinkDto> {
        let record = if self.configured_payment_link_updates {
            self.configured_preflight_payment_link_update(&request)
                .await?
        } else {
            self.base.get_payment_link(&request.id).await?
        };
        let stripe_id = stored_stripe_id(&record.id, record.provider_fields.as_ref(), "plink_")?;
        let mut overrides = json!({});
        if let Some(value) = &request.payment_methods {
            overrides["payment_method_types"] = json!(value);
        }
        let mut fields = merge_fields(request.provider_fields.take(), overrides)?;
        if request.payment_methods.is_none() {
            fields
                .as_object_mut()
                .ok_or_else(invalid_response)?
                .remove("payment_method_types");
        }
        let provider = if self.configured_payment_link_updates {
            self.client
                .update_configured(StripeResource::PaymentLink, &stripe_id, &fields)
                .await?
        } else {
            self.client
                .update(StripeResource::PaymentLink, &stripe_id, &fields)
                .await?
        };
        validate_read_identity(&provider, &stripe_id, "payment_link")?;
        request.amount = Some(line_amount(&provider)?);
        request.provider_fields = Some(provider);
        self.base.update_payment_link(request).await
    }
    async fn get_payment_link(&self, id: &str) -> BlueprintResult<PaymentLinkDto> {
        if self.configured_cached_reads {
            return self.configured_get_payment_link(id).await;
        }
        let provider = self
            .client
            .retrieve(StripeResource::PaymentLink, id, &json!({}))
            .await?;
        let mut value = self.base.get_payment_link(id).await?;
        value.provider_fields = Some(provider);
        Ok(value)
    }
    async fn expire_payment_link(&self, id: &str) -> BlueprintResult<()> {
        self.link_status(id, "EXPIRED").await?;
        self.base.expire_payment_link(id).await
    }
    async fn handle_payment_success(&self, id: &str) -> BlueprintResult<()> {
        self.link_status(id, "COMPLETED").await?;
        self.base.handle_payment_success(id).await
    }
    async fn handle_payment_failure(&self, id: &str) -> BlueprintResult<()> {
        self.link_status(id, "FAILED").await?;
        self.base.handle_payment_failure(id).await
    }
    async fn list_payment_links(
        &self,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<PaymentLinkDto>> {
        if self.configured_cached_reads {
            return self.configured_list_payment_links(ids).await;
        }
        let provider = self
            .client
            .list(StripeResource::PaymentLink, &json!({"active":true}))
            .await?;
        let data = provider["data"].as_array().ok_or_else(invalid_response)?;
        let mut result = Vec::new();
        for mut value in self.base.list_payment_links(ids).await? {
            if let Some(provider) = data.iter().find(|v| v["id"].as_str() == Some(&value.id)) {
                value.provider_fields = Some(provider.clone());
                result.push(value);
            }
        }
        Ok(result)
    }
}
impl<S: BillingStore> BillingPortalService for StripeBillingService<S> {
    async fn create_billing_portal_session(
        &self,
        mut request: CreateBillingPortalDto,
    ) -> BlueprintResult<BillingPortalDto> {
        let fields = merge_fields(
            request.provider_fields.take(),
            json!({"customer":request.customer_id}),
        )?;
        let provider = self
            .client
            .create(StripeResource::BillingPortal, &fields)
            .await?;
        validate_creation_shape(&provider, "billing_portal.session")?;
        request.id = Some(provider_id(&provider)?.into());
        request.uri = Some(portal_uri(&provider)?);
        request.expires_at = Utc::now() + Duration::minutes(5);
        request.provider_fields = Some(provider);
        self.base.create_billing_portal_session(request).await
    }
    async fn get_billing_portal_session(&self, id: &str) -> BlueprintResult<BillingPortalDto> {
        self.base.get_billing_portal_session(id).await
    }
    async fn update_billing_portal_session(
        &self,
        mut request: UpdateBillingPortalDto,
    ) -> BlueprintResult<BillingPortalDto> {
        let original = self.base.get_billing_portal_session(&request.id).await?;
        let customer = request
            .customer_id
            .as_ref()
            .filter(|v| !v.is_empty())
            .unwrap_or(&original.customer_id);
        let fields = merge_fields(request.provider_fields.take(), json!({"customer":customer}))?;
        let provider = self
            .client
            .create(StripeResource::BillingPortal, &fields)
            .await?;
        validate_creation_shape(&provider, "billing_portal.session")?;
        request.uri = Some(portal_uri(&provider)?);
        request.expires_at = Some(Utc::now() + Duration::minutes(5));
        request.provider_fields = Some(provider);
        self.base.update_billing_portal_session(request).await
    }
    async fn expire_billing_portal_session(&self, id: &str) -> BlueprintResult<()> {
        self.base.expire_billing_portal_session(id).await
    }
}
fn merge_fields(fields: Option<Value>, overrides: Value) -> BlueprintResult<Value> {
    let mut fields = match fields {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(fields)) => fields,
        _ => {
            return Err(BlueprintError::new(
                "billing_validation",
                "Invalid Stripe billing fields",
            ));
        }
    };
    let Value::Object(overrides) = overrides else {
        return Err(invalid_response());
    };
    fields.extend(overrides);
    Ok(Value::Object(fields))
}
fn provider_id(provider: &Value) -> BlueprintResult<&str> {
    provider["id"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(invalid_response)
}
fn product(provider: &Value) -> BlueprintResult<Value> {
    provider
        .get("product")
        .filter(|v| v.is_object() || v.is_string())
        .cloned()
        .ok_or_else(invalid_response)
}
fn portal_uri(provider: &Value) -> BlueprintResult<String> {
    provider["url"]
        .as_str()
        .filter(|value| {
            !value
                .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
                .is_empty()
        })
        .map(str::to_owned)
        .ok_or_else(invalid_response)
}
fn provider_uri(provider: &Value) -> BlueprintResult<Option<String>> {
    match provider.get("url") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(invalid_response()),
    }
}
fn external_id(id: &str) -> BlueprintResult<&str> {
    if id.is_empty() {
        Err(BlueprintError::new(
            "billing_not_found",
            "Billing provider reference not found",
        ))
    } else {
        Ok(id)
    }
}
fn line_amount(provider: &Value) -> BlueprintResult<f64> {
    let Some(data) = provider.get("line_items").and_then(|v| v.get("data")) else {
        return Ok(0.0);
    };
    let total = data
        .as_array()
        .ok_or_else(invalid_response)?
        .iter()
        .try_fold(0.0, |total, item| {
            item["amount_total"]
                .as_f64()
                .map(|amount| total + amount)
                .ok_or_else(invalid_response)
        })?;
    if total.is_finite() {
        Ok(total)
    } else {
        Err(invalid_response())
    }
}
/// Provider mutations target the Stripe id stored with the record, never a
/// local public id. The stored provider object's id wins; a record without one
/// is still usable when its own id is a Stripe id (the default service stores
/// records under their Stripe id). Anything else fails before any Stripe call.
fn stored_stripe_id(
    record_id: &str,
    provider_fields: Option<&Value>,
    prefix: &str,
) -> BlueprintResult<String> {
    let stripe_shaped = |id: &&str| {
        id.strip_prefix(prefix).is_some_and(|rest| {
            !rest.is_empty() && rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
    };
    match provider_fields.and_then(|fields| fields.get("id")) {
        Some(stored) => stored.as_str().filter(stripe_shaped),
        None => Some(record_id).filter(stripe_shaped),
    }
    .map(str::to_owned)
    .ok_or_else(|| {
        BlueprintError::new(
            "billing_provider_reference_missing",
            "Billing record has no stored Stripe id",
        )
    })
}
fn invalid_response() -> BlueprintError {
    BlueprintError::new("stripe_invalid_response", "Invalid Stripe billing response")
}

fn validate_read_identity(provider: &Value, expected: &str, kind: &str) -> BlueprintResult<()> {
    if provider["id"].as_str() != Some(expected) || provider["object"].as_str() != Some(kind) {
        return Err(invalid_response());
    }
    Ok(())
}

fn validate_provider_reference(id: &str) -> BlueprintResult<()> {
    if id.len() > 1024
        || id.chars().any(char::is_control)
        || id
            .trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
            .is_empty()
    {
        return Err(invalid_response());
    }
    Ok(())
}

fn validate_creation_shape(provider: &Value, kind: &str) -> BlueprintResult<()> {
    validate_provider_reference(provider_id(provider)?)?;
    if provider["object"].as_str() != Some(kind) {
        return Err(invalid_response());
    }
    Ok(())
}

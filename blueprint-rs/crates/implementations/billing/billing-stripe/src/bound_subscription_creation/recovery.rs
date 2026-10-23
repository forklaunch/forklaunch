//! Exact request-key linkage, not proof of process origin or payment success.
use super::{
    BlueprintResult, BoundSubscriptionCreateRequest, BoundSubscriptionCreationOutcome,
    BoundSubscriptionCreationProfile, Method, PostgresBillingStore, StripeBillingService,
    SubscriptionCreationStatus, TrustedSubscriptionPublisher, Uuid, Value, completed, invalid,
    json, unresolved, validation,
};
use forklaunch_billing_base::services::postgres::SubscriptionCreationRecovery;

fn seconds(value: &Value) -> BlueprintResult<i64> {
    let number = value
        .as_f64()
        .filter(|n| n.is_finite() && *n >= 0.0 && *n <= 253_402_300_799.0 && n.fract() == 0.0)
        .ok_or_else(invalid)?;
    format!("{number:.0}").parse().map_err(|_| invalid())
}
fn event_identity(
    value: &Value,
    id: &str,
    recovery: &SubscriptionCreationRecovery,
    now: i64,
) -> BlueprintResult<()> {
    let created = seconds(&value["created"])?;
    let object_created = seconds(&value["data"]["object"]["created"])?;
    let recorded = recovery.recorded_at_seconds();
    if value["id"] != id
        || value["object"] != "event"
        || value["type"] != "customer.subscription.created"
        || value["livemode"].as_bool() != Some(recovery.scope().livemode())
        || value["api_version"] != recovery.api_version()
        || value
            .get("account")
            .is_some_and(|v| !v.is_null() && v.as_str() != Some(recovery.scope().account_id()))
        || value.get("context").is_some_and(|v| !v.is_null())
        || value["request"]["id"]
            .as_str()
            .is_none_or(|v| !crate::bound_plan_creation::safe_reference(v))
        || value["request"]["idempotency_key"]
            .as_str()
            .is_none_or(|v| !recovery.matches_event_request_key(v))
        || object_created < recorded.checked_sub(5).ok_or_else(invalid)?
        || created < object_created
        || created > recorded.checked_add(300).ok_or_else(invalid)?
        || created > now.checked_add(5).ok_or_else(invalid)?
    {
        return Err(invalid());
    }
    Ok(())
}
fn request(
    profile: &BoundSubscriptionCreationProfile,
    recovery: &SubscriptionCreationRecovery,
) -> BlueprintResult<BoundSubscriptionCreateRequest> {
    let payload = recovery.payload();
    let plans: Vec<_> = profile
        .plans
        .iter()
        .map(|(id, op)| json!([id.to_string(), op.to_string()]))
        .collect();
    if recovery.scope() != &profile.scope
        || payload["trustedPlans"] != json!(plans)
        || payload["collectionPolicy"] != "automatic_default_incomplete_v1"
    {
        return Err(invalid());
    }
    let parse = |field: &str| {
        Uuid::parse_str(payload["request"][field].as_str().ok_or_else(invalid)?)
            .map_err(|_| invalid())
    };
    let description = match &payload["request"]["description"] {
        Value::Null => None,
        Value::String(v) => Some(v.clone()),
        _ => return Err(invalid()),
    };
    Ok(BoundSubscriptionCreateRequest {
        local_customer_id: parse("localCustomerId")?,
        local_plan_id: parse("localPlanId")?,
        quantity: payload["request"]["quantity"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
            .ok_or_else(invalid)?,
        description,
    })
}
impl StripeBillingService<PostgresBillingStore> {
    /// Preflights direct recovery and separate publication roles, without provider I/O.
    /// # Errors
    /// Rejects unsupported transport, missing recovery grants or publisher scope mismatch.
    pub async fn validate_bound_subscription_recovery_configuration(
        &self,
        publisher: &TrustedSubscriptionPublisher,
    ) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_subscription_recovery_access()
            .await?;
        publisher
            .validate_creation_access(self.base().store())
            .await
    }

    /// Read-only provider recovery of an immutable operation using a creation
    /// event with the exact server-generated request key. This does not prove
    /// process origin. The -5s/+5min reservation window and exact event rendering
    /// version are a deliberately restricted policy, not Stripe guarantees.
    /// No dispatch ticket/key is reconstructed; no provider POST/retry occurs.
    /// # Errors
    /// Rejects unsupported profile/scope/event evidence or local authority changes.
    /// Post-publication uncertainty remains inspectable by operation UUID.
    #[allow(clippy::too_many_lines)] // Keep the GET-only proof and publication order visible.
    pub async fn recover_bound_subscription(
        &self,
        profile: &BoundSubscriptionCreationProfile,
        publisher: &TrustedSubscriptionPublisher,
        operation: Uuid,
        event_id: &str,
    ) -> BlueprintResult<BoundSubscriptionCreationOutcome> {
        let version = self.client().direct_creation_version()?;
        if !event_id.starts_with("evt_") || !crate::bound_plan_creation::safe_reference(event_id) {
            return Err(invalid());
        }
        publisher.validate_for(self.base().store()).await?;
        if let Some(SubscriptionCreationStatus::Completed { record, .. }) = self
            .base()
            .store()
            .inspect_bound_subscription_creation(operation)
            .await?
        {
            return completed(operation, record);
        }
        let recovery = self
            .base()
            .store()
            .snapshot_subscription_recovery(operation)
            .await?;
        if recovery.api_version() != version {
            return Err(invalid());
        }
        let request = request(profile, &recovery)?;
        let context = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await?;
        let event = self
            .client()
            .subscription_creation_request(
                &context,
                Method::GET,
                &["v1", "events", event_id],
                None,
                &json!({}),
            )
            .await?;
        event_identity(&event, event_id, &recovery, chrono::Utc::now().timestamp())?;
        let dependency = recovery.dependencies();
        let customer_id = dependency.provider_customer_id();
        let receipt = dependency.plan_receipt();
        let snapshot = &event["data"]["object"];
        let (id, start) = validation::subscription(
            snapshot,
            None,
            customer_id,
            receipt,
            &request,
            context.livemode(),
        )?;
        let customer = self
            .client()
            .subscription_creation_request(
                &context,
                Method::GET,
                &["v1", "customers", customer_id],
                None,
                &json!({}),
            )
            .await?;
        validation::customer(&customer, customer_id, context.livemode())?;
        let plan = self
            .client()
            .subscription_creation_request(
                &context,
                Method::GET,
                &["v1", "plans", dependency.provider_plan_id()],
                None,
                &json!({"expand":["product"]}),
            )
            .await?;
        validation::plan(&plan, receipt, context.livemode(), true)?;
        let current = self
            .client()
            .subscription_creation_request(
                &context,
                Method::GET,
                &["v1", "subscriptions", &id],
                None,
                &json!({}),
            )
            .await?;
        let (_, current_start) = validation::subscription(
            &current,
            Some(&id),
            customer_id,
            receipt,
            &request,
            context.livemode(),
        )?;
        if (snapshot["status"] == "active" && current["status"] != "active")
            || current_start != start
            || current["items"]["data"][0]["id"] != snapshot["items"]["data"][0]["id"]
            || seconds(&current["created"])? != seconds(&snapshot["created"])?
        {
            return Err(invalid());
        }
        let now = chrono::Utc::now();
        let mut record = json!({"id":recovery.public_id().to_string(),"partyId":dependency.party_id(),"partyType":dependency.party_type(),"productId":request.local_plan_id.to_string(),"active":current["status"]=="active","externalId":current["id"],"billingProvider":"stripe","startDate":start,"status":current["status"],"providerFields":current,"createdAt":now,"updatedAt":now});
        if let Some(description) = request.description {
            record["description"] = json!(description);
        }
        if publisher
            .complete_recovered(recovery, record.clone(), event, || {
                context.ensure_current(self.client())
            })
            .await
            .is_err()
        {
            return Ok(unresolved(operation));
        }
        if context.ensure_current(self.client()).is_err() {
            return Ok(
                BoundSubscriptionCreationOutcome::RecordedEvidenceNoLongerCurrent {
                    operation_id: operation,
                },
            );
        }
        completed(operation, record)
    }
}

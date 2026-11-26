//! Explicit direct completed-creation read; local lifecycle fields are not synchronized.
use super::{
    BoundSubscriptionCreateRequest, BoundSubscriptionCreationProfile, Method, PostgresBillingStore,
    StripeBillingService, SubscriptionCollectionPolicy, SubscriptionDto, Uuid, Value, json,
    validation,
};
use forklaunch_billing_base::services::{BillingKind, postgres::SubscriptionCreationReadSnapshot};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
fn mismatch() -> BlueprintError {
    BlueprintError::new(
        "billing_bound_subscription_read_mismatch",
        "Subscription creation evidence does not match",
    )
}
fn request(
    profile: &BoundSubscriptionCreationProfile,
    snapshot: &SubscriptionCreationReadSnapshot,
) -> BlueprintResult<BoundSubscriptionCreateRequest> {
    let payload = snapshot.payload();
    let plans: Vec<_> = profile
        .plans
        .iter()
        .map(|(id, op)| json!([id.to_string(), op.to_string()]))
        .collect();
    let policy = match profile.policy {
        SubscriptionCollectionPolicy::AutomaticDefaultIncomplete => {
            "automatic_default_incomplete_v1"
        }
    };
    if payload["trustedPlans"] != json!(plans) || payload["collectionPolicy"] != policy {
        return Err(mismatch());
    }
    let parse = |field: &str| {
        Uuid::parse_str(payload["request"][field].as_str().ok_or_else(mismatch)?)
            .map_err(|_| mismatch())
    };
    let description = match &payload["request"]["description"] {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        _ => return Err(mismatch()),
    };
    Ok(BoundSubscriptionCreateRequest {
        local_customer_id: parse("localCustomerId")?,
        local_plan_id: parse("localPlanId")?,
        quantity: payload["request"]["quantity"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 9_007_199_254_740_991)
            .ok_or_else(mismatch)?,
        description,
    })
}
impl StripeBillingService<PostgresBillingStore> {
    /// Checks direct transport and local read/audit access without provider I/O.
    /// No publisher or creation authority is required or inferred.
    /// # Errors
    /// Denies managed transport and missing local privileges.
    pub async fn validate_bound_subscription_read_configuration(&self) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_subscription_creation_read_access()
            .await
    }

    /// Reads only completed direct creation provenance (including validated
    /// event-recovered receipts). Requires the original trusted Plan mapping and
    /// unchanged Customer/Plan records, bindings and completed Plan provenance.
    /// No publisher, claims, row locks, dispatch ticket or provider POST is used.
    /// Supported current states are unchanged active/incomplete or incomplete to
    /// active with the same item/start/terms. Only providerFields refresh; local
    /// status/active/party/product remain the receipt's, matching configured DTO
    /// enrichment. This is not lifecycle synchronization or payment/entitlement
    /// proof. External Party ownership remains an administrative assertion.
    /// # Errors
    /// Rejects managed transport, absent/changed provenance, scope/policy changes,
    /// unsupported provider states, and context expiry after final DB/audit commit.
    pub async fn get_bound_created_subscription(
        &self,
        profile: &BoundSubscriptionCreationProfile,
        public_id: Uuid,
    ) -> BlueprintResult<SubscriptionDto> {
        let version = self.client().direct_creation_version()?;
        let store = self.base().store();
        let binding = store
            .stripe_binding_by_public_id(&profile.scope, BillingKind::Subscription, public_id)
            .await?
            .ok_or_else(mismatch)?;
        let snapshot = store
            .subscription_creation_read_snapshot(&binding, version)
            .await?;
        let request = request(profile, &snapshot)?;
        let customer_id = snapshot.provider_customer_id();
        let receipt = snapshot.plan_receipt();
        let plan_id = receipt["externalId"]
            .as_str()
            .filter(|v| crate::bound_plan_creation::safe_reference(v))
            .ok_or_else(mismatch)?;
        let original = &snapshot.original_record()["providerFields"];
        let (_, original_start) = validation::subscription(
            original,
            Some(binding.provider_id()),
            customer_id,
            receipt,
            &request,
            profile.scope.livemode(),
        )
        .map_err(|_| mismatch())?;
        let context = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await?;
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
        validation::customer(&customer, customer_id, context.livemode()).map_err(|_| mismatch())?;
        let plan = self
            .client()
            .subscription_creation_request(
                &context,
                Method::GET,
                &["v1", "plans", plan_id],
                None,
                &json!({"expand":["product"]}),
            )
            .await?;
        validation::plan(&plan, receipt, context.livemode(), true).map_err(|_| mismatch())?;
        let provider = self
            .client()
            .subscription_creation_request(
                &context,
                Method::GET,
                &["v1", "subscriptions", binding.provider_id()],
                None,
                &json!({}),
            )
            .await?;
        let (_, start) = validation::subscription(
            &provider,
            Some(binding.provider_id()),
            customer_id,
            receipt,
            &request,
            context.livemode(),
        )
        .map_err(|_| mismatch())?;
        if start != original_start
            || provider["items"]["data"][0]["id"] != original["items"]["data"][0]["id"]
            || (original["status"] == "active" && provider["status"] != "active")
        {
            return Err(mismatch());
        }
        let record = store.read_subscription_creation_snapshot(&snapshot).await?;
        context.ensure_current(self.client())?;
        let mut record: SubscriptionDto = serde_json::from_value(record).map_err(|_| mismatch())?;
        record.provider_fields = Some(provider);
        Ok(record)
    }
}

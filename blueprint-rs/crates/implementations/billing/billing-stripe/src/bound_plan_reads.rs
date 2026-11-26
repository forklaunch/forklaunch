//! Explicit completed-creation reads; no administrative-alias fallback or writes.
use crate::{
    bound_plan_creation::{
        BoundPlanCreateRequest, BoundPlanCreationProfile, fields, plan, product,
    },
    services::StripeBillingService,
};
use forklaunch_billing_base::services::{BillingKind, postgres::PostgresBillingStore};
use forklaunch_billing_interface::services::PlanDto;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

fn mismatch() -> BlueprintError {
    BlueprintError::new(
        "billing_bound_plan_read_mismatch",
        "Plan creation evidence does not match",
    )
}
impl StripeBillingService<PostgresBillingStore> {
    /// Validates direct transport configuration and local read/audit grants.
    /// No provider request, publication credential or financial authority is used.
    /// # Errors
    /// Denies managed transport and missing local table-level read/audit grants.
    pub async fn validate_bound_plan_read_configuration(&self) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_plan_creation_read_access()
            .await
    }

    /// Reads an actor-owned, directly created public UUID using sealed creation
    /// provenance and a fresh expanded Stripe Plan/Product observation. Only the
    /// Product presentation is refreshed; all local fields remain the receipt's.
    /// Requires journal SELECT and audited durable reads. The original normalized
    /// product allowlist must match current trusted policy exactly. No provider
    /// write, automatic alias provisioning or payment/entitlement assertion occurs.
    /// Data linearizes at the final matching database SELECT; provider freshness
    /// is checked after the audit commit. Later remote/local changes remain possible.
    /// # Errors
    /// Denies managed transport, absent provenance, modified local/binding state,
    /// changed policy or provider terms, and expired verification after DB waits.
    pub async fn get_bound_created_plan(
        &self,
        profile: &BoundPlanCreationProfile,
        public_id: Uuid,
    ) -> BlueprintResult<PlanDto> {
        let version = self.client().direct_creation_version()?;
        let store = self.base().store();
        let binding = store
            .stripe_binding_by_public_id(&profile.scope, BillingKind::Plan, public_id)
            .await?
            .ok_or_else(mismatch)?;
        let snapshot = store.plan_creation_read_snapshot(&binding, version).await?;
        if snapshot.payload()["trustedProducts"] != json!(profile.products) {
            return Err(mismatch());
        }
        let request: BoundPlanCreateRequest =
            serde_json::from_value(snapshot.payload()["request"].clone())
                .map_err(|_| mismatch())?;
        if fields(profile, &request)? != snapshot.payload()["providerFields"] {
            return Err(mismatch());
        }
        let context = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await?;
        let provider = self
            .client()
            .plan_creation_request(
                &context,
                Method::GET,
                &["v1", "plans", binding.provider_id()],
                None,
                &json!({"expand":["product"]}),
            )
            .await?;
        plan(
            &provider,
            Some(binding.provider_id()),
            &request,
            context.livemode(),
        )
        .map_err(|_| mismatch())?;
        product(
            &provider["product"],
            &request.product_id,
            context.livemode(),
        )
        .map_err(|_| mismatch())?;
        let record = store.read_plan_creation_snapshot(&snapshot).await?;
        context.ensure_current(self.client())?;
        let mut record: PlanDto = serde_json::from_value(record).map_err(|_| mismatch())?;
        record.provider_fields = Some(provider["product"].clone());
        Ok(record)
    }
}

//! Explicit direct subscription creation. Creation/active never means paid.
//! Customer defaults, balances and invoice configuration can affect financial
//! outcomes; this profile does not attest invoice totals or payment success.
mod read;
mod recovery;
mod validation;
use crate::{client::VerifiedStripeContext, services::StripeBillingService};
use forklaunch_billing_base::services::postgres::{
    PostgresBillingStore, StripeBindingScope, SubscriptionCreationClaim,
    SubscriptionCreationStatus, TrustedSubscriptionPublisher,
};
use forklaunch_billing_interface::services::SubscriptionDto;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use reqwest::Method;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Explicit trusted financial collection policy, never inferred from DTO fields.
pub enum SubscriptionCollectionPolicy {
    AutomaticDefaultIncomplete,
}
/// Server-authorized Plan UUID -> completed creation operation associations.
/// Caller must separately authorize which customer/party may subscribe; billing
/// cannot synchronize an externally owned IAM party lifecycle.
pub struct BoundSubscriptionCreationProfile {
    scope: StripeBindingScope,
    plans: BTreeMap<Uuid, Uuid>,
    policy: SubscriptionCollectionPolicy,
}
impl BoundSubscriptionCreationProfile {
    /// # Errors
    /// Denies empty, duplicate, nil or oversized trusted provenance mappings.
    pub fn trusted(
        scope: StripeBindingScope,
        plans: Vec<(Uuid, Uuid)>,
        policy: SubscriptionCollectionPolicy,
    ) -> BlueprintResult<Self> {
        if plans.is_empty()
            || plans.len() > 1000
            || plans.iter().any(|(id, op)| id.is_nil() || op.is_nil())
        {
            return Err(invalid());
        }
        let count = plans.len();
        let plans: BTreeMap<_, _> = plans.into_iter().collect();
        if plans.len() != count {
            return Err(invalid());
        }
        Ok(Self {
            scope,
            plans,
            policy,
        })
    }
}
/// Library-only finite request, not the existing generic HTTP creation DTO.
pub struct BoundSubscriptionCreateRequest {
    pub local_customer_id: Uuid,
    pub local_plan_id: Uuid,
    pub quantity: u64,
    pub description: Option<String>,
}
/// Historical completion or an inspectable unknown result. No automatic retry.
pub enum BoundSubscriptionCreationOutcome {
    Completed {
        operation_id: Uuid,
        record: Box<SubscriptionDto>,
    },
    Unresolved {
        operation_id: Uuid,
    },
    RecordedEvidenceNoLongerCurrent {
        operation_id: Uuid,
    },
}
fn invalid() -> BlueprintError {
    BlueprintError::new(
        "billing_bound_subscription_invalid",
        "Invalid bound subscription creation",
    )
}
fn unresolved(operation_id: Uuid) -> BoundSubscriptionCreationOutcome {
    BoundSubscriptionCreationOutcome::Unresolved { operation_id }
}
fn completed(
    operation_id: Uuid,
    record: Value,
) -> BlueprintResult<BoundSubscriptionCreationOutcome> {
    Ok(BoundSubscriptionCreationOutcome::Completed {
        operation_id,
        record: Box::new(serde_json::from_value(record).map_err(|_| invalid())?),
    })
}
impl StripeBillingService<PostgresBillingStore> {
    /// Preflights explicit direct creation and both roles without provider I/O.
    /// # Errors
    /// Rejects managed transport, missing local grants or invalid publisher identity.
    pub async fn validate_bound_subscription_configuration(
        &self,
        publisher: &TrustedSubscriptionPublisher,
    ) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_subscription_creation_access()
            .await?;
        publisher
            .validate_creation_access(self.base().store())
            .await
    }

    /// One provider POST only after confirmed reservation. Explicit separate
    /// publisher credentials are required; default routes/legacy APIs unchanged.
    /// Customer and Plan GETs establish bounded observations, not atomic remote
    /// locks or ongoing authority after publication. Unknown results retain claims.
    /// # Errors
    /// Denies unsupported configuration/input/ownership before dispatch; after
    /// effects, ambiguity is returned by operation UUID for receipt inspection.
    // Keep the verified-read, reservation, single dispatch and publication order visible.
    #[allow(clippy::too_many_lines)]
    pub async fn create_bound_subscription(
        &self,
        profile: &BoundSubscriptionCreationProfile,
        publisher: &TrustedSubscriptionPublisher,
        operation: Uuid,
        request: BoundSubscriptionCreateRequest,
    ) -> BlueprintResult<BoundSubscriptionCreationOutcome> {
        let version = self.client().direct_creation_version()?;
        if operation.is_nil()
            || request.local_customer_id.is_nil()
            || request.quantity == 0
            || request.quantity > 9_007_199_254_740_991
            || request
                .description
                .as_ref()
                .is_some_and(|v| v.len() > 500 || v.contains('\0'))
        {
            return Err(invalid());
        }
        let plan_operation = *profile
            .plans
            .get(&request.local_plan_id)
            .ok_or_else(invalid)?;
        publisher.validate_for(self.base().store()).await?;
        let snapshot = self
            .base()
            .store()
            .snapshot_subscription_dependencies(
                &profile.scope,
                request.local_customer_id,
                request.local_plan_id,
                plan_operation,
                version,
            )
            .await?;
        let customer_id = snapshot.provider_customer_id().to_owned();
        let plan_id = snapshot.provider_plan_id().to_owned();
        // Existing sealed Subscription binding constraints require cus_/sub_.
        // Do not dispatch an identity this explicit storage profile cannot bind.
        if !customer_id.starts_with("cus_")
            || !crate::bound_plan_creation::safe_reference(&customer_id)
        {
            return Err(invalid());
        }
        let receipt = snapshot.plan_receipt().clone();
        let party_id = snapshot.party_id().to_owned();
        let party_type = snapshot.party_type().to_owned();
        let context = self
            .client()
            .verify_context(profile.scope.account_id(), profile.scope.livemode())
            .await?;
        let customer = self
            .client()
            .subscription_creation_request(
                &context,
                Method::GET,
                &["v1", "customers", &customer_id],
                None,
                &json!({}),
            )
            .await?;
        validation::customer(&customer, &customer_id, context.livemode())?;
        let plan = self
            .client()
            .subscription_creation_request(
                &context,
                Method::GET,
                &["v1", "plans", &plan_id],
                None,
                &json!({"expand":["product"]}),
            )
            .await?;
        validation::plan(&plan, &receipt, context.livemode(), true)?;
        let policy = match profile.policy {
            SubscriptionCollectionPolicy::AutomaticDefaultIncomplete => {
                "automatic_default_incomplete_v1"
            }
        };
        let mut fields = json!({"customer":customer_id,"items":[{"plan":plan_id,"quantity":request.quantity}],"collection_method":"charge_automatically","payment_behavior":"default_incomplete"});
        if let Some(description) = &request.description {
            fields["description"] = json!(description);
        }
        let trusted_plans: Vec<_> = profile
            .plans
            .iter()
            .map(|(id, op)| json!([id.to_string(), op.to_string()]))
            .collect();
        let payload = json!({"request":{"localCustomerId":request.local_customer_id.to_string(),"localPlanId":request.local_plan_id.to_string(),"quantity":request.quantity,"description":request.description},"providerFields":fields,"collectionPolicy":policy,"trustedPlans":trusted_plans});
        let ticket = match self
            .base()
            .store()
            .reserve_subscription_creation(snapshot, operation, &payload, || {
                context.ensure_current(self.client())
            })
            .await
        {
            Ok(SubscriptionCreationClaim::Reserved(ticket)) => ticket,
            Ok(SubscriptionCreationClaim::Completed { record, .. }) => {
                return completed(operation, record);
            }
            Err(error)
                if error.code == "billing_provider_binding_conflict"
                    || error.code == "billing_validation" =>
            {
                return Err(error);
            }
            Ok(SubscriptionCreationClaim::Existing(_)) | Err(_) => {
                return Ok(unresolved(operation));
            }
        };
        let Ok((provider, start_date)) = self
            .dispatch_bound_subscription(
                &context,
                &customer_id,
                &receipt,
                &request,
                &fields,
                ticket.idempotency_key(),
            )
            .await
        else {
            return Ok(unresolved(operation));
        };
        let now = chrono::Utc::now();
        let mut record = json!({"id":ticket.public_id().to_string(),"partyId":party_id,"partyType":party_type,"productId":request.local_plan_id.to_string(),"active":provider["status"]=="active","externalId":provider["id"],"billingProvider":"stripe","startDate":start_date,"status":provider["status"],"providerFields":provider,"createdAt":now,"updatedAt":now});
        if let Some(description) = request.description {
            record["description"] = json!(description);
        }
        if publisher
            .complete(ticket, record.clone(), || {
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
    async fn dispatch_bound_subscription(
        &self,
        context: &VerifiedStripeContext,
        customer: &str,
        receipt: &Value,
        request: &BoundSubscriptionCreateRequest,
        fields: &Value,
        key: &str,
    ) -> BlueprintResult<(Value, chrono::DateTime<chrono::Utc>)> {
        let created = self
            .client()
            .subscription_creation_request(
                context,
                Method::POST,
                &["v1", "subscriptions"],
                Some(key),
                fields,
            )
            .await?;
        let (id, start) = validation::subscription(
            &created,
            None,
            customer,
            receipt,
            request,
            context.livemode(),
        )?;
        let current = self
            .client()
            .subscription_creation_request(
                context,
                Method::GET,
                &["v1", "subscriptions", &id],
                None,
                &json!({}),
            )
            .await?;
        let (_, current_start) = validation::subscription(
            &current,
            Some(&id),
            customer,
            receipt,
            request,
            context.livemode(),
        )?;
        if current_start != start
            || created["items"]["data"][0]["id"] != current["items"]["data"][0]["id"]
        {
            return Err(invalid());
        }
        Ok((current, current_start))
    }
    /// Historical receipt inspection only; never retries an unresolved POST.
    /// # Errors
    /// Returns tenant/actor/audit/storage or receipt-shape errors.
    pub async fn inspect_bound_subscription_creation(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<BoundSubscriptionCreationOutcome>> {
        match self
            .base()
            .store()
            .inspect_bound_subscription_creation(operation)
            .await?
        {
            Some(SubscriptionCreationStatus::Pending(_)) => Ok(Some(unresolved(operation))),
            Some(SubscriptionCreationStatus::Completed { record, .. }) => {
                completed(operation, record).map(Some)
            }
            None => Ok(None),
        }
    }
}

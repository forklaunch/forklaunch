//! Explicit managed UUID creation and inspect-only interrupted publication.
use crate::{
    bound_creation::{
        BoundPaymentLinkCreationOutcome, BoundPaymentLinkCreationProfile, completed, fields,
        invalid,
    },
    services::StripeBillingService,
};
use forklaunch_billing_base::services::postgres::{
    ManagedPaymentLinkPublication, PaymentLinkCreationClaim, PaymentLinkCreationStatus,
    PostgresBillingStore, StripeBindingScope,
};
use forklaunch_billing_interface::services::CreatePaymentLinkDto;
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Explicit server-owned managed profile; never populated from public provider fields.
pub struct ManagedBoundPaymentLinkCreationProfile {
    base: BoundPaymentLinkCreationProfile,
    profile_id: String,
}
impl ManagedBoundPaymentLinkCreationProfile {
    /// # Errors
    /// Rejects invalid scope/profile/price configuration.
    pub fn trusted(
        scope: StripeBindingScope,
        profile_id: String,
        prices: Vec<String>,
    ) -> BlueprintResult<Self> {
        if profile_id.trim().is_empty()
            || profile_id.len() > 128
            || profile_id.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        Ok(Self {
            base: BoundPaymentLinkCreationProfile::trusted(scope, prices)?,
            profile_id,
        })
    }
}
fn unresolved(operation: Uuid) -> BoundPaymentLinkCreationOutcome {
    BoundPaymentLinkCreationOutcome::Unresolved {
        operation_id: operation,
    }
}
fn execution(fields: &Value) -> BlueprintResult<(String, String)> {
    let mut url = reqwest::Url::parse("https://canonical.invalid").map_err(|_| invalid())?;
    {
        let mut form = url.query_pairs_mut();
        form.append_pair("currency", fields["currency"].as_str().ok_or_else(invalid)?);
        for method in fields["payment_method_types"]
            .as_array()
            .ok_or_else(invalid)?
        {
            form.append_pair(
                "payment_method_types[]",
                method.as_str().ok_or_else(invalid)?,
            );
        }
        for (index, item) in fields["line_items"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .enumerate()
        {
            form.append_pair(
                &format!("line_items[{index}][price]"),
                item["price"].as_str().ok_or_else(invalid)?,
            );
            form.append_pair(
                &format!("line_items[{index}][quantity]"),
                &item["quantity"].as_u64().ok_or_else(invalid)?.to_string(),
            );
        }
    }
    let payload = url.query().ok_or_else(invalid)?.to_owned();
    if payload.len() > 4096 {
        return Err(invalid());
    }
    let hash = base16ct::lower::encode_string(&Sha256::digest(payload.as_bytes()));
    Ok((payload, hash))
}
fn validate_publication_profile(
    profile: &ManagedBoundPaymentLinkCreationProfile,
    publication: &ManagedPaymentLinkPublication,
) -> BlueprintResult<()> {
    if publication.scope() != &profile.base.scope
        || publication.trusted_prices() != &json!(profile.base.prices)
        || publication.expected_envelope()["profileId"] != profile.profile_id
    {
        return Err(invalid());
    }
    Ok(())
}
impl StripeBillingService<PostgresBillingStore> {
    /// Creates at most once through the dedicated signed managed profile. Current
    /// configuration is verified and sealed before local reservation or dispatch.
    /// Duplicate pending operations never dispatch; use explicit inspection.
    /// # Errors
    /// Rejects invalid requests, scope changes and operation payload conflicts.
    pub async fn create_managed_bound_payment_link(
        &self,
        profile: &ManagedBoundPaymentLinkCreationProfile,
        operation: Uuid,
        request: CreatePaymentLinkDto,
    ) -> BlueprintResult<BoundPaymentLinkCreationOutcome> {
        let version = self.client().managed_creation_version()?;
        if operation.is_nil() {
            return Err(invalid());
        }
        let fields = fields(&profile.base, &request)?;
        let (execution_payload, execution_hash) = execution(&fields)?;
        let context = self
            .client()
            .managed_creation_context(
                profile.base.scope.account_id(),
                profile.base.scope.livemode(),
                &profile.profile_id,
            )
            .await?;
        let payload = json!({"request":request,"providerFields":fields,"trustedPrices":profile.base.prices,"managed":{
            "kind":"payment_link_creation_v1","profileId":profile.profile_id,"configurationId":context.configuration_id(),"scope":context.scope(),
            "executionPayload":execution_payload,"executionHash":execution_hash
        }});
        let claim = match self
            .base()
            .store()
            .reserve_payment_link_creation(&profile.base.scope, operation, version, &payload)
            .await
        {
            Ok(claim) => claim,
            Err(error)
                if error.code == "billing_provider_binding_conflict"
                    || error.code == "billing_validation" =>
            {
                return Err(error);
            }
            Err(_) => return Ok(unresolved(operation)),
        };
        let dispatch = match claim {
            PaymentLinkCreationClaim::Reserved(ticket) => ticket,
            PaymentLinkCreationClaim::Existing(PaymentLinkCreationStatus::Pending { .. }) => {
                return Ok(unresolved(operation));
            }
            PaymentLinkCreationClaim::Existing(PaymentLinkCreationStatus::Completed {
                record,
                ..
            }) => return completed(operation, record),
        };
        let Ok(Some(publication)) = self
            .base()
            .store()
            .inspect_managed_payment_link_publication(operation)
            .await
        else {
            return Ok(unresolved(operation));
        };
        let Ok(Some(provider)) = self
            .client()
            .create_managed_payment_link(&context, dispatch, publication.expected_envelope())
            .await
        else {
            return Ok(unresolved(operation));
        };
        self.publish_managed_creation(profile, operation, publication, provider)
            .await
    }
    /// Reads the remote journal only, then atomically publishes a still-owned
    /// local pending intent. Never reconstructs or repeats a provider dispatch.
    /// # Errors
    /// Rejects foreign actors, invalid intent profiles and storage failures.
    pub async fn reconcile_managed_bound_payment_link_creation(
        &self,
        profile: &ManagedBoundPaymentLinkCreationProfile,
        operation: Uuid,
    ) -> BlueprintResult<BoundPaymentLinkCreationOutcome> {
        self.client().managed_creation_version()?;
        if operation.is_nil() {
            return Err(invalid());
        }
        if let Some(PaymentLinkCreationStatus::Completed { record, .. }) = self
            .base()
            .store()
            .inspect_managed_payment_link_creation(
                operation,
                &profile.base.scope,
                &profile.profile_id,
                self.client().managed_creation_version()?,
            )
            .await?
        {
            return completed(operation, record);
        }
        let Some(publication) = self
            .base()
            .store()
            .inspect_managed_payment_link_publication(operation)
            .await?
        else {
            return Ok(unresolved(operation));
        };
        validate_publication_profile(profile, &publication)?;
        let Ok(context) = self
            .client()
            .managed_creation_context(
                profile.base.scope.account_id(),
                profile.base.scope.livemode(),
                &profile.profile_id,
            )
            .await
        else {
            return Ok(unresolved(operation));
        };
        let Ok(Some(provider)) = self
            .client()
            .inspect_managed_payment_link(&context, publication.expected_envelope())
            .await
        else {
            return Ok(unresolved(operation));
        };
        self.publish_managed_creation(profile, operation, publication, provider)
            .await
    }
    async fn publish_managed_creation(
        &self,
        profile: &ManagedBoundPaymentLinkCreationProfile,
        operation: Uuid,
        publication: ManagedPaymentLinkPublication,
        provider: Value,
    ) -> BlueprintResult<BoundPaymentLinkCreationOutcome> {
        validate_publication_profile(profile, &publication)?;
        // Re-observe current configuration/generation after remote delivery,
        // rather than treating the historical remote receipt as current authority.
        let Ok(context) = self
            .client()
            .managed_creation_context(
                profile.base.scope.account_id(),
                profile.base.scope.livemode(),
                &profile.profile_id,
            )
            .await
        else {
            return Ok(unresolved(operation));
        };
        if context.matches(publication.expected_envelope()).is_err() {
            return Ok(unresolved(operation));
        }
        let request = publication.request();
        let now = chrono::Utc::now();
        let record = json!({"id":publication.expected_envelope()["publicId"],"amount":request["amount"],"currency":request["currency"],"paymentMethods":request["paymentMethods"],"status":request["status"],"providerFields":provider,"createdAt":now,"updatedAt":now});
        if self
            .base()
            .store()
            .complete_managed_payment_link_publication(publication, record.clone(), || {
                context.ensure_current(self.client())
            })
            .await
            .is_err()
        {
            return Ok(unresolved(operation));
        }
        if context.ensure_current(self.client()).is_err() {
            return Ok(
                BoundPaymentLinkCreationOutcome::RecordedEvidenceNoLongerCurrent {
                    operation_id: operation,
                },
            );
        }
        completed(operation, record)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_form_has_only_fixed_strings_and_integer_quantities() {
        let (payload,hash)=execution(&json!({"currency":"USD","payment_method_types":["card","us_bank_account"],"line_items":[{"price":"price_owned","quantity":15}]})).unwrap();
        assert_eq!(
            payload,
            "currency=USD&payment_method_types%5B%5D=card&payment_method_types%5B%5D=us_bank_account&line_items%5B0%5D%5Bprice%5D=price_owned&line_items%5B0%5D%5Bquantity%5D=15"
        );
        assert_eq!(hash.len(), 64);
        assert!(execution(&json!({"currency":"USD","payment_method_types":["card"],"line_items":[{"price":"price_owned","quantity":15.5}]})).is_err());
    }
}

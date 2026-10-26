//! Explicit actor-owned metadata-only expiration. Never deactivates a Stripe URL.
mod recovery;
use crate::{bound_creation::BoundPaymentLinkCreationProfile, services::StripeBillingService};
use forklaunch_billing_base::services::postgres::{
    PaymentLinkExpirationClaim, PaymentLinkExpirationStatus, PostgresBillingStore,
    StripeBindingScope, TrustedPaymentLinkExpirationPublisher,
};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::json;
use uuid::Uuid;
/// Separate trusted capability selection; read/creation profiles do not select it.
pub struct BoundPaymentLinkExpirationProfile {
    creation: BoundPaymentLinkCreationProfile,
}
impl BoundPaymentLinkExpirationProfile {
    /// # Errors
    /// Rejects invalid explicit trusted price configuration.
    pub fn trusted(scope: StripeBindingScope, prices: Vec<String>) -> BlueprintResult<Self> {
        Ok(Self {
            creation: BoundPaymentLinkCreationProfile::trusted(scope, prices)?,
        })
    }
}
/// IDs-only outcome. Unknown means inspect, never repeat a provider POST.
pub enum BoundPaymentLinkExpirationOutcome {
    Completed { operation_id: Uuid, public_id: Uuid },
    Unresolved { operation_id: Uuid, public_id: Uuid },
    RecordedEvidenceNoLongerCurrent { operation_id: Uuid, public_id: Uuid },
}
pub(crate) fn invalid() -> BlueprintError {
    BlueprintError::new(
        "billing_bound_payment_link_expiration_invalid",
        "Invalid bound payment link expiration",
    )
}
fn historical(status: PaymentLinkExpirationStatus) -> BoundPaymentLinkExpirationOutcome {
    match status {
        PaymentLinkExpirationStatus::Pending {
            operation_id,
            public_id,
        } => BoundPaymentLinkExpirationOutcome::Unresolved {
            operation_id,
            public_id,
        },
        PaymentLinkExpirationStatus::Completed {
            operation_id,
            public_id,
        } => BoundPaymentLinkExpirationOutcome::Completed {
            operation_id,
            public_id,
        },
    }
}
impl StripeBillingService<PostgresBillingStore> {
    /// Local direct-client/grant/publisher validation, with zero provider requests.
    /// # Errors
    /// Denies managed/Connect, backup selection by caller, missing grants or shared authority.
    pub async fn validate_bound_payment_link_expiration_configuration(
        &self,
        publisher: &TrustedPaymentLinkExpirationPublisher,
    ) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_payment_link_expiration_access()
            .await?;
        publisher
            .validate_configuration_for(self.base().store())
            .await
    }

    /// Historical IDs/status inspection only. No provider reads, redispatch or URL.
    /// # Errors
    /// Denies changed actor, tenant, public ID, scope or immutable trusted policy.
    pub async fn inspect_bound_payment_link_expiration(
        &self,
        profile: &BoundPaymentLinkExpirationProfile,
        operation_id: Uuid,
        public_id: Uuid,
    ) -> BlueprintResult<Option<BoundPaymentLinkExpirationOutcome>> {
        let version = self.client().direct_creation_version()?;
        Ok(self
            .base()
            .store()
            .inspect_payment_link_expiration(
                &profile.creation.scope,
                version,
                operation_id,
                public_id,
                &json!(profile.creation.prices),
            )
            .await?
            .map(historical))
    }
    /// One confirmed reservation permits exactly one metadata.status=EXPIRED POST
    /// to the immutable created provider ID. It does NOT set active=false, revoke
    /// a provider URL, modify money/entitlements or synchronize local payment state.
    /// Completion hides the locally bound record using an immutable journal tombstone.
    /// Pending retains two dependency claims indefinitely; no automatic retry/release.
    /// # Errors
    /// Invalid authority fails before POST. Once reserved, unknown effects return
    /// Unresolved; inspection is required after any ambiguous COMMIT/response.
    pub async fn expire_bound_created_payment_link(
        &self,
        publisher: &TrustedPaymentLinkExpirationPublisher,
        profile: &BoundPaymentLinkExpirationProfile,
        operation_id: Uuid,
        public_id: Uuid,
    ) -> BlueprintResult<BoundPaymentLinkExpirationOutcome> {
        if operation_id.is_nil() || public_id.is_nil() {
            return Err(invalid());
        }
        self.client().direct_creation_version()?;
        publisher.validate_for(self.base().store()).await?;
        if let Some(status) = self
            .inspect_bound_payment_link_expiration(profile, operation_id, public_id)
            .await?
        {
            return Ok(status);
        }
        let (snapshot, provider_id) = match self
            .prepare_bound_created_payment_link(&profile.creation, public_id)
            .await
        {
            Ok(prepared) => prepared,
            Err(error) => {
                return self
                    .inspect_bound_payment_link_expiration(profile, operation_id, public_id)
                    .await?
                    .ok_or(error);
            }
        };
        let context = self
            .client()
            .verify_context(
                profile.creation.scope.account_id(),
                profile.creation.scope.livemode(),
            )
            .await?;
        self.client()
            .retrieve_payment_link_in_context(&context, &provider_id)
            .await?;
        let claim = self
            .base()
            .store()
            .reserve_payment_link_expiration(
                operation_id,
                snapshot,
                &json!(profile.creation.prices),
                || context.ensure_current(self.client()),
            )
            .await;
        let unknown = || BoundPaymentLinkExpirationOutcome::Unresolved {
            operation_id,
            public_id,
        };
        let ticket = match claim {
            Ok(PaymentLinkExpirationClaim::Existing(status)) => return Ok(historical(status)),
            Ok(PaymentLinkExpirationClaim::Reserved(ticket)) => ticket,
            Err(error) if error.code == "billing_payment_link_expiration_commit_unknown" => {
                return Ok(unknown());
            }
            Err(error) => {
                return self
                    .inspect_bound_payment_link_expiration(profile, operation_id, public_id)
                    .await?
                    .ok_or(error);
            }
        };
        if context.ensure_current(self.client()).is_err() {
            return Ok(unknown());
        }
        let Ok(response) = self
            .client()
            .expire_payment_link_metadata(&context, &provider_id, ticket.idempotency_key())
            .await
        else {
            return Ok(unknown());
        };
        if response["id"] != provider_id
            || response["object"] != "payment_link"
            || response["livemode"].as_bool() != Some(profile.creation.scope.livemode())
            || response["metadata"]["status"] != "EXPIRED"
        {
            return Ok(unknown());
        }
        if publisher
            .complete(ticket, || context.ensure_current(self.client()))
            .await
            .is_err()
        {
            return Ok(unknown());
        }
        if context.ensure_current(self.client()).is_err() {
            return Ok(
                BoundPaymentLinkExpirationOutcome::RecordedEvidenceNoLongerCurrent {
                    operation_id,
                    public_id,
                },
            );
        }
        Ok(BoundPaymentLinkExpirationOutcome::Completed {
            operation_id,
            public_id,
        })
    }
}

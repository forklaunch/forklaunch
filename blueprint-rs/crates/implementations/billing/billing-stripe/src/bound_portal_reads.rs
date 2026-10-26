//! Explicit local historical access; no Stripe read, URL refresh or remote validity proof.
use crate::{bound_portal_creation::BoundPortalCreationProfile, services::StripeBillingService};
use forklaunch_billing_base::services::{BillingKind, postgres::PostgresBillingStore};
use forklaunch_billing_interface::services::BillingPortalDto;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use uuid::Uuid;

/// Original stored DTO returned inside its original, unextended lifetime.
/// The URL may already be invalid remotely. Never a new session or paid-state proof.
pub struct StoredBoundPortalSession {
    pub record: BillingPortalDto,
    /// Final completion-clock TTL check, not the time at which all data is current.
    /// The data snapshot linearizes at the last matching SELECT before audit COMMIT.
    pub observed_at: chrono::DateTime<chrono::Utc>,
}
impl StripeBillingService<PostgresBillingStore> {
    /// No provider GET/POST. Requires current local provenance and same server policy.
    /// # Errors
    /// Rejects managed mode, missing/changed authority, actor/scope mismatch and expiry.
    pub async fn get_stored_bound_portal_session(
        &self,
        profile: &BoundPortalCreationProfile,
        public_id: Uuid,
    ) -> BlueprintResult<StoredBoundPortalSession> {
        let version = self.client().direct_creation_version()?;
        let missing = || {
            BlueprintError::new(
                "billing_provider_binding_conflict",
                "Stored portal evidence does not match",
            )
        };
        let binding = self
            .base()
            .store()
            .stripe_binding_by_public_id(&profile.scope, BillingKind::BillingPortal, public_id)
            .await?
            .ok_or_else(missing)?;
        let (record, observed_at) = self
            .base()
            .store()
            .read_stored_portal_creation(
                &binding,
                version,
                &profile.configuration_id,
                &profile.return_url,
            )
            .await?;
        Ok(StoredBoundPortalSession {
            record: serde_json::from_value(record).map_err(|_| missing())?,
            observed_at,
        })
    }
}

/// Local access state only. Never revokes an already issued provider URL.
pub enum PortalLocalExpirationOutcome {
    Expired,
    CommitUnknown,
}
impl StripeBillingService<PostgresBillingStore> {
    /// Checks explicit local mutation grants, with no provider I/O.
    /// # Errors
    /// Denies managed clients, unsafe roles or missing local privileges.
    pub async fn validate_portal_local_expiration_configuration(&self) -> BlueprintResult<()> {
        self.client().direct_creation_version()?;
        self.base()
            .store()
            .validate_portal_local_expiration_access()
            .await
    }
    /// Actor/profile-bound historical local expiration status. Absence is not
    /// proof of rollback after a lost COMMIT acknowledgement.
    /// # Errors
    /// Denies missing or mismatched completed creation provenance.
    pub async fn inspect_stored_bound_portal_expiration(
        &self,
        profile: &BoundPortalCreationProfile,
        public_id: Uuid,
    ) -> BlueprintResult<bool> {
        let version = self.client().direct_creation_version()?;
        self.base()
            .store()
            .inspect_portal_local_expiration(
                &profile.scope,
                public_id,
                version,
                &profile.configuration_id,
                &profile.return_url,
            )
            .await
    }
    /// Idempotent local access revocation. No provider GET/POST or financial effects.
    /// First publication requires unchanged current local proof and original TTL;
    /// completed repeats are historical and never expose a URL.
    /// # Errors
    /// Denies missing/currently changed provenance or pre-COMMIT write failure.
    pub async fn expire_stored_bound_portal_session(
        &self,
        profile: &BoundPortalCreationProfile,
        public_id: Uuid,
    ) -> BlueprintResult<PortalLocalExpirationOutcome> {
        let version = self.client().direct_creation_version()?;
        if self
            .inspect_stored_bound_portal_expiration(profile, public_id)
            .await?
        {
            return Ok(PortalLocalExpirationOutcome::Expired);
        }
        let missing = || {
            BlueprintError::new(
                "billing_provider_binding_conflict",
                "Stored portal evidence does not match",
            )
        };
        let binding = self
            .base()
            .store()
            .stripe_binding_by_public_id(&profile.scope, BillingKind::BillingPortal, public_id)
            .await?
            .ok_or_else(missing)?;
        match self
            .base()
            .store()
            .expire_stored_portal_creation(
                &binding,
                version,
                &profile.configuration_id,
                &profile.return_url,
            )
            .await
        {
            Ok(true) => Ok(PortalLocalExpirationOutcome::Expired),
            Ok(false) => Ok(PortalLocalExpirationOutcome::CommitUnknown),
            Err(error) => {
                if self
                    .inspect_stored_bound_portal_expiration(profile, public_id)
                    .await?
                {
                    Ok(PortalLocalExpirationOutcome::Expired)
                } else {
                    Err(error)
                }
            }
        }
    }
}

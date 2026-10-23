//! Publication-only recovery for explicitly marked managed creation intents.
use super::{
    IntentRow, PaymentLinkCreationReservation, PaymentLinkCreationStatus, PostgresBillingStore,
    conflict, storage_error,
};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use uuid::Uuid;

/// Sealed local publication snapshot. This never grants provider dispatch.
/// No Clone/Debug/Serde and no access to a server idempotency key.
pub struct ManagedPaymentLinkPublication {
    ticket: PaymentLinkCreationReservation,
    request: Value,
    trusted_prices: Value,
    envelope: Value,
}
impl ManagedPaymentLinkPublication {
    #[must_use]
    pub fn expected_envelope(&self) -> &Value {
        &self.envelope
    }
    #[must_use]
    pub fn scope(&self) -> &super::StripeBindingScope {
        &self.ticket.scope
    }
    #[must_use]
    pub fn trusted_prices(&self) -> &Value {
        &self.trusted_prices
    }
    #[must_use]
    pub fn request(&self) -> &Value {
        &self.request
    }
}
impl PostgresBillingStore {
    /// Reads historical managed status only under the exact configured local scope.
    /// # Errors
    /// Rejects direct profiles, foreign actors and mismatched scope/version/profile.
    pub async fn inspect_managed_payment_link_creation(
        &self,
        operation: Uuid,
        scope: &super::StripeBindingScope,
        profile_id: &str,
        api_version: &str,
    ) -> BlueprintResult<Option<PaymentLinkCreationStatus>> {
        self.binding_actor()?;
        let mut tx = self.transaction().await?;
        let Some(row) = self.creation_row(&mut tx, operation).await? else {
            return Ok(None);
        };
        let status = self.creation_status(operation, row.clone())?;
        let intent = self.open_creation(operation, "intent", &row.8)?;
        let canonical: Value =
            serde_json::from_str(intent["canonical"].as_str().ok_or_else(storage_error)?)
                .map_err(|_| storage_error())?;
        let managed = &canonical["payload"]["managed"];
        if row.2 != scope.deployment_label
            || row.3 != scope.account_id
            || row.5 != scope.livemode
            || row.4 != api_version
            || managed["kind"] != "payment_link_creation_v1"
            || managed["profileId"] != profile_id
        {
            return Err(conflict());
        }
        self.audit_creation(&mut tx, operation, "inspect").await?;
        tx.commit().await.map_err(super::db_error)?;
        Ok(Some(status))
    }

    /// Loads a publication-only capability from an actor-owned sealed managed
    /// intent. It cannot be converted to a provider dispatch reservation.
    /// # Errors
    /// Rejects direct intents, foreign actors and invalid encrypted identities.
    pub async fn inspect_managed_payment_link_publication(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<ManagedPaymentLinkPublication>> {
        self.binding_actor()?;
        let mut tx = self.transaction().await?;
        let Some(row) = self.creation_row(&mut tx, operation).await? else {
            return Ok(None);
        };
        if !matches!(
            self.creation_status(operation, row.clone())?,
            PaymentLinkCreationStatus::Pending { .. }
        ) {
            return Ok(None);
        }
        let identity: (String,String) = sqlx::query_as("SELECT reservation_id::text,idempotency_key FROM fl_billing.payment_link_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid")
            .bind(self.tenant.id()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(super::db_error)?;
        let result = self.managed_publication(operation, row, identity)?;
        self.audit_creation(&mut tx, operation, "inspect").await?;
        tx.commit().await.map_err(super::db_error)?;
        Ok(Some(result))
    }
    fn managed_publication(
        &self,
        operation: Uuid,
        row: IntentRow,
        identity: (String, String),
    ) -> BlueprintResult<ManagedPaymentLinkPublication> {
        let intent = self.open_creation(operation, "intent", &row.8)?;
        let canonical: Value =
            serde_json::from_str(intent["canonical"].as_str().ok_or_else(storage_error)?)
                .map_err(|_| storage_error())?;
        let payload = &canonical["payload"];
        let managed = &payload["managed"];
        if managed["kind"] != "payment_link_creation_v1" {
            return Err(conflict());
        }
        let scope = &managed["scope"];
        if scope["accountId"] != row.3
            || scope["livemode"].as_bool() != Some(row.5)
            || scope["apiVersion"] != row.4
        {
            return Err(conflict());
        }
        let envelope = json!({
            "operationId":operation.to_string(),"publicId":row.0,"tenantId":self.tenant.id(),"nativeIntentHash":row.6,
            "executionPayload":managed["executionPayload"],"executionHash":managed["executionHash"],
            "instanceId":scope["instanceId"],"organizationId":scope["organizationId"],"accountId":row.3,
            "livemode":row.5,"apiVersion":row.4,"bindingGeneration":scope["bindingGeneration"],
            "profileId":managed["profileId"],"configurationId":managed["configurationId"]
        });
        Ok(ManagedPaymentLinkPublication {
            ticket: PaymentLinkCreationReservation {
                tenant: self.tenant.id().into(),
                operation_id: operation,
                public_id: Uuid::parse_str(&row.0).map_err(|_| storage_error())?,
                reservation_id: Uuid::parse_str(&identity.0).map_err(|_| storage_error())?,
                actor_id: row.1,
                scope: super::StripeBindingScope {
                    deployment_label: row.2,
                    account_id: row.3,
                    livemode: row.5,
                },
                api_version: row.4,
                payload_hash: row.6,
                idempotency_key: identity.1,
            },
            request: payload["request"].clone(),
            trusted_prices: payload["trustedPrices"].clone(),
            envelope,
        })
    }
    /// Trusted adapter publication after a fresh authenticated remote inspection.
    /// `publication` contains no dispatch capability. Remote envelope validation
    /// must precede this call; mutable cache/provider fields are never authority.
    /// # Errors
    /// Any error requires local receipt inspection, including ambiguous COMMIT.
    pub async fn complete_managed_payment_link_publication<F>(
        &self,
        publication: ManagedPaymentLinkPublication,
        record: Value,
        fresh: F,
    ) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        self.complete_payment_link_creation(publication.ticket, record, fresh)
            .await
    }
}

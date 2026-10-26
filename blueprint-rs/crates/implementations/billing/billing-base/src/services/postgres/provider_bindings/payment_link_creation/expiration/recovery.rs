//! A pending journal can restore publication proof, never dispatch authority.
use super::{
    PaymentLinkCreationReadSnapshot, PaymentLinkExpirationStatus, PostgresBillingStore, canonical,
    conflict, db_error, hash, publication::Completion, storage_error,
};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use uuid::Uuid;
/// Opaque publication-only snapshot. No Clone/Debug/Serde or request key getter.
pub struct PaymentLinkExpirationRecovery {
    pub(super) completion: Completion,
    key: String,
    recorded_at: i64,
}
impl PaymentLinkExpirationRecovery {
    #[must_use]
    pub fn matches_event_request_key(&self, key: &str) -> bool {
        self.key == key
    }
    #[must_use]
    pub const fn recorded_at_seconds(&self) -> i64 {
        self.recorded_at
    }
}
impl PostgresBillingStore {
    /// Validates read/lock/inspection capabilities without reservation INSERT authority.
    /// # Errors
    /// Rejects missing individual grants or migration-owner membership.
    pub async fn validate_payment_link_expiration_recovery_access(&self) -> BlueprintResult<()> {
        self.validate_payment_link_creation_read_access().await?;
        let mut tx = self.transaction().await?;
        self.expiration_role(&mut tx).await?;
        for (table, privilege) in [
            ("records", "UPDATE"),
            ("provider_bindings", "UPDATE"),
            ("payment_link_expiration_dependency_claims", "SELECT"),
            ("payment_link_expiration_audit", "INSERT"),
            ("subscription_publication_identity", "SELECT"),
        ] {
            let allowed: bool =
                sqlx::query_scalar("SELECT has_table_privilege(current_user,$1,$2)")
                    .bind(format!("fl_billing.{table}"))
                    .bind(privilege)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(db_error)?;
            if !allowed {
                return Err(conflict());
            }
        }
        tx.commit().await.map_err(db_error)
    }

    /// Locks unchanged created provenance and exactly two pending dependency claims.
    /// No provider verification or dispatch capability is supplied by this method.
    /// # Errors
    /// Denies actor/scope/profile changes, completed journals, missing claims or stale proof.
    pub async fn snapshot_payment_link_expiration_recovery(
        &self,
        operation: Uuid,
        snapshot: PaymentLinkCreationReadSnapshot,
        profile: &Value,
    ) -> BlueprintResult<PaymentLinkExpirationRecovery> {
        super::super::super::subscription_creation::budget(profile, 0, &mut 0, &mut 0)?;
        let mut tx = self.transaction().await?;
        self.expiration_role(&mut tx).await?;
        self.lock_expiration_snapshot(&mut tx, &snapshot).await?;
        let row = self
            .expiration_row(&mut tx, operation)
            .await?
            .ok_or_else(conflict)?;
        let binding = &snapshot.binding;
        if !matches!(
            self.expiration_status(
                operation,
                &row,
                &binding.scope,
                &snapshot.api_version,
                binding.public_id,
                profile
            )?,
            PaymentLinkExpirationStatus::Pending { .. }
        ) {
            return Err(conflict());
        }
        let value = self.open_expiration(operation, "intent", &row.3)?;
        let text = value.as_str().ok_or_else(storage_error)?;
        let expected = json!({"tenant":self.tenant.id(),"actor":self.binding_actor()?,"operation":operation.to_string(),"publicId":binding.public_id.to_string(),"deployment":binding.scope.deployment_label,"account":binding.scope.account_id,"livemode":binding.scope.livemode,"apiVersion":snapshot.api_version,"profile":profile,"proof":snapshot.proof});
        super::super::super::subscription_creation::budget(&expected, 0, &mut 0, &mut 0)?;
        let expected_text = canonical(&expected).to_string();
        if expected_text != text || hash(text) != row.2 {
            return Err(conflict());
        }
        let (reservation,key,recorded_at):(String,String,i64)=sqlx::query_as("SELECT reservation_id::text,idempotency_key,floor(extract(epoch FROM created_at))::bigint FROM fl_billing.payment_link_expiration_intents WHERE tenant_id=$1 AND operation_id=$2::uuid AND state='pending'")
            .bind(self.tenant.id()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(db_error)?;
        let claims:Vec<(String,String)>=sqlx::query_as("SELECT dependency_kind,dependency_id FROM fl_billing.payment_link_expiration_dependency_claims WHERE tenant_id=$1 AND operation_id=$2::uuid ORDER BY dependency_kind")
            .bind(self.tenant.id()).bind(operation.to_string()).fetch_all(&mut *tx).await.map_err(db_error)?;
        if claims
            != vec![
                (
                    "payment_link_binding".into(),
                    binding.binding_id.to_string(),
                ),
                ("payment_link_record".into(), binding.storage_id.clone()),
            ]
        {
            return Err(conflict());
        }
        // Reuse the existing allowed inspect audit action; no publication/release occurs here.
        self.expiration_audit(&mut tx, operation, "inspect").await?;
        tx.commit().await.map_err(db_error)?;
        Ok(PaymentLinkExpirationRecovery {
            completion: Completion {
                operation,
                public_id: binding.public_id,
                reservation: Uuid::parse_str(&reservation).map_err(|_| storage_error())?,
                hash: row.2,
                canonical: text.into(),
                snapshot: Box::new(snapshot),
            },
            key,
            recorded_at,
        })
    }
}

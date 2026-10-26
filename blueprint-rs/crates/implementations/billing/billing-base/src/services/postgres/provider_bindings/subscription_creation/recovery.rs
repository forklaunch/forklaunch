//! Publication-only recovery snapshots; never reconstruct dispatch authority.
use super::{
    PostgresBillingStore, StripeBindingScope, SubscriptionDependencySnapshot, budget, canonical,
    conflict, db_error, publication::Publication, storage_error,
};
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use uuid::Uuid;

/// Opaque same-actor pending intent. No key getter, Clone, Debug or Serde.
/// Matching a key identifies a provider request, not its originating process.
pub struct SubscriptionCreationRecovery {
    pub(super) publication: Publication,
    key: String,
    created: i64,
    payload: Value,
}
impl SubscriptionCreationRecovery {
    #[must_use]
    pub fn matches_event_request_key(&self, candidate: &str) -> bool {
        self.key == candidate
    }
    #[must_use]
    pub const fn recorded_at_seconds(&self) -> i64 {
        self.created
    }
    #[must_use]
    pub const fn public_id(&self) -> Uuid {
        self.publication.public_id
    }
    #[must_use]
    pub fn payload(&self) -> &Value {
        &self.payload
    }
    #[must_use]
    pub fn scope(&self) -> &StripeBindingScope {
        &self.publication.snapshot.scope
    }
    #[must_use]
    pub fn api_version(&self) -> &str {
        &self.publication.snapshot.api_version
    }
    #[must_use]
    pub fn dependencies(&self) -> &SubscriptionDependencySnapshot {
        &self.publication.snapshot
    }
}
fn uuid(value: &Value, field: &str) -> BlueprintResult<Uuid> {
    Uuid::parse_str(value[field].as_str().ok_or_else(storage_error)?).map_err(|_| storage_error())
}
type Extra = (String, String, i64);
impl PostgresBillingStore {
    /// Checks recovery/inspection grants without granting reservation or dispatch authority.
    /// # Errors
    /// Rejects missing dependency-lock, intent/claims SELECT or audit INSERT privileges.
    pub async fn validate_subscription_recovery_access(&self) -> BlueprintResult<()> {
        self.subscription_dependency_access().await?;
        let allowed:bool=sqlx::query_scalar("SELECT has_table_privilege(current_user,'fl_billing.subscription_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.subscription_dependency_claims','SELECT') AND has_table_privilege(current_user,'fl_billing.subscription_creation_audit','INSERT')").fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        Ok(())
    }

    /// Restores only publication authority from an immutable pending journal.
    /// Locks exact current dependencies and checks all four persistent claims.
    /// No provider verification is performed here; the trusted adapter does that.
    /// # Errors
    /// Denies actor/scope/dependency changes, missing claims or completed intents.
    pub async fn snapshot_subscription_recovery(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<SubscriptionCreationRecovery> {
        let mut tx = self.transaction().await?;
        self.subscription_runtime_role(&mut tx).await?;
        let row = self
            .subscription_intent_row(&mut tx, operation)
            .await?
            .ok_or_else(conflict)?;
        if row.4 != "pending" || row.5.is_some() {
            return Err(conflict());
        }
        let envelope = self.open_subscription_intent(operation, &row)?;
        budget(&envelope, 0, &mut 0, &mut 0)?;
        if envelope["tenant"] != self.tenant.id()
            || envelope["actor"] != self.binding_actor()?
            || envelope["operation"] != operation.to_string()
        {
            return Err(conflict());
        }
        let scope = StripeBindingScope::administrative(
            envelope["deployment"]
                .as_str()
                .ok_or_else(storage_error)?
                .into(),
            envelope["account"]
                .as_str()
                .ok_or_else(storage_error)?
                .into(),
            envelope["livemode"].as_bool().ok_or_else(storage_error)?,
        )?;
        let snapshot = SubscriptionDependencySnapshot {
            tenant: self.tenant.id().into(),
            actor: self.binding_actor()?.into(),
            scope,
            api_version: envelope["apiVersion"]
                .as_str()
                .ok_or_else(storage_error)?
                .into(),
            customer: uuid(&envelope, "customer")?,
            plan: uuid(&envelope, "plan")?,
            plan_operation: uuid(&envelope, "planOperation")?,
            data: envelope["dependencies"].clone(),
        };
        // The runtime intentionally has no journal UPDATE policy, so SELECT
        // FOR UPDATE would be invisible under RLS. Exact dependency locks also
        // serialize completion (the publisher locks them first), allowing this
        // immutable journal read/recheck without granting runtime completion rights.
        if self
            .subscription_dependency_data(&mut tx, &snapshot)
            .await?
            != snapshot.data
        {
            return Err(conflict());
        }
        let extra:Extra=sqlx::query_as("SELECT reservation_id::text,idempotency_key,floor(extract(epoch FROM created_at))::bigint FROM fl_billing.subscription_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid")
            .bind(self.tenant.id()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(db_error)?;
        if self
            .subscription_intent_row(&mut tx, operation)
            .await?
            .as_ref()
            != Some(&row)
        {
            return Err(conflict());
        }
        let claims:Vec<(String,String)>=sqlx::query_as("SELECT dependency_kind,dependency_id FROM fl_billing.subscription_dependency_claims WHERE tenant_id=$1 AND operation_id=$2::uuid ORDER BY dependency_kind")
            .bind(self.tenant.id()).bind(operation.to_string()).fetch_all(&mut *tx).await.map_err(db_error)?;
        let expected = vec![
            (
                "customer_binding".into(),
                snapshot.data["customerBinding"]["binding_id"]
                    .as_str()
                    .ok_or_else(storage_error)?
                    .into(),
            ),
            ("customer_record".into(), snapshot.customer.to_string()),
            (
                "plan_binding".into(),
                snapshot.data["planBinding"]["binding_id"]
                    .as_str()
                    .ok_or_else(storage_error)?
                    .into(),
            ),
            ("plan_record".into(), snapshot.plan.to_string()),
        ];
        if claims != expected {
            return Err(conflict());
        }
        self.audit_subscription_creation(&mut tx, operation, "inspect")
            .await?;
        tx.commit().await.map_err(db_error)?;
        Ok(SubscriptionCreationRecovery {
            publication: Publication {
                operation_id: operation,
                public_id: Uuid::parse_str(&row.0).map_err(|_| storage_error())?,
                reservation_id: Uuid::parse_str(&extra.0).map_err(|_| storage_error())?,
                payload_hash: row.2,
                canonical: canonical(&envelope).to_string(),
                snapshot: Box::new(snapshot),
            },
            key: extra.1,
            created: extra.2,
            payload: envelope["payload"].clone(),
        })
    }
    /// Historical encrypted recovery evidence, bound to operation/public ID/hash.
    /// Direct-created receipts remain valid and return no recovery evidence.
    /// # Errors
    /// Denies wrong actors, corrupt evidence and audit/storage failures.
    pub async fn inspect_subscription_recovery_evidence(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<Value>> {
        let mut tx = self.transaction().await?;
        self.subscription_runtime_role(&mut tx).await?;
        let result = if let Some(row) = self.subscription_intent_row(&mut tx, operation).await? {
            self.open_subscription_intent(operation, &row)?;
            let evidence = if row.4 == "completed" {
                self.open_subscription_receipt_envelope(operation, &row)?
                    .get("publicationEvidence")
                    .cloned()
            } else {
                None
            };
            self.audit_subscription_creation(&mut tx, operation, "inspect")
                .await?;
            evidence
        } else {
            None
        };
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
}

//! Completed payment-link provenance is read authority only, never a dispatch capability.
use super::{DurableStripeBinding, PostgresBillingStore, conflict, db_error, storage_error};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Opaque completed-creation snapshot. No dispatch key or publication authority.
pub struct PaymentLinkCreationReadSnapshot {
    pub(super) binding: DurableStripeBinding,
    pub(super) actor: String,
    pub(super) api_version: String,
    pub(super) proof: Value,
    pub(super) payload: Value,
    pub(super) record: Value,
}
impl PaymentLinkCreationReadSnapshot {
    /// Serialized size of retained bounded proof, payload and record; no proof exposure.
    /// # Errors
    /// Rejects serialization or size overflow.
    pub fn evidence_size(&self) -> BlueprintResult<usize> {
        struct Count(usize);
        impl std::io::Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .ok_or_else(|| std::io::Error::other("size overflow"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        serde_json::to_writer(&mut count, &(&self.proof, &self.payload, &self.record))
            .map_err(|_| storage_error())?;
        Ok(count.0)
    }
    /// Immutable creation request/profile, not mutable local provider fields.
    #[must_use]
    pub fn payload(&self) -> &Value {
        &self.payload
    }
}
impl PostgresBillingStore {
    /// Checks local read/audit privileges only; never performs provider I/O.
    /// # Errors
    /// Denies missing schema usage, table SELECT or sensitive-audit INSERT.
    pub async fn validate_payment_link_creation_read_access(&self) -> BlueprintResult<()> {
        let allowed:bool=sqlx::query_scalar("SELECT has_schema_privilege(current_user,'fl_billing','USAGE') AND has_table_privilege(current_user,'fl_billing.records','SELECT') AND has_table_privilege(current_user,'fl_billing.provider_bindings','SELECT') AND has_table_privilege(current_user,'fl_billing.payment_link_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.payment_link_expiration_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.sensitive_read_audit','INSERT')").fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        Ok(())
    }

    fn open_payment_link_read_evidence(
        &self,
        operation: Uuid,
        field: &str,
        ciphertext: &str,
    ) -> BlueprintResult<Value> {
        // Bound ciphertext/decrypted JSON before parsing, budgeting or cloning.
        if ciphertext.len() > 524_288 {
            return Err(conflict());
        }
        let raw = self
            .encryptor
            .decrypt(ciphertext, self.tenant.id())
            .map_err(|_| storage_error())?;
        if raw.len() > 262_144 {
            return Err(conflict());
        }
        let envelope: Value = serde_json::from_str(&raw).map_err(|_| storage_error())?;
        super::super::subscription_creation::budget(&envelope, 0, &mut 0, &mut 0)
            .map_err(|_| conflict())?;
        if envelope["kind"] != "payment_link_creation"
            || envelope["operationId"] != operation.to_string()
            || envelope["field"] != field
        {
            return Err(conflict());
        }
        Ok(envelope["value"].clone())
    }
    /// Reads actor-owned completed provenance and the current exact durable body.
    /// Missing provenance never falls back to an administrative alias.
    /// # Errors
    /// Rejects foreign scope/actor, incomplete journals and modified bodies/bindings.
    pub async fn payment_link_creation_read_snapshot(
        &self,
        binding: &DurableStripeBinding,
        api_version: &str,
    ) -> BlueprintResult<PaymentLinkCreationReadSnapshot> {
        if binding.tenant != self.tenant.id() || binding.kind != BillingKind::PaymentLink {
            return Err(conflict());
        }
        let actor = self.binding_actor()?.to_owned();
        let mut tx = self.transaction().await?;
        let operation: Option<String> = sqlx::query_scalar("SELECT operation_id::text FROM fl_billing.payment_link_creation_intents WHERE tenant_id=$1 AND public_id=$2::uuid")
            .bind(self.tenant.id()).bind(binding.public_id.to_string())
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let operation =
            Uuid::parse_str(&operation.ok_or_else(conflict)?).map_err(|_| storage_error())?;
        let row = self
            .creation_row(&mut tx, operation)
            .await?
            .ok_or_else(conflict)?;
        if row.1 != actor || row.7 != "completed" {
            return Err(conflict());
        }
        let sealed = self.open_payment_link_read_evidence(operation, "intent", &row.8)?;
        let canonical = sealed["canonical"]
            .as_str()
            .filter(|v| v.len() <= 65_536)
            .ok_or_else(storage_error)?;
        if sealed["publicId"] != row.0 || super::intent_hash(&row.0, canonical) != row.6 {
            return Err(conflict());
        }
        let intent: Value = serde_json::from_str(canonical).map_err(|_| storage_error())?;
        super::super::subscription_creation::budget(&intent, 0, &mut 0, &mut 0)
            .map_err(|_| conflict())?;
        let receipt = self.open_payment_link_read_evidence(
            operation,
            "receipt",
            row.9.as_deref().ok_or_else(conflict)?,
        )?;
        if row.0 != binding.public_id.to_string()
            || row.2 != binding.scope.deployment_label
            || row.3 != binding.scope.account_id
            || row.4 != api_version
            || row.5 != binding.scope.livemode
            || intent["tenant"] != self.tenant.id()
            || intent["operation"] != operation.to_string()
            || intent["deployment"] != binding.scope.deployment_label
            || intent["account"] != binding.scope.account_id
            || intent["livemode"] != binding.scope.livemode
            || intent["apiVersion"] != api_version
            || receipt["id"] != binding.storage_id
            || receipt["providerFields"]["id"] != binding.provider_id
        {
            return Err(conflict());
        }
        let observed = self
            .payment_link_read_observation(&mut tx, binding, operation)
            .await?;
        if observed["intent"] != row.8
            || observed["receipt"] != row.9.clone().map_or(Value::Null, Value::String)
            || observed["hash"] != row.6
        {
            return Err(conflict());
        }
        super::super::subscription_creation::budget(&observed, 0, &mut 0, &mut 0)
            .map_err(|_| conflict())?;
        let record = self
            .decode(
                &mut tx,
                Scope::Durable,
                BillingKind::PaymentLink,
                &binding.storage_id,
                observed["record"].clone(),
                "payment_link_creation_read",
            )
            .await?;
        if record != receipt {
            return Err(conflict());
        }
        // Auditing can block. Reobserve after it, so an earlier matching SELECT
        // cannot conceal changes committed while waiting for the audit write.
        if self
            .payment_link_read_observation(&mut tx, binding, operation)
            .await?
            != observed
        {
            return Err(conflict());
        }
        tx.commit().await.map_err(db_error)?;
        Ok(PaymentLinkCreationReadSnapshot {
            binding: binding.clone(),
            actor,
            api_version: api_version.into(),
            proof: json!({"observed":observed,"operation":operation.to_string()}),
            payload: intent["payload"].clone(),
            record,
        })
    }
    pub(super) async fn payment_link_read_observation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: &DurableStripeBinding,
        operation: Uuid,
    ) -> BlueprintResult<Value> {
        let value: Option<sqlx::types::Json<Value>> = sqlx::query_scalar("SELECT jsonb_build_object('record',r.data,'recordVersion',r.xmin::text,'binding',to_jsonb(b),'bindingVersion',b.xmin::text,'intent',j.intent_ciphertext,'receipt',j.receipt_ciphertext,'hash',j.payload_hash,'journalVersion',j.xmin::text) FROM fl_billing.provider_bindings b JOIN fl_billing.records r ON r.tenant_id=b.tenant_id AND r.scope=b.scope AND r.kind=b.kind AND r.id=b.storage_id JOIN fl_billing.payment_link_creation_intents j ON j.tenant_id=b.tenant_id AND j.public_id=b.public_id AND j.operation_id=$9::uuid AND j.state='completed' AND j.actor_id=$10 WHERE b.tenant_id=$1 AND b.kind='payment_link' AND b.scope='durable' AND b.public_id=$2::uuid AND b.binding_id=$3::uuid AND b.storage_id=$4 AND b.provider_id=$5 AND b.deployment_label=$6 AND b.account_id=$7 AND b.livemode=$8 AND r.data->>'id'=$4 AND NOT EXISTS(SELECT 1 FROM fl_billing.payment_link_expiration_intents e WHERE e.tenant_id=b.tenant_id AND e.public_id=b.public_id AND e.state='completed')")
            .bind(self.tenant.id()).bind(binding.public_id.to_string()).bind(binding.binding_id.to_string())
            .bind(&binding.storage_id).bind(&binding.provider_id).bind(&binding.scope.deployment_label)
            .bind(&binding.scope.account_id).bind(binding.scope.livemode).bind(operation.to_string())
            .bind(self.binding_actor()?).fetch_optional(&mut **tx).await.map_err(db_error)?;
        Ok(value.ok_or_else(conflict)?.0)
    }
    /// Rechecks full provenance after provider I/O. The final matching SELECT is
    /// the data observation point; callers must check provider freshness after
    /// this awaited method (including audit commit), not before it.
    /// # Errors
    /// Rejects changed actor, journal, binding generation/xmin or exact local body.
    pub async fn read_payment_link_creation_snapshot(
        &self,
        snapshot: &PaymentLinkCreationReadSnapshot,
    ) -> BlueprintResult<Value> {
        if snapshot.actor != self.binding_actor()? {
            return Err(conflict());
        }
        let current = self
            .payment_link_creation_read_snapshot(&snapshot.binding, &snapshot.api_version)
            .await?;
        if current.proof != snapshot.proof {
            return Err(conflict());
        }
        Ok(current.record)
    }
}

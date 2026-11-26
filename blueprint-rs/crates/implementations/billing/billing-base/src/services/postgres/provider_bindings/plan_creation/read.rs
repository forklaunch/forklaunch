//! Completed Plan provenance is read authority only, never a dispatch capability.
use super::{DurableStripeBinding, PostgresBillingStore, conflict, db_error, storage_error};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Opaque completed-creation snapshot. No dispatch key or publication authority.
pub struct PlanCreationReadSnapshot {
    binding: DurableStripeBinding,
    actor: String,
    api_version: String,
    proof: Value,
    payload: Value,
    record: Value,
}
impl PlanCreationReadSnapshot {
    /// Immutable creation request/profile, not mutable local provider fields.
    #[must_use]
    pub fn payload(&self) -> &Value {
        &self.payload
    }
}
impl PostgresBillingStore {
    /// Checks local table-level read/audit privileges only, without provider I/O
    /// or publishing authority. Existing generic CRUD privileges are not changed.
    /// # Errors
    /// Fails before listening when schema usage, SELECT or audit INSERT is absent.
    pub async fn validate_plan_creation_read_access(&self) -> BlueprintResult<()> {
        let allowed: bool = sqlx::query_scalar("SELECT has_schema_privilege(current_user,'fl_billing','USAGE') AND has_table_privilege(current_user,'fl_billing.records','SELECT') AND has_table_privilege(current_user,'fl_billing.provider_bindings','SELECT') AND has_table_privilege(current_user,'fl_billing.plan_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.sensitive_read_audit','INSERT')")
            .fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        Ok(())
    }

    /// Reads actor-owned completed provenance and the current exact durable body.
    /// Missing provenance never falls back to an administrative alias.
    /// # Errors
    /// Rejects foreign scope/actor, incomplete journals and modified bodies/bindings.
    pub async fn plan_creation_read_snapshot(
        &self,
        binding: &DurableStripeBinding,
        api_version: &str,
    ) -> BlueprintResult<PlanCreationReadSnapshot> {
        if binding.tenant != self.tenant.id() || binding.kind != BillingKind::Plan {
            return Err(conflict());
        }
        let actor = self.binding_actor()?.to_owned();
        let mut tx = self.transaction().await?;
        let operation: Option<String> = sqlx::query_scalar("SELECT operation_id::text FROM fl_billing.plan_creation_intents WHERE tenant_id=$1 AND public_id=$2::uuid")
            .bind(self.tenant.id()).bind(binding.public_id.to_string())
            .fetch_optional(&mut *tx).await.map_err(db_error)?;
        let operation =
            Uuid::parse_str(&operation.ok_or_else(conflict)?).map_err(|_| storage_error())?;
        let row = self
            .plan_creation_row(&mut tx, operation)
            .await?
            .ok_or_else(conflict)?;
        let super::PlanCreationStatus::Completed {
            record: receipt, ..
        } = self.plan_creation_status(operation, row.clone())?
        else {
            return Err(conflict());
        };
        let sealed = self.open_plan_creation(operation, "intent", &row.8)?;
        let intent: Value =
            serde_json::from_str(sealed["canonical"].as_str().ok_or_else(storage_error)?)
                .map_err(|_| storage_error())?;
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
            || receipt["externalId"] != binding.provider_id
        {
            return Err(conflict());
        }
        let observed = self
            .plan_read_observation(&mut tx, binding, operation)
            .await?;
        if observed["intent"] != row.8
            || observed["receipt"] != row.9.clone().map_or(Value::Null, Value::String)
            || observed["hash"] != row.6
        {
            return Err(conflict());
        }
        let record = self
            .decode(
                &mut tx,
                Scope::Durable,
                BillingKind::Plan,
                &binding.storage_id,
                observed["record"].clone(),
                "plan_creation_read",
            )
            .await?;
        if record != receipt {
            return Err(conflict());
        }
        // Auditing can block. Reobserve after it, so an earlier matching SELECT
        // cannot conceal changes committed while waiting for the audit write.
        if self
            .plan_read_observation(&mut tx, binding, operation)
            .await?
            != observed
        {
            return Err(conflict());
        }
        tx.commit().await.map_err(db_error)?;
        Ok(PlanCreationReadSnapshot {
            binding: binding.clone(),
            actor,
            api_version: api_version.into(),
            proof: json!({"observed":observed,"operation":operation.to_string()}),
            payload: intent["payload"].clone(),
            record,
        })
    }
    async fn plan_read_observation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: &DurableStripeBinding,
        operation: Uuid,
    ) -> BlueprintResult<Value> {
        let value: Option<sqlx::types::Json<Value>> = sqlx::query_scalar("SELECT jsonb_build_object('record',r.data,'recordVersion',r.xmin::text,'binding',to_jsonb(b),'bindingVersion',b.xmin::text,'intent',j.intent_ciphertext,'receipt',j.receipt_ciphertext,'hash',j.payload_hash,'journalVersion',j.xmin::text) FROM fl_billing.provider_bindings b JOIN fl_billing.records r ON r.tenant_id=b.tenant_id AND r.scope=b.scope AND r.kind=b.kind AND r.id=b.storage_id JOIN fl_billing.plan_creation_intents j ON j.tenant_id=b.tenant_id AND j.public_id=b.public_id AND j.operation_id=$9::uuid AND j.state='completed' AND j.actor_id=$10 WHERE b.tenant_id=$1 AND b.kind='plan' AND b.scope='durable' AND b.public_id=$2::uuid AND b.binding_id=$3::uuid AND b.storage_id=$4 AND b.provider_id=$5 AND b.deployment_label=$6 AND b.account_id=$7 AND b.livemode=$8 AND r.data->>'id'=$4")
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
    pub async fn read_plan_creation_snapshot(
        &self,
        snapshot: &PlanCreationReadSnapshot,
    ) -> BlueprintResult<Value> {
        if snapshot.actor != self.binding_actor()? {
            return Err(conflict());
        }
        let current = self
            .plan_creation_read_snapshot(&snapshot.binding, &snapshot.api_version)
            .await?;
        if current.proof != snapshot.proof {
            return Err(conflict());
        }
        Ok(current.record)
    }
}

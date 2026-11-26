//! Immutable dependency reservations and explicitly trusted publication.
use super::super::{db_error, invalid, storage_error};
use super::{PostgresBillingStore, StripeBindingScope, conflict};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
mod publication;
mod read;
mod recovery;
pub use publication::{SubscriptionCreationStatus, TrustedSubscriptionPublisher};
pub use read::SubscriptionCreationReadSnapshot;
pub use recovery::SubscriptionCreationRecovery;

/// Opaque local evidence, not authenticated provider evidence. The external Party
/// association remains an administrative assertion, not a cross-service FK.
pub struct SubscriptionDependencySnapshot {
    tenant: String,
    actor: String,
    scope: StripeBindingScope,
    api_version: String,
    customer: Uuid,
    plan: Uuid,
    plan_operation: Uuid,
    data: Value,
}
impl SubscriptionDependencySnapshot {
    #[must_use]
    pub fn provider_customer_id(&self) -> &str {
        self.data["customerBinding"]["provider_customer_id"]
            .as_str()
            .unwrap_or_default()
    }
    #[must_use]
    pub fn provider_plan_id(&self) -> &str {
        self.data["planBinding"]["provider_id"]
            .as_str()
            .unwrap_or_default()
    }
    #[must_use]
    pub fn party_id(&self) -> &str {
        self.data["customerBinding"]["party_id"]
            .as_str()
            .unwrap_or_default()
    }
    #[must_use]
    pub fn party_type(&self) -> &str {
        self.data["customerBinding"]["party_type"]
            .as_str()
            .unwrap_or_default()
    }
    #[must_use]
    pub fn plan_receipt(&self) -> &Value {
        &self.data["planReceipt"]
    }
}
/// Confirmed local reservation for the explicitly trusted provider adapter.
/// No Clone/Debug/Serde. Inspection never reconstructs this capability.
pub struct SubscriptionCreationReservation {
    operation_id: Uuid,
    public_id: Uuid,
    reservation_id: Uuid,
    payload_hash: String,
    idempotency_key: String,
    canonical: String,
    snapshot: Box<SubscriptionDependencySnapshot>,
}
impl SubscriptionCreationReservation {
    #[must_use]
    pub const fn operation_id(&self) -> Uuid {
        self.operation_id
    }
    #[must_use]
    pub const fn public_id(&self) -> Uuid {
        self.public_id
    }
    #[must_use]
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }
}
/// Historical unresolved intent. No expiry or implicit retry/release.
pub struct SubscriptionCreationPending {
    pub operation_id: Uuid,
    pub public_id: Uuid,
}
pub enum SubscriptionCreationClaim {
    Reserved(SubscriptionCreationReservation),
    Existing(SubscriptionCreationPending),
    Completed { operation_id: Uuid, record: Value },
}
type RecordRow = (String, sqlx::types::Json<Value>);
type IntentRow = (String, String, String, String, String, Option<String>);

// This journal carries two encrypted record snapshots plus sealed provenance,
// hence a 256 KiB envelope budget rather than the simpler 64 KiB Plan profile.
// Count worst-case JSON escaping before cloning/sorting public input. Recursion
// is bounded to 32 levels and 8192 nodes, including the outer envelope.
pub(super) fn budget(
    value: &Value,
    depth: usize,
    nodes: &mut usize,
    bytes: &mut usize,
) -> BlueprintResult<()> {
    if depth > 32 || *nodes >= 8192 {
        return Err(invalid());
    }
    *nodes += 1;
    *bytes = bytes.checked_add(32).ok_or_else(invalid)?;
    match value {
        Value::String(value) => {
            *bytes = bytes
                .checked_add(value.len().checked_mul(6).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
        }
        Value::Array(values) => {
            for value in values {
                budget(value, depth + 1, nodes, bytes)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                *bytes = bytes
                    .checked_add(key.len().checked_mul(6).ok_or_else(invalid)?)
                    .ok_or_else(invalid)?;
                if *bytes > 262_144 {
                    return Err(invalid());
                }
                budget(value, depth + 1, nodes, bytes)?;
            }
        }
        _ => {}
    }
    if *bytes > 262_144 {
        return Err(invalid());
    }
    Ok(())
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonical(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        value => value.clone(),
    }
}
fn hash(public_id: &str, value: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"forklaunch/subscription-reservation/v1\0");
    hash.update(public_id.as_bytes());
    hash.update(b"\0");
    hash.update(value.as_bytes());
    base16ct::lower::encode_string(&hash.finalize())
}
impl PostgresBillingStore {
    async fn subscription_dependency_access(&self) -> BlueprintResult<()> {
        let allowed:bool=sqlx::query_scalar("SELECT has_schema_privilege(current_user,'fl_billing','USAGE') AND has_table_privilege(current_user,'fl_billing.records','SELECT') AND has_table_privilege(current_user,'fl_billing.records','UPDATE') AND has_table_privilege(current_user,'fl_billing.provider_bindings','SELECT') AND has_table_privilege(current_user,'fl_billing.provider_bindings','UPDATE') AND has_table_privilege(current_user,'fl_billing.stripe_customer_bindings','SELECT') AND has_table_privilege(current_user,'fl_billing.stripe_customer_bindings','UPDATE') AND has_table_privilege(current_user,'fl_billing.plan_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.sensitive_read_audit','INSERT') AND has_table_privilege(current_user,'fl_billing.subscription_publication_identity','SELECT')").fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        Ok(())
    }
    /// Checks ordinary-role grants used for dependency locks, reservation and inspection.
    /// # Errors
    /// Rejects missing local privileges; does not contact the provider or claim ownership.
    pub async fn validate_subscription_creation_access(&self) -> BlueprintResult<()> {
        self.subscription_dependency_access().await?;
        let allowed:bool=sqlx::query_scalar("SELECT has_table_privilege(current_user,'fl_billing.subscription_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.subscription_creation_intents','INSERT') AND has_table_privilege(current_user,'fl_billing.subscription_dependency_claims','INSERT') AND has_table_privilege(current_user,'fl_billing.subscription_creation_audit','INSERT')").fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        Ok(())
    }

    async fn subscription_runtime_role(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> BlueprintResult<()> {
        let member:bool=sqlx::query_scalar("SELECT pg_has_role(current_user,relowner,'MEMBER') FROM pg_class WHERE oid='fl_billing.subscription_dependency_guards'::regclass")
            .fetch_one(&mut **tx).await.map_err(db_error)?;
        if member {
            return Err(invalid());
        }
        Ok(())
    }
    async fn subscription_record(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        kind: &str,
        id: Uuid,
    ) -> BlueprintResult<RecordRow> {
        let row:RecordRow=sqlx::query_as("SELECT xmin::text,data FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind=$2 AND id=$3 FOR UPDATE")
            .bind(self.tenant.id()).bind(kind).bind(id.to_string()).fetch_optional(&mut **tx).await.map_err(db_error)?.ok_or_else(conflict)?;
        if row.1.0["id"] != id.to_string() {
            return Err(conflict());
        }
        Ok(row)
    }
    async fn subscription_dependency_data(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        snapshot: &SubscriptionDependencySnapshot,
    ) -> BlueprintResult<Value> {
        // Fixed order also used by reservation: records Customer then Plan,
        // followed by Customer binding then Plan binding, then guard counters.
        let customer = self
            .subscription_record(tx, "customer", snapshot.customer)
            .await?;
        let plan = self.subscription_record(tx, "plan", snapshot.plan).await?;
        let customer_binding:sqlx::types::Json<Value>=sqlx::query_scalar("SELECT to_jsonb(b) FROM fl_billing.stripe_customer_bindings b WHERE tenant_id=$1 AND deployment_label=$2 AND account_id=$3 AND livemode=$4 AND local_customer_id=$5::uuid AND storage_id=$6 AND verified_api_version=$7 FOR UPDATE")
            .bind(self.tenant.id()).bind(&snapshot.scope.deployment_label).bind(&snapshot.scope.account_id).bind(snapshot.scope.livemode).bind(snapshot.customer.to_string()).bind(snapshot.customer.to_string()).bind(&snapshot.api_version).fetch_optional(&mut **tx).await.map_err(db_error)?.ok_or_else(conflict)?;
        let plan_binding:sqlx::types::Json<Value>=sqlx::query_scalar("SELECT to_jsonb(b) FROM fl_billing.provider_bindings b WHERE tenant_id=$1 AND deployment_label=$2 AND account_id=$3 AND livemode=$4 AND kind='plan' AND public_id=$5::uuid AND storage_id=$6 FOR UPDATE")
            .bind(self.tenant.id()).bind(&snapshot.scope.deployment_label).bind(&snapshot.scope.account_id).bind(snapshot.scope.livemode).bind(snapshot.plan.to_string()).bind(snapshot.plan.to_string()).fetch_optional(&mut **tx).await.map_err(db_error)?.ok_or_else(conflict)?;
        let provenance:Option<(String,String)>=sqlx::query_as("SELECT intent_ciphertext,receipt_ciphertext FROM fl_billing.plan_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid AND public_id=$3::uuid AND deployment_label=$4 AND account_id=$5 AND livemode=$6 AND api_version=$7 AND state='completed'")
            .bind(self.tenant.id()).bind(snapshot.plan_operation.to_string()).bind(snapshot.plan.to_string()).bind(&snapshot.scope.deployment_label).bind(&snapshot.scope.account_id).bind(snapshot.scope.livemode).bind(&snapshot.api_version).fetch_optional(&mut **tx).await.map_err(db_error)?;
        let (intent_ciphertext, receipt_ciphertext) = provenance.ok_or_else(conflict)?;
        let receipt =
            self.open_plan_creation(snapshot.plan_operation, "receipt", &receipt_ciphertext)?;
        let current = self
            .decode(
                tx,
                Scope::Durable,
                BillingKind::Plan,
                &snapshot.plan.to_string(),
                plan.1.0.clone(),
                "subscription_dependency",
            )
            .await?;
        if receipt != current
            || receipt["id"] != snapshot.plan.to_string()
            || receipt["externalId"] != plan_binding.0["provider_id"]
        {
            return Err(conflict());
        }
        Ok(
            json!({"customerVersion":customer.0,"customerData":customer.1.0,"planVersion":plan.0,"planData":plan.1.0,"customerBinding":customer_binding.0,"planBinding":plan_binding.0,"planReceipt":receipt,"planIntentCiphertext":intent_ciphertext,"planReceiptCiphertext":receipt_ciphertext}),
        )
    }
    /// Locks and snapshots exact local generations and completed Plan provenance.
    /// Does not verify Stripe or the lifecycle of an externally owned Party.
    /// # Errors
    /// Denies foreign/missing authority, modified Plan DTOs and unsafe owner roles.
    pub async fn snapshot_subscription_dependencies(
        &self,
        scope: &StripeBindingScope,
        customer: Uuid,
        plan: Uuid,
        plan_operation: Uuid,
        api_version: &str,
    ) -> BlueprintResult<SubscriptionDependencySnapshot> {
        let actor = self.binding_actor()?;
        if [customer, plan, plan_operation].iter().any(Uuid::is_nil)
            || api_version.is_empty()
            || api_version.len() > 128
        {
            return Err(invalid());
        }
        let mut snapshot = SubscriptionDependencySnapshot {
            tenant: self.tenant.id().into(),
            actor: actor.into(),
            scope: scope.clone(),
            api_version: api_version.into(),
            customer,
            plan,
            plan_operation,
            data: Value::Null,
        };
        let mut tx = self.transaction().await?;
        self.subscription_runtime_role(&mut tx).await?;
        snapshot.data = self
            .subscription_dependency_data(&mut tx, &snapshot)
            .await?;
        tx.commit().await.map_err(db_error)?;
        Ok(snapshot)
    }
    async fn audit_subscription_creation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        operation: Uuid,
        action: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("INSERT INTO fl_billing.subscription_creation_audit(id,tenant_id,operation_id,actor_id,correlation_id,operation) VALUES($1::uuid,$2,$3::uuid,$4,$5,$6)")
            .bind(Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(operation.to_string()).bind(self.binding_actor()?).bind(&self.audit.correlation_id).bind(action).execute(&mut **tx).await.map_err(db_error)?;
        Ok(())
    }
    fn open_subscription_intent(&self, operation: Uuid, row: &IntentRow) -> BlueprintResult<Value> {
        if row.1 != self.binding_actor()? {
            return Err(conflict());
        }
        let plaintext = self
            .encryptor
            .decrypt(&row.3, self.tenant.id())
            .map_err(|_| storage_error())?;
        let envelope: Value = serde_json::from_str(&plaintext).map_err(|_| storage_error())?;
        let canonical = envelope["canonical"].as_str().ok_or_else(storage_error)?;
        if envelope["kind"] != "subscription_creation"
            || envelope["operationId"] != operation.to_string()
            || envelope["publicId"] != row.0
            || hash(&row.0, canonical) != row.2
        {
            return Err(storage_error());
        }
        serde_json::from_str(canonical).map_err(|_| storage_error())
    }
    async fn subscription_intent_row(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        operation: Uuid,
    ) -> BlueprintResult<Option<IntentRow>> {
        sqlx::query_as("SELECT public_id::text,actor_id,payload_hash,intent_ciphertext,state,receipt_ciphertext FROM fl_billing.subscription_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid")
            .bind(self.tenant.id()).bind(operation.to_string()).fetch_optional(&mut **tx).await.map_err(db_error)
    }
    /// Trusted-adapter reservation only: caller freshness checks are not provider
    /// proof themselves. Only a separate trusted publisher can complete/release.
    /// An uncertain COMMIT may have retained all claims; inspect, never retry.
    /// # Errors
    /// Denies changed snapshots/payloads/actors, audit failure or expired context.
    pub async fn reserve_subscription_creation<F>(
        &self,
        snapshot: SubscriptionDependencySnapshot,
        operation: Uuid,
        payload: &Value,
        fresh: F,
    ) -> BlueprintResult<SubscriptionCreationClaim>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        if snapshot.tenant != self.tenant.id()
            || snapshot.actor != self.binding_actor()?
            || operation.is_nil()
        {
            return Err(conflict());
        }
        // Budget both potentially large inputs before json! clones either one.
        let (mut nodes, mut bytes) = (0, 0);
        budget(payload, 1, &mut nodes, &mut bytes)?;
        budget(&snapshot.data, 1, &mut nodes, &mut bytes)?;
        let envelope = json!({"tenant":self.tenant.id(),"operation":operation.to_string(),"actor":snapshot.actor,"deployment":snapshot.scope.deployment_label,"account":snapshot.scope.account_id,"livemode":snapshot.scope.livemode,"apiVersion":snapshot.api_version,"customer":snapshot.customer.to_string(),"plan":snapshot.plan.to_string(),"planOperation":snapshot.plan_operation.to_string(),"dependencies":snapshot.data,"payload":payload});
        budget(&envelope, 0, &mut 0, &mut 0)?;
        let canonical = canonical(&envelope).to_string();
        if canonical.len() > 262_144 {
            return Err(invalid());
        }
        let public = Uuid::new_v4();
        let reservation_id = Uuid::new_v4();
        let idempotency_key = format!("fl-subscription-{}", Uuid::new_v4());
        let payload_hash = hash(&public.to_string(), &canonical);
        let encrypted=self.encryptor.seal(&json!({"kind":"subscription_creation","operationId":operation.to_string(),"publicId":public.to_string(),"canonical":canonical}).to_string(),self.tenant.id(),"[Billing subscription reservation]").map_err(|_|storage_error())?;
        let mut tx = self.transaction().await?;
        self.subscription_runtime_role(&mut tx).await?;
        if self
            .subscription_dependency_data(&mut tx, &snapshot)
            .await?
            != snapshot.data
        {
            return Err(conflict());
        }
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        let inserted=sqlx::query("INSERT INTO fl_billing.subscription_creation_intents(tenant_id,operation_id,public_id,reservation_id,actor_id,deployment_label,account_id,livemode,api_version,payload_hash,idempotency_key,intent_ciphertext) VALUES($1,$2::uuid,$3::uuid,$4::uuid,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(tenant_id,operation_id) DO NOTHING")
            .bind(self.tenant.id()).bind(operation.to_string()).bind(public.to_string()).bind(reservation_id.to_string()).bind(&snapshot.actor).bind(&snapshot.scope.deployment_label).bind(&snapshot.scope.account_id).bind(snapshot.scope.livemode).bind(&snapshot.api_version).bind(&payload_hash).bind(&idempotency_key).bind(encrypted).execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        let result = if inserted == 1 {
            for (kind, id) in [
                ("customer_record", snapshot.customer.to_string()),
                ("plan_record", snapshot.plan.to_string()),
                (
                    "customer_binding",
                    snapshot.data["customerBinding"]["binding_id"]
                        .as_str()
                        .ok_or_else(storage_error)?
                        .into(),
                ),
                (
                    "plan_binding",
                    snapshot.data["planBinding"]["binding_id"]
                        .as_str()
                        .ok_or_else(storage_error)?
                        .into(),
                ),
            ] {
                sqlx::query("INSERT INTO fl_billing.subscription_dependency_claims(tenant_id,operation_id,dependency_kind,dependency_id) VALUES($1,$2::uuid,$3,$4)").bind(self.tenant.id()).bind(operation.to_string()).bind(kind).bind(id).execute(&mut *tx).await.map_err(db_error)?;
            }
            self.audit_subscription_creation(&mut tx, operation, "reserve")
                .await?;
            SubscriptionCreationClaim::Reserved(SubscriptionCreationReservation {
                operation_id: operation,
                public_id: public,
                reservation_id,
                payload_hash,
                idempotency_key,
                canonical,
                snapshot: Box::new(snapshot),
            })
        } else {
            let row = self
                .subscription_intent_row(&mut tx, operation)
                .await?
                .ok_or_else(conflict)?;
            self.open_subscription_intent(operation, &row)?;
            if row.2 != hash(&row.0, &canonical) {
                return Err(conflict());
            }
            self.audit_subscription_creation(&mut tx, operation, "inspect")
                .await?;
            if row.4 == "completed" {
                SubscriptionCreationClaim::Completed {
                    operation_id: operation,
                    record: self.open_subscription_receipt(operation, &row)?,
                }
            } else if row.4 == "pending" && row.5.is_none() {
                SubscriptionCreationClaim::Existing(SubscriptionCreationPending {
                    operation_id: operation,
                    public_id: Uuid::parse_str(&row.0).map_err(|_| storage_error())?,
                })
            } else {
                return Err(storage_error());
            }
        };
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
    /// Historical unresolved reservation lookup; no dispatch capability or live
    /// provider assertion. Completed operations require `inspect_bound_subscription_creation`.
    /// # Errors
    /// Denies foreign actor, damaged ciphertext and audit/storage failures.
    pub async fn inspect_subscription_creation(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<SubscriptionCreationPending>> {
        self.binding_actor()?;
        let mut tx = self.transaction().await?;
        self.subscription_runtime_role(&mut tx).await?;
        let result = if let Some(row) = self.subscription_intent_row(&mut tx, operation).await? {
            self.open_subscription_intent(operation, &row)?;
            if row.4 != "pending" || row.5.is_some() {
                return Err(conflict());
            }
            self.audit_subscription_creation(&mut tx, operation, "inspect")
                .await?;
            Some(SubscriptionCreationPending {
                operation_id: operation,
                public_id: Uuid::parse_str(&row.0).map_err(|_| storage_error())?,
            })
        } else {
            None
        };
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subscription_envelope_budgets_precede_canonical_allocation() {
        let good = json!({"b":[1,true],"a":"value"});
        assert!(budget(&good, 0, &mut 0, &mut 0).is_ok());
        assert_eq!(
            canonical(&good).to_string(),
            "{\"a\":\"value\",\"b\":[1,true]}"
        );
        let mut nested = Value::Null;
        for _ in 0..40 {
            nested = json!([nested]);
        }
        for bad in [nested, json!(vec![0; 9000]), json!("x".repeat(262_145))] {
            assert!(budget(&bad, 0, &mut 0, &mut 0).is_err());
        }
    }
}

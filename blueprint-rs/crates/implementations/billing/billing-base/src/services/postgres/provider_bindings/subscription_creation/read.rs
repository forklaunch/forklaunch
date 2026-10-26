//! Read-only completed provenance. No dependency locks, claims or publication capability.
use super::super::DurableStripeBinding;
use super::{PostgresBillingStore, budget, conflict, db_error, storage_error};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Opaque completed-creation read evidence; contains no dispatch key or publisher.
pub struct SubscriptionCreationReadSnapshot {
    binding: DurableStripeBinding,
    actor: String,
    version: String,
    proof: Value,
    intent: Value,
    record: Value,
}
impl SubscriptionCreationReadSnapshot {
    #[must_use]
    pub fn payload(&self) -> &Value {
        &self.intent["payload"]
    }
    #[must_use]
    pub fn plan_receipt(&self) -> &Value {
        &self.intent["dependencies"]["planReceipt"]
    }
    #[must_use]
    pub fn provider_customer_id(&self) -> &str {
        self.intent["dependencies"]["customerBinding"]["provider_customer_id"]
            .as_str()
            .unwrap_or_default()
    }
    #[must_use]
    pub fn original_record(&self) -> &Value {
        &self.record
    }
}
fn id(value: &Value, key: &str) -> BlueprintResult<Uuid> {
    Uuid::parse_str(value[key].as_str().ok_or_else(storage_error)?).map_err(|_| storage_error())
}
impl PostgresBillingStore {
    /// Validates local table read and sensitive-audit insertion grants only.
    /// No provider request, row lock, claims or publication privilege is used.
    /// # Errors
    /// Denies missing schema usage or any required table privilege.
    pub async fn validate_subscription_creation_read_access(&self) -> BlueprintResult<()> {
        let allowed: bool = sqlx::query_scalar("SELECT has_schema_privilege(current_user,'fl_billing','USAGE') AND has_table_privilege(current_user,'fl_billing.records','SELECT') AND has_table_privilege(current_user,'fl_billing.provider_bindings','SELECT') AND has_table_privilege(current_user,'fl_billing.stripe_customer_bindings','SELECT') AND has_table_privilege(current_user,'fl_billing.subscription_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.plan_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.sensitive_read_audit','INSERT')")
            .fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        Ok(())
    }

    /// Resolves actor-owned completed direct provenance and exact current local
    /// dependencies without FOR UPDATE or publisher/claim privileges. Plan
    /// provenance may belong to another actor, as authorized at creation time.
    /// # Errors
    /// Denies missing/changed receipts, bodies, ownership, scope or dependency generations.
    pub async fn subscription_creation_read_snapshot(
        &self,
        binding: &DurableStripeBinding,
        version: &str,
    ) -> BlueprintResult<SubscriptionCreationReadSnapshot> {
        if binding.tenant != self.tenant.id() || binding.kind != BillingKind::Subscription {
            return Err(conflict());
        }
        let actor = self.binding_actor()?.to_owned();
        let mut tx = self.transaction().await?;
        let operation:Option<String>=sqlx::query_scalar("SELECT operation_id::text FROM fl_billing.subscription_creation_intents WHERE tenant_id=$1 AND public_id=$2::uuid")
            .bind(self.tenant.id()).bind(binding.public_id.to_string()).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let operation =
            Uuid::parse_str(&operation.ok_or_else(conflict)?).map_err(|_| storage_error())?;
        let row = self
            .subscription_intent_row(&mut tx, operation)
            .await?
            .ok_or_else(conflict)?;
        let intent = self.open_subscription_intent(operation, &row)?;
        let receipt = self.open_subscription_receipt(operation, &row)?;
        budget(&intent, 0, &mut 0, &mut 0)?;
        if intent["tenant"] != self.tenant.id()
            || intent["actor"] != actor
            || intent["operation"] != operation.to_string()
            || intent["deployment"] != binding.scope.deployment_label
            || intent["account"] != binding.scope.account_id
            || intent["livemode"] != binding.scope.livemode
            || intent["apiVersion"] != version
            || receipt["id"] != binding.storage_id
            || receipt["externalId"] != binding.provider_id
            || receipt["productId"] != intent["plan"]
            || receipt["partyId"] != intent["dependencies"]["customerBinding"]["party_id"]
            || receipt["partyType"] != intent["dependencies"]["customerBinding"]["party_type"]
        {
            return Err(conflict());
        }
        let observed = self
            .subscription_read_observation(&mut tx, binding, operation, &intent)
            .await?;
        if observed["intent"] != row.3
            || observed["receipt"] != row.5.clone().map_or(Value::Null, Value::String)
            || observed["hash"] != row.2
        {
            return Err(conflict());
        }
        for key in [
            "customerData",
            "customerVersion",
            "planData",
            "planVersion",
            "customerBinding",
            "planBinding",
            "planIntentCiphertext",
            "planReceiptCiphertext",
        ] {
            if observed[key] != intent["dependencies"][key] {
                return Err(conflict());
            }
        }
        self.validate_read_plan_provenance(&intent, &observed)?;
        let record = self
            .decode(
                &mut tx,
                Scope::Durable,
                BillingKind::Subscription,
                &binding.storage_id,
                observed["record"].clone(),
                "subscription_creation_read",
            )
            .await?;
        if record != receipt {
            return Err(conflict());
        }
        if self
            .subscription_read_observation(&mut tx, binding, operation, &intent)
            .await?
            != observed
        {
            return Err(conflict());
        }
        tx.commit().await.map_err(db_error)?;
        Ok(SubscriptionCreationReadSnapshot {
            binding: binding.clone(),
            actor,
            version: version.into(),
            proof: observed,
            intent,
            record,
        })
    }
    fn validate_read_plan_provenance(
        &self,
        intent: &Value,
        observed: &Value,
    ) -> BlueprintResult<()> {
        let operation = id(intent, "planOperation")?;
        let plan_id = id(intent, "plan")?.to_string();
        let sealed = self.open_plan_creation(
            operation,
            "intent",
            observed["planIntentCiphertext"]
                .as_str()
                .ok_or_else(storage_error)?,
        )?;
        let canonical = sealed["canonical"].as_str().ok_or_else(storage_error)?;
        if sealed["publicId"] != plan_id
            || super::super::plan_creation::intent_hash(&plan_id, canonical)
                != observed["planPayloadHash"]
        {
            return Err(conflict());
        }
        let plan_intent: Value = serde_json::from_str(canonical).map_err(|_| storage_error())?;
        for key in ["tenant", "deployment", "account", "livemode", "apiVersion"] {
            if plan_intent[key] != intent[key] {
                return Err(conflict());
            }
        }
        let receipt = self.open_plan_creation(
            operation,
            "receipt",
            observed["planReceiptCiphertext"]
                .as_str()
                .ok_or_else(storage_error)?,
        )?;
        if plan_intent["operation"] != operation.to_string()
            || receipt != intent["dependencies"]["planReceipt"]
            || receipt["id"] != plan_id
            || receipt["externalId"] != observed["planBinding"]["provider_id"]
        {
            return Err(conflict());
        }
        Ok(())
    }
    async fn subscription_read_observation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: &DurableStripeBinding,
        operation: Uuid,
        intent: &Value,
    ) -> BlueprintResult<Value> {
        let observed:Option<sqlx::types::Json<Value>>=sqlx::query_scalar("SELECT jsonb_build_object('record',r.data,'recordVersion',r.xmin::text,'binding',to_jsonb(b),'bindingVersion',b.xmin::text,'intent',j.intent_ciphertext,'receipt',j.receipt_ciphertext,'hash',j.payload_hash,'journalVersion',j.xmin::text,'customerData',c.data,'customerVersion',c.xmin::text,'planData',p.data,'planVersion',p.xmin::text,'customerBinding',to_jsonb(cb),'customerBindingVersion',cb.xmin::text,'planBinding',to_jsonb(pb),'planBindingVersion',pb.xmin::text,'planIntentCiphertext',pj.intent_ciphertext,'planReceiptCiphertext',pj.receipt_ciphertext,'planJournalVersion',pj.xmin::text,'planPayloadHash',pj.payload_hash) FROM fl_billing.provider_bindings b JOIN fl_billing.records r ON r.tenant_id=b.tenant_id AND r.scope=b.scope AND r.kind=b.kind AND r.id=b.storage_id JOIN fl_billing.subscription_creation_intents j ON j.tenant_id=b.tenant_id AND j.public_id=b.public_id AND j.operation_id=$9::uuid AND j.state='completed' AND j.actor_id=$10 JOIN fl_billing.records c ON c.tenant_id=b.tenant_id AND c.scope='durable' AND c.kind='customer' AND c.id=$11 JOIN fl_billing.records p ON p.tenant_id=b.tenant_id AND p.scope='durable' AND p.kind='plan' AND p.id=$12 JOIN fl_billing.stripe_customer_bindings cb ON cb.tenant_id=c.tenant_id AND cb.local_customer_id::text=c.id AND cb.storage_id=c.id AND cb.deployment_label=b.deployment_label AND cb.account_id=b.account_id AND cb.livemode=b.livemode AND cb.verified_api_version=$14 JOIN fl_billing.provider_bindings pb ON pb.tenant_id=p.tenant_id AND pb.scope='durable' AND pb.kind='plan' AND pb.public_id::text=p.id AND pb.storage_id=p.id AND pb.deployment_label=b.deployment_label AND pb.account_id=b.account_id AND pb.livemode=b.livemode JOIN fl_billing.plan_creation_intents pj ON pj.tenant_id=pb.tenant_id AND pj.public_id=pb.public_id AND pj.operation_id=$13::uuid AND pj.state='completed' AND pj.deployment_label=b.deployment_label AND pj.account_id=b.account_id AND pj.livemode=b.livemode AND pj.api_version=$14 WHERE b.tenant_id=$1 AND b.kind='subscription' AND b.scope='durable' AND b.public_id=$2::uuid AND b.binding_id=$3::uuid AND b.storage_id=$4 AND b.provider_id=$5 AND b.deployment_label=$6 AND b.account_id=$7 AND b.livemode=$8 AND r.data->>'id'=$4 AND c.data->>'id'=$11 AND p.data->>'id'=$12 AND b.subscription_provider_customer_id=cb.provider_customer_id AND b.local_party_id=cb.party_id AND b.local_party_type=cb.party_type")
            .bind(self.tenant.id()).bind(binding.public_id.to_string()).bind(binding.binding_id.to_string()).bind(&binding.storage_id).bind(&binding.provider_id).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode).bind(operation.to_string()).bind(self.binding_actor()?).bind(id(intent,"customer")?.to_string()).bind(id(intent,"plan")?.to_string()).bind(id(intent,"planOperation")?.to_string()).bind(intent["apiVersion"].as_str().ok_or_else(storage_error)?)
            .fetch_optional(&mut **tx).await.map_err(db_error)?;
        Ok(observed.ok_or_else(conflict)?.0)
    }
    /// Final current proof after provider reads and audit waits. Data linearizes
    /// at the last matching SELECT; caller checks context again after COMMIT.
    /// # Errors
    /// Denies changed actor, completed provenance, bodies or binding generations.
    pub async fn read_subscription_creation_snapshot(
        &self,
        snapshot: &SubscriptionCreationReadSnapshot,
    ) -> BlueprintResult<Value> {
        if snapshot.actor != self.binding_actor()? {
            return Err(conflict());
        }
        let current = self
            .subscription_creation_read_snapshot(&snapshot.binding, &snapshot.version)
            .await?;
        if current.proof != snapshot.proof {
            return Err(conflict());
        }
        Ok(current.record)
    }
}

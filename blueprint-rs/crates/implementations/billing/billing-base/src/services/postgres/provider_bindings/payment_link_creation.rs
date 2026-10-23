//! Trusted adapter journal; never a public provider-evidence ingestion endpoint.
mod admission;
mod expiration;
pub use expiration::{
    PaymentLinkExpirationClaim, PaymentLinkExpirationRecovery, PaymentLinkExpirationReservation,
    PaymentLinkExpirationStatus, TrustedPaymentLinkExpirationPublisher,
};
mod managed;
mod read;
use super::super::{db_error, invalid, storage_error};
use super::{DurableStripeBinding, PostgresBillingStore, StripeBindingScope, conflict};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
pub use managed::ManagedPaymentLinkPublication;
pub use read::PaymentLinkCreationReadSnapshot;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// One dispatch capability, issued only after a confirmed reservation commit.
/// No Clone/Debug/Serde; lookups cannot reconstruct this capability.
pub struct PaymentLinkCreationReservation {
    tenant: String,
    operation_id: Uuid,
    public_id: Uuid,
    reservation_id: Uuid,
    actor_id: String,
    scope: StripeBindingScope,
    api_version: String,
    payload_hash: String,
    idempotency_key: String,
}
impl PaymentLinkCreationReservation {
    #[must_use]
    pub const fn public_id(&self) -> Uuid {
        self.public_id
    }
    #[must_use]
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }
}
/// Historical operation status, not a live provider/ownership assertion.
pub enum PaymentLinkCreationStatus {
    Pending { operation_id: Uuid, public_id: Uuid },
    Completed { operation_id: Uuid, record: Value },
}
/// Only a newly committed reservation can dispatch. Existing operations cannot.
pub enum PaymentLinkCreationClaim {
    Reserved(PaymentLinkCreationReservation),
    Existing(PaymentLinkCreationStatus),
}
type IntentRow = (
    String,
    String,
    String,
    String,
    String,
    bool,
    String,
    String,
    String,
    Option<String>,
);
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let sorted: std::collections::BTreeMap<_, _> = map.iter().collect();
            Value::Object(
                sorted
                    .into_iter()
                    .map(|(k, v)| (k.clone(), canonical(v)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(canonical).collect()),
        value => value.clone(),
    }
}
fn intent_hash(public_id: &str, canonical: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"forklaunch/payment-link-creation/v1\0");
    hash.update(public_id.as_bytes());
    hash.update(b"\0");
    hash.update(canonical.as_bytes());
    base16ct::lower::encode_string(&hash.finalize())
}
impl PostgresBillingStore {
    /// Checks only local grants required by explicit creation and receipt inspection.
    /// # Errors
    /// Rejects missing schema usage or journal/record/binding/audit write privileges.
    pub async fn validate_payment_link_creation_access(&self) -> BlueprintResult<()> {
        let allowed:bool=sqlx::query_scalar("SELECT has_schema_privilege(current_user,'fl_billing','USAGE') AND has_table_privilege(current_user,'fl_billing.records','INSERT') AND has_table_privilege(current_user,'fl_billing.provider_bindings','INSERT') AND has_table_privilege(current_user,'fl_billing.payment_link_creation_intents','SELECT') AND has_table_privilege(current_user,'fl_billing.payment_link_creation_intents','INSERT') AND has_table_privilege(current_user,'fl_billing.payment_link_creation_intents','UPDATE') AND has_table_privilege(current_user,'fl_billing.payment_link_creation_audit','INSERT') AND has_table_privilege(current_user,'fl_billing.provider_binding_audit','INSERT')").fetch_one(&self.pool).await.map_err(db_error)?;
        if !allowed {
            return Err(conflict());
        }
        Ok(())
    }

    fn seal_creation(
        &self,
        operation: Uuid,
        field: &str,
        value: &Value,
    ) -> BlueprintResult<String> {
        self.encryptor.seal(&json!({"kind":"payment_link_creation","operationId":operation.to_string(),"field":field,"value":value}).to_string(),self.tenant.id(),"[Billing creation journal]").map_err(|_|storage_error())
    }
    fn open_creation(
        &self,
        operation: Uuid,
        field: &str,
        ciphertext: &str,
    ) -> BlueprintResult<Value> {
        let raw = self
            .encryptor
            .decrypt(ciphertext, self.tenant.id())
            .map_err(|_| storage_error())?;
        let envelope: Value = serde_json::from_str(&raw).map_err(|_| storage_error())?;
        if envelope["kind"] != "payment_link_creation"
            || envelope["operationId"] != operation.to_string()
            || envelope["field"] != field
        {
            return Err(storage_error());
        }
        Ok(envelope["value"].clone())
    }
    async fn audit_creation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        operation: Uuid,
        action: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("INSERT INTO fl_billing.payment_link_creation_audit(id,tenant_id,operation_id,actor_id,correlation_id,operation) VALUES($1::uuid,$2,$3::uuid,$4,$5,$6)")
            .bind(Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(operation.to_string()).bind(self.binding_actor()?).bind(&self.audit.correlation_id).bind(action).execute(&mut **tx).await.map_err(db_error)?;
        Ok(())
    }
    /// Trusted adapter reservation, never authorize from providerFields or cache.
    /// Same tenant/actor/scope/version/payload returns historical status, never a
    /// second dispatch ticket. A commit error must be inspected, not retried.
    /// # Errors
    /// Denies malformed IDs, conflicting payloads/scopes/actors, and storage errors.
    /// Before cloning, admits at most 32 nested levels, 8192 values and 65536
    /// serialized UTF-8 bytes for the complete immutable intent envelope.
    pub async fn reserve_payment_link_creation(
        &self,
        scope: &StripeBindingScope,
        operation: Uuid,
        api_version: &str,
        payload: &Value,
    ) -> BlueprintResult<PaymentLinkCreationClaim> {
        let actor = self.binding_actor()?;
        if operation.is_nil() || api_version.is_empty() || api_version.len() > 128 {
            return Err(invalid());
        }
        admission::validate(self.tenant.id(), scope, operation, api_version, payload)?;
        let canonical=canonical(&json!({"tenant":self.tenant.id(),"operation":operation.to_string(),"deployment":scope.deployment_label,"account":scope.account_id,"livemode":scope.livemode,"apiVersion":api_version,"payload":payload})).to_string();
        if canonical.len() > 65536 {
            return Err(invalid());
        }
        let public_id = Uuid::new_v4();
        let hash = intent_hash(&public_id.to_string(), &canonical);
        let ticket = PaymentLinkCreationReservation {
            tenant: self.tenant.id().into(),
            operation_id: operation,
            public_id,
            reservation_id: Uuid::new_v4(),
            actor_id: actor.into(),
            scope: scope.clone(),
            api_version: api_version.into(),
            payload_hash: hash.clone(),
            idempotency_key: format!("fl-plink-{}", Uuid::new_v4()),
        };
        let encrypted = self.seal_creation(
            operation,
            "intent",
            &json!({"publicId":ticket.public_id.to_string(),"canonical":canonical}),
        )?;
        let mut tx = self.transaction().await?;
        let inserted=sqlx::query("INSERT INTO fl_billing.payment_link_creation_intents(tenant_id,operation_id,public_id,reservation_id,actor_id,deployment_label,account_id,livemode,api_version,payload_hash,idempotency_key,intent_ciphertext) VALUES($1,$2::uuid,$3::uuid,$4::uuid,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(tenant_id,operation_id) DO NOTHING")
            .bind(self.tenant.id()).bind(operation.to_string()).bind(ticket.public_id.to_string()).bind(ticket.reservation_id.to_string()).bind(actor).bind(&scope.deployment_label).bind(&scope.account_id).bind(scope.livemode).bind(api_version).bind(&hash).bind(&ticket.idempotency_key).bind(encrypted).execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        let result = if inserted == 1 {
            self.audit_creation(&mut tx, operation, "reserve").await?;
            PaymentLinkCreationClaim::Reserved(ticket)
        } else {
            let row = self
                .creation_row(&mut tx, operation)
                .await?
                .ok_or_else(conflict)?;
            if row.1 != actor
                || row.2 != scope.deployment_label
                || row.3 != scope.account_id
                || row.4 != api_version
                || row.5 != scope.livemode
                || row.6 != intent_hash(&row.0, &canonical)
            {
                return Err(conflict());
            }
            let status = self.creation_status(operation, row)?;
            self.audit_creation(&mut tx, operation, "inspect").await?;
            PaymentLinkCreationClaim::Existing(status)
        };
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
    async fn creation_row(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        operation: Uuid,
    ) -> BlueprintResult<Option<IntentRow>> {
        sqlx::query_as("SELECT public_id::text,actor_id,deployment_label,account_id,api_version,livemode,payload_hash,state,intent_ciphertext,receipt_ciphertext FROM fl_billing.payment_link_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid")
            .bind(self.tenant.id()).bind(operation.to_string()).fetch_optional(&mut **tx).await.map_err(db_error)
    }
    fn creation_status(
        &self,
        operation: Uuid,
        row: IntentRow,
    ) -> BlueprintResult<PaymentLinkCreationStatus> {
        if row.1 != self.binding_actor()? {
            return Err(conflict());
        }
        let public_id = Uuid::parse_str(&row.0).map_err(|_| storage_error())?;
        let intent = self.open_creation(operation, "intent", &row.8)?;
        if intent["publicId"] != row.0
            || intent_hash(
                &row.0,
                intent["canonical"].as_str().ok_or_else(storage_error)?,
            ) != row.6
        {
            return Err(storage_error());
        }
        match (row.7.as_str(), row.9) {
            ("pending", None) => Ok(PaymentLinkCreationStatus::Pending {
                operation_id: operation,
                public_id,
            }),
            ("completed", Some(ciphertext)) => {
                let record = self.open_creation(operation, "receipt", &ciphertext)?;
                if record["id"] != row.0 {
                    return Err(storage_error());
                }
                Ok(PaymentLinkCreationStatus::Completed {
                    operation_id: operation,
                    record,
                })
            }
            _ => Err(storage_error()),
        }
    }
    /// Reads an immutable historical receipt or unresolved intent for this actor.
    /// It does not prove the provider object still exists or remains unchanged.
    /// # Errors
    /// Denies foreign actors and malformed/encryption/audit failures.
    pub async fn inspect_payment_link_creation(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<PaymentLinkCreationStatus>> {
        self.binding_actor()?;
        let mut tx = self.transaction().await?;
        let result = self
            .creation_row(&mut tx, operation)
            .await?
            .map(|row| self.creation_status(operation, row))
            .transpose()?;
        if result.is_some() {
            self.audit_creation(&mut tx, operation, "inspect").await?;
        }
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
    /// Trusted adapter completion after authenticated provider creation. Not a
    /// general evidence ingestion API. All publication writes commit together.
    /// `fresh` must validate the pinned provider context after awaited work.
    /// # Errors
    /// Any error, especially COMMIT, requires receipt inspection; it does not
    /// prove rollback or absence of provider effects. No dispatch ticket is returned.
    pub async fn complete_payment_link_creation<F>(
        &self,
        ticket: PaymentLinkCreationReservation,
        record: Value,
        fresh: F,
    ) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        if ticket.tenant != self.tenant.id()
            || ticket.actor_id != self.binding_actor()?
            || record["id"] != ticket.public_id.to_string()
        {
            return Err(conflict());
        }
        let provider = &record["providerFields"];
        let provider_id = provider["id"]
            .as_str()
            .filter(|v| {
                v.starts_with("plink_")
                    && v.len() > 6
                    && v.len() <= 128
                    && v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
            .ok_or_else(invalid)?;
        if provider["object"] != "payment_link"
            || provider["livemode"].as_bool() != Some(ticket.scope.livemode)
        {
            return Err(conflict());
        }
        let encoded = self.encode(Scope::Durable, BillingKind::PaymentLink, record.clone())?;
        let receipt = self.seal_creation(ticket.operation_id, "receipt", &record)?;
        let binding = DurableStripeBinding {
            tenant: ticket.tenant.clone(),
            scope: ticket.scope.clone(),
            kind: BillingKind::PaymentLink,
            public_id: ticket.public_id,
            binding_id: Uuid::new_v4(),
            storage_id: ticket.public_id.to_string(),
            provider_id: provider_id.into(),
            subscription_ownership: None,
        };
        let mut tx = self.transaction().await?;
        let locked:Option<(String,String)>=sqlx::query_as("SELECT state,intent_ciphertext FROM fl_billing.payment_link_creation_intents WHERE tenant_id=$1 AND operation_id=$2::uuid AND reservation_id=$3::uuid AND public_id=$4::uuid AND actor_id=$5 AND payload_hash=$6 AND account_id=$7 AND livemode=$8 AND deployment_label=$9 AND api_version=$10 FOR UPDATE")
            .bind(self.tenant.id()).bind(ticket.operation_id.to_string()).bind(ticket.reservation_id.to_string()).bind(ticket.public_id.to_string()).bind(&ticket.actor_id).bind(&ticket.payload_hash).bind(&ticket.scope.account_id).bind(ticket.scope.livemode).bind(&ticket.scope.deployment_label).bind(&ticket.api_version).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let (state, ciphertext) = locked.ok_or_else(conflict)?;
        if state != "pending" {
            return Err(conflict());
        }
        let intent = self.open_creation(ticket.operation_id, "intent", &ciphertext)?;
        let canonical = intent["canonical"].as_str().ok_or_else(storage_error)?;
        if intent_hash(&ticket.public_id.to_string(), canonical) != ticket.payload_hash {
            return Err(conflict());
        }
        let payload: Value = serde_json::from_str(canonical).map_err(|_| storage_error())?;
        for field in ["amount", "currency", "paymentMethods", "status"] {
            if record[field] != payload["payload"]["request"][field] {
                return Err(conflict());
            }
        }
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        sqlx::query("INSERT INTO fl_billing.records(tenant_id,scope,kind,id,data) VALUES($1,'durable','payment_link',$2,$3)").bind(self.tenant.id()).bind(ticket.public_id.to_string()).bind(sqlx::types::Json(encoded)).execute(&mut *tx).await.map_err(db_error)?;
        sqlx::query("INSERT INTO fl_billing.provider_bindings(tenant_id,kind,public_id,binding_id,storage_id,deployment_label,account_id,livemode,provider_id) VALUES($1,'payment_link',$2::uuid,$3::uuid,$4,$5,$6,$7,$8)")
            .bind(self.tenant.id()).bind(ticket.public_id.to_string()).bind(binding.binding_id.to_string()).bind(&binding.storage_id).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode).bind(provider_id).execute(&mut *tx).await.map_err(db_error)?;
        self.audit_binding(&mut tx, &binding, "provision").await?;
        self.audit_creation(&mut tx, ticket.operation_id, "complete")
            .await?;
        sqlx::query("UPDATE fl_billing.payment_link_creation_intents SET state='completed',receipt_ciphertext=$3 WHERE tenant_id=$1 AND operation_id=$2::uuid").bind(self.tenant.id()).bind(ticket.operation_id.to_string()).bind(receipt).execute(&mut *tx).await.map_err(db_error)?;
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        tx.commit().await.map_err(db_error)
    }
}

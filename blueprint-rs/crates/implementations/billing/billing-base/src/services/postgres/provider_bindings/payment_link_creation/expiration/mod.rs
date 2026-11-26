//! Single-dispatch metadata-only expiration. Pending never implies retry authority.
mod publication;
mod recovery;
use super::super::{PostgresBillingStore, StripeBindingScope, conflict};
use super::{PaymentLinkCreationReadSnapshot, canonical};
use super::{db_error, invalid, storage_error};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
pub use publication::TrustedPaymentLinkExpirationPublisher;
pub use recovery::PaymentLinkExpirationRecovery;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Historical local operation state; neither provider deactivation nor payment proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaymentLinkExpirationStatus {
    Pending { operation_id: Uuid, public_id: Uuid },
    Completed { operation_id: Uuid, public_id: Uuid },
}
/// Only the newly confirmed reservation contains dispatch authority.
pub enum PaymentLinkExpirationClaim {
    Reserved(PaymentLinkExpirationReservation),
    Existing(PaymentLinkExpirationStatus),
}
/// Opaque one-dispatch capability; never reconstructed from an inspection.
pub struct PaymentLinkExpirationReservation {
    operation: Uuid,
    public_id: Uuid,
    reservation: Uuid,
    hash: String,
    canonical: String,
    key: String,
    snapshot: Box<PaymentLinkCreationReadSnapshot>,
}
impl PaymentLinkExpirationReservation {
    /// Fixed server-issued key for this single dispatch; not retry permission.
    #[must_use]
    pub fn idempotency_key(&self) -> &str {
        &self.key
    }
}
type Row = (String, String, String, String, String, Option<String>);
fn hash(value: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"forklaunch/payment-link-expiration/v1\0");
    digest.update(value.as_bytes());
    base16ct::lower::encode_string(&digest.finalize())
}
fn unknown() -> BlueprintError {
    BlueprintError::new(
        "billing_payment_link_expiration_commit_unknown",
        "Payment-link expiration commit outcome requires inspection",
    )
}
impl PostgresBillingStore {
    /// Checks explicit reservation, dependency-lock and inspection grants locally.
    /// # Errors
    /// Rejects missing individual privileges or migration-owner membership.
    pub async fn validate_payment_link_expiration_access(&self) -> BlueprintResult<()> {
        self.validate_payment_link_creation_read_access().await?;
        let mut tx = self.transaction().await?;
        self.expiration_role(&mut tx).await?;
        for (table, privilege) in [
            ("records", "UPDATE"),
            ("provider_bindings", "UPDATE"),
            ("payment_link_expiration_intents", "INSERT"),
            ("payment_link_expiration_dependency_claims", "INSERT"),
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
    async fn expiration_role(&self, tx: &mut Transaction<'_, Postgres>) -> BlueprintResult<()> {
        let forbidden: bool = sqlx::query_scalar("SELECT pg_has_role(current_user,relowner,'MEMBER') FROM pg_class WHERE oid='fl_billing.payment_link_expiration_dependency_guards'::regclass")
            .fetch_one(&mut **tx).await.map_err(db_error)?;
        if forbidden {
            return Err(conflict());
        }
        Ok(())
    }
    async fn expiration_audit(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        operation: Uuid,
        action: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("INSERT INTO fl_billing.payment_link_expiration_audit(id,tenant_id,operation_id,actor_id,correlation_id,operation) VALUES($1::uuid,$2,$3::uuid,$4,$5,$6)")
            .bind(Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(operation.to_string()).bind(self.binding_actor()?).bind(&self.audit.correlation_id).bind(action)
            .execute(&mut **tx).await.map_err(db_error)?;
        Ok(())
    }
    fn seal_expiration(
        &self,
        operation: Uuid,
        field: &str,
        value: &Value,
    ) -> BlueprintResult<String> {
        super::super::subscription_creation::budget(value, 0, &mut 0, &mut 0)?;
        self.encryptor.seal(&json!({"kind":"payment_link_expiration","operation":operation.to_string(),"field":field,"value":value}).to_string(),self.tenant.id(),"[Payment link expiration]").map_err(|_|storage_error())
    }
    fn open_expiration(
        &self,
        operation: Uuid,
        field: &str,
        encrypted: &str,
    ) -> BlueprintResult<Value> {
        if encrypted.len() > 524_288 {
            return Err(storage_error());
        }
        let raw = self
            .encryptor
            .decrypt(encrypted, self.tenant.id())
            .map_err(|_| storage_error())?;
        if raw.len() > 262_144 {
            return Err(storage_error());
        }
        let envelope: Value = serde_json::from_str(&raw).map_err(|_| storage_error())?;
        super::super::subscription_creation::budget(&envelope, 0, &mut 0, &mut 0)?;
        if envelope["kind"] != "payment_link_expiration"
            || envelope["operation"] != operation.to_string()
            || envelope["field"] != field
        {
            return Err(storage_error());
        }
        Ok(envelope["value"].clone())
    }
    async fn expiration_row(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        operation: Uuid,
    ) -> BlueprintResult<Option<Row>> {
        sqlx::query_as("SELECT public_id::text,actor_id,payload_hash,intent_ciphertext,state,receipt_ciphertext FROM fl_billing.payment_link_expiration_intents WHERE tenant_id=$1 AND operation_id=$2::uuid")
            .bind(self.tenant.id()).bind(operation.to_string()).fetch_optional(&mut **tx).await.map_err(db_error)
    }
    fn expiration_status(
        &self,
        operation: Uuid,
        row: &Row,
        scope: &StripeBindingScope,
        api: &str,
        public_id: Uuid,
        profile: &Value,
    ) -> BlueprintResult<PaymentLinkExpirationStatus> {
        if row.0 != public_id.to_string() || row.1 != self.binding_actor()? {
            return Err(conflict());
        }
        let value = self.open_expiration(operation, "intent", &row.3)?;
        let canonical = value.as_str().ok_or_else(storage_error)?;
        if hash(canonical) != row.2 {
            return Err(storage_error());
        }
        let intent: Value = serde_json::from_str(canonical).map_err(|_| storage_error())?;
        if intent["tenant"] != self.tenant.id()
            || intent["actor"] != row.1
            || intent["operation"] != operation.to_string()
            || intent["publicId"] != row.0
            || intent["deployment"] != scope.deployment_label
            || intent["account"] != scope.account_id
            || intent["livemode"] != scope.livemode
            || intent["apiVersion"] != api
            || intent["profile"] != *profile
        {
            return Err(conflict());
        }
        match (row.4.as_str(), &row.5) {
            ("pending", None) => Ok(PaymentLinkExpirationStatus::Pending {
                operation_id: operation,
                public_id,
            }),
            ("completed", Some(ciphertext)) => {
                let receipt = self.open_expiration(operation, "receipt", ciphertext)?;
                if receipt["hash"] != row.2
                    || receipt["publicId"] != row.0
                    || receipt["status"] != "EXPIRED"
                {
                    return Err(storage_error());
                }
                if let Some(recovery) = receipt.get("recovery")
                    && (recovery["kind"] != "event_linked_metadata_expiration_v1"
                        || recovery["evidenceHash"]
                            != hash(&super::canonical(&recovery["evidence"]).to_string()))
                {
                    return Err(storage_error());
                }
                Ok(PaymentLinkExpirationStatus::Completed {
                    operation_id: operation,
                    public_id,
                })
            }
            _ => Err(storage_error()),
        }
    }
    /// IDs-only historical inspection. Absence never proves rollback or permits dispatch.
    /// # Errors
    /// Rejects mismatched actor, tenant, scope, public ID or immutable profile.
    pub async fn inspect_payment_link_expiration(
        &self,
        scope: &StripeBindingScope,
        api: &str,
        operation: Uuid,
        public_id: Uuid,
        profile: &Value,
    ) -> BlueprintResult<Option<PaymentLinkExpirationStatus>> {
        if operation.is_nil() || public_id.is_nil() {
            return Err(invalid());
        }
        let mut tx = self.transaction().await?;
        self.expiration_role(&mut tx).await?;
        let result = self
            .expiration_row(&mut tx, operation)
            .await?
            .map(|row| self.expiration_status(operation, &row, scope, api, public_id, profile))
            .transpose()?;
        if result.is_some() {
            self.expiration_audit(&mut tx, operation, "inspect").await?;
        }
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
    async fn lock_expiration_snapshot(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        snapshot: &PaymentLinkCreationReadSnapshot,
    ) -> BlueprintResult<()> {
        if snapshot.actor != self.binding_actor()? || snapshot.binding.tenant != self.tenant.id() {
            return Err(conflict());
        }
        let found:Option<String>=sqlx::query_scalar("SELECT id FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='payment_link' AND id=$2 FOR UPDATE")
            .bind(self.tenant.id()).bind(&snapshot.binding.storage_id).fetch_optional(&mut **tx).await.map_err(db_error)?;
        if found.is_none() {
            return Err(conflict());
        }
        let found:Option<String>=sqlx::query_scalar("SELECT binding_id::text FROM fl_billing.provider_bindings WHERE tenant_id=$1 AND kind='payment_link' AND binding_id=$2::uuid FOR UPDATE")
            .bind(self.tenant.id()).bind(snapshot.binding.binding_id.to_string()).fetch_optional(&mut **tx).await.map_err(db_error)?;
        if found.is_none() {
            return Err(conflict());
        }
        let operation = Uuid::parse_str(
            snapshot.proof["operation"]
                .as_str()
                .ok_or_else(storage_error)?,
        )
        .map_err(|_| storage_error())?;
        if self
            .payment_link_read_observation(tx, &snapshot.binding, operation)
            .await?
            != snapshot.proof["observed"]
        {
            return Err(conflict());
        }
        Ok(())
    }
    /// Reserves one metadata-only dispatch after locking exact completed creation proof.
    /// No provider I/O. A commit error does not issue a ticket and requires inspection.
    /// # Errors
    /// Denies stale proof, role/actor mismatch, competing operations and expired context.
    pub async fn reserve_payment_link_expiration<F>(
        &self,
        operation: Uuid,
        snapshot: PaymentLinkCreationReadSnapshot,
        profile: &Value,
        fresh: F,
    ) -> BlueprintResult<PaymentLinkExpirationClaim>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        if operation.is_nil() {
            return Err(invalid());
        }
        super::super::subscription_creation::budget(profile, 0, &mut 0, &mut 0)?;
        let binding = &snapshot.binding;
        let expected = json!({"tenant":self.tenant.id(),"actor":self.binding_actor()?,"operation":operation.to_string(),"publicId":binding.public_id.to_string(),"deployment":binding.scope.deployment_label,"account":binding.scope.account_id,"livemode":binding.scope.livemode,"apiVersion":snapshot.api_version,"profile":profile,"proof":snapshot.proof});
        super::super::subscription_creation::budget(&expected, 0, &mut 0, &mut 0)?;
        let canonical = canonical(&expected).to_string();
        let digest = hash(&canonical);
        let reservation = Uuid::new_v4();
        let key = format!("fl-plink-expire-{}", Uuid::new_v4());
        let encrypted =
            self.seal_expiration(operation, "intent", &Value::String(canonical.clone()))?;
        let mut tx = self.transaction().await?;
        self.expiration_role(&mut tx).await?;
        self.lock_expiration_snapshot(&mut tx, &snapshot).await?;
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        let inserted=sqlx::query("INSERT INTO fl_billing.payment_link_expiration_intents(tenant_id,operation_id,public_id,reservation_id,actor_id,deployment_label,account_id,livemode,api_version,payload_hash,idempotency_key,intent_ciphertext) VALUES($1,$2::uuid,$3::uuid,$4::uuid,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(tenant_id,operation_id) DO NOTHING")
            .bind(self.tenant.id()).bind(operation.to_string()).bind(binding.public_id.to_string()).bind(reservation.to_string()).bind(self.binding_actor()?).bind(&binding.scope.deployment_label).bind(&binding.scope.account_id).bind(binding.scope.livemode).bind(&snapshot.api_version).bind(&digest).bind(&key).bind(encrypted).execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        if inserted == 0 {
            let row = self
                .expiration_row(&mut tx, operation)
                .await?
                .ok_or_else(conflict)?;
            let status = self.expiration_status(
                operation,
                &row,
                &binding.scope,
                &snapshot.api_version,
                binding.public_id,
                profile,
            )?;
            self.expiration_audit(&mut tx, operation, "inspect").await?;
            if let Err(error) = fresh() {
                tx.rollback().await.map_err(db_error)?;
                return Err(error);
            }
            tx.commit().await.map_err(|_| unknown())?;
            return Ok(PaymentLinkExpirationClaim::Existing(status));
        }
        for (kind, id) in [
            ("payment_link_record", binding.storage_id.clone()),
            ("payment_link_binding", binding.binding_id.to_string()),
        ] {
            sqlx::query("INSERT INTO fl_billing.payment_link_expiration_dependency_claims(tenant_id,operation_id,dependency_kind,dependency_id) VALUES($1,$2::uuid,$3,$4)").bind(self.tenant.id()).bind(operation.to_string()).bind(kind).bind(id).execute(&mut *tx).await.map_err(db_error)?;
        }
        self.expiration_audit(&mut tx, operation, "reserve").await?;
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        tx.commit().await.map_err(|_| unknown())?;
        Ok(PaymentLinkExpirationClaim::Reserved(
            PaymentLinkExpirationReservation {
                operation,
                public_id: binding.public_id,
                reservation,
                hash: digest,
                canonical,
                key,
                snapshot: Box::new(snapshot),
            },
        ))
    }
}

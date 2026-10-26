//! Trusted separate-role publication, never ordinary runtime completion authority.
use super::CheckoutDependencySnapshot;
use super::{
    CheckoutCreationPending, CheckoutCreationReservation, IntentRow, PostgresBillingStore, budget,
    commit_unknown, conflict, db_error, hash, invalid, storage_error,
};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const FUNCTION: &str =
    "fl_billing.publish_bound_checkout(text,uuid,uuid,text,jsonb,jsonb,text,text,text)";
/// Explicit trusted server credential. This does not protect against a stolen
/// publisher credential. Ordinary runtime must neither inherit nor execute it.
/// No Clone/Debug/Serde, no public connection getter or auto-commit SQL helper.
pub struct TrustedCheckoutPublisher {
    store: PostgresBillingStore,
    runtime: PgPool,
}
/// Historical receipt; neither current provider state nor payment-success proof.
pub enum CheckoutCreationStatus {
    Pending(CheckoutCreationPending),
    Completed { operation_id: Uuid, public_id: Uuid },
}
// Publication authority contains no dispatch key and cannot become a reservation.
pub(super) struct Publication {
    pub operation_id: Uuid,
    pub public_id: Uuid,
    pub reservation_id: Uuid,
    pub payload_hash: String,
    pub canonical: String,
    pub snapshot: Box<CheckoutDependencySnapshot>,
}
impl From<CheckoutCreationReservation> for Publication {
    fn from(ticket: CheckoutCreationReservation) -> Self {
        Self {
            operation_id: ticket.operation_id,
            public_id: ticket.public_id,
            reservation_id: ticket.reservation_id,
            payload_hash: ticket.payload_hash,
            canonical: ticket.canonical,
            snapshot: ticket.snapshot,
        }
    }
}
type DatabaseIdentity = (String, Option<String>, Option<i32>, String);
async fn database_identity(pool: &PgPool) -> BlueprintResult<DatabaseIdentity> {
    sqlx::query_as("SELECT current_database(),inet_server_addr()::text,inet_server_port(),database_identity::text FROM fl_billing.subscription_publication_identity WHERE singleton")
        .fetch_one(pool).await.map_err(db_error)
}
async fn separated_roles(runtime: &PgPool, publisher: &PgPool) -> BlueprintResult<()> {
    let runtime_user: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(runtime)
        .await
        .map_err(db_error)?;
    let publisher_user: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(publisher)
        .await
        .map_err(db_error)?;
    let (member,execute):(bool,bool)=sqlx::query_as("SELECT pg_has_role(current_user,$1,'MEMBER'),has_function_privilege(current_user,$2,'EXECUTE')")
        .bind(&publisher_user).bind(FUNCTION).fetch_one(runtime).await.map_err(db_error)?;
    let publisher_can: bool =
        sqlx::query_scalar("SELECT has_function_privilege(current_user,$1,'EXECUTE')")
            .bind(FUNCTION)
            .fetch_one(publisher)
            .await
            .map_err(db_error)?;
    if runtime_user == publisher_user || member || execute || !publisher_can {
        return Err(invalid());
    }
    Ok(())
}
impl PostgresBillingStore {
    /// Explicit separate publisher connection; no default runtime-pool fallback.
    /// Checks equal configured host/port/database, observed database/server tuple
    /// and a migration-generated immutable UUID. This is configuration evidence,
    /// not cryptographic proof that two administratively cloned clusters differ.
    /// # Errors
    /// Denies owner/BYPASSRLS roles, inherited publication or mismatched database.
    pub async fn trusted_checkout_publisher(
        &self,
        publisher: PgPool,
    ) -> BlueprintResult<TrustedCheckoutPublisher> {
        self.binding_actor()?;
        let source = self.pool.connect_options();
        let target = publisher.connect_options();
        if source.get_host() != target.get_host()
            || source.get_port() != target.get_port()
            || source.get_database() != target.get_database()
        {
            return Err(invalid());
        }
        if database_identity(&self.pool).await? != database_identity(&publisher).await? {
            return Err(invalid());
        }
        separated_roles(&self.pool, &publisher).await?;
        let store = Self::new(
            publisher,
            self.tenant.clone(),
            self.encryptor.clone(),
            self.audit.clone(),
        )
        .await?;
        let mut runtime_tx = self.transaction().await?;
        self.checkout_runtime_role(&mut runtime_tx).await?;
        runtime_tx.commit().await.map_err(db_error)?;
        let mut publisher_tx = store.transaction().await?;
        store.checkout_runtime_role(&mut publisher_tx).await?;
        publisher_tx.commit().await.map_err(db_error)?;
        Ok(TrustedCheckoutPublisher {
            store,
            runtime: self.pool.clone(),
        })
    }
    pub(super) fn open_checkout_receipt(
        &self,
        operation: Uuid,
        row: &IntentRow,
    ) -> BlueprintResult<Value> {
        Ok(self.open_checkout_receipt_envelope(operation, row)?["record"].clone())
    }
    pub(super) fn open_checkout_receipt_envelope(
        &self,
        operation: Uuid,
        row: &IntentRow,
    ) -> BlueprintResult<Value> {
        self.open_checkout_intent(operation, row)?;
        if row.4 != "completed" {
            return Err(conflict());
        }
        let ciphertext = row.5.as_deref().ok_or_else(storage_error)?;
        let plaintext = self
            .encryptor
            .decrypt(ciphertext, self.tenant.id())
            .map_err(|_| storage_error())?;
        let envelope: Value = serde_json::from_str(&plaintext).map_err(|_| storage_error())?;
        if envelope["kind"] != "checkout_creation_receipt"
            || envelope["operationId"] != operation.to_string()
            || envelope["publicId"] != row.0
            || envelope["payloadHash"] != row.2
            || envelope["record"]["id"] != row.0
        {
            return Err(storage_error());
        }
        Ok(envelope)
    }
    /// Inspects a pending intent or encrypted completed receipt; never recreates
    /// a dispatch capability or releases claims from an unresolved operation.
    /// # Errors
    /// Denies wrong actor/tenant, invalid receipts and missing audit privileges.
    pub async fn inspect_bound_checkout_creation(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<CheckoutCreationStatus>> {
        self.binding_actor()?;
        let mut tx = self.transaction().await?;
        self.checkout_runtime_role(&mut tx).await?;
        let result = if let Some(row) = self.checkout_intent_row(&mut tx, operation).await? {
            self.open_checkout_intent(operation, &row)?;
            let status = if row.4 == "completed" {
                CheckoutCreationStatus::Completed {
                    operation_id: operation,
                    public_id: Uuid::parse_str(
                        self.open_checkout_receipt(operation, &row)?["id"]
                            .as_str()
                            .ok_or_else(storage_error)?,
                    )
                    .map_err(|_| storage_error())?,
                }
            } else if row.4 == "pending" && row.5.is_none() {
                CheckoutCreationStatus::Pending(CheckoutCreationPending {
                    operation_id: operation,
                    public_id: Uuid::parse_str(&row.0).map_err(|_| storage_error())?,
                })
            } else {
                return Err(storage_error());
            };
            self.audit_checkout_creation(&mut tx, operation, "inspect")
                .await?;
            Some(status)
        } else {
            None
        };
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
}
impl TrustedCheckoutPublisher {
    /// Derives an actor-scoped wrapper from an explicitly validated trusted publisher.
    /// Credentials, pool identity and runtime-role separation remain unchanged.
    /// Only trusted server authentication may choose this audit actor.
    /// # Errors
    /// Rejects invalid/absent actors and malformed audit context.
    pub fn with_audit_context(
        &self,
        audit: forklaunch_core::domain::AuditContext,
    ) -> BlueprintResult<Self> {
        let store = self.store.with_audit_context(audit)?;
        store.binding_actor()?;
        Ok(Self {
            store,
            runtime: self.runtime.clone(),
        })
    }

    /// Validates explicit publication configuration before any provider effect.
    /// # Errors
    /// Denies a different runtime database, tenant, actor or publication privilege.
    pub async fn validate_for(&self, runtime: &PostgresBillingStore) -> BlueprintResult<()> {
        if runtime.tenant.id() != self.store.tenant.id()
            || runtime.binding_actor()? != self.store.binding_actor()?
        {
            return Err(conflict());
        }
        let source = self.runtime.connect_options();
        let target = runtime.pool.connect_options();
        if source.get_host() != target.get_host()
            || source.get_port() != target.get_port()
            || source.get_database() != target.get_database()
            || source.get_username() != target.get_username()
        {
            return Err(conflict());
        }
        if database_identity(&runtime.pool).await? != database_identity(&self.store.pool).await? {
            return Err(conflict());
        }
        separated_roles(&runtime.pool, &self.store.pool).await
    }
    /// Called only by the trusted adapter after exact authenticated response
    /// validation. The publisher role is authority; encrypted bytes are not a
    /// SQL-verifiable Stripe signature. No ordinary runtime completion privilege.
    /// All locks, publication, receipt and release occur in this transaction.
    /// # Errors
    /// Includes uncertain COMMIT: inspect receipt, never infer rollback or retry.
    pub async fn complete<F>(
        &self,
        ticket: CheckoutCreationReservation,
        record: Value,
        fresh: F,
    ) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        self.publish(ticket.into(), record, fresh).await
    }
    async fn publish<F>(&self, ticket: Publication, record: Value, fresh: F) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        let store = &self.store;
        if ticket.snapshot.tenant != store.tenant.id()
            || ticket.snapshot.actor != store.binding_actor()?
            || record["id"] != ticket.public_id.to_string()
        {
            return Err(conflict());
        }
        let (mut nodes, mut bytes) = (0, 0);
        budget(&record, 1, &mut nodes, &mut bytes)?;
        separated_roles(&self.runtime, &store.pool).await?;
        if database_identity(&self.runtime).await? != database_identity(&store.pool).await? {
            return Err(conflict());
        }
        let expected: Value =
            serde_json::from_str(&ticket.canonical).map_err(|_| storage_error())?;
        if hash(&ticket.public_id.to_string(), &ticket.canonical) != ticket.payload_hash {
            return Err(conflict());
        }
        let provider = record["providerFields"]["id"]
            .as_str()
            .ok_or_else(invalid)?
            .to_owned();
        let encoded = store.encode(Scope::Durable, BillingKind::CheckoutSession, record.clone())?;
        let envelope = json!({"kind":"checkout_creation_receipt","operationId":ticket.operation_id.to_string(),"publicId":ticket.public_id.to_string(),"payloadHash":ticket.payload_hash,"record":record});
        budget(&envelope, 0, &mut 0, &mut 0)?;
        let receipt = store
            .encryptor
            .seal(
                &envelope.to_string(),
                store.tenant.id(),
                "[Billing checkout receipt]",
            )
            .map_err(|_| storage_error())?;
        let mut tx = store.transaction().await?;
        store.checkout_runtime_role(&mut tx).await?;
        if store
            .checkout_dependency_data(&mut tx, &ticket.snapshot)
            .await?
            != ticket.snapshot.data
        {
            return Err(conflict());
        }
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        sqlx::query(
            "SELECT fl_billing.publish_bound_checkout($1,$2::uuid,$3::uuid,$4,$5,$6,$7,$8,$9)",
        )
        .bind(store.tenant.id())
        .bind(ticket.operation_id.to_string())
        .bind(ticket.reservation_id.to_string())
        .bind(&ticket.payload_hash)
        .bind(sqlx::types::Json(expected))
        .bind(sqlx::types::Json(encoded))
        .bind(provider)
        .bind(receipt)
        .bind(&store.audit.correlation_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        tx.commit().await.map_err(|_| commit_unknown())
    }
}

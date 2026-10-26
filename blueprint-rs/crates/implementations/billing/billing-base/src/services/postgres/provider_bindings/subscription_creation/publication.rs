//! Trusted separate-role publication, never ordinary runtime completion authority.
use super::{
    IntentRow, PostgresBillingStore, SubscriptionCreationPending, SubscriptionCreationReservation,
    budget, conflict, db_error, hash, invalid, storage_error,
};
use super::{SubscriptionDependencySnapshot, recovery::SubscriptionCreationRecovery};
use crate::services::{BillingKind, Scope};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const FUNCTION: &str =
    "fl_billing.publish_bound_subscription(text,uuid,uuid,text,jsonb,jsonb,text,text,text)";
/// Explicit trusted server credential. This does not protect against a stolen
/// publisher credential. Ordinary runtime must neither inherit nor execute it.
/// No Clone/Debug/Serde, no public connection getter or auto-commit SQL helper.
pub struct TrustedSubscriptionPublisher {
    store: PostgresBillingStore,
    runtime: PgPool,
}
/// Historical receipt; neither current provider state nor payment-success proof.
pub enum SubscriptionCreationStatus {
    Pending(SubscriptionCreationPending),
    Completed { operation_id: Uuid, record: Value },
}
// Publication authority contains no dispatch key and cannot become a reservation.
pub(super) struct Publication {
    pub operation_id: Uuid,
    pub public_id: Uuid,
    pub reservation_id: Uuid,
    pub payload_hash: String,
    pub canonical: String,
    pub snapshot: Box<SubscriptionDependencySnapshot>,
}
impl From<SubscriptionCreationReservation> for Publication {
    fn from(ticket: SubscriptionCreationReservation) -> Self {
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
    pub async fn trusted_subscription_publisher(
        &self,
        publisher: PgPool,
    ) -> BlueprintResult<TrustedSubscriptionPublisher> {
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
        self.subscription_runtime_role(&mut runtime_tx).await?;
        runtime_tx.commit().await.map_err(db_error)?;
        let mut publisher_tx = store.transaction().await?;
        store.subscription_runtime_role(&mut publisher_tx).await?;
        publisher_tx.commit().await.map_err(db_error)?;
        Ok(TrustedSubscriptionPublisher {
            store,
            runtime: self.pool.clone(),
        })
    }
    pub(super) fn open_subscription_receipt(
        &self,
        operation: Uuid,
        row: &IntentRow,
    ) -> BlueprintResult<Value> {
        Ok(self.open_subscription_receipt_envelope(operation, row)?["record"].clone())
    }
    pub(super) fn open_subscription_receipt_envelope(
        &self,
        operation: Uuid,
        row: &IntentRow,
    ) -> BlueprintResult<Value> {
        self.open_subscription_intent(operation, row)?;
        if row.4 != "completed" {
            return Err(conflict());
        }
        let ciphertext = row.5.as_deref().ok_or_else(storage_error)?;
        let plaintext = self
            .encryptor
            .decrypt(ciphertext, self.tenant.id())
            .map_err(|_| storage_error())?;
        let envelope: Value = serde_json::from_str(&plaintext).map_err(|_| storage_error())?;
        if envelope["kind"] != "subscription_creation_receipt"
            || envelope["operationId"] != operation.to_string()
            || envelope["publicId"] != row.0
            || envelope["payloadHash"] != row.2
            || envelope["record"]["id"] != row.0
        {
            return Err(storage_error());
        }
        if let Some(evidence) = envelope.get("publicationEvidence") {
            budget(evidence, 0, &mut 0, &mut 0)?;
            if evidence["kind"] != "subscription_created_event"
                || evidence["eventHash"]
                    != hash("event", &super::canonical(&evidence["event"]).to_string())
            {
                return Err(storage_error());
            }
        }
        Ok(envelope)
    }
    /// Inspects a pending intent or encrypted completed receipt; never recreates
    /// a dispatch capability or releases claims from an unresolved operation.
    /// # Errors
    /// Denies wrong actor/tenant, invalid receipts and missing audit privileges.
    pub async fn inspect_bound_subscription_creation(
        &self,
        operation: Uuid,
    ) -> BlueprintResult<Option<SubscriptionCreationStatus>> {
        self.binding_actor()?;
        let mut tx = self.transaction().await?;
        self.subscription_runtime_role(&mut tx).await?;
        let result = if let Some(row) = self.subscription_intent_row(&mut tx, operation).await? {
            self.open_subscription_intent(operation, &row)?;
            let status = if row.4 == "completed" {
                SubscriptionCreationStatus::Completed {
                    operation_id: operation,
                    record: self.open_subscription_receipt(operation, &row)?,
                }
            } else if row.4 == "pending" && row.5.is_none() {
                SubscriptionCreationStatus::Pending(SubscriptionCreationPending {
                    operation_id: operation,
                    public_id: Uuid::parse_str(&row.0).map_err(|_| storage_error())?,
                })
            } else {
                return Err(storage_error());
            };
            self.audit_subscription_creation(&mut tx, operation, "inspect")
                .await?;
            Some(status)
        } else {
            None
        };
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
}
impl TrustedSubscriptionPublisher {
    /// Derives an authenticated actor wrapper without changing the validated pools/tenant.
    /// Only trusted server authentication may select this actor; no new credential authority.
    /// # Errors
    /// Rejects missing/blank actors or invalid audit context.
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
    /// Checks publisher role separation and dependency-lock/read grants before dispatch.
    /// # Errors
    /// Denies mismatched runtime identity/actor, publication privilege or local read grants.
    pub async fn validate_creation_access(
        &self,
        runtime: &PostgresBillingStore,
    ) -> BlueprintResult<()> {
        self.validate_for(runtime).await?;
        self.store.subscription_dependency_access().await
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
        ticket: SubscriptionCreationReservation,
        record: Value,
        fresh: F,
    ) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        self.publish(ticket.into(), record, None, fresh).await
    }
    /// Publishes only after the trusted adapter validates fresh read-only event
    /// evidence. This capability cannot be converted into a dispatch reservation.
    /// # Errors
    /// Denies changed dependencies, invalid evidence budgets or uncertain COMMIT.
    pub async fn complete_recovered<F>(
        &self,
        recovery: SubscriptionCreationRecovery,
        record: Value,
        event: Value,
        fresh: F,
    ) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        budget(&event, 0, &mut 0, &mut 0)?;
        let evidence = json!({"kind":"subscription_created_event", "eventHash":hash("event", &super::canonical(&event).to_string()), "event":event});
        self.publish(recovery.publication, record, Some(evidence), fresh)
            .await
    }
    async fn publish<F>(
        &self,
        ticket: Publication,
        record: Value,
        evidence: Option<Value>,
        fresh: F,
    ) -> BlueprintResult<()>
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
        if let Some(value) = &evidence {
            budget(value, 1, &mut nodes, &mut bytes)?;
        }
        separated_roles(&self.runtime, &store.pool).await?;
        if database_identity(&self.runtime).await? != database_identity(&store.pool).await? {
            return Err(conflict());
        }
        let expected: Value =
            serde_json::from_str(&ticket.canonical).map_err(|_| storage_error())?;
        if hash(&ticket.public_id.to_string(), &ticket.canonical) != ticket.payload_hash {
            return Err(conflict());
        }
        let provider = record["externalId"]
            .as_str()
            .ok_or_else(invalid)?
            .to_owned();
        let encoded = store.encode(Scope::Durable, BillingKind::Subscription, record.clone())?;
        let mut envelope = json!({"kind":"subscription_creation_receipt","operationId":ticket.operation_id.to_string(),"publicId":ticket.public_id.to_string(),"payloadHash":ticket.payload_hash,"record":record});
        if let Some(evidence) = evidence {
            envelope["publicationEvidence"] = evidence;
        }
        budget(&envelope, 0, &mut 0, &mut 0)?;
        let receipt = store
            .encryptor
            .seal(
                &envelope.to_string(),
                store.tenant.id(),
                "[Billing subscription receipt]",
            )
            .map_err(|_| storage_error())?;
        let mut tx = store.transaction().await?;
        store.subscription_runtime_role(&mut tx).await?;
        if store
            .subscription_dependency_data(&mut tx, &ticket.snapshot)
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
            "SELECT fl_billing.publish_bound_subscription($1,$2::uuid,$3::uuid,$4,$5,$6,$7,$8,$9)",
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
        tx.commit().await.map_err(db_error)
    }
}

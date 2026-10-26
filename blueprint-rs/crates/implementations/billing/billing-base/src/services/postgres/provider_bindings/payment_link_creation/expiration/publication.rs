//! Separate-role completion authority; ordinary runtime can never release claims.
use super::{
    PaymentLinkExpirationReservation, PostgresBillingStore, conflict, db_error, hash, invalid,
    storage_error, unknown,
};
use forklaunch_core::domain::BlueprintResult;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
const FUNCTION: &str =
    "fl_billing.publish_payment_link_expiration(text,uuid,uuid,text,jsonb,text,text)";
/// Explicitly trusted separate publisher credential. No default-pool fallback.
/// This boundary does not protect against stolen publisher credentials.
pub struct TrustedPaymentLinkExpirationPublisher {
    store: PostgresBillingStore,
    runtime: PgPool,
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
    pub async fn trusted_payment_link_expiration_publisher(
        &self,
        publisher: PgPool,
    ) -> BlueprintResult<TrustedPaymentLinkExpirationPublisher> {
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
        self.expiration_role(&mut runtime_tx).await?;
        runtime_tx.commit().await.map_err(db_error)?;
        let mut publisher_tx = store.transaction().await?;
        store.expiration_role(&mut publisher_tx).await?;
        publisher_tx.commit().await.map_err(db_error)?;
        Ok(TrustedPaymentLinkExpirationPublisher {
            store,
            runtime: self.pool.clone(),
        })
    }
}
impl TrustedPaymentLinkExpirationPublisher {
    /// Checks selected runtime identity and every publisher read/lock privilege.
    /// # Errors
    /// Denies incomplete publisher grants before any provider effect.
    pub async fn validate_configuration_for(
        &self,
        runtime: &PostgresBillingStore,
    ) -> BlueprintResult<()> {
        self.validate_for(runtime).await?;
        let usage: bool =
            sqlx::query_scalar("SELECT has_schema_privilege(current_user,'fl_billing','USAGE')")
                .fetch_one(&self.store.pool)
                .await
                .map_err(db_error)?;
        if !usage {
            return Err(conflict());
        }
        for (table, privilege) in [
            ("records", "SELECT"),
            ("records", "UPDATE"),
            ("provider_bindings", "SELECT"),
            ("provider_bindings", "UPDATE"),
            ("payment_link_creation_intents", "SELECT"),
            ("payment_link_expiration_intents", "SELECT"),
            ("subscription_publication_identity", "SELECT"),
        ] {
            let allowed: bool =
                sqlx::query_scalar("SELECT has_table_privilege(current_user,$1,$2)")
                    .bind(format!("fl_billing.{table}"))
                    .bind(privilege)
                    .fetch_one(&self.store.pool)
                    .await
                    .map_err(db_error)?;
            if !allowed {
                return Err(conflict());
            }
        }
        Ok(())
    }

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

    /// Completes one acknowledged metadata-only dispatch and releases its two claims.
    /// Owns the transaction; freshness is checked after all awaited writes.
    /// # Errors
    /// COMMIT errors require inspection, never automatic provider retry.
    pub async fn complete<F>(
        &self,
        ticket: PaymentLinkExpirationReservation,
        fresh: F,
    ) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        self.complete_proof(
            Completion {
                operation: ticket.operation,
                public_id: ticket.public_id,
                reservation: ticket.reservation,
                hash: ticket.hash,
                canonical: ticket.canonical,
                snapshot: ticket.snapshot,
            },
            None,
            fresh,
        )
        .await
    }
    /// Publishes a verified read-only recovery; cannot dispatch or expose a key.
    /// The caller is the trusted provider adapter and must verify the supplied evidence.
    /// # Errors
    /// Rejects oversized evidence, stale dependencies, missing claims and uncertain COMMIT.
    pub async fn complete_recovery<F>(
        &self,
        recovery: super::PaymentLinkExpirationRecovery,
        evidence: &Value,
        fresh: F,
    ) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        super::super::super::subscription_creation::budget(evidence, 0, &mut 0, &mut 0)?;
        self.complete_proof(recovery.completion, Some(evidence), fresh)
            .await
    }
    async fn complete_proof<F>(
        &self,
        ticket: Completion,
        evidence: Option<&Value>,
        fresh: F,
    ) -> BlueprintResult<()>
    where
        F: Fn() -> BlueprintResult<()> + Send + Sync,
    {
        let store = &self.store;
        if ticket.snapshot.actor != store.binding_actor()?
            || ticket.snapshot.binding.tenant != store.tenant.id()
            || hash(&ticket.canonical) != ticket.hash
        {
            return Err(conflict());
        }
        separated_roles(&self.runtime, &store.pool).await?;
        if database_identity(&self.runtime).await? != database_identity(&store.pool).await? {
            return Err(conflict());
        }
        let expected: Value =
            serde_json::from_str(&ticket.canonical).map_err(|_| storage_error())?;
        let mut receipt_value =
            json!({"hash":ticket.hash,"publicId":ticket.public_id.to_string(),"status":"EXPIRED"});
        if let Some(evidence) = evidence {
            receipt_value["recovery"] = json!({"kind":"event_linked_metadata_expiration_v1","evidence":evidence,"evidenceHash":super::hash(&super::canonical(evidence).to_string())});
        }
        let receipt = store.seal_expiration(ticket.operation, "receipt", &receipt_value)?;
        let mut tx = store.transaction().await?;
        store.expiration_role(&mut tx).await?;
        store
            .lock_expiration_snapshot(&mut tx, &ticket.snapshot)
            .await?;
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        sqlx::query(
            "SELECT fl_billing.publish_payment_link_expiration($1,$2::uuid,$3::uuid,$4,$5,$6,$7)",
        )
        .bind(store.tenant.id())
        .bind(ticket.operation.to_string())
        .bind(ticket.reservation.to_string())
        .bind(&ticket.hash)
        .bind(sqlx::types::Json(expected))
        .bind(receipt)
        .bind(&store.audit.correlation_id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        if let Err(error) = fresh() {
            tx.rollback().await.map_err(db_error)?;
            return Err(error);
        }
        tx.commit().await.map_err(|_| unknown())
    }
}

/// Private local publication fields; unlike reservation this contains no dispatch key.
pub(super) struct Completion {
    pub(super) operation: Uuid,
    pub(super) public_id: Uuid,
    pub(super) reservation: Uuid,
    pub(super) hash: String,
    pub(super) canonical: String,
    pub(super) snapshot: Box<super::PaymentLinkCreationReadSnapshot>,
}

//! Tenant-isolated billing persistence and TTL cache. Provider payloads and opaque
//! session URIs are encrypted; plaintext reads and actor audits commit together.
mod checkout_repair;
mod provider_bindings;
pub use provider_bindings::{
    PortalCreationClaim, PortalCreationPending, PortalCreationReservation, PortalCreationStatus,
    PortalDependencySnapshot, TrustedPortalPublisher,
};
mod webhooks;
pub use checkout_repair::CheckoutRepairSnapshot;
pub use provider_bindings::{
    CustomerRegistrationSnapshot, DurableStripeCustomerBinding, TrustedCustomerParty,
};
pub use provider_bindings::{
    DurableStripeBinding, ManagedPaymentLinkPublication, PausedEffectAcknowledgement,
    PausedSubscriptionEffectSnapshot, PaymentLinkCreationClaim, PaymentLinkCreationReadSnapshot,
    PaymentLinkCreationReservation, PaymentLinkCreationStatus, PaymentLinkExpirationClaim,
    PaymentLinkExpirationRecovery, PaymentLinkExpirationReservation, PaymentLinkExpirationStatus,
    StripeBindingScope, StripeSubscriptionOwnership, TrustedPaymentLinkExpirationPublisher,
};
pub use provider_bindings::{
    PlanCreationClaim, PlanCreationReadSnapshot, PlanCreationReservation, PlanCreationStatus,
};
pub use provider_bindings::{
    SubscriptionCreationClaim, SubscriptionCreationPending, SubscriptionCreationReadSnapshot,
    SubscriptionCreationRecovery, SubscriptionCreationReservation, SubscriptionCreationStatus,
    SubscriptionDependencySnapshot, TrustedSubscriptionPublisher,
};
pub use webhooks::{PendingWebhook, WebhookResolution};

use super::{
    BillingKind, BillingStore, Scope, conflict, invalid, merge, not_found, storage_error,
    validate_id, validate_record,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::{
    TenantContext, begin_tenant_transaction, encryption::FieldEncryptor,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use std::{sync::Arc, time::Duration};
enum DecodeProfile<'a> {
    Legacy(&'a str),
    ConfiguredCached,
}
#[derive(Clone)]
pub struct PostgresBillingStore {
    pool: PgPool,
    tenant: TenantContext,
    encryptor: Arc<FieldEncryptor>,
    audit: AuditContext,
    webhook_slots: Arc<tokio::sync::Semaphore>,
}
impl PostgresBillingStore {
    /// # Errors
    /// Returns sanitized migration failures. Run with an owner connection.
    pub async fn migrate(pool: &PgPool) -> BlueprintResult<()> {
        let mut tx = pool.begin().await.map_err(db_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(0x464C_4249_4C01_i64)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        sqlx::query("CREATE SCHEMA IF NOT EXISTS fl_billing")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        sqlx::query("SET LOCAL search_path=fl_billing,pg_catalog")
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        let mut migrator = sqlx::migrate!("./migrations");
        migrator.set_locking(false);
        migrator
            .run_direct(None, &mut *tx, false)
            .await
            .map_err(|_| storage_error())?;
        tx.commit().await.map_err(db_error)
    }
    /// # Errors
    /// Rejects runtime roles bypassing RLS and missing audit correlation IDs.
    pub async fn new(
        pool: PgPool,
        tenant: TenantContext,
        encryptor: Arc<FieldEncryptor>,
        audit: AuditContext,
    ) -> BlueprintResult<Self> {
        validate_audit(&audit)?;
        let (bypass,): (bool,) = sqlx::query_as(
            "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
        )
        .fetch_one(&pool)
        .await
        .map_err(db_error)?;
        if bypass {
            return Err(BlueprintError::new(
                "billing_unsafe_database_role",
                "Billing runtime requires a role without SUPERUSER or BYPASSRLS",
            ));
        }
        let webhook_slots = Arc::new(tokio::sync::Semaphore::new(
            (pool.options().get_max_connections() / 2).max(1) as usize,
        ));
        Ok(Self {
            pool,
            tenant,
            encryptor,
            audit,
            webhook_slots,
        })
    }
    /// # Errors
    /// Rejects missing correlation IDs.
    pub fn with_audit_context(&self, audit: AuditContext) -> BlueprintResult<Self> {
        validate_audit(&audit)?;
        Ok(Self {
            audit,
            ..self.clone()
        })
    }
    /// Claims a verified platform nonce across replicas for ten minutes.
    /// # Errors
    /// Rejects malformed nonces or returns sanitized database failures.
    pub async fn claim_platform_event_nonce(&self, nonce: &str) -> BlueprintResult<bool> {
        validate_id(nonce)?;
        let mut tx = self.transaction().await?;
        sqlx::query(
            "DELETE FROM fl_billing.platform_event_nonces WHERE tenant_id=$1 AND expires_at<=now()",
        )
        .bind(self.tenant.id())
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
        let inserted = sqlx::query("INSERT INTO fl_billing.platform_event_nonces(tenant_id,nonce,expires_at) VALUES($1,$2,now()+interval '10 minutes') ON CONFLICT DO NOTHING")
            .bind(self.tenant.id()).bind(nonce).execute(&mut *tx).await.map_err(db_error)?.rows_affected();
        tx.commit().await.map_err(db_error)?;
        Ok(inserted == 1)
    }
    async fn transaction(&self) -> BlueprintResult<Transaction<'_, Postgres>> {
        begin_tenant_transaction(&self.pool, &self.tenant)
            .await
            .map_err(|_| storage_error())
    }
    fn encode(&self, scope: Scope, kind: BillingKind, mut data: Value) -> BlueprintResult<Value> {
        validate_record(kind, &data)?;
        for &field in sensitive_fields(kind) {
            if let Some(value) = data.get(field) {
                let envelope = json!({"scope":scope.as_str(),"kind":kind.as_str(),"id":data["id"],"field":field,"value":value});
                let encrypted = self
                    .encryptor
                    .seal(
                        &envelope.to_string(),
                        self.tenant.id(),
                        "[Billing provider data]",
                    )
                    .map_err(|_| storage_error())?;
                data[field] = json!(encrypted);
            }
        }
        Ok(data)
    }
    async fn decode(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Scope,
        kind: BillingKind,
        id: &str,
        data: Value,
        operation: &str,
    ) -> BlueprintResult<Value> {
        self.decode_profile(tx, scope, kind, id, data, DecodeProfile::Legacy(operation))
            .await
    }
    async fn decode_profile(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Scope,
        kind: BillingKind,
        id: &str,
        mut data: Value,
        profile: DecodeProfile<'_>,
    ) -> BlueprintResult<Value> {
        if data["id"].as_str() != Some(id) {
            return Err(storage_error());
        }
        let mut sensitive = false;
        for &field in sensitive_fields(kind) {
            if let Some(value) = data.get(field) {
                let encrypted = value.as_str().ok_or_else(storage_error)?;
                let plaintext = self
                    .encryptor
                    .decrypt(encrypted, self.tenant.id())
                    .map_err(|_| storage_error())?;
                let envelope: Value =
                    serde_json::from_str(&plaintext).map_err(|_| storage_error())?;
                if envelope["scope"] != scope.as_str()
                    || envelope["kind"] != kind.as_str()
                    || envelope["id"] != id
                    || envelope["field"] != field
                {
                    return Err(storage_error());
                }
                data[field] = envelope["value"].clone();
                sensitive = true;
            }
        }
        if matches!(&profile, DecodeProfile::ConfiguredCached) {
            data = super::cached_dates::normalize(kind, data)?;
        }
        validate_record(kind, &data).map_err(|_| storage_error())?;
        let operation = match profile {
            DecodeProfile::Legacy(operation) => operation,
            DecodeProfile::ConfiguredCached => "get_configured_cached",
        };
        if sensitive {
            sqlx::query("INSERT INTO fl_billing.sensitive_read_audit(id,tenant_id,scope,kind,record_id,actor_id,correlation_id,operation) VALUES($1::uuid,$2,$3,$4,$5,$6,$7,$8)")
                .bind(uuid::Uuid::new_v4().to_string()).bind(self.tenant.id()).bind(scope.as_str()).bind(kind.as_str()).bind(id).bind(&self.audit.actor_id).bind(&self.audit.correlation_id).bind(operation).execute(&mut **tx).await.map_err(db_error)?;
        }
        Ok(data)
    }
    /// Removes expired cache rows; durable backups are retained.
    /// # Errors
    /// Returns sanitized storage failures.
    pub async fn purge_expired_cache(&self) -> BlueprintResult<u64> {
        let mut tx = self.transaction().await?;
        let result=sqlx::query("DELETE FROM fl_billing.records WHERE tenant_id=$1 AND scope='cache' AND expires_at<=now()").bind(self.tenant.id()).execute(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(result.rows_affected())
    }
}
impl BillingStore for PostgresBillingStore {
    async fn insert(
        &self,
        scope: Scope,
        kind: BillingKind,
        data: Value,
        ttl: Duration,
    ) -> BlueprintResult<Value> {
        if kind == BillingKind::Customer && scope != Scope::Durable {
            return Err(invalid());
        }
        let id = data["id"].as_str().ok_or_else(invalid)?;
        validate_id(id)?;
        let encoded = self.encode(scope, kind, data.clone())?;
        let milliseconds = ttl_ms(ttl)?;
        let mut tx = self.transaction().await?;
        // Cache puts replace existing values, exactly as the TS TTL cache does.
        let sql = if scope == Scope::Cache {
            "INSERT INTO fl_billing.records(tenant_id,scope,kind,id,data,expires_at) VALUES($1,$2,$3,$4,$5,now()+$6::bigint*interval '1 millisecond') ON CONFLICT(tenant_id,scope,kind,id) DO UPDATE SET data=EXCLUDED.data,expires_at=EXCLUDED.expires_at"
        } else {
            "INSERT INTO fl_billing.records(tenant_id,scope,kind,id,data,expires_at) VALUES($1,$2,$3,$4,$5,CASE WHEN $2='cache' THEN now()+$6::bigint*interval '1 millisecond' ELSE NULL END)"
        };
        sqlx::query(sql)
            .bind(self.tenant.id())
            .bind(scope.as_str())
            .bind(kind.as_str())
            .bind(id)
            .bind(sqlx::types::Json(encoded))
            .bind(milliseconds)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(data)
    }
    async fn get(
        &self,
        scope: Scope,
        kind: BillingKind,
        id: &str,
    ) -> BlueprintResult<Option<Value>> {
        validate_id(id)?;
        let mut tx = self.transaction().await?;
        let row:Option<sqlx::types::Json<Value>>=sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND scope=$2 AND kind=$3 AND id=$4 AND (expires_at IS NULL OR expires_at>now())")
            .bind(self.tenant.id()).bind(scope.as_str()).bind(kind.as_str()).bind(id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let value = match row {
            Some(row) => Some(self.decode(&mut tx, scope, kind, id, row.0, "get").await?),
            None => None,
        };
        tx.commit().await.map_err(db_error)?;
        Ok(value)
    }
    async fn get_configured_cached(
        &self,
        kind: BillingKind,
        id: &str,
    ) -> BlueprintResult<Option<Value>> {
        validate_id(id)?;
        super::cached_dates::ensure_kind(kind)?;
        let mut tx = self.transaction().await?;
        let row: Option<sqlx::types::Json<Value>> = sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND scope='cache' AND kind=$2 AND id=$3 AND (expires_at IS NULL OR expires_at>now())")
            .bind(self.tenant.id()).bind(kind.as_str()).bind(id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let value = match row {
            Some(row) => Some(
                self.decode_profile(
                    &mut tx,
                    Scope::Cache,
                    kind,
                    id,
                    row.0,
                    DecodeProfile::ConfiguredCached,
                )
                .await?,
            ),
            None => None,
        };
        tx.commit().await.map_err(db_error)?;
        Ok(value)
    }
    async fn list_configured_cached(
        &self,
        ids: Option<&[String]>,
        limit: usize,
    ) -> BlueprintResult<Vec<(String, Value)>> {
        super::cached_dates::selection(ids, limit)?;
        let mut tx = self.transaction().await?;
        let selected = if let Some(ids) = ids.filter(|ids| !ids.is_empty()) {
            ids.to_vec()
        } else {
            let bound =
                i64::try_from(limit + 1).map_err(|_| super::cached_dates::list_invalid())?;
            let keys: Vec<String> = sqlx::query_scalar("SELECT id FROM fl_billing.records WHERE tenant_id=$1 AND scope='cache' AND kind='payment_link' AND (expires_at IS NULL OR expires_at>now()) ORDER BY created_at,id LIMIT $2")
                .bind(self.tenant.id()).bind(bound).fetch_all(&mut *tx).await.map_err(db_error)?;
            if keys.len() > limit {
                return Err(super::cached_dates::list_invalid());
            }
            keys
        };
        super::cached_dates::selection(Some(&selected), limit)?;
        let mut result = Vec::with_capacity(selected.len());
        for id in selected {
            let raw: Option<sqlx::types::Json<Value>> = sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND scope='cache' AND kind='payment_link' AND id=$2 AND (expires_at IS NULL OR expires_at>now())")
                .bind(self.tenant.id()).bind(&id).fetch_optional(&mut *tx).await.map_err(db_error)?;
            let raw = raw.ok_or_else(super::cached_dates::list_invalid)?;
            let value = self
                .decode_profile(
                    &mut tx,
                    Scope::Cache,
                    BillingKind::PaymentLink,
                    &id,
                    raw.0,
                    DecodeProfile::ConfiguredCached,
                )
                .await?;
            result.push((id, value));
        }
        tx.commit().await.map_err(db_error)?;
        Ok(result)
    }
    async fn patch(
        &self,
        scope: Scope,
        kind: BillingKind,
        id: &str,
        patch: Value,
        ttl: Duration,
    ) -> BlueprintResult<Value> {
        validate_id(id)?;
        let mut tx = self.transaction().await?;
        let row:Option<sqlx::types::Json<Value>>=sqlx::query_scalar("SELECT data FROM fl_billing.records WHERE tenant_id=$1 AND scope=$2 AND kind=$3 AND id=$4 AND (expires_at IS NULL OR expires_at>now()) FOR UPDATE")
            .bind(self.tenant.id()).bind(scope.as_str()).bind(kind.as_str()).bind(id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let old = self
            .decode(
                &mut tx,
                scope,
                kind,
                id,
                row.ok_or_else(not_found)?.0,
                "update",
            )
            .await?;
        let data = merge(old, &patch)?;
        if data["id"].as_str() != Some(id) {
            return Err(invalid());
        }
        let encoded = self.encode(scope, kind, data.clone())?;
        sqlx::query("UPDATE fl_billing.records SET data=$5,expires_at=CASE WHEN scope='cache' THEN now()+$6::bigint*interval '1 millisecond' ELSE NULL END WHERE tenant_id=$1 AND scope=$2 AND kind=$3 AND id=$4")
            .bind(self.tenant.id()).bind(scope.as_str()).bind(kind.as_str()).bind(id).bind(sqlx::types::Json(encoded)).bind(ttl_ms(ttl)?).execute(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(data)
    }
    async fn delete(&self, scope: Scope, kind: BillingKind, id: &str) -> BlueprintResult<bool> {
        validate_id(id)?;
        let mut tx = self.transaction().await?;
        // An expired cache row is already logically absent.
        let row:Option<(bool,)>=sqlx::query_as("DELETE FROM fl_billing.records WHERE tenant_id=$1 AND scope=$2 AND kind=$3 AND id=$4 RETURNING expires_at IS NULL OR expires_at>now()")
            .bind(self.tenant.id()).bind(scope.as_str()).bind(kind.as_str()).bind(id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(row.is_some_and(|(live,)| live))
    }
    async fn list(
        &self,
        scope: Scope,
        kind: BillingKind,
        ids: Option<&[String]>,
    ) -> BlueprintResult<Vec<Value>> {
        let ids = ids.filter(|ids| !ids.is_empty());
        if let Some(ids) = ids {
            for id in ids {
                validate_id(id)?;
            }
        }
        let mut tx = self.transaction().await?;
        let rows:Vec<(String,sqlx::types::Json<Value>)>=sqlx::query_as("SELECT id,data FROM fl_billing.records WHERE tenant_id=$1 AND scope=$2 AND kind=$3 AND (expires_at IS NULL OR expires_at>now()) AND ($4::text[] IS NULL OR id=ANY($4)) ORDER BY created_at,id")
            .bind(self.tenant.id()).bind(scope.as_str()).bind(kind.as_str()).bind(ids.map(<[String]>::to_vec)).fetch_all(&mut *tx).await.map_err(db_error)?;
        let mut decoded = Vec::with_capacity(rows.len());
        for (id, data) in rows {
            decoded.push(
                self.decode(&mut tx, scope, kind, &id, data.0, "list")
                    .await?,
            );
        }
        if scope == Scope::Cache
            && let Some(ids) = ids
        {
            decoded = ids
                .iter()
                .map(|id| {
                    decoded
                        .iter()
                        .find(|row| row["id"] == id.as_str())
                        .cloned()
                        .ok_or_else(not_found)
                })
                .collect::<BlueprintResult<Vec<_>>>()?;
        }
        tx.commit().await.map_err(db_error)?;
        Ok(decoded)
    }
    async fn find_active_subscription(
        &self,
        party_id: &str,
        party_type: &str,
    ) -> BlueprintResult<Option<Value>> {
        validate_id(party_id)?;
        let mut tx = self.transaction().await?;
        let row:Option<(String,sqlx::types::Json<Value>)>=sqlx::query_as("SELECT id,data FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='subscription' AND data->>'partyId'=$2 AND data->>'partyType'=$3 AND data->>'active'='true' ORDER BY created_at,id LIMIT 1")
            .bind(self.tenant.id()).bind(party_id).bind(party_type).fetch_optional(&mut *tx).await.map_err(db_error)?;
        let value = match row {
            Some((id, row)) => Some(
                self.decode(
                    &mut tx,
                    Scope::Durable,
                    BillingKind::Subscription,
                    &id,
                    row.0,
                    "party-lookup",
                )
                .await?,
            ),
            None => None,
        };
        tx.commit().await.map_err(db_error)?;
        Ok(value)
    }
    async fn find_subscription_id_by_external_id(
        &self,
        external_id: &str,
    ) -> BlueprintResult<Option<String>> {
        validate_id(external_id)?;
        let mut tx = self.transaction().await?;
        let id=sqlx::query_scalar("SELECT id FROM fl_billing.records WHERE tenant_id=$1 AND scope='durable' AND kind='subscription' AND data->>'externalId'=$2 LIMIT 1")
            .bind(self.tenant.id()).bind(external_id).fetch_optional(&mut *tx).await.map_err(db_error)?;
        tx.commit().await.map_err(db_error)?;
        Ok(id)
    }
}
fn ttl_ms(ttl: Duration) -> BlueprintResult<i64> {
    let milliseconds = i64::try_from(ttl.as_millis()).map_err(|_| invalid())?;
    if milliseconds <= 0 || milliseconds > 31_536_000_000 {
        return Err(invalid());
    }
    Ok(milliseconds)
}
fn validate_audit(audit: &AuditContext) -> BlueprintResult<()> {
    if audit.correlation_id.trim().is_empty() {
        return Err(invalid());
    }
    Ok(())
}
#[allow(clippy::needless_pass_by_value)] // Used directly with Result::map_err.
fn db_error(error: sqlx::Error) -> BlueprintError {
    if error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .as_deref()
        == Some("23505")
    {
        conflict()
    } else {
        storage_error()
    }
}

fn sensitive_fields(kind: BillingKind) -> &'static [&'static str] {
    if kind == BillingKind::Customer {
        &["email", "name"]
    } else {
        &["providerFields", "uri"]
    }
}

pub use provider_bindings::{
    CheckoutCreationClaim, CheckoutCreationPending, CheckoutCreationReadSnapshot,
    CheckoutCreationReservation, CheckoutCreationStatus, CheckoutDependencySnapshot,
    TrustedCheckoutPublisher,
};

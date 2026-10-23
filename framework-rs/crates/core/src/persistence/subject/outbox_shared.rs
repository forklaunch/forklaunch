use super::SubjectRequest;
use crate::encryption::EnvelopeKey;
use crate::{
    common::{FrameworkError, FrameworkResult},
};
use sqlx::{PgPool, Row};
use std::{future::Future, sync::Arc};
use uuid::Uuid;
use zeroize::Zeroizing;

/// Stable per-target idempotency ID and opaque service-owned locator. No Debug or
/// Serialize implementation. Locators must select immutable object versions, or
/// the destination must use conditional deletion to preserve later writes.
pub struct ErasureTarget {
    pub id: Uuid,
    pub locator: Zeroizing<Vec<u8>>,
}
/// Return success only after durable deletion (or verified prior deletion) of
/// the exact version. Retries and concurrent invocations use identical IDs.
/// Authorization/holds must be rechecked; unsupported targets fail closed.
pub trait ExternalErasureDestination: Sync {
    fn erase(
        &self,
        request: &SubjectRequest<'_>,
        target: &ErasureTarget,
    ) -> impl Future<Output = FrameworkResult<()>> + Send;
}
#[derive(Debug, PartialEq, Eq)]
pub struct ErasureReconciliation {
    pub acknowledged: usize,
    pub complete: bool,
}
pub struct PostgresErasureOutbox {
    pool: PgPool,
    key: Arc<EnvelopeKey>,
    audit: Arc<dyn SubjectEraseAudit>,
}
impl PostgresErasureOutbox {
    #[must_use]
    pub fn new(pool: PgPool, key: Arc<EnvelopeKey>, audit: Arc<dyn SubjectEraseAudit>) -> Self {
        Self { pool, key, audit }
    }
    /// Owner-run DDL. Runtime needs SELECT/INSERT on manifests and SELECT/INSERT/
    /// UPDATE on tasks, with forced tenant RLS; no DELETE privileges. Keep scope
    /// keys and receipt tombstones until operation IDs can never be retried.
    #[must_use]
    pub const fn migration_sql() -> &'static str {
        "CREATE TABLE forklaunch_erasure_sets(tenant_id uuid NOT NULL,operation_id uuid NOT NULL,scope_tag text NOT NULL,manifest_tag text NOT NULL,target_count integer NOT NULL CHECK(target_count BETWEEN 0 AND 1000),PRIMARY KEY(tenant_id,operation_id));CREATE TABLE forklaunch_erasure_tasks(tenant_id uuid NOT NULL,operation_id uuid NOT NULL,target_id uuid NOT NULL,ciphertext text NOT NULL,complete boolean NOT NULL DEFAULT false,PRIMARY KEY(tenant_id,operation_id,target_id),FOREIGN KEY(tenant_id,operation_id) REFERENCES forklaunch_erasure_sets(tenant_id,operation_id));ALTER TABLE forklaunch_erasure_sets ENABLE ROW LEVEL SECURITY;ALTER TABLE forklaunch_erasure_sets FORCE ROW LEVEL SECURITY;CREATE POLICY erasure_sets_tenant ON forklaunch_erasure_sets USING(tenant_id::text=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant_id::text=current_setting('forklaunch.tenant_id',true));ALTER TABLE forklaunch_erasure_tasks ENABLE ROW LEVEL SECURITY;ALTER TABLE forklaunch_erasure_tasks FORCE ROW LEVEL SECURITY;CREATE POLICY erasure_tasks_tenant ON forklaunch_erasure_tasks USING(tenant_id::text=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant_id::text=current_setting('forklaunch.tenant_id',true));"
    }
    /// Seal the complete declared target set inside the caller's tenant-bound
    /// source deletion transaction. No commit is performed here. Invoke before
    /// source mutation; on error roll back the entire transaction. Audit failure
    /// writes nothing. A changed target set under an existing operation is denied.
    /// # Errors
    /// Rejects wrong tenant transaction, invalid/reused scope, or storage failure.
    pub async fn enqueue(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        request: &SubjectRequest<'_>,
        targets: &[ErasureTarget],
    ) -> FrameworkResult<()> {
        let (tenant, operation, binding) = scope(request)?;
        if targets.len() > 1000 {
            return Err(error());
        }
        let mut sorted: Vec<_> = targets.iter().collect();
        sorted.sort_by_key(|target| target.id);
        if sorted.windows(2).any(|pair| pair[0].id == pair[1].id)
            || sorted.iter().any(|target| {
                target.id.is_nil() || target.locator.is_empty() || target.locator.len() > 4096
            })
        {
            return Err(error());
        }
        let active_tenant: Option<String> =
            sqlx::query_scalar("SELECT current_setting('forklaunch.tenant_id',true)")
                .fetch_one(&mut **tx)
                .await
                .map_err(storage)?;
        if active_tenant.as_deref() != Some(request.tenant.id()) {
            return Err(error());
        }
        sqlx::query("SELECT set_config('synchronous_commit','on',true),set_config('statement_timeout','5000',true),set_config('lock_timeout','2000',true)")
            .execute(&mut **tx).await.map_err(storage)?;
        self.audit.intent(request).await?;
        let mut manifest = Zeroizing::new(Vec::new());
        for target in &sorted {
            manifest.extend_from_slice(target.id.as_bytes());
            manifest.extend_from_slice(
                &u64::try_from(target.locator.len())
                    .map_err(|_| error())?
                    .to_be_bytes(),
            );
            manifest.extend_from_slice(&target.locator);
        }
        if manifest.len() > 1_048_576 {
            return Err(error());
        }
        let scope_tag = self
            .key
            .sign_public_metadata(tenant, operation, "erasure-scope", &binding)
            .map_err(|_| error())?;
        let manifest_tag = self
            .key
            .sign_public_metadata(tenant, operation, "erasure-manifest", &manifest)
            .map_err(|_| error())?;
        let inserted=sqlx::query("INSERT INTO forklaunch_erasure_sets(tenant_id,operation_id,scope_tag,manifest_tag,target_count) VALUES($1::uuid,$2::uuid,$3,$4,$5) ON CONFLICT DO NOTHING")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(scope_tag).bind(manifest_tag).bind(i32::try_from(targets.len()).map_err(|_|error())?).execute(&mut **tx).await.map_err(storage)?.rows_affected();
        let row=sqlx::query("SELECT scope_tag,manifest_tag,target_count FROM forklaunch_erasure_sets WHERE tenant_id=$1::uuid AND operation_id=$2::uuid").bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut **tx).await.map_err(storage)?;
        self.key
            .verify_public_metadata(
                tenant,
                operation,
                "erasure-scope",
                &binding,
                &row.try_get::<String, _>("scope_tag").map_err(storage)?,
            )
            .map_err(|_| error())?;
        self.key
            .verify_public_metadata(
                tenant,
                operation,
                "erasure-manifest",
                &manifest,
                &row.try_get::<String, _>("manifest_tag").map_err(storage)?,
            )
            .map_err(|_| error())?;
        if inserted == 0 {
            return Ok(());
        }
        for target in sorted {
            let mut bytes = Zeroizing::new(operation.as_bytes().to_vec());
            bytes.extend_from_slice(&target.locator);
            let ciphertext = self
                .key
                .seal(tenant, target.id, "subject-erasure-target", &bytes)
                .map_err(|_| error())?;
            sqlx::query("INSERT INTO forklaunch_erasure_tasks(tenant_id,operation_id,target_id,ciphertext) VALUES($1::uuid,$2::uuid,$3::uuid,$4)").bind(tenant.to_string()).bind(operation.to_string()).bind(target.id.to_string()).bind(ciphertext).execute(&mut **tx).await.map_err(storage)?;
        }
        Ok(())
    }
    /// Reconcile at most one requested batch. Failed destination/receipt leaves
    /// durable pending work; retries may invoke a destination more than once.
    /// No database lock is held over remote work. This never reports whole-subject
    /// erasure: completion means only all explicitly sealed targets acknowledged.
    /// # Errors
    /// Missing manifest, failed intent, tampering, destination or SQL failure.
    pub async fn reconcile<D: ExternalErasureDestination>(
        &self,
        request: &SubjectRequest<'_>,
        destination: &D,
    ) -> FrameworkResult<ErasureReconciliation> {
        let (tenant, operation, binding) = scope(request)?;
        self.audit.intent(request).await?;
        let mut tx = begin_tenant_transaction(&self.pool, request.tenant).await?;
        let manifest=sqlx::query("SELECT scope_tag,target_count FROM forklaunch_erasure_sets WHERE tenant_id=$1::uuid AND operation_id=$2::uuid").bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        self.key
            .verify_public_metadata(
                tenant,
                operation,
                "erasure-scope",
                &binding,
                &manifest
                    .try_get::<String, _>("scope_tag")
                    .map_err(storage)?,
            )
            .map_err(|_| error())?;
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM forklaunch_erasure_tasks WHERE tenant_id=$1::uuid AND operation_id=$2::uuid").bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        if count
            != i64::from(
                manifest
                    .try_get::<i32, _>("target_count")
                    .map_err(storage)?,
            )
        {
            return Err(error());
        }
        let rows=sqlx::query("SELECT target_id::text,ciphertext FROM forklaunch_erasure_tasks WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND NOT complete ORDER BY target_id LIMIT $3").bind(tenant.to_string()).bind(operation.to_string()).bind(i64::try_from(request.batch_size).map_err(|_|error())?).fetch_all(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        let mut acknowledged = 0;
        for row in rows {
            let id = Uuid::parse_str(&row.try_get::<String, _>("target_id").map_err(storage)?)
                .map_err(|_| error())?;
            let bytes = self
                .key
                .open(
                    tenant,
                    id,
                    "subject-erasure-target",
                    &row.try_get::<String, _>("ciphertext").map_err(storage)?,
                )
                .map_err(|_| error())?;
            if bytes.len() < 16 || &bytes[..16] != operation.as_bytes() {
                return Err(error());
            }
            let target = ErasureTarget {
                id,
                locator: Zeroizing::new(bytes[16..].to_vec()),
            };
            destination.erase(request, &target).await?;
            let mut tx = begin_tenant_transaction(&self.pool, request.tenant).await?;
            sqlx::query("SELECT set_config('synchronous_commit','on',true)")
                .execute(&mut *tx)
                .await
                .map_err(storage)?;
            acknowledged+=usize::try_from(sqlx::query("UPDATE forklaunch_erasure_tasks SET complete=true WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND target_id=$3::uuid AND NOT complete").bind(tenant.to_string()).bind(operation.to_string()).bind(id.to_string()).execute(&mut *tx).await.map_err(storage)?.rows_affected()).map_err(|_|error())?;
            tx.commit().await.map_err(storage)?;
        }
        let mut tx = begin_tenant_transaction(&self.pool, request.tenant).await?;
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM forklaunch_erasure_tasks WHERE tenant_id=$1::uuid AND operation_id=$2::uuid AND NOT complete)").bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(ErasureReconciliation {
            acknowledged,
            complete: !pending,
        })
    }
}
fn scope(request: &SubjectRequest<'_>) -> FrameworkResult<(Uuid, Uuid, Zeroizing<Vec<u8>>)> {
    request.validate(1)?;
    let tenant = Uuid::parse_str(request.tenant.id()).map_err(|_| error())?;
    let operation = Uuid::parse_str(request.operation_id).map_err(|_| error())?;
    if tenant.is_nil() || operation.is_nil() {
        return Err(error());
    }
    let binding = serde_json::to_vec(&(
        request.entity.entity(),
        request.entity.subject_field(),
        request.subject,
    ))
    .map_err(|_| error())?;
    Ok((tenant, operation, Zeroizing::new(binding)))
}
fn storage(_: sqlx::Error) -> FrameworkError {
    error()
}
fn error() -> FrameworkError {
    FrameworkError::new(
        "external_erasure_incomplete",
        "External erasure did not complete",
    )
}

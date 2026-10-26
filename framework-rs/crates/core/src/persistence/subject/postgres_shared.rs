use super::{EraseBatch, SubjectEraseStore, SubjectRequest};
use crate::encryption::EnvelopeKey;
use crate::{
    common::{FrameworkError, FrameworkResult},
    persistence::compliance::ComplianceMetadata,
};
use sqlx::{PgPool, Row};
use std::{future::Future, pin::Pin, sync::Arc};
use uuid::Uuid;

/// Application schema: tenant/id UUID, subject text, `created_at` timestamptz,
/// hold boolean NOT NULL. IDs and creation timestamps must be immutable and assigned by the database; IDs
/// unique per tenant. The subject relationship is explicitly declared metadata.
pub struct SubjectEraseColumns {
    pub table: String,
    pub tenant: String,
    pub id: String,
    pub subject: String,
    pub created_at: String,
    pub hold: String,
}
/// Bind an authorized actor and acknowledge durable intent before each batch.
/// Intent does not mean deletion completed. No data values are supplied beyond
/// the already-authorized request scope; implementations must not log identifiers.
pub trait SubjectEraseAudit: Send + Sync {
    fn intent<'a>(
        &'a self,
        request: &'a SubjectRequest<'_>,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>>;
}
pub struct PostgresSubjectEraseStore {
    pool: PgPool,
    key: Arc<EnvelopeKey>,
    columns: SubjectEraseColumns,
    entity: &'static str,
    audit: Arc<dyn SubjectEraseAudit>,
}
impl PostgresSubjectEraseStore {
    /// Explicitly select database-only semantics. A configured hold prevents
    /// completion, rather than silently counting excluded records as erased.
    /// This adapter never interprets a creator/email field as subject ownership.
    /// # Errors
    /// Rejects unsafe/overlapping identifiers or undeclared subject relationships.
    pub fn database_only<T: ComplianceMetadata>(
        pool: PgPool,
        key: Arc<EnvelopeKey>,
        columns: SubjectEraseColumns,
        audit: Arc<dyn SubjectEraseAudit>,
    ) -> FrameworkResult<Self> {
        let names = [
            &columns.tenant,
            &columns.id,
            &columns.subject,
            &columns.created_at,
            &columns.hold,
        ];
        let mut unique = std::collections::BTreeSet::new();
        if !identifier(&columns.table)
            || names
                .iter()
                .any(|s| !identifier(s) || !unique.insert(s.as_str()))
            || !T::FIELDS.iter().any(|f| f.storage_name == columns.subject)
        {
            return Err(error("invalid_subject_columns"));
        }
        Ok(Self {
            pool,
            key,
            columns,
            entity: T::ENTITY,
            audit,
        })
    }
    /// Owner-run migration; no implicit schema changes or runtime grants. Grant
    /// only SELECT/INSERT/UPDATE to the restricted runtime role; never DELETE.
    #[must_use]
    pub const fn migration_sql() -> &'static str {
        "CREATE TABLE forklaunch_subject_erasure(tenant_id uuid NOT NULL,operation_id uuid NOT NULL,scope_tag text NOT NULL,started_at timestamptz NOT NULL DEFAULT clock_timestamp(),completed boolean NOT NULL DEFAULT false,deleted_count bigint NOT NULL DEFAULT 0 CHECK(deleted_count>=0),PRIMARY KEY(tenant_id,operation_id)); ALTER TABLE forklaunch_subject_erasure ENABLE ROW LEVEL SECURITY; ALTER TABLE forklaunch_subject_erasure FORCE ROW LEVEL SECURITY; CREATE POLICY subject_erasure_tenant ON forklaunch_subject_erasure USING(tenant_id::text=current_setting('forklaunch.tenant_id',true)) WITH CHECK(tenant_id::text=current_setting('forklaunch.tenant_id',true));"
    }
    #[allow(clippy::too_many_lines)] // Keep durable registration and deletion transaction ordering visible.
    async fn apply(&self, request: &SubjectRequest<'_>) -> FrameworkResult<EraseBatch> {
        request.validate(1)?;
        let tenant =
            Uuid::parse_str(request.tenant.id()).map_err(|_| error("invalid_subject_scope"))?;
        let operation =
            Uuid::parse_str(request.operation_id).map_err(|_| error("invalid_subject_scope"))?;
        if tenant.is_nil()
            || operation.is_nil()
            || request.entity.entity() != self.entity
            || request.entity.subject_field() != self.columns.subject
        {
            return Err(error("invalid_subject_scope"));
        }
        let c = &self.columns;
        let binding = serde_json::to_vec(&(
            "database-only-erase",
            self.entity,
            &c.table,
            &c.tenant,
            &c.id,
            &c.subject,
            &c.created_at,
            &c.hold,
            request.subject,
        ))
        .map_err(|_| error("invalid_subject_scope"))?;
        let tags = self
            .key
            .blind_indexes(tenant, "subject-erasure-scope", &binding)
            .map_err(|_| error("subject_scope_unavailable"))?;
        self.audit
            .intent(request)
            .await
            .map_err(|_| error("subject_audit_failed"))?;
        let mut tx = begin_tenant_transaction(&self.pool, request.tenant).await?;
        sqlx::query("SELECT set_config('statement_timeout','5000',true),set_config('lock_timeout','2000',true),set_config('synchronous_commit','on',true)").execute(&mut *tx).await.map_err(storage)?;
        sqlx::query("INSERT INTO forklaunch_subject_erasure(tenant_id,operation_id,scope_tag) VALUES($1::uuid,$2::uuid,$3) ON CONFLICT DO NOTHING")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(&tags[0]).execute(&mut *tx).await.map_err(storage)?;
        let ledger=sqlx::query("SELECT scope_tag,completed FROM forklaunch_subject_erasure WHERE tenant_id=$1::uuid AND operation_id=$2::uuid FOR UPDATE")
            .bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        let tag: String = ledger.try_get("scope_tag").map_err(storage)?;
        if !tags.contains(&tag) {
            return Err(error("subject_operation_reused"));
        }
        if ledger.try_get::<bool, _>("completed").map_err(storage)? {
            tx.commit().await.map_err(storage)?;
            return Ok(EraseBatch {
                deleted: 0,
                complete: true,
            });
        }
        // Commit the immutable operation cutoff before deletion. A rolled-back
        // batch or a process crash must not silently enlarge a retry's scope.
        tx.commit().await.map_err(storage)?;
        let mut tx = begin_tenant_transaction(&self.pool, request.tenant).await?;
        sqlx::query("SELECT set_config('statement_timeout','5000',true),set_config('lock_timeout','2000',true),set_config('synchronous_commit','on',true)").execute(&mut *tx).await.map_err(storage)?;
        let completed: bool = sqlx::query_scalar("SELECT completed FROM forklaunch_subject_erasure WHERE tenant_id=$1::uuid AND operation_id=$2::uuid FOR UPDATE")
            .bind(tenant.to_string()).bind(operation.to_string()).fetch_one(&mut *tx).await.map_err(storage)?;
        if completed {
            tx.commit().await.map_err(storage)?;
            return Ok(EraseBatch {
                deleted: 0,
                complete: true,
            });
        }
        let scope = format!(
            "\"{}\"=$1::uuid AND \"{}\"=$2 AND \"{}\"<=(SELECT started_at FROM forklaunch_subject_erasure WHERE tenant_id=$1::uuid AND operation_id=$3::uuid)",
            c.tenant, c.subject, c.created_at
        );
        let delete = format!(
            "WITH selected AS (SELECT \"{id}\" FROM \"{table}\" WHERE {scope} AND NOT \"{hold}\" ORDER BY \"{created}\",\"{id}\" LIMIT $4) DELETE FROM \"{table}\" target USING selected WHERE target.\"{tenant}\"=$1::uuid AND target.\"{id}\"=selected.\"{id}\" AND target.\"{subject}\"=$2 AND NOT target.\"{hold}\" AND target.\"{created}\"<=(SELECT started_at FROM forklaunch_subject_erasure WHERE tenant_id=$1::uuid AND operation_id=$3::uuid)",
            id = c.id,
            table = c.table,
            hold = c.hold,
            created = c.created_at,
            tenant = c.tenant,
            subject = c.subject
        );
        let deleted = sqlx::query(validated_identifiers_sql(&delete))
            .bind(tenant.to_string())
            .bind(request.subject)
            .bind(operation.to_string())
            .bind(i64::try_from(request.batch_size).map_err(|_| error("invalid_subject_scope"))?)
            .execute(&mut *tx)
            .await
            .map_err(storage)?
            .rows_affected();
        let remaining = format!("SELECT EXISTS(SELECT 1 FROM \"{}\" WHERE {scope})", c.table);
        let pending: bool = sqlx::query_scalar(validated_identifiers_sql(&remaining))
            .bind(tenant.to_string())
            .bind(request.subject)
            .bind(operation.to_string())
            .fetch_one(&mut *tx)
            .await
            .map_err(storage)?;
        if pending && deleted == 0 {
            return Err(error("subject_erasure_held"));
        }
        sqlx::query("UPDATE forklaunch_subject_erasure SET completed=$3,deleted_count=deleted_count+$4 WHERE tenant_id=$1::uuid AND operation_id=$2::uuid")
            .bind(tenant.to_string()).bind(operation.to_string()).bind(!pending).bind(i64::try_from(deleted).map_err(|_|error("subject_storage_failed"))?).execute(&mut *tx).await.map_err(storage)?;
        tx.commit().await.map_err(storage)?;
        Ok(EraseBatch {
            deleted: usize::try_from(deleted).map_err(|_| error("subject_storage_failed"))?,
            complete: !pending,
        })
    }
}
impl SubjectEraseStore for PostgresSubjectEraseStore {
    async fn erase_batch(&self, request: &SubjectRequest<'_>) -> FrameworkResult<EraseBatch> {
        self.apply(request).await
    }
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}
fn storage(_: sqlx::Error) -> FrameworkError {
    error("subject_storage_failed")
}
fn error(code: &str) -> FrameworkError {
    FrameworkError::new(code, "Subject operation did not complete")
}

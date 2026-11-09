use super::{ApplyOutcome, PreparedMigration};
use crate::common::{FrameworkError, FrameworkResult};
use sqlx::{PgPool, Row};

/// Existing storage columns. Tenant/resource/checkpoint columns are UUID; the
/// encrypted value/index columns are text. This adapter does not create schema.
/// Writers must preserve completed checkpoints after switching to the new policy.
pub struct MigrationColumns {
    pub table: String,
    pub tenant: String,
    pub resource: String,
    pub ciphertext: String,
    pub blind_index: Option<String>,
    pub operation: String,
}
/// Bounded row-level migration adapter. Application roles must enforce RLS; each
/// transaction binds `forklaunch.tenant_id` in addition to explicit WHERE scope.
pub struct PostgresMigrationStore {
    pool: PgPool,
    columns: MigrationColumns,
}
impl PostgresMigrationStore {
    /// # Errors
    /// Rejects unsafe SQL identifiers and overlapping storage columns.
    pub fn new(pool: PgPool, columns: MigrationColumns) -> FrameworkResult<Self> {
        let names = [
            &columns.tenant,
            &columns.resource,
            &columns.ciphertext,
            &columns.operation,
        ];
        let mut unique = std::collections::BTreeSet::new();
        let valid = |name: &str| {
            !name.is_empty()
                && name.len() <= 63
                && name.bytes().enumerate().all(|(i, c)| {
                    c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
                })
        };
        if !valid(&columns.table)
            || names
                .iter()
                .any(|name| !valid(name) || !unique.insert(name.as_str()))
            || columns
                .blind_index
                .as_ref()
                .is_some_and(|name| !valid(name) || !unique.insert(name.as_str()))
        {
            return Err(error("invalid_migration_columns"));
        }
        Ok(Self { pool, columns })
    }
    /// Atomically compare the original ciphertext, replace its value/index, and
    /// store the stable operation ID. Retry after lost acknowledgment is safe.
    /// No batch-wide transaction or unbounded scan is performed by this adapter.
    /// # Errors
    /// Returns a sanitized error on storage failure or missing index-column mapping.
    pub async fn apply(&self, plan: &PreparedMigration) -> FrameworkResult<ApplyOutcome> {
        if plan.replacement().blind_index().is_some() && self.columns.blind_index.is_none() {
            return Err(error("missing_migration_index_column"));
        }
        let c = &self.columns;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| error("migration_storage_failed"))?;
        sqlx::query("SELECT set_config('forklaunch.tenant_id',$1,true),set_config('statement_timeout','5000',true),set_config('lock_timeout','2000',true),set_config('synchronous_commit','on',true)")
            .bind(plan.context.tenant.to_string()).execute(&mut *tx).await.map_err(|_|error("migration_storage_failed"))?;
        let select = format!(
            "SELECT \"{}\" AS ciphertext,\"{}\"::text AS operation FROM \"{}\" WHERE \"{}\"=$1::uuid AND \"{}\"=$2::uuid FOR UPDATE",
            c.ciphertext, c.operation, c.table, c.tenant, c.resource
        );
        // Identifiers were validated by `new`; data stays bound.
        let row = sqlx::query(sqlx::AssertSqlSafe(select))
            .bind(plan.context.tenant.to_string())
            .bind(plan.context.resource.to_string())
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| error("migration_storage_failed"))?;
        let Some(row) = row else {
            return Ok(ApplyOutcome::Conflict);
        };
        let operation: Option<String> = row
            .try_get("operation")
            .map_err(|_| error("migration_storage_failed"))?;
        if operation.as_deref() == Some(plan.operation_id.to_string().as_str()) {
            tx.commit()
                .await
                .map_err(|_| error("migration_storage_failed"))?;
            return Ok(ApplyOutcome::AlreadyApplied);
        }
        let current: String = row
            .try_get("ciphertext")
            .map_err(|_| error("migration_storage_failed"))?;
        if current != plan.expected {
            return Ok(ApplyOutcome::Conflict);
        }
        let index = c
            .blind_index
            .as_ref()
            .map_or_else(String::new, |name| format!(",\"{name}\"=$6"));
        let update = format!(
            "UPDATE \"{}\" SET \"{}\"=$3,\"{}\"=$4::uuid{} WHERE \"{}\"=$1::uuid AND \"{}\"=$2::uuid AND \"{}\"=$5",
            c.table, c.ciphertext, c.operation, index, c.tenant, c.resource, c.ciphertext
        );
        let query = sqlx::query(sqlx::AssertSqlSafe(update))
            .bind(plan.context.tenant.to_string())
            .bind(plan.context.resource.to_string())
            .bind(plan.replacement.ciphertext())
            .bind(plan.operation_id.to_string())
            .bind(&plan.expected);
        let changed = if c.blind_index.is_some() {
            query
                .bind(plan.replacement.blind_index())
                .execute(&mut *tx)
                .await
        } else {
            query.execute(&mut *tx).await
        }
        .map_err(|_| error("migration_storage_failed"))?;
        if changed.rows_affected() != 1 {
            return Err(error("migration_compare_failed"));
        }
        tx.commit()
            .await
            .map_err(|_| error("migration_storage_failed"))?;
        Ok(ApplyOutcome::Applied)
    }
}
fn error(code: &str) -> FrameworkError {
    FrameworkError::new(
        code,
        "Protected migration storage operation could not be completed",
    )
}

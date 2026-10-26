use super::{RetentionAction, RetentionRequest, RetentionStore};
use crate::{
    common::{FrameworkError, FrameworkResult},
    persistence::compliance::ComplianceMetadata,
};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
/// All names are unqualified SQL identifiers. Identity must be unique per tenant;
/// timestamps are timestamptz and classified nullable fields must accept NULL.
pub struct RetentionColumns {
    pub table: String,
    pub tenant: String,
    pub id: String,
    pub created_at: String,
    pub anonymized_at: String,
}
/// Acknowledged audit intent before each mutation, never an assertion of success.
/// Implementations must bind an authorized service actor and durable audit sink.
pub trait RetentionAudit: Sync + Send {
    fn intent<'a>(
        &'a self,
        request: RetentionRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>>;
}
pub struct PostgresRetentionStore {
    pool: sqlx::PgPool,
    columns: RetentionColumns,
    entity: &'static str,
    nullable: Vec<&'static str>,
    audit: Arc<dyn RetentionAudit>,
}
impl PostgresRetentionStore {
    /// # Errors
    /// Rejects unsafe identifiers, overlapping control columns or classification
    /// that could null a tenant, identity, timestamp or checkpoint column.
    pub fn new<T: ComplianceMetadata>(
        pool: sqlx::PgPool,
        columns: RetentionColumns,
        audit: Arc<dyn RetentionAudit>,
    ) -> FrameworkResult<Self> {
        let valid = |s: &str| {
            !s.is_empty()
                && s.len() <= 63
                && s.bytes().enumerate().all(|(i, b)| {
                    b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())
                })
        };
        let names = [
            &columns.tenant,
            &columns.id,
            &columns.created_at,
            &columns.anonymized_at,
        ];
        let mut unique = std::collections::BTreeSet::new();
        let nullable: Vec<_> = T::FIELDS
            .iter()
            .filter(|f| f.can_anonymize())
            .map(|f| f.storage_name)
            .collect();
        if !valid(&columns.table)
            || names
                .iter()
                .any(|s| !valid(s) || !unique.insert(s.as_str()))
            || nullable.iter().any(|s| !valid(s) || unique.contains(s))
        {
            return Err(error());
        }
        Ok(Self {
            pool,
            columns,
            entity: T::ENTITY,
            nullable,
            audit,
        })
    }
    fn validate(&self, r: RetentionRequest<'_>) -> FrameworkResult<()> {
        if r.policy.entity() != self.entity
            || r.policy.anonymize_fields() != self.nullable
            || r.policy.cutoff() > r.now
            || !(1..=10_000).contains(&r.batch_size)
        {
            return Err(error());
        }
        Ok(())
    }
    fn predicate(&self, r: RetentionRequest<'_>) -> String {
        let c = &self.columns;
        let extra = if r.policy.action() == RetentionAction::Anonymize {
            format!(" AND \"{}\" IS NULL", c.anonymized_at)
        } else {
            String::new()
        };
        format!(
            "\"{}\"::text=$1 AND \"{}\"<to_timestamp($2){extra}",
            c.tenant, c.created_at
        )
    }
    async fn transaction(
        &self,
        r: RetentionRequest<'_>,
    ) -> FrameworkResult<sqlx::Transaction<'_, sqlx::Postgres>> {
        let mut tx = begin_tenant_transaction(&self.pool, r.tenant).await?;
        sqlx::query("SELECT set_config('statement_timeout','5000',true),set_config('lock_timeout','2000',true),set_config('synchronous_commit','on',true)").execute(&mut *tx).await.map_err(|_|error())?;
        Ok(tx)
    }
}
impl RetentionStore for PostgresRetentionStore {
    async fn count(&self, r: RetentionRequest<'_>) -> FrameworkResult<u64> {
        self.validate(r)?;
        let mut tx = self.transaction(r).await?;
        let sql = format!(
            "SELECT count(*) FROM \"{}\" WHERE {}",
            self.columns.table,
            self.predicate(r)
        );
        let count: i64 = sqlx::query_scalar(validated_identifiers_sql(&sql))
            .bind(r.tenant.id())
            .bind(epoch(r.policy.cutoff())?)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| error())?;
        tx.commit().await.map_err(|_| error())?;
        u64::try_from(count).map_err(|_| error())
    }
    async fn apply_batch(&self, r: RetentionRequest<'_>) -> FrameworkResult<usize> {
        self.validate(r)?;
        let cutoff = epoch(r.policy.cutoff())?;
        let now = epoch(r.now)?;
        self.audit.intent(r).await.map_err(|_| error())?;
        let mut tx = self.transaction(r).await?;
        let c = &self.columns;
        let selected = format!(
            "WITH selected AS (SELECT \"{}\" FROM \"{}\" WHERE {} ORDER BY \"{}\",\"{}\" LIMIT $3 FOR UPDATE)",
            c.id,
            c.table,
            self.predicate(r),
            c.created_at,
            c.id
        );
        let sql = match r.policy.action() {
            RetentionAction::Delete => format!(
                "{selected} DELETE FROM \"{}\" AS target USING selected WHERE target.\"{}\"=selected.\"{}\" AND target.\"{}\"::text=$1",
                c.table, c.id, c.id, c.tenant
            ),
            RetentionAction::Anonymize => {
                let mut fields = String::new();
                for name in &self.nullable {
                    fields.push_str(",\"");
                    fields.push_str(name);
                    fields.push_str("\"=NULL");
                }
                format!(
                    "{selected} UPDATE \"{}\" AS target SET \"{}\"=to_timestamp($4){fields} FROM selected WHERE target.\"{}\"=selected.\"{}\" AND target.\"{}\"::text=$1",
                    c.table, c.anonymized_at, c.id, c.id, c.tenant
                )
            }
        };
        let query = sqlx::query(validated_identifiers_sql(&sql))
            .bind(r.tenant.id())
            .bind(cutoff)
            .bind(i64::try_from(r.batch_size).map_err(|_| error())?);
        let changed = match r.policy.action() {
            RetentionAction::Delete => query.execute(&mut *tx).await,
            RetentionAction::Anonymize => query.bind(now).execute(&mut *tx).await,
        }
        .map_err(|_| error())?;
        tx.commit().await.map_err(|_| error())?;
        usize::try_from(changed.rows_affected()).map_err(|_| error())
    }
}
fn epoch(time: SystemTime) -> FrameworkResult<f64> {
    Ok(time
        .duration_since(UNIX_EPOCH)
        .map_err(|_| error())?
        .as_secs_f64())
}
fn error() -> FrameworkError {
    FrameworkError::new(
        "retention_storage_failed",
        "Retention storage operation could not be completed",
    )
}

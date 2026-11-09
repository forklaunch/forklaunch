//! Explicit database-only subject erasure. External objects, caches, queues and
//! backups require separate adapters/reconciliation; this adapter never certifies
//! their erasure. Retain scope authentication keys while operations remain
//! retryable; removing a key fails closed. Runtime roles require tenant RLS and
//! neither SUPERUSER nor BYPASSRLS.
use crate::persistence::begin_tenant_transaction;
include!("postgres_shared.rs");

// Only constructor-validated, quoted SQL identifiers are interpolated; data stays bound.
fn validated_identifiers_sql(sql: &str) -> sqlx::AssertSqlSafe<&str> {
    sqlx::AssertSqlSafe(sql)
}

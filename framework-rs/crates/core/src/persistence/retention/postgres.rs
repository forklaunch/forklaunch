//! Opt-in SQL adapter for application-owned retention tables and policies.
use crate::persistence::begin_tenant_transaction;
include!("postgres_shared.rs");

// Only constructor-validated, quoted SQL identifiers are interpolated; data stays bound.
fn validated_identifiers_sql(sql: &str) -> sqlx::AssertSqlSafe<&str> {
    sqlx::AssertSqlSafe(sql)
}

//! Tenant context and transaction-scoped `PostgreSQL` isolation helpers.
pub mod compliance;
#[cfg(feature = "encryption")]
#[path = "field_encryptor.rs"]
pub mod encryption;
#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "postgres09")]
pub mod postgres09;
#[cfg(feature = "encryption")]
pub mod protected;
pub mod retention;
pub mod subject;
#[cfg(feature = "postgres")]
pub use postgres::{
    PostgresConnectionPolicy, clone_runtime_postgres_pool, connect_migration_postgres,
    connect_runtime_postgres,
};

#[cfg(feature = "encryption")]
pub mod json;

use crate::common::{FrameworkError, FrameworkResult};

/// Validated tenant identity bound to one request/unit of work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantContext(String);

impl TenantContext {
    /// # Errors
    /// Rejects empty, oversized, or control-character tenant identifiers.
    pub fn new(tenant_id: impl Into<String>) -> FrameworkResult<Self> {
        let tenant_id = tenant_id.into();
        if tenant_id.trim().is_empty()
            || tenant_id.len() > 256
            || tenant_id.chars().any(char::is_control)
        {
            return Err(FrameworkError::new(
                "invalid_tenant",
                "Invalid tenant identity",
            ));
        }
        Ok(Self(tenant_id))
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.0
    }
}

/// RLS DDL for a table with a `tenant_id` column. Run once in a migration using
/// an owner role; application roles must have neither SUPERUSER nor BYPASSRLS.
///
/// # Errors
/// Rejects identifiers that cannot safely be interpolated into SQL DDL.
pub fn postgres_tenant_rls_sql(table: &str) -> FrameworkResult<String> {
    if table.is_empty()
        || table.len() > 63
        || !table.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
    {
        return Err(FrameworkError::new(
            "invalid_table",
            "Expected an unqualified SQL table identifier",
        ));
    }
    Ok(format!(
        "ALTER TABLE \"{table}\" ENABLE ROW LEVEL SECURITY; ALTER TABLE \"{table}\" FORCE ROW LEVEL SECURITY; CREATE POLICY forklaunch_tenant_policy ON \"{table}\" USING (tenant_id = current_setting('forklaunch.tenant_id', true)) WITH CHECK (tenant_id = current_setting('forklaunch.tenant_id', true));"
    ))
}

/// Begins a transaction and sets tenant context with transaction-local scope.
/// Pool reuse cannot carry this context into the next transaction.
///
/// # Errors
/// Returns a safe error if `PostgreSQL` cannot create/configure the transaction.
#[cfg(feature = "postgres")]
pub async fn begin_tenant_transaction<'a>(
    pool: &'a sqlx::PgPool,
    tenant: &TenantContext,
) -> FrameworkResult<sqlx::Transaction<'a, sqlx::Postgres>> {
    postgres::begin_tenant_transaction(pool, tenant).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tenant_context_and_rls_identifiers_are_validated() {
        for tenant in ["", " ", "tenant\nother"] {
            assert!(TenantContext::new(tenant).is_err());
        }
        assert_eq!(TenantContext::new("tenant-1").unwrap().id(), "tenant-1");
        for table in [
            "",
            "1table",
            "schema.table",
            "users; DROP TABLE users",
            "users\"",
            "users--",
        ] {
            assert!(postgres_tenant_rls_sql(table).is_err());
        }
        let sql = postgres_tenant_rls_sql("iam_users").unwrap();
        assert!(sql.contains("FORCE ROW LEVEL SECURITY"));
        assert!(sql.contains("WITH CHECK"));
    }
}

//! Connection policy and trusted tenant configuration; never derives identity from request input.
use crate::error::{AppError, Result};
use forklaunch_core::{config, persistence::{PostgresConnectionPolicy, connect_migration_postgres}};
use uuid::Uuid;

pub struct DatabaseConfig {
    pub url: String,
    pub tenant: Uuid,
    pub policy: PostgresConnectionPolicy,
}
fn policy() -> Result<PostgresConnectionPolicy> {
    let allow: String = config::required("DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT")?;
    let allow = match allow.as_str() {
        "false" => false,
        "true" => true,
        _ => return Err(AppError::InvalidConfig("DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT must be true or false".into())),
    };
    Ok(PostgresConnectionPolicy { allow_plaintext_development: allow, max_connections: 8 })
}
impl DatabaseConfig {
    pub fn from_env() -> Result<Self> {
        let url = config::required("DATABASE_URL")?;
        let tenant: Uuid = config::required("TENANT_ID")?;
        if tenant.is_nil() { return Err(AppError::InvalidConfig("TENANT_ID must be a trusted non-nil UUID".into())); }
        Ok(Self { url, tenant, policy: policy()? })
    }
}

pub async fn migrate() -> Result<()> {
    let url: String = config::required("MIGRATION_DATABASE_URL")?;
    let pool = connect_migration_postgres(&url, policy()?).await.map_err(|_| AppError::Storage("Migration connection rejected".into()))?;
    let result = sqlx::migrate!("./migrations").run(&pool).await;
    pool.close().await;
    result.map_err(|_| AppError::Storage("Service migration failed".into()))
}

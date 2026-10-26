//! SQL remains service-owned. Framework policies enforce tenant, encryption and audited disclosure boundaries.
use std::sync::Arc;
use uuid::Uuid;
use forklaunch_core::config;
use forklaunch_core::encryption::{EnvelopeError, EnvelopeKey, EnvelopeProfile};
use forklaunch_core::persistence::{TenantContext, begin_tenant_transaction, connect_runtime_postgres,
    protected::{ProtectedContext, ProtectedField}};
use crate::{error::{AppError, Result}, persistence::{database::DatabaseConfig, compliance_schema::StoredRecord}};
use sqlx::PgPool;

#[derive(Clone)]
pub struct PostgresRepository {
    pool: PgPool,
    tenant: Uuid,
    context: TenantContext,
    key: Arc<EnvelopeKey>,
}
fn unavailable() -> AppError { AppError::Storage("Protected record storage unavailable".into()) }
impl PostgresRepository {
    pub async fn connect(config: DatabaseConfig) -> Result<Self> {
        let namespace: String = config::required("STORAGE_ENCRYPTION_NAMESPACE")?;
        let profile = EnvelopeProfile::new(&namespace).map_err(|_| unavailable())?;
        let key = EnvelopeKey::from_env(profile).map_err(|error| match error {
            EnvelopeError::Configuration(error) => error.into(),
            _ => unavailable(),
        })?;
        let context = TenantContext::new(config.tenant.to_string()).map_err(|_| unavailable())?;
        let pool = connect_runtime_postgres(&config.url, config.policy).await.map_err(|_| unavailable())?;
        let repository = Self { pool, tenant: config.tenant, context, key: Arc::new(key) };
        // Migration ownership is explicit; startup never silently creates storage.
        if repository.count().await.is_err() { repository.close().await; return Err(unavailable()); }
        Ok(repository)
    }
    pub async fn insert(&self, id: Uuid, name: &str) -> Result<()> {
        let protected = ProtectedField::seal(&self.key, ProtectedContext::new(self.tenant, id).map_err(|_| unavailable())?, StoredRecord::protected_name_ciphertext().map_err(|_| unavailable())?, name.as_bytes()).map_err(|_| unavailable())?;
        let row = StoredRecord { id, tenant_id: self.context.id().to_owned(), name_ciphertext: protected };
        row.validate().map_err(|_| unavailable())?;
        let mut transaction = begin_tenant_transaction(&self.pool, &self.context).await.map_err(|_| unavailable())?;
        sqlx::query("INSERT INTO records (tenant_id,id,name_ciphertext) VALUES ($1,$2,$3)")
            .bind(&row.tenant_id).bind(row.id).bind(row.name_ciphertext.ciphertext()).execute(&mut *transaction).await.map_err(|_| unavailable())?;
        transaction.commit().await.map_err(|_| unavailable())
    }
    /// Returns an encrypted member. Plaintext access requires the framework's explicit acknowledged audit API.
    pub async fn protected_name(&self, id: Uuid) -> Result<Option<ProtectedField>> {
        let mut transaction = begin_tenant_transaction(&self.pool, &self.context).await.map_err(|_| unavailable())?;
        let row: Option<(Uuid, String, String)> = sqlx::query_as("SELECT id,tenant_id,name_ciphertext FROM records WHERE tenant_id=$1 AND id=$2")
            .bind(self.context.id()).bind(id).fetch_optional(&mut *transaction).await.map_err(|_| unavailable())?;
        transaction.commit().await.map_err(|_| unavailable())?;
        row.map(|(id, tenant_id, ciphertext)| {
            let row = StoredRecord { id, tenant_id, name_ciphertext: ProtectedField::from_ciphertext(
                StoredRecord::protected_name_ciphertext().map_err(|_| unavailable())?, ciphertext,
            ).map_err(|_| unavailable())? };
            row.validate().map_err(|_| unavailable())?;
            Ok(row.name_ciphertext)
        }).transpose()
    }
    pub async fn count(&self) -> Result<usize> {
        let mut transaction = begin_tenant_transaction(&self.pool, &self.context).await.map_err(|_| unavailable())?;
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM records WHERE tenant_id=$1")
            .bind(self.context.id()).fetch_one(&mut *transaction).await.map_err(|_| unavailable())?;
        transaction.commit().await.map_err(|_| unavailable())?;
        usize::try_from(count).map_err(|_| unavailable())
    }
    pub async fn close(&self) { self.pool.close().await; }
}

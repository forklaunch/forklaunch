//! Encrypted durable browser sessions with tenant RLS and hashed cookie IDs.
#[cfg(feature = "oidc-sessions")]
mod oidc;

use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::{
    TenantContext, begin_tenant_transaction, encryption::FieldEncryptor,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{fmt, sync::Arc};
use tower_sessions::{
    SessionStore,
    session::{Id, Record},
    session_store,
};

const MAX_RECORD_BYTES: usize = 64 * 1024;

/// Shared `PostgreSQL` cookie store. IAM migrations must be applied by a schema
/// owner, and the runtime role needs CRUD on `fl_iam_browser_sessions`.
/// Use a stable server-owned namespace per application/cookie policy.
#[derive(Clone)]
pub struct PostgresCookieStore {
    pool: PgPool,
    tenant: TenantContext,
    namespace: String,
    encryptor: Arc<FieldEncryptor>,
}

impl fmt::Debug for PostgresCookieStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresCookieStore")
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    namespace: String,
    record: Record,
}

fn failed() -> session_store::Error {
    session_store::Error::Backend("IAM browser session storage unavailable".to_owned())
}

fn id_hash(id: &Id) -> String {
    base16ct::lower::encode_string(&Sha256::digest(id.to_string().as_bytes()))
}

impl PostgresCookieStore {
    /// Uses the existing IAM migrations and field-encryption key ring. Retained
    /// decryption keys allow key rotation without discarding active sessions.
    ///
    /// # Errors
    /// Rejects empty/oversized/control-character namespaces and privileged roles.
    /// Database errors are sanitized and never contain credentials or records.
    pub async fn new(
        pool: PgPool,
        tenant: TenantContext,
        namespace: impl Into<String>,
        encryptor: Arc<FieldEncryptor>,
    ) -> BlueprintResult<Self> {
        let namespace = namespace.into();
        if namespace.trim().is_empty()
            || namespace.len() > 128
            || namespace.chars().any(char::is_control)
        {
            return Err(BlueprintError::new(
                "iam_cookie_namespace_invalid",
                "Invalid browser session namespace",
            ));
        }
        let bypass: bool = sqlx::query_scalar(
            "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
        )
        .fetch_one(&pool)
        .await
        .map_err(|_| {
            BlueprintError::new(
                "iam_cookie_store_failed",
                "IAM browser session storage unavailable",
            )
        })?;
        if bypass {
            return Err(BlueprintError::new(
                "iam_unsafe_database_role",
                "IAM runtime requires a role without SUPERUSER or BYPASSRLS",
            ));
        }
        Ok(Self {
            pool,
            tenant,
            namespace,
            encryptor,
        })
    }

    async fn transaction(&self) -> session_store::Result<Transaction<'_, Postgres>> {
        begin_tenant_transaction(&self.pool, &self.tenant)
            .await
            .map_err(|_| failed())
    }

    fn encode(&self, record: &Record) -> session_store::Result<String> {
        let text = serde_json::to_string(&Envelope {
            namespace: self.namespace.clone(),
            record: record.clone(),
        })
        .map_err(|_| failed())?;
        if text.len() > MAX_RECORD_BYTES {
            return Err(failed());
        }
        self.encryptor
            .seal(&text, self.tenant.id(), "[IAM browser session]")
            .map_err(|_| failed())
    }

    fn decode(&self, id: &Id, encrypted: &str, expires_at: i64) -> session_store::Result<Record> {
        if encrypted.len() > MAX_RECORD_BYTES * 4 {
            return Err(failed());
        }
        let plaintext = self
            .encryptor
            .decrypt(encrypted, self.tenant.id())
            .map_err(|_| failed())?;
        if plaintext.len() > MAX_RECORD_BYTES {
            return Err(failed());
        }
        let envelope: Envelope = serde_json::from_str(&plaintext).map_err(|_| failed())?;
        if envelope.namespace != self.namespace
            || envelope.record.id != *id
            || envelope.record.expiry_date.unix_timestamp() != expires_at
        {
            return Err(failed());
        }
        Ok(envelope.record)
    }

    /// Deletes expired cookie records only in this tenant and namespace.
    /// Applications may invoke this from their scheduled maintenance path.
    ///
    /// # Errors
    /// Returns a sanitized store error when persistence fails.
    pub async fn purge_expired(&self) -> session_store::Result<u64> {
        let mut tx = self.transaction().await?;
        let count = sqlx::query("DELETE FROM fl_iam_browser_sessions WHERE tenant_id=$1 AND namespace=$2 AND expires_at<=floor(extract(epoch FROM clock_timestamp()))::BIGINT")
            .bind(self.tenant.id()).bind(&self.namespace).execute(&mut *tx).await.map_err(|_| failed())?.rows_affected();
        tx.commit().await.map_err(|_| failed())?;
        Ok(count)
    }
}

#[async_trait::async_trait]
impl SessionStore for PostgresCookieStore {
    async fn create(&self, record: &mut Record) -> session_store::Result<()> {
        let mut tx = self.transaction().await?;
        for _ in 0..8 {
            let encrypted = self.encode(record)?;
            let inserted = sqlx::query("INSERT INTO fl_iam_browser_sessions (tenant_id,namespace,id_hash,encrypted_record,expires_at) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
                .bind(self.tenant.id()).bind(&self.namespace).bind(id_hash(&record.id)).bind(encrypted).bind(record.expiry_date.unix_timestamp())
                .execute(&mut *tx).await.map_err(|_| failed())?.rows_affected();
            if inserted == 1 {
                tx.commit().await.map_err(|_| failed())?;
                return Ok(());
            }
            record.id = Id::default();
        }
        Err(failed())
    }

    async fn save(&self, record: &Record) -> session_store::Result<()> {
        let encrypted = self.encode(record)?;
        let mut tx = self.transaction().await?;
        // Updates cannot resurrect deleted or expired cookies, including an
        // in-flight request racing with logout or cookie-ID rotation.
        let updated = sqlx::query("UPDATE fl_iam_browser_sessions SET encrypted_record=$4,expires_at=$5 WHERE tenant_id=$1 AND namespace=$2 AND id_hash=$3 AND expires_at>floor(extract(epoch FROM clock_timestamp()))::BIGINT")
            .bind(self.tenant.id()).bind(&self.namespace).bind(id_hash(&record.id)).bind(encrypted).bind(record.expiry_date.unix_timestamp())
            .execute(&mut *tx).await.map_err(|_| failed())?.rows_affected();
        if updated != 1 {
            return Err(failed());
        }
        tx.commit().await.map_err(|_| failed())
    }

    async fn load(&self, id: &Id) -> session_store::Result<Option<Record>> {
        let mut tx = self.transaction().await?;
        let row: Option<(String,i64)> = sqlx::query_as("SELECT encrypted_record,expires_at FROM fl_iam_browser_sessions WHERE tenant_id=$1 AND namespace=$2 AND id_hash=$3 AND expires_at>floor(extract(epoch FROM clock_timestamp()))::BIGINT")
            .bind(self.tenant.id()).bind(&self.namespace).bind(id_hash(id)).fetch_optional(&mut *tx).await.map_err(|_| failed())?;
        let value = row
            .map(|(encrypted, expiry)| self.decode(id, &encrypted, expiry))
            .transpose()?;
        tx.commit().await.map_err(|_| failed())?;
        Ok(value)
    }

    async fn delete(&self, id: &Id) -> session_store::Result<()> {
        let mut tx = self.transaction().await?;
        sqlx::query("DELETE FROM fl_iam_browser_sessions WHERE tenant_id=$1 AND namespace=$2 AND id_hash=$3")
            .bind(self.tenant.id()).bind(&self.namespace).bind(id_hash(id)).execute(&mut *tx).await.map_err(|_| failed())?;
        tx.commit().await.map_err(|_| failed())
    }
}

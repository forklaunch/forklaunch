//! Encrypted, audited directory persistence with tenant RLS and atomic relationships.
use super::{
    DirectoryKind, DirectoryStore, DirectoryTransaction, conflict, invalid, not_found, record_id,
    storage_error,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::{
    TenantContext, begin_tenant_transaction, encryption::FieldEncryptor,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;

mod authorization;
mod identities;
pub use authorization::DirectoryAuthorization;

const SENSITIVE: &[&str] = &[
    "email",
    "firstName",
    "lastName",
    "phoneNumber",
    "name",
    "domain",
    "slug",
    "subscription",
    "logoUrl",
    "providerFields",
];

#[derive(Clone)]
pub struct PostgresDirectoryStore {
    pool: PgPool,
    tenant: TenantContext,
    encryptor: Arc<FieldEncryptor>,
    audit: AuditContext,
}
impl PostgresDirectoryStore {
    /// Run versioned IAM migrations with a schema-owner connection.
    /// # Errors
    /// Returns sanitized database failures.
    pub async fn migrate(pool: &PgPool) -> BlueprintResult<()> {
        crate::postgres::PostgresIamService::migrate(pool).await
    }
    /// # Errors
    /// Rejects RLS-bypassing roles and missing audit correlation IDs.
    pub async fn new(
        pool: PgPool,
        tenant: TenantContext,
        encryptor: Arc<FieldEncryptor>,
        audit: AuditContext,
    ) -> BlueprintResult<Self> {
        validate_audit(&audit)?;
        let bypass: bool = sqlx::query_scalar(
            "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
        )
        .fetch_one(&pool)
        .await
        .map_err(db_error)?;
        if bypass {
            return Err(BlueprintError::new(
                "iam_unsafe_database_role",
                "IAM runtime requires a role without SUPERUSER or BYPASSRLS",
            ));
        }
        Ok(Self {
            pool,
            tenant,
            encryptor,
            audit,
        })
    }
    /// # Errors
    /// Rejects missing correlation IDs.
    pub fn with_audit_context(&self, audit: AuditContext) -> BlueprintResult<Self> {
        validate_audit(&audit)?;
        Ok(Self {
            audit,
            ..self.clone()
        })
    }
    fn encode(&self, kind: DirectoryKind, mut row: Value) -> BlueprintResult<Value> {
        let id = record_id(&row)?.to_owned();
        for field in SENSITIVE {
            if let Some(value) = row.get(*field) {
                let envelope = json!({"kind":kind.as_str(),"id":id,"field":field,"value":value});
                row[*field] = json!(
                    self.encryptor
                        .seal(
                            &envelope.to_string(),
                            self.tenant.id(),
                            "[IAM directory data]"
                        )
                        .map_err(|_| storage_error())?
                );
            }
        }
        Ok(row)
    }
    fn decode(&self, kind: DirectoryKind, id: &str, mut row: Value) -> BlueprintResult<Value> {
        if record_id(&row).map_err(|_| storage_error())? != id {
            return Err(storage_error());
        }
        for field in SENSITIVE {
            if let Some(value) = row.get(*field) {
                let plaintext = self
                    .encryptor
                    .decrypt(value.as_str().ok_or_else(storage_error)?, self.tenant.id())
                    .map_err(|_| storage_error())?;
                let envelope: Value =
                    serde_json::from_str(&plaintext).map_err(|_| storage_error())?;
                if envelope["kind"] != kind.as_str()
                    || envelope["id"] != id
                    || envelope["field"] != *field
                    || envelope.get("value").is_none()
                {
                    return Err(storage_error());
                }
                row[*field] = envelope["value"].clone();
            }
        }
        Ok(row)
    }
}
pub struct PostgresDirectoryTransaction<'a> {
    store: &'a PostgresDirectoryStore,
    tx: Transaction<'a, Postgres>,
    write: bool,
    audit: AuditContext,
}
impl DirectoryStore for PostgresDirectoryStore {
    type Transaction<'a> = PostgresDirectoryTransaction<'a>;
    async fn begin(&self, write: bool) -> BlueprintResult<Self::Transaction<'_>> {
        let mut tx = begin_tenant_transaction(&self.pool, &self.tenant)
            .await
            .map_err(|_| storage_error())?;
        // Shared read locks preserve a consistent hydrated graph; an exclusive
        // write lock serializes relationship changes across service replicas.
        let query = if write {
            "SELECT pg_advisory_xact_lock(hashtextextended($1,0))"
        } else {
            "SELECT pg_advisory_xact_lock_shared(hashtextextended($1,0))"
        };
        sqlx::query(query)
            .bind(format!("forklaunch:iam-directory:{}", self.tenant.id()))
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        Ok(PostgresDirectoryTransaction {
            store: self,
            tx,
            write,
            audit: self.audit.clone(),
        })
    }
}
impl PostgresDirectoryTransaction<'_> {
    async fn audit(
        &mut self,
        kind: DirectoryKind,
        id: &str,
        operation: &str,
    ) -> BlueprintResult<()> {
        sqlx::query("INSERT INTO fl_iam_directory_audit(id,tenant_id,kind,record_id,actor_id,correlation_id,operation) VALUES($1::uuid,$2,$3,$4,$5,$6,$7)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(self.store.tenant.id()).bind(kind.as_str()).bind(id).bind(&self.audit.actor_id).bind(&self.audit.correlation_id).bind(operation).execute(&mut *self.tx).await.map_err(db_error)?;
        Ok(())
    }
    async fn claims(&mut self, kind: DirectoryKind, row: &Value) -> BlueprintResult<()> {
        let id = record_id(row)?;
        sqlx::query(
            "DELETE FROM fl_iam_directory_unique_claims WHERE tenant_id=$1 AND kind=$2 AND id=$3",
        )
        .bind(self.store.tenant.id())
        .bind(kind.as_str())
        .bind(id)
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?;
        for field in match kind {
            DirectoryKind::User => &["email", "subscription"][..],
            DirectoryKind::Organization => &["subscription"][..],
            _ => &[],
        } {
            if let Some(value) = row.get(*field).filter(|value| !value.is_null()) {
                let value = value.as_str().ok_or_else(invalid)?;
                for index in self
                    .store
                    .encryptor
                    .blind_indexes(
                        &json!([kind.as_str(), field, value]).to_string(),
                        self.store.tenant.id(),
                    )
                    .map_err(|_| storage_error())?
                {
                    sqlx::query("INSERT INTO fl_iam_directory_unique_claims(tenant_id,kind,id,field,blind_index) VALUES($1,$2,$3,$4,$5)").bind(self.store.tenant.id()).bind(kind.as_str()).bind(id).bind(field).bind(index).execute(&mut *self.tx).await.map_err(db_error)?;
                }
            }
        }
        Ok(())
    }
    fn require_write(&self) -> BlueprintResult<()> {
        if self.write { Ok(()) } else { Err(invalid()) }
    }
}
impl DirectoryTransaction for PostgresDirectoryTransaction<'_> {
    async fn get(&mut self, kind: DirectoryKind, id: &str) -> BlueprintResult<Option<Value>> {
        let row: Option<Value> = sqlx::query_scalar(
            "SELECT data FROM fl_iam_directory WHERE tenant_id=$1 AND kind=$2 AND id=$3",
        )
        .bind(self.store.tenant.id())
        .bind(kind.as_str())
        .bind(id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        if let Some(row) = row {
            let value = self.store.decode(kind, id, row)?;
            self.audit(kind, id, "read").await?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }
    async fn list(
        &mut self,
        kind: DirectoryKind,
        ids: Option<&[String]>,
        organization: Option<&str>,
    ) -> BlueprintResult<Vec<Value>> {
        if ids.is_some_and(|ids| ids.len() > 4096) {
            return Err(invalid());
        }
        let rows: Vec<(String,Value)> = sqlx::query_as("SELECT id,data FROM fl_iam_directory WHERE tenant_id=$1 AND kind=$2 AND ($3::text[] IS NULL OR id=ANY($3)) AND ($4::text IS NULL OR data->>'organization'=$4) ORDER BY sequence LIMIT 4097").bind(self.store.tenant.id()).bind(kind.as_str()).bind(ids).bind(organization).fetch_all(&mut *self.tx).await.map_err(db_error)?;
        if rows.len() > 4096 {
            return Err(super::scan_limit_error());
        }
        let mut values = Vec::with_capacity(rows.len());
        for (id, row) in rows {
            values.push(self.store.decode(kind, &id, row)?);
            self.audit(kind, &id, "read").await?;
        }
        Ok(values)
    }
    async fn insert(&mut self, kind: DirectoryKind, row: Value) -> BlueprintResult<()> {
        self.require_write()?;
        let id = record_id(&row)?.to_owned();
        let encoded = self.store.encode(kind, row.clone())?;
        sqlx::query("INSERT INTO fl_iam_directory(tenant_id,kind,id,data) VALUES($1,$2,$3,$4)")
            .bind(self.store.tenant.id())
            .bind(kind.as_str())
            .bind(&id)
            .bind(encoded)
            .execute(&mut *self.tx)
            .await
            .map_err(db_error)?;
        self.claims(kind, &row).await?;
        self.audit(kind, &id, "create").await
    }
    async fn replace(&mut self, kind: DirectoryKind, row: Value) -> BlueprintResult<()> {
        self.require_write()?;
        let id = record_id(&row)?.to_owned();
        let encoded = self.store.encode(kind, row.clone())?;
        let affected = sqlx::query(
            "UPDATE fl_iam_directory SET data=$4 WHERE tenant_id=$1 AND kind=$2 AND id=$3",
        )
        .bind(self.store.tenant.id())
        .bind(kind.as_str())
        .bind(&id)
        .bind(encoded)
        .execute(&mut *self.tx)
        .await
        .map_err(db_error)?
        .rows_affected();
        if affected == 0 {
            return Err(not_found());
        }
        self.claims(kind, &row).await?;
        self.audit(kind, &id, "update").await
    }
    async fn delete(&mut self, kind: DirectoryKind, id: &str) -> BlueprintResult<()> {
        self.require_write()?;
        let affected =
            sqlx::query("DELETE FROM fl_iam_directory WHERE tenant_id=$1 AND kind=$2 AND id=$3")
                .bind(self.store.tenant.id())
                .bind(kind.as_str())
                .bind(id)
                .execute(&mut *self.tx)
                .await
                .map_err(db_error)?
                .rows_affected();
        if affected == 0 {
            return Ok(());
        }
        let related = match kind {
            DirectoryKind::Role => Some((DirectoryKind::User, "roles")),
            DirectoryKind::Permission => Some((DirectoryKind::Role, "permissions")),
            DirectoryKind::Organization => Some((DirectoryKind::User, "organization")),
            DirectoryKind::User => None,
        };
        if let Some((family, field)) = related {
            let query = if field == "organization" {
                "UPDATE fl_iam_directory SET data=data-$3::text WHERE tenant_id=$1 AND kind=$2 AND data->>$3::text=$4::text RETURNING id"
            } else {
                "UPDATE fl_iam_directory SET data=jsonb_set(data,ARRAY[$3::text],(data->$3::text)-$4::text) WHERE tenant_id=$1 AND kind=$2 AND data @> jsonb_build_object($3::text,jsonb_build_array($4::text)) RETURNING id"
            };
            let changed: Vec<String> = sqlx::query_scalar(query)
                .bind(self.store.tenant.id())
                .bind(family.as_str())
                .bind(field)
                .bind(id)
                .fetch_all(&mut *self.tx)
                .await
                .map_err(db_error)?;
            for id in changed {
                self.audit(family, &id, "relationship_delete").await?;
            }
        }
        self.audit(kind, id, "delete").await
    }
    async fn commit(self) -> BlueprintResult<()> {
        self.tx.commit().await.map_err(db_error)
    }
}
fn validate_audit(audit: &AuditContext) -> BlueprintResult<()> {
    if audit.correlation_id.trim().is_empty() {
        Err(invalid())
    } else {
        Ok(())
    }
}
#[allow(clippy::needless_pass_by_value)]
fn db_error(error: sqlx::Error) -> BlueprintError {
    if error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .as_deref()
        == Some("23505")
    {
        conflict()
    } else {
        storage_error()
    }
}

#[cfg(test)]
mod tests;

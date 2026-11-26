//! Asynchronous `PostgreSQL` IAM storage with tenant isolation and persistent sessions.
//!
//! Run migrations with a schema-owner connection. Runtime connections must use a
//! role without SUPERUSER/BYPASSRLS privileges. Bind a store to the authenticated
//! tenant for each request. `Argon2` work runs outside the async executor.
mod async_service;
mod directory_identities;
mod provider_identities;

use super::credentials::{hash_password, verify_password};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::domain::Page;
use forklaunch_core::domain::PageRequest;
use forklaunch_core::persistence::{TenantContext, begin_tenant_transaction};
use forklaunch_iam_interface::{
    Credentials, Organization, Permission, Role, Session, User, invalid_credentials,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction, types::Json};
use std::sync::Arc;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

fn storage_error(_: impl std::fmt::Display) -> BlueprintError {
    BlueprintError::new("iam_storage_error", "IAM storage operation failed")
}
fn invalid_input() -> BlueprintError {
    BlueprintError::new("iam_invalid_input", "Invalid IAM input")
}
pub(crate) fn token_hash(token: &str) -> String {
    base16ct::lower::encode_string(&Sha256::digest(token.as_bytes()))
}

/// Bound to one tenant; every query also includes a tenant predicate in addition
/// to transaction-local RLS context. Clone it freely with its underlying pool.
#[derive(Clone)]
pub struct PostgresIamService {
    pool: PgPool,
    tenant: TenantContext,
    dummy_hash: Arc<String>,
    #[cfg(feature = "jwt-sessions")]
    jwt_issuer: Option<Arc<crate::jwt_sessions::RsaJwtSessionIssuer>>,
}

impl PostgresIamService {
    /// Applies embedded, versioned migrations using a schema-owner pool.
    ///
    /// # Errors
    /// Returns a storage error if migration locking or execution fails.
    pub async fn migrate(pool: &PgPool) -> BlueprintResult<()> {
        MIGRATOR.run(pool).await.map_err(storage_error)
    }

    /// Creates a tenant-bound store and prepares a dummy password hash so missing
    /// users still incur password verification work.
    ///
    /// # Errors
    /// Returns an error for SUPERUSER/BYPASSRLS connections, database failures,
    /// or password hashing failures.
    pub async fn new(pool: PgPool, tenant: TenantContext) -> BlueprintResult<Self> {
        let bypass: (bool,) = sqlx::query_as(
            "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
        )
        .fetch_one(&pool)
        .await
        .map_err(storage_error)?;
        if bypass.0 {
            return Err(BlueprintError::new(
                "iam_unsafe_database_role",
                "IAM runtime requires a role without SUPERUSER or BYPASSRLS",
            ));
        }
        let dummy_hash =
            tokio::task::spawn_blocking(|| hash_password(&uuid::Uuid::new_v4().to_string()))
                .await
                .map_err(storage_error)??;
        Ok(Self {
            pool,
            tenant,
            dummy_hash: Arc::new(dummy_hash),
            #[cfg(feature = "jwt-sessions")]
            jwt_issuer: None,
        })
    }

    /// Reuses the pool and dummy hash while binding another validated tenant.
    #[must_use]
    pub fn for_tenant(&self, tenant: TenantContext) -> Self {
        Self {
            pool: self.pool.clone(),
            tenant,
            dummy_hash: Arc::clone(&self.dummy_hash),
            #[cfg(feature = "jwt-sessions")]
            jwt_issuer: self.jwt_issuer.clone(),
        }
    }

    /// Issue RSA-signed tokens for future logins. The complete token's digest is
    /// still persisted; local session validation always checks current storage.
    #[cfg(feature = "jwt-sessions")]
    #[must_use]
    pub fn with_jwt_session_issuer(
        mut self,
        issuer: Arc<crate::jwt_sessions::RsaJwtSessionIssuer>,
    ) -> Self {
        self.jwt_issuer = Some(issuer);
        self
    }

    async fn transaction(&self) -> BlueprintResult<Transaction<'_, Postgres>> {
        begin_tenant_transaction(&self.pool, &self.tenant)
            .await
            .map_err(storage_error)
    }

    /// # Errors
    /// Returns an error for invalid identity, duplicate ID/email, hashing, or storage failure.
    pub async fn create_user(&self, mut user: User, password: String) -> BlueprintResult<User> {
        user.email = user.email.trim().to_lowercase();
        if user.id.trim().is_empty() || user.email.is_empty() || password.is_empty() {
            return Err(invalid_input());
        }
        let hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(storage_error)??;
        let mut tx = self.transaction().await?;
        sqlx::query("INSERT INTO fl_iam_users (tenant_id,id,email,user_json,password_hash) VALUES ($1,$2,$3,$4,$5)")
            .bind(self.tenant.id()).bind(&user.id).bind(&user.email).bind(Json(&user)).bind(hash)
            .execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(user)
    }

    /// # Errors
    /// Returns a storage error on lookup failure.
    pub async fn get_user(&self, id: &str) -> BlueprintResult<Option<User>> {
        let mut tx = self.transaction().await?;
        let row: Option<(Json<User>,)> =
            sqlx::query_as("SELECT user_json FROM fl_iam_users WHERE tenant_id=$1 AND id=$2")
                .bind(self.tenant.id())
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(row.map(|(Json(user),)| user))
    }

    /// # Errors
    /// Returns a storage error on lookup failure.
    pub async fn find_user_by_email(&self, email: &str) -> BlueprintResult<Option<User>> {
        let mut tx = self.transaction().await?;
        let row: Option<(Json<User>,)> =
            sqlx::query_as("SELECT user_json FROM fl_iam_users WHERE tenant_id=$1 AND email=$2")
                .bind(self.tenant.id())
                .bind(email.trim().to_lowercase())
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(row.map(|(Json(user),)| user))
    }

    /// Updates profile/roles without changing the password hash. Disabling a user
    /// invalidates session validation immediately.
    ///
    /// # Errors
    /// Returns an error for invalid identity/email or storage failure.
    pub async fn update_user(&self, mut user: User) -> BlueprintResult<bool> {
        user.email = user.email.trim().to_lowercase();
        if user.id.trim().is_empty() || user.email.is_empty() {
            return Err(invalid_input());
        }
        let mut tx = self.transaction().await?;
        let result = sqlx::query(
            "UPDATE fl_iam_users SET email=$3,user_json=$4 WHERE tenant_id=$1 AND id=$2",
        )
        .bind(self.tenant.id())
        .bind(&user.id)
        .bind(&user.email)
        .bind(Json(&user))
        .execute(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() != 0)
    }

    /// Changes a password and revokes every existing session in one transaction.
    /// Concurrent credential checks lock the same user row before issuing sessions.
    ///
    /// # Errors
    /// Rejects empty passwords and returns safe hashing/storage errors.
    pub async fn set_password(&self, user_id: &str, password: String) -> BlueprintResult<bool> {
        if password.is_empty() {
            return Err(invalid_input());
        }
        let hash = tokio::task::spawn_blocking(move || hash_password(&password))
            .await
            .map_err(storage_error)??;
        let mut tx = self.transaction().await?;
        let updated =
            sqlx::query("UPDATE fl_iam_users SET password_hash=$3 WHERE tenant_id=$1 AND id=$2")
                .bind(self.tenant.id())
                .bind(user_id)
                .bind(hash)
                .execute(&mut *tx)
                .await
                .map_err(storage_error)?
                .rows_affected()
                != 0;
        if updated {
            sqlx::query("DELETE FROM fl_iam_sessions WHERE tenant_id=$1 AND user_id=$2")
                .bind(self.tenant.id())
                .bind(user_id)
                .execute(&mut *tx)
                .await
                .map_err(storage_error)?;
        }
        tx.commit().await.map_err(storage_error)?;
        Ok(updated)
    }

    /// # Errors
    /// Returns an error on deletion failure. Related sessions are deleted atomically.
    pub async fn delete_user(&self, id: &str) -> BlueprintResult<bool> {
        let mut tx = self.transaction().await?;
        let result = sqlx::query("DELETE FROM fl_iam_users WHERE tenant_id=$1 AND id=$2")
            .bind(self.tenant.id())
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() != 0)
    }

    /// Lists users using bounded keyset pagination; cursors are opaque user IDs.
    ///
    /// # Errors
    /// Rejects limits outside 1..=1000 or storage failures.
    pub async fn list_users(&self, request: PageRequest) -> BlueprintResult<Page<User>> {
        let limit = page_limit(request.limit)?;
        let mut tx = self.transaction().await?;
        let rows: Vec<(Json<User>,)> = sqlx::query_as(
            "SELECT user_json FROM fl_iam_users WHERE tenant_id=$1 AND id>$2 ORDER BY id LIMIT $3",
        )
        .bind(self.tenant.id())
        .bind(request.cursor.unwrap_or_default())
        .bind(limit + 1)
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        let mut items: Vec<_> = rows.into_iter().map(|(Json(user),)| user).collect();
        let has_more = items.len() > request.limit;
        items.truncate(request.limit);
        let next = has_more
            .then(|| items.last().map(|user| user.id.clone()))
            .flatten();
        Ok(Page {
            items,
            next_cursor: next,
        })
    }

    /// # Errors
    /// Returns an error for invalid identity/slug, duplicates, or storage failure.
    pub async fn create_organization(&self, organization: Organization) -> BlueprintResult<()> {
        if organization.id.trim().is_empty() || organization.slug.trim().is_empty() {
            return Err(invalid_input());
        }
        let mut tx = self.transaction().await?;
        sqlx::query("INSERT INTO fl_iam_organizations (tenant_id,id,slug,organization_json) VALUES ($1,$2,$3,$4)")
            .bind(self.tenant.id()).bind(&organization.id).bind(&organization.slug).bind(Json(&organization)).execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)
    }

    /// # Errors
    /// Returns a storage error on lookup failure.
    pub async fn get_organization(&self, id: &str) -> BlueprintResult<Option<Organization>> {
        let mut tx = self.transaction().await?;
        let row: Option<(Json<Organization>,)> = sqlx::query_as(
            "SELECT organization_json FROM fl_iam_organizations WHERE tenant_id=$1 AND id=$2",
        )
        .bind(self.tenant.id())
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(row.map(|(Json(organization),)| organization))
    }

    /// # Errors
    /// Returns an error for invalid identity/slug or storage failure.
    pub async fn update_organization(&self, organization: Organization) -> BlueprintResult<bool> {
        if organization.id.trim().is_empty() || organization.slug.trim().is_empty() {
            return Err(invalid_input());
        }
        let mut tx = self.transaction().await?;
        let result = sqlx::query("UPDATE fl_iam_organizations SET slug=$3,organization_json=$4 WHERE tenant_id=$1 AND id=$2")
            .bind(self.tenant.id()).bind(&organization.id).bind(&organization.slug).bind(Json(&organization)).execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() != 0)
    }

    /// # Errors
    /// Returns a storage error on deletion failure.
    pub async fn delete_organization(&self, id: &str) -> BlueprintResult<bool> {
        let mut tx = self.transaction().await?;
        let result = sqlx::query("DELETE FROM fl_iam_organizations WHERE tenant_id=$1 AND id=$2")
            .bind(self.tenant.id())
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() != 0)
    }

    /// # Errors
    /// Rejects invalid page sizes or storage failures.
    pub async fn list_organizations(
        &self,
        request: PageRequest,
    ) -> BlueprintResult<Page<Organization>> {
        let limit = page_limit(request.limit)?;
        let mut tx = self.transaction().await?;
        let rows: Vec<(Json<Organization>,)> = sqlx::query_as("SELECT organization_json FROM fl_iam_organizations WHERE tenant_id=$1 AND id>$2 ORDER BY id LIMIT $3")
            .bind(self.tenant.id()).bind(request.cursor.unwrap_or_default()).bind(limit+1).fetch_all(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        let mut items: Vec<_> = rows.into_iter().map(|(Json(org),)| org).collect();
        let has_more = items.len() > request.limit;
        items.truncate(request.limit);
        let next = has_more
            .then(|| items.last().map(|org| org.id.clone()))
            .flatten();
        Ok(Page {
            items,
            next_cursor: next,
        })
    }

    /// # Errors
    /// Rejects empty role names or storage failures.
    pub async fn put_role(&self, role: Role) -> BlueprintResult<()> {
        if role.name.trim().is_empty() {
            return Err(invalid_input());
        }
        let mut tx = self.transaction().await?;
        sqlx::query("INSERT INTO fl_iam_roles (tenant_id,name,role_json) VALUES ($1,$2,$3) ON CONFLICT (tenant_id,name) DO UPDATE SET role_json=EXCLUDED.role_json")
            .bind(self.tenant.id()).bind(&role.name).bind(Json(&role)).execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)
    }

    /// # Errors
    /// Returns a storage error on role lookup failure.
    pub async fn roles_for_user(&self, user_id: &str) -> BlueprintResult<Vec<Role>> {
        let mut tx = self.transaction().await?;
        let user: Option<(Json<User>,)> =
            sqlx::query_as("SELECT user_json FROM fl_iam_users WHERE tenant_id=$1 AND id=$2")
                .bind(self.tenant.id())
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(storage_error)?;
        let names = user.map_or_else(Vec::new, |(Json(user),)| user.roles);
        let rows: Vec<(Json<Role>,)> = sqlx::query_as(
            "SELECT role_json FROM fl_iam_roles WHERE tenant_id=$1 AND name=ANY($2) ORDER BY name",
        )
        .bind(self.tenant.id())
        .bind(names)
        .fetch_all(&mut *tx)
        .await
        .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(rows.into_iter().map(|(Json(role),)| role).collect())
    }

    /// # Errors
    /// Returns a storage error on permission lookup failure.
    pub async fn permissions_for_user(&self, user_id: &str) -> BlueprintResult<Vec<Permission>> {
        let mut permissions = Vec::new();
        for role in self.roles_for_user(user_id).await? {
            for permission in role.permissions {
                if !permissions.contains(&permission) {
                    permissions.push(permission);
                }
            }
        }
        Ok(permissions)
    }

    /// Verifies credentials and persists a one-hour session. Tokens are opaque
    /// unless a JWT issuer was explicitly configured. Only a SHA-256 token digest
    /// is stored; plaintext tokens are returned once.
    ///
    /// # Errors
    /// Returns invalid credentials for unknown/disabled users and wrong passwords;
    /// returns safe storage errors for backend failures.
    pub async fn authenticate(&self, credentials: Credentials) -> BlueprintResult<Session> {
        let mut tx = self.transaction().await?;
        let row = sqlx::query("SELECT user_json,password_hash FROM fl_iam_users WHERE tenant_id=$1 AND email=$2 FOR UPDATE")
            .bind(self.tenant.id()).bind(credentials.email.trim().to_lowercase()).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let user: Option<User> = row
            .as_ref()
            .map(|row| {
                row.try_get::<Json<User>, _>("user_json")
                    .map(|Json(user)| user)
            })
            .transpose()
            .map_err(storage_error)?;
        let hash = row
            .as_ref()
            .map(|row| row.try_get::<String, _>("password_hash"))
            .transpose()
            .map_err(storage_error)?
            .unwrap_or_else(|| (*self.dummy_hash).clone());
        let valid =
            tokio::task::spawn_blocking(move || verify_password(&credentials.password, &hash))
                .await
                .map_err(storage_error)?;
        let user = user
            .filter(|user| !user.disabled && valid)
            .ok_or_else(invalid_credentials)?;
        let (token, expiry) = self.persist_session(&mut tx, &user).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(Session {
            id: token,
            user_id: user.id,
            expires_at_unix_seconds: expiry,
        })
    }

    async fn persist_session(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        user: &User,
    ) -> BlueprintResult<(String, u64)> {
        #[cfg(feature = "jwt-sessions")]
        if let Some(issuer) = &self.jwt_issuer {
            let (now,): (i64,) =
                sqlx::query_as("SELECT floor(extract(epoch FROM clock_timestamp()))::BIGINT")
                    .fetch_one(&mut **tx)
                    .await
                    .map_err(storage_error)?;
            let now = u64::try_from(now).map_err(storage_error)?;
            let expires_at = now.checked_add(3600).ok_or_else(invalid_input)?;
            let roles: Vec<(Json<Role>,)> = sqlx::query_as(
                "SELECT role_json FROM fl_iam_roles WHERE tenant_id=$1 AND name=ANY($2) ORDER BY name",
            ).bind(self.tenant.id()).bind(&user.roles)
                .fetch_all(&mut **tx).await.map_err(storage_error)?;
            let permissions = roles
                .into_iter()
                .flat_map(|(Json(role),)| role.permissions)
                .map(|permission| format!("{}:{}", permission.resource, permission.action))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let issuer = Arc::clone(issuer);
            let owned_user = user.clone();
            let tenant_id = self.tenant.id().to_owned();
            let token = tokio::task::spawn_blocking(move || {
                issuer.issue(&owned_user, &tenant_id, &permissions, now, expires_at)
            })
            .await
            .map_err(storage_error)??;
            sqlx::query("INSERT INTO fl_iam_sessions (tenant_id,token_hash,user_id,expires_at) VALUES ($1,$2,$3,to_timestamp($4::BIGINT::DOUBLE PRECISION))")
                .bind(self.tenant.id()).bind(token_hash(&token)).bind(&user.id)
                .bind(i64::try_from(expires_at).map_err(storage_error)?)
                .execute(&mut **tx).await.map_err(storage_error)?;
            return Ok((token, expires_at));
        }
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let expiry: (i64,) = sqlx::query_as("INSERT INTO fl_iam_sessions (tenant_id,token_hash,user_id,expires_at) VALUES ($1,$2,$3,now()+interval '1 hour') RETURNING EXTRACT(EPOCH FROM expires_at)::BIGINT")
            .bind(self.tenant.id()).bind(token_hash(&token)).bind(&user.id).fetch_one(&mut **tx).await.map_err(storage_error)?;
        Ok((token, u64::try_from(expiry.0).map_err(storage_error)?))
    }

    /// # Errors
    /// Returns a storage error on validation failure; revoked, expired, disabled,
    /// unknown, and cross-tenant sessions return `None`.
    pub async fn validate_session(&self, token: &str) -> BlueprintResult<Option<Session>> {
        let mut tx = self.transaction().await?;
        let row: Option<(String, i64)> = sqlx::query_as("SELECT s.user_id,EXTRACT(EPOCH FROM s.expires_at)::BIGINT FROM fl_iam_sessions s JOIN fl_iam_users u ON u.tenant_id=s.tenant_id AND u.id=s.user_id WHERE s.tenant_id=$1 AND s.token_hash=$2 AND s.expires_at>now() AND (u.user_json->>'disabled')::BOOLEAN=false")
            .bind(self.tenant.id()).bind(token_hash(token)).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        row.map(|(user_id, expiry)| {
            Ok(Session {
                id: token.to_owned(),
                user_id,
                expires_at_unix_seconds: u64::try_from(expiry).map_err(storage_error)?,
            })
        })
        .transpose()
    }

    /// # Errors
    /// Returns a storage error if revocation fails.
    pub async fn revoke_session(&self, token: &str) -> BlueprintResult<bool> {
        let mut tx = self.transaction().await?;
        let result =
            sqlx::query("DELETE FROM fl_iam_sessions WHERE tenant_id=$1 AND token_hash=$2")
                .bind(self.tenant.id())
                .bind(token_hash(token))
                .execute(&mut *tx)
                .await
                .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() != 0)
    }

    /// # Errors
    /// Returns a storage error if retention cleanup fails.
    pub async fn purge_expired_sessions(&self) -> BlueprintResult<u64> {
        let mut tx = self.transaction().await?;
        let result =
            sqlx::query("DELETE FROM fl_iam_sessions WHERE tenant_id=$1 AND expires_at<=now()")
                .bind(self.tenant.id())
                .execute(&mut *tx)
                .await
                .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected())
    }
}

fn page_limit(limit: usize) -> BlueprintResult<i64> {
    if !(1..=1000).contains(&limit) {
        return Err(invalid_input());
    }
    i64::try_from(limit).map_err(storage_error)
}

#[cfg(test)]
mod tests;

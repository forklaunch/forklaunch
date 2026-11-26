//! Current directory grants bound to a complete persisted session token.
use super::{
    DirectoryKind, DirectoryStore, DirectoryTransaction, PostgresDirectoryStore, db_error,
    storage_error,
};
use forklaunch_core::domain::AuditContext;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_iam_interface::services::UserDto;
use std::collections::BTreeSet;

/// One coherent, uncached directory authorization snapshot. No profile data or
/// credential/provider grant claims are copied into this value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryAuthorization {
    pub auth_user_id: String,
    pub directory_user_id: String,
    pub organization_id: Option<String>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub expires_at_unix_seconds: u64,
}

impl PostgresDirectoryStore {
    /// Resolves current directory role names, permission slugs and organization
    /// using a complete, current persisted session token and explicit binding.
    /// Shared directory locks and ordered user/session locks keep identity,
    /// grants and organization coherent during this read. No results are cached.
    /// Session/JWT subjects or matching email addresses cannot select an identity.
    /// # Errors
    /// Returns sanitized storage/decryption/audit errors. Unbound, disabled,
    /// expired, revoked, deleted or foreign-tenant identities return `None`.
    pub async fn authorization_for_session(
        &self,
        token: &str,
    ) -> BlueprintResult<Option<DirectoryAuthorization>> {
        let store =
            self.with_audit_context(AuditContext::system(self.audit.correlation_id.clone()))?;
        let mut tx = store.begin(false).await?;
        let digest = crate::postgres::token_hash(token);
        // Lock the user before the session, matching password change/deletion.
        // A separate second read detects a session revoked while acquiring user lock.
        let pair: Option<(String, String)> = sqlx::query_as("SELECT u.id,b.directory_user_id FROM fl_iam_users u JOIN fl_iam_sessions s ON s.tenant_id=u.tenant_id AND s.user_id=u.id JOIN fl_iam_directory_identities b ON b.tenant_id=u.tenant_id AND b.auth_user_id=u.id WHERE u.tenant_id=$1 AND s.token_hash=$2 AND s.expires_at>now() AND (u.user_json->>'disabled')::BOOLEAN=false FOR SHARE OF u")
            .bind(store.tenant.id()).bind(&digest).fetch_optional(&mut *tx.tx).await.map_err(db_error)?;
        let Some((auth_user_id, directory_user_id)) = pair else {
            return Ok(None);
        };
        let expiry: Option<i64> = sqlx::query_scalar("SELECT EXTRACT(EPOCH FROM expires_at)::BIGINT FROM fl_iam_sessions WHERE tenant_id=$1 AND token_hash=$2 AND user_id=$3 AND expires_at>now() FOR SHARE")
            .bind(store.tenant.id()).bind(&digest).bind(&auth_user_id).fetch_optional(&mut *tx.tx).await.map_err(db_error)?;
        let Some(expiry) = expiry else {
            return Ok(None);
        };
        tx.audit.actor_id = Some(directory_user_id.clone());
        let Some(row) = tx.get(DirectoryKind::User, &directory_user_id).await? else {
            return Ok(None);
        };
        let hydrated =
            crate::directory::mapping::hydrate(&mut tx, DirectoryKind::User, row).await?;
        let user: UserDto = serde_json::from_value(hydrated).map_err(|_| storage_error())?;
        let roles = user
            .roles
            .iter()
            .map(|role| role.name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let permissions = user
            .roles
            .iter()
            .flat_map(|role| {
                role.permissions
                    .iter()
                    .map(|permission| permission.slug.clone())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let snapshot = DirectoryAuthorization {
            auth_user_id,
            directory_user_id,
            organization_id: user.organization,
            roles,
            permissions,
            expires_at_unix_seconds: u64::try_from(expiry).map_err(|_| storage_error())?,
        };
        tx.commit().await?;
        Ok(Some(snapshot))
    }
}

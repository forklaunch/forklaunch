//! Resolve administrative directory links only from current persisted sessions.
use super::{BlueprintResult, PostgresIamService, storage_error, token_hash};

impl PostgresIamService {
    /// Resolves a directory identity from a valid persisted opaque or signed token.
    /// Provider logins use the same persisted sessions. Raw user IDs, unverified
    /// JWT claims and matching email addresses never resolve an association.
    /// Disabled users, expired/revoked sessions and other tenants return `None`.
    /// # Errors
    /// Returns sanitized storage failures.
    pub async fn directory_user_for_session(&self, token: &str) -> BlueprintResult<Option<String>> {
        let mut tx = self.transaction().await?;
        let directory = sqlx::query_scalar("SELECT b.directory_user_id FROM fl_iam_sessions s JOIN fl_iam_users u ON u.tenant_id=s.tenant_id AND u.id=s.user_id JOIN fl_iam_directory_identities b ON b.tenant_id=u.tenant_id AND b.auth_user_id=u.id JOIN fl_iam_directory d ON d.tenant_id=b.tenant_id AND d.kind=b.directory_kind AND d.id=b.directory_user_id WHERE s.tenant_id=$1 AND s.token_hash=$2 AND s.expires_at>now() AND (u.user_json->>'disabled')::BOOLEAN=false")
            .bind(self.tenant.id()).bind(token_hash(token))
            .fetch_optional(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(directory)
    }
}

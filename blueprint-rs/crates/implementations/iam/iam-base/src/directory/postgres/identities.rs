//! Explicit administrative associations. Callers must authorize these methods;
//! directory CRUD DTOs, passwords and provider claims never invoke them.
use super::{
    BlueprintResult, DirectoryKind, DirectoryStore, DirectoryTransaction, PostgresDirectoryStore,
    PostgresDirectoryTransaction, conflict, db_error, invalid, not_found,
};

fn validate_ids(directory: &str, auth: &str) -> BlueprintResult<()> {
    if [directory, auth]
        .iter()
        .any(|id| id.is_empty() || id.len() > 256)
    {
        return Err(invalid());
    }
    Ok(())
}

impl PostgresDirectoryStore {
    fn require_identity_actor(&self) -> BlueprintResult<()> {
        if self
            .audit
            .actor_id
            .as_deref()
            .is_none_or(|actor| actor.trim().is_empty())
        {
            return Err(invalid());
        }
        Ok(())
    }

    /// Administratively associates two existing tenant-local identities.
    /// The exact pair is idempotent; either identity being linked elsewhere fails.
    /// No email, password, organization or grants are copied. Callers must
    /// authorize this operation; the store's actor/correlation audit is atomic.
    /// # Errors
    /// Rejects missing/blank audit actors, absent identities, conflicting links and storage/audit failures.
    pub async fn bind_auth_identity(
        &self,
        directory_id: &str,
        auth_user_id: &str,
    ) -> BlueprintResult<()> {
        self.require_identity_actor()?;
        validate_ids(directory_id, auth_user_id)?;
        let mut tx = self.begin(true).await?;
        tx.lock_auth_user(auth_user_id).await?;
        let directory_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fl_iam_directory WHERE tenant_id=$1 AND kind='user' AND id=$2)")
            .bind(self.tenant.id()).bind(directory_id).fetch_one(&mut *tx.tx).await.map_err(db_error)?;
        if !directory_exists {
            return Err(not_found());
        }
        let existing = tx.identity_binding(directory_id).await?;
        if let Some(existing) = existing {
            if existing != auth_user_id {
                return Err(conflict());
            }
        } else {
            sqlx::query("INSERT INTO fl_iam_directory_identities(tenant_id,directory_user_id,auth_user_id) VALUES($1,$2,$3)")
                .bind(self.tenant.id()).bind(directory_id).bind(auth_user_id)
                .execute(&mut *tx.tx).await.map_err(db_error)?;
            tx.audit(DirectoryKind::User, directory_id, "bind_auth_identity")
                .await?;
        }
        tx.commit().await
    }

    /// Administratively removes only the specified current pair, with actor audit.
    /// Independent credentials, provider bindings and sessions remain valid;
    /// those sessions immediately stop resolving a directory association.
    /// Callers must authorize this operation. Use coordinated deletion when the
    /// credential account and all of its sessions must also be removed.
    /// # Errors
    /// Rejects missing/blank audit actors, malformed IDs and storage/audit failures.
    /// A mismatched pair returns false.
    pub async fn unbind_auth_identity(
        &self,
        directory_id: &str,
        expected_auth_user_id: &str,
    ) -> BlueprintResult<bool> {
        self.require_identity_actor()?;
        validate_ids(directory_id, expected_auth_user_id)?;
        let mut tx = self.begin(true).await?;
        let removed = sqlx::query("DELETE FROM fl_iam_directory_identities WHERE tenant_id=$1 AND directory_user_id=$2 AND auth_user_id=$3")
            .bind(self.tenant.id()).bind(directory_id).bind(expected_auth_user_id)
            .execute(&mut *tx.tx).await.map_err(db_error)?.rows_affected() != 0;
        if removed {
            tx.audit(DirectoryKind::User, directory_id, "unbind_auth_identity")
                .await?;
        }
        tx.commit().await?;
        Ok(removed)
    }

    /// Administratively deletes an exact linked pair in one audited transaction.
    /// Credential-user deletion cascades persisted sessions and provider bindings;
    /// directory deletion includes its uniqueness claims and existing audit policy.
    /// Ordinary directory CRUD rejects deletion while a binding exists.
    /// Callers must authorize this operation; no request DTO triggers it implicitly.
    /// # Errors
    /// Rejects missing/blank audit actors, missing/mismatched pairs and storage/audit failures; all
    /// credential, session, provider and directory changes roll back together.
    pub async fn delete_linked_identity(
        &self,
        directory_id: &str,
        expected_auth_user_id: &str,
    ) -> BlueprintResult<()> {
        self.require_identity_actor()?;
        validate_ids(directory_id, expected_auth_user_id)?;
        let mut tx = self.begin(true).await?;
        tx.lock_auth_user(expected_auth_user_id).await?;
        if tx.identity_binding(directory_id).await?.as_deref() != Some(expected_auth_user_id) {
            return Err(conflict());
        }
        sqlx::query("DELETE FROM fl_iam_users WHERE tenant_id=$1 AND id=$2")
            .bind(self.tenant.id())
            .bind(expected_auth_user_id)
            .execute(&mut *tx.tx)
            .await
            .map_err(db_error)?;
        tx.audit(DirectoryKind::User, directory_id, "delete_auth_identity")
            .await?;
        tx.delete(DirectoryKind::User, directory_id).await?;
        tx.commit().await
    }
}

impl PostgresDirectoryTransaction<'_> {
    async fn lock_auth_user(&mut self, id: &str) -> BlueprintResult<()> {
        let exists: Option<String> = sqlx::query_scalar(
            "SELECT id FROM fl_iam_users WHERE tenant_id=$1 AND id=$2 FOR UPDATE",
        )
        .bind(self.store.tenant.id())
        .bind(id)
        .fetch_optional(&mut *self.tx)
        .await
        .map_err(db_error)?;
        exists.ok_or_else(not_found)?;
        Ok(())
    }
    async fn identity_binding(&mut self, directory_id: &str) -> BlueprintResult<Option<String>> {
        sqlx::query_scalar("SELECT auth_user_id FROM fl_iam_directory_identities WHERE tenant_id=$1 AND directory_user_id=$2")
            .bind(self.store.tenant.id()).bind(directory_id).fetch_optional(&mut *self.tx).await.map_err(db_error)
    }
}

#[cfg(test)]
mod tests;

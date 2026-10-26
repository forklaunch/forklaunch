//! Explicit provider bindings. These administrative methods must not be exposed
//! as unauthenticated endpoints or fed directly from unverified callback claims.
use super::{BlueprintError, BlueprintResult, PostgresIamService, invalid_input, storage_error};
#[cfg(any(feature = "oidc-sessions", test))]
use super::{Json, Session, User, invalid_credentials};

fn validate_identity(issuer: &str, subject: &str) -> BlueprintResult<()> {
    if [issuer, subject]
        .iter()
        .any(|value| value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control))
    {
        return Err(invalid_input());
    }
    Ok(())
}

impl PostgresIamService {
    /// Provision an exact, case-sensitive issuer/subject binding to an existing
    /// local user. Callers must authorize this administrative operation. Never
    /// infer ownership from matching email addresses or other mutable claims.
    /// Repeating the same binding is idempotent; rebinding to another user fails.
    /// # Errors
    /// Rejects malformed identities, absent users, conflicting bindings or storage failures.
    pub async fn bind_provider_identity(
        &self,
        issuer: &str,
        subject: &str,
        user_id: &str,
    ) -> BlueprintResult<()> {
        validate_identity(issuer, subject)?;
        let mut tx = self.transaction().await?;
        let result = sqlx::query("INSERT INTO fl_iam_provider_identities (tenant_id,issuer,subject,user_id) VALUES ($1,$2,$3,$4) ON CONFLICT (tenant_id,issuer,subject) DO UPDATE SET user_id=fl_iam_provider_identities.user_id WHERE fl_iam_provider_identities.user_id=EXCLUDED.user_id")
            .bind(self.tenant.id()).bind(issuer).bind(subject).bind(user_id)
            .execute(&mut *tx).await.map_err(storage_error)?;
        if result.rows_affected() != 1 {
            return Err(BlueprintError::new(
                "iam_provider_identity_conflict",
                "Provider identity is already bound",
            ));
        }
        tx.commit().await.map_err(storage_error)
    }

    /// Remove a binding only if it still belongs to the specified user. Existing
    /// local sessions remain valid; revoke those separately when policy requires.
    /// Callers must authorize this administrative operation.
    /// # Errors
    /// Rejects malformed identities or storage failures.
    pub async fn unbind_provider_identity(
        &self,
        issuer: &str,
        subject: &str,
        user_id: &str,
    ) -> BlueprintResult<bool> {
        validate_identity(issuer, subject)?;
        let mut tx = self.transaction().await?;
        let result = sqlx::query("DELETE FROM fl_iam_provider_identities WHERE tenant_id=$1 AND issuer=$2 AND subject=$3 AND user_id=$4")
            .bind(self.tenant.id()).bind(issuer).bind(subject).bind(user_id)
            .execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() == 1)
    }

    /// Verify an OIDC authorization-code response before resolving its exact
    /// issuer/subject binding and issuing a revocable local session. Provider
    /// tokens are dropped; email, roles and other provider claims grant no access.
    /// The caller MUST consume pending state from the initiating browser's
    /// session, not retrieve it using the callback state alone. This method does
    /// not implement HTTP callbacks, browser binding or provider account linking.
    /// # Errors
    /// Rejects invalid provider responses, missing bindings, disabled users and storage failures.
    #[cfg(feature = "oidc-sessions")]
    pub async fn authenticate_oidc(
        &self,
        provider: &forklaunch_core::auth::oidc::OidcProvider,
        pending: forklaunch_core::auth::oidc::PendingAuthorization,
        returned_state: &str,
        code: &str,
    ) -> BlueprintResult<Session> {
        let login = provider
            .exchange(pending, returned_state, code)
            .await
            .map_err(|_| invalid_credentials())?;
        self.session_for_provider_identity(
            login.claims.issuer().as_str(),
            login.claims.subject().as_str(),
        )
        .await
    }

    #[cfg(any(feature = "oidc-sessions", test))]
    async fn session_for_provider_identity(
        &self,
        issuer: &str,
        subject: &str,
    ) -> BlueprintResult<Session> {
        validate_identity(issuer, subject)?;
        let mut tx = self.transaction().await?;
        // Lock both binding and user so unlink, disable and delete serialize with
        // issuance. No network exchange or provider tokens enter this transaction.
        let row: Option<(Json<User>,)> = sqlx::query_as("SELECT u.user_json FROM fl_iam_provider_identities p JOIN fl_iam_users u ON u.tenant_id=p.tenant_id AND u.id=p.user_id WHERE p.tenant_id=$1 AND p.issuer=$2 AND p.subject=$3 FOR UPDATE OF p,u")
            .bind(self.tenant.id()).bind(issuer).bind(subject)
            .fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let user = row
            .map(|(Json(user),)| user)
            .filter(|user| !user.disabled)
            .ok_or_else(invalid_credentials)?;
        let (id, expires_at_unix_seconds) = self.persist_session(&mut tx, &user).await?;
        tx.commit().await.map_err(storage_error)?;
        Ok(Session {
            id,
            user_id: user.id,
            expires_at_unix_seconds,
        })
    }
}

#[cfg(test)]
mod tests;

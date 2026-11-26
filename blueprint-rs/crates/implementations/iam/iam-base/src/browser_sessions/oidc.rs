use super::PostgresCookieStore;
use forklaunch_core::auth::oidc::PendingAuthorization;
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize)]
struct Envelope {
    namespace: String,
    browser_hash: String,
    state_hash: String,
    pending: String,
    expires_at: i64,
}
fn failed() -> BlueprintError {
    BlueprintError::new(
        "iam_oidc_pending_invalid",
        "Invalid or unavailable OIDC login state",
    )
}
fn hash(value: &str) -> String {
    base16ct::lower::encode_string(&Sha256::digest(value.as_bytes()))
}
fn bounded(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
impl PostgresCookieStore {
    /// Persist one encrypted authorization flow per existing browser cookie.
    /// A newer start supersedes the previous flow. Cookie deletion cascades here.
    /// # Errors
    /// Rejects invalid binding/state, absent cookies, expiry and storage failures.
    pub async fn save_oidc_pending(
        &self,
        browser: &str,
        pending: PendingAuthorization,
    ) -> BlueprintResult<()> {
        if !bounded(browser) || !bounded(pending.state()) {
            return Err(failed());
        }
        let browser_hash = hash(browser);
        let state_hash = hash(pending.state());
        let expires_at = pending.expires_at_unix_seconds();
        let envelope = Envelope {
            namespace: self.namespace.clone(),
            browser_hash: browser_hash.clone(),
            state_hash: state_hash.clone(),
            pending: pending.into_storage_secret().map_err(|_| failed())?,
            expires_at,
        };
        let plaintext = serde_json::to_string(&envelope).map_err(|_| failed())?;
        let encrypted = self
            .encryptor
            .seal(&plaintext, self.tenant.id(), "[IAM OIDC pending]")
            .map_err(|_| failed())?;
        let mut tx = self.transaction().await.map_err(|_| failed())?;
        sqlx::query("INSERT INTO fl_iam_oidc_pending (tenant_id,namespace,browser_hash,state_hash,encrypted_pending,expires_at) SELECT $1,$2,$3,$4,$5,$6 FROM fl_iam_browser_sessions WHERE tenant_id=$1 AND namespace=$2 AND id_hash=$3 AND expires_at>floor(extract(epoch FROM clock_timestamp()))::BIGINT ON CONFLICT (tenant_id,namespace,browser_hash) DO UPDATE SET state_hash=EXCLUDED.state_hash,encrypted_pending=EXCLUDED.encrypted_pending,expires_at=EXCLUDED.expires_at")
            .bind(self.tenant.id()).bind(&self.namespace).bind(browser_hash).bind(state_hash).bind(encrypted).bind(expires_at)
            .execute(&mut *tx).await.map_err(|_| failed()).and_then(|result| if result.rows_affected()==1 {Ok(())} else {Err(failed())})?;
        tx.commit().await.map_err(|_| failed())
    }

    /// Atomically consume matching browser-bound state before any provider I/O.
    /// Replay, another browser/tenant, expired state and corrupt envelopes fail.
    /// # Errors
    /// Returns sanitized state/storage errors; consumed entries are never restored.
    pub async fn consume_oidc_pending(
        &self,
        browser: &str,
        state: &str,
    ) -> BlueprintResult<PendingAuthorization> {
        if !bounded(browser) || !bounded(state) {
            return Err(failed());
        }
        let browser_hash = hash(browser);
        let state_hash = hash(state);
        let mut tx = self.transaction().await.map_err(|_| failed())?;
        let row: Option<(String,i64)> = sqlx::query_as("DELETE FROM fl_iam_oidc_pending p USING fl_iam_browser_sessions b WHERE p.tenant_id=$1 AND p.namespace=$2 AND p.browser_hash=$3 AND p.state_hash=$4 AND b.tenant_id=p.tenant_id AND b.namespace=p.namespace AND b.id_hash=p.browser_hash AND b.expires_at>floor(extract(epoch FROM clock_timestamp()))::BIGINT RETURNING p.encrypted_pending,p.expires_at")
            .bind(self.tenant.id()).bind(&self.namespace).bind(&browser_hash).bind(&state_hash).fetch_optional(&mut *tx).await.map_err(|_| failed())?;
        tx.commit().await.map_err(|_| failed())?;
        let (encrypted, expiry) = row.ok_or_else(failed)?;
        if encrypted.len() > 128 * 1024 || expiry <= chrono::Utc::now().timestamp() {
            return Err(failed());
        }
        let plaintext = self
            .encryptor
            .decrypt(&encrypted, self.tenant.id())
            .map_err(|_| failed())?;
        if plaintext.len() > 64 * 1024 {
            return Err(failed());
        }
        let envelope: Envelope = serde_json::from_str(&plaintext).map_err(|_| failed())?;
        if envelope.namespace != self.namespace
            || envelope.browser_hash != browser_hash
            || envelope.state_hash != state_hash
            || envelope.expires_at != expiry
        {
            return Err(failed());
        }
        let pending =
            PendingAuthorization::from_storage_secret(&envelope.pending).map_err(|_| failed())?;
        if pending.state() != state || pending.expires_at_unix_seconds() != expiry {
            return Err(failed());
        }
        Ok(pending)
    }
}

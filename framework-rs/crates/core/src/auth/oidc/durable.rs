//! Explicit secret-bearing persistence for authorization-code exchanges.
use super::{PendingAuthorization, failed};
use forklaunch_common::FrameworkResult;
use oauth2::{CsrfToken, PkceCodeVerifier, url::Url};
use openidconnect::Nonce;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    version: u8,
    url: String,
    state: String,
    nonce: String,
    verifier: String,
    issuer: String,
    client_id: String,
    redirect: String,
    expires_at: i64,
}
pub(super) fn valid_expiry(expiry: i64) -> bool {
    let now = chrono::Utc::now().timestamp();
    expiry
        .checked_sub(now)
        .is_some_and(|remaining| (1..=600).contains(&remaining))
}
impl PendingAuthorization {
    /// Absolute expiry retained across restarts. Storage must enforce this too.
    #[must_use]
    pub const fn expires_at_unix_seconds(&self) -> i64 {
        self.expires_at
    }

    /// Consume pending state into a SECRET persistence payload. The payload
    /// contains the PKCE verifier and nonce. Only place it in authenticated
    /// encrypted server storage bound to the initiating browser and tenant.
    /// Never expose it to browsers, logs or untrusted deserialization sources.
    /// # Errors
    /// Rejects expired state or serialization errors.
    pub fn into_storage_secret(self) -> FrameworkResult<String> {
        if self.created.elapsed() >= std::time::Duration::from_secs(600)
            || !valid_expiry(self.expires_at)
        {
            return Err(failed());
        }
        serde_json::to_string(&Stored {
            version: 1,
            url: self.url.to_string(),
            state: self.state.secret().clone(),
            nonce: self.nonce.secret().clone(),
            verifier: self.verifier.secret().clone(),
            issuer: self.issuer,
            client_id: self.client_id,
            redirect: self.redirect,
            expires_at: self.expires_at,
        })
        .map_err(|_| failed())
    }
    /// Restore only after authenticated decryption and atomic single-use removal
    /// from browser-bound server storage. This is pending state, not verified
    /// identity: `OidcProvider::exchange` still verifies all provider credentials.
    /// # Errors
    /// Rejects malformed, oversized, expired or future-lifetime payloads.
    pub fn from_storage_secret(secret: &str) -> FrameworkResult<Self> {
        if secret.len() > 32 * 1024 {
            return Err(failed());
        }
        let value: Stored = serde_json::from_str(secret).map_err(|_| failed())?;
        if value.version != 1
            || !valid_expiry(value.expires_at)
            || [
                &value.state,
                &value.nonce,
                &value.verifier,
                &value.issuer,
                &value.client_id,
                &value.redirect,
            ]
            .iter()
            .any(|part| part.is_empty() || part.len() > 4096 || part.chars().any(char::is_control))
            || !(43..=128).contains(&value.verifier.len())
            || value.url.len() > 16 * 1024
        {
            return Err(failed());
        }
        Ok(Self {
            url: Url::parse(&value.url).map_err(|_| failed())?,
            state: CsrfToken::new(value.state),
            nonce: Nonce::new(value.nonce),
            verifier: PkceCodeVerifier::new(value.verifier),
            issuer: value.issuer,
            client_id: value.client_id,
            redirect: value.redirect,
            created: Instant::now(),
            expires_at: value.expires_at,
        })
    }
}

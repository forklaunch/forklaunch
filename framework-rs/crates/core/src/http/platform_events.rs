//! Instance-signed incoming platform events. Event-ID business deduplication
//! belongs to the handler; this verifier atomically claims delivery nonces.
use crate::domain::BlueprintError;
use crate::domain::BlueprintResult;
use forklaunch_core::auth::request_hmac::{HmacAuthorization, SignedBody};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::Duration,
};
use zeroize::Zeroizing;
pub const PLATFORM_EVENT_KEY_ID: &str = "platform";
pub const PLATFORM_EVENTS_PATH: &str = "/platform-events";
pub const PLATFORM_EVENT_MAX_SKEW: Duration = Duration::from_secs(300);
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformEvent<Data = Value> {
    pub id: String,
    pub feature: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub occurred_at: String,
    pub data: Data,
}
pub type NonceClaimFuture<'a> = Pin<Box<dyn Future<Output = BlueprintResult<bool>> + Send + 'a>>;
/// Claims must be atomic across all verifiers sharing the store. Keep each
/// nonce through `expires_at`, including its last valid millisecond. Replicated
/// applications should supply a shared backend; memory covers one process.
pub trait PlatformEventNonceStore: Send + Sync {
    fn claim<'a>(&'a self, nonce: &'a str, now: i64, expires_at: i64) -> NonceClaimFuture<'a>;
}
#[derive(Debug)]
pub struct MemoryPlatformEventNonces {
    nonces: Mutex<BTreeMap<String, i64>>,
    capacity: usize,
}
impl MemoryPlatformEventNonces {
    /// # Errors
    /// Rejects zero or unreasonably large capacity.
    pub fn new(capacity: usize) -> BlueprintResult<Self> {
        if capacity == 0 || capacity > 1_000_000 {
            return Err(invalid("platform_event_configuration"));
        }
        Ok(Self {
            nonces: Mutex::new(BTreeMap::new()),
            capacity,
        })
    }
}
impl Default for MemoryPlatformEventNonces {
    fn default() -> Self {
        Self {
            nonces: Mutex::new(BTreeMap::new()),
            capacity: 10_000,
        }
    }
}
impl PlatformEventNonceStore for MemoryPlatformEventNonces {
    fn claim<'a>(&'a self, nonce: &'a str, now: i64, expires_at: i64) -> NonceClaimFuture<'a> {
        Box::pin(async move {
            if nonce.is_empty() || nonce.len() > 1024 || nonce.chars().any(char::is_control) {
                return Err(invalid("platform_event_invalid_nonce"));
            }
            let mut nonces = self
                .nonces
                .lock()
                .map_err(|_| invalid("platform_event_nonce_store"))?;
            nonces.retain(|_, expires| *expires >= now);
            if nonces.contains_key(nonce) {
                return Ok(false);
            }
            if nonces.len() >= self.capacity {
                return Err(invalid("platform_event_nonce_capacity"));
            }
            nonces.insert(nonce.to_owned(), expires_at);
            Ok(true)
        })
    }
}
#[derive(Clone)]
pub struct PlatformEventVerifier {
    secret: Arc<Zeroizing<String>>,
    nonces: Arc<dyn PlatformEventNonceStore>,
}
impl std::fmt::Debug for PlatformEventVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformEventVerifier")
            .finish_non_exhaustive()
    }
}
impl PlatformEventVerifier {
    /// # Errors
    /// Rejects invalid server-owned instance keys. Supply shared nonce storage
    /// for replicas; verified events still need handler-owned ID deduplication.
    pub fn new(secret: String, nonces: Arc<dyn PlatformEventNonceStore>) -> BlueprintResult<Self> {
        if secret.is_empty() || secret.len() > 4096 || secret.chars().any(char::is_control) {
            return Err(invalid("platform_event_configuration"));
        }
        Ok(Self {
            secret: Arc::new(Zeroizing::new(secret)),
            nonces,
        })
    }
    /// # Errors
    /// Rejects malformed/forged/stale/replayed delivery and invalid event shape.
    pub async fn verify(
        &self,
        method: &str,
        path: &str,
        authorization: &str,
        body: &[u8],
    ) -> BlueprintResult<VerifiedPlatformEvent> {
        self.verify_at(
            method,
            path,
            authorization,
            body,
            chrono::Utc::now().timestamp_millis(),
        )
        .await
    }
    /// Deterministic-clock variant for conformance checks.
    /// # Errors
    /// Rejects malformed/forged/stale/replayed delivery and invalid event shape.
    pub async fn verify_at(
        &self,
        method: &str,
        path: &str,
        authorization: &str,
        body: &[u8],
        now: i64,
    ) -> BlueprintResult<VerifiedPlatformEvent> {
        if authorization.len() > 16 * 1024 || body.len() > 1024 * 1024 {
            return Err(invalid("platform_event_verification"));
        }
        let auth = HmacAuthorization::parse(authorization)
            .map_err(|_| invalid("platform_event_verification"))?;
        if auth.key_id() != PLATFORM_EVENT_KEY_ID
            || !auth.verify_at(
                self.secret.as_bytes(),
                &method.to_ascii_uppercase(),
                path,
                SignedBody::Bytes(body),
                now,
                PLATFORM_EVENT_MAX_SKEW,
            )
        {
            return Err(invalid("platform_event_verification"));
        }
        // A future timestamp can remain valid for another five minutes after
        // the first five minutes elapse. Retain the nonce through that point.
        let expires_at = auth.timestamp_unix_millis().saturating_add(300_000);
        if !self.nonces.claim(auth.nonce(), now, expires_at).await? {
            return Err(invalid("platform_event_replay"));
        }
        let payload: Value =
            serde_json::from_slice(body).map_err(|_| invalid("platform_event_verification"))?;
        if !["id", "type", "feature"]
            .iter()
            .all(|field| payload[*field].is_string())
        {
            return Err(invalid("platform_event_verification"));
        }
        Ok(VerifiedPlatformEvent { payload })
    }
}
/// Signature-verified payload. Shape validation matches the actual TypeScript
/// verifier (string id/type/feature); applications may request the richer DTO.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedPlatformEvent {
    payload: Value,
}
impl VerifiedPlatformEvent {
    #[must_use]
    pub const fn payload(&self) -> &Value {
        &self.payload
    }
    #[must_use]
    pub fn into_payload(self) -> Value {
        self.payload
    }
    /// # Errors
    /// Rejects fields missing from the application's typed event DTO.
    pub fn into_event<Data: serde::de::DeserializeOwned>(
        self,
    ) -> BlueprintResult<PlatformEvent<Data>> {
        serde_json::from_value(self.payload).map_err(|_| invalid("platform_event_invalid_data"))
    }
}
fn invalid(code: &str) -> BlueprintError {
    BlueprintError::new(code, "Platform event verification failed")
}

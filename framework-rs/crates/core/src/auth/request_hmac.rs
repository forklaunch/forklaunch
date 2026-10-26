//! TypeScript `createHmacToken` / `generateHmacAuthHeaders` wire compatibility.
//! Signs the method, path (without query), verbatim UTF-8 body, normalized ISO
//! timestamp and nonce. The path is the full wire path the client sent
//! (`/payment-link/abc`), never an Express router-relative path; the SDK signers
//! (`forklaunch_universal_sdk::HmacSigner`, TS `universalSdk({ hmac })`) sign it. JSON callers should serialize once, sign the wire bytes,
//! and send those same bytes rather than reserializing a JSON object.
//!
//! [`HmacAuthorization::verify`](crate::auth::request_hmac::HmacAuthorization::verify) matches the TypeScript signature check;
//! [`HmacAuthorization::verify_at`](crate::auth::request_hmac::HmacAuthorization::verify_at) also enforces timestamp freshness. A shared
//! nonce store is an application-owned policy, separate from signature checking.
use crate::auth::{AuthError, hmac_sign, hmac_verify};
use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use std::{fmt, time::Duration};

/// Authenticate a request and atomically claim its nonce in a shared store.
/// The store key includes the key ID; expiry is the last timestamp at which
/// this signature could pass freshness validation (including future skew).
/// Storage failure fails closed. Never claim a nonce before signature checking.
/// # Errors
/// Rejects invalid or stale signatures, previously claimed nonces, and unavailable nonce storage.
#[allow(clippy::too_many_arguments)]
pub async fn verify_with_nonce_store<F, Fut, E>(
    header: &str,
    secret: &[u8],
    key_id: &str,
    method: &str,
    path: &str,
    body: SignedBody<'_>,
    now_unix_millis: i64,
    max_skew: Duration,
    claim: F,
) -> Result<(), AuthError>
where
    F: FnOnce(String, i64) -> Fut,
    Fut: std::future::Future<Output = Result<bool, E>>,
{
    let auth = HmacAuthorization::parse(header)?;
    if secret.len() < 32
        || max_skew.is_zero()
        || max_skew > Duration::from_secs(300)
        || auth.key_id() != key_id
        || !auth.verify_at(secret, method, path, body, now_unix_millis, max_skew)
    {
        return Err(AuthError::InvalidCredentials);
    }
    let expiry = auth
        .timestamp_unix_millis()
        .checked_add(
            i64::try_from(max_skew.as_millis()).map_err(|_| AuthError::InvalidCredentials)?,
        )
        .ok_or(AuthError::InvalidCredentials)?;
    // Length prefix prevents ambiguous key/nonce pairs.
    let nonce = format!("{}:{}{}", key_id.len(), key_id, auth.nonce());
    if !claim(nonce, expiry)
        .await
        .map_err(|_| AuthError::InvalidCredentials)?
    {
        return Err(AuthError::InvalidCredentials);
    }
    Ok(())
}

#[cfg(test)]
mod replay_tests;

/// Preserve TypeScript's distinction between an absent/falsy string body and
/// a Buffer (including a zero-length Buffer). `Bytes` uses Node-compatible lossy
/// UTF-8 decoding. Raw JSON bytes preserve whitespace and property ordering.
#[derive(Clone, Copy)]
pub enum SignedBody<'a> {
    Absent,
    Text(&'a str),
    Bytes(&'a [u8]),
}

/// A parsed credential. Debug never exposes the credential or signature.
pub struct HmacAuthorization<'a> {
    key_id: &'a str,
    timestamp: DateTime<Utc>,
    nonce: &'a str,
    signature: &'a str,
}

impl fmt::Debug for HmacAuthorization<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HmacAuthorization")
            .finish_non_exhaustive()
    }
}

impl<'a> HmacAuthorization<'a> {
    /// # Errors
    /// Rejects malformed, oversized or ambiguous authorization headers.
    pub fn parse(header: &'a str) -> Result<Self, AuthError> {
        if header.len() > 4096 {
            return Err(AuthError::InvalidCredentials);
        }
        let mut parts = header.split(' ');
        if parts.next() != Some("HMAC") {
            return Err(AuthError::InvalidCredentials);
        }
        let key_id = part(parts.next(), "keyId=")?;
        let timestamp = part(parts.next(), "ts=")?;
        let nonce = part(parts.next(), "nonce=")?;
        let signature = part(parts.next(), "signature=")?;
        if parts.next().is_some()
            || !component(key_id)
            || !component(nonce)
            || signature.len() != 44
        {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(Self {
            key_id,
            timestamp: parse_timestamp(timestamp)?,
            nonce,
            signature,
        })
    }

    #[must_use]
    pub const fn key_id(&self) -> &str {
        self.key_id
    }
    #[must_use]
    pub const fn nonce(&self) -> &str {
        self.nonce
    }
    #[must_use]
    pub fn timestamp_unix_millis(&self) -> i64 {
        self.timestamp.timestamp_millis()
    }

    /// Verifies the signature in constant time. Select the configured secret by
    /// [`Self::key_id`] before calling; unknown IDs must fail closed.
    #[must_use]
    pub fn verify(&self, secret: &[u8], method: &str, path: &str, body: SignedBody<'_>) -> bool {
        let Ok(payload) = payload(method, path, body, self.timestamp, self.nonce) else {
            return false;
        };
        !secret.is_empty() && hmac_verify(secret, payload.as_bytes(), self.signature)
    }

    /// Verifies signature and absolute clock skew, including future timestamps.
    /// Replay rejection additionally needs an atomic, shared nonce claim after
    /// verification; an in-process cache cannot provide that across replicas.
    #[must_use]
    pub fn verify_at(
        &self,
        secret: &[u8],
        method: &str,
        path: &str,
        body: SignedBody<'_>,
        now_unix_millis: i64,
        max_skew: Duration,
    ) -> bool {
        u128::from(self.timestamp_unix_millis().abs_diff(now_unix_millis)) <= max_skew.as_millis()
            && self.verify(secret, method, path, body)
    }
}

/// Creates a TypeScript-compatible signature using an RFC3339 timestamp.
/// Milliseconds are normalized exactly as `Date.toISOString()`.
///
/// # Errors
/// Rejects empty secrets, invalid timestamps or newline-injected components.
pub fn create_hmac_token(
    secret: &[u8],
    method: &str,
    path: &str,
    body: SignedBody<'_>,
    timestamp: &str,
    nonce: &str,
) -> Result<String, AuthError> {
    if secret.is_empty() {
        return Err(AuthError::InvalidCredentials);
    }
    let payload = payload(method, path, body, parse_timestamp(timestamp)?, nonce)?;
    Ok(hmac_sign(secret, payload.as_bytes()))
}

/// Creates the complete `authorization` header; callers supply a fresh nonce.
///
/// # Errors
/// Rejects invalid key IDs/nonces/timestamps or signing inputs.
pub fn generate_hmac_auth_header(
    secret: &[u8],
    key_id: &str,
    method: &str,
    path: &str,
    body: SignedBody<'_>,
    timestamp: &str,
    nonce: &str,
) -> Result<String, AuthError> {
    if !component(key_id) {
        return Err(AuthError::InvalidCredentials);
    }
    let timestamp = parse_timestamp(timestamp)?.to_rfc3339_opts(SecondsFormat::Millis, true);
    let signature = create_hmac_token(secret, method, path, body, &timestamp, nonce)?;
    Ok(format!(
        "HMAC keyId={key_id} ts={timestamp} nonce={nonce} signature={signature}"
    ))
}

fn part<'a>(part: Option<&'a str>, prefix: &str) -> Result<&'a str, AuthError> {
    part.and_then(|part| part.strip_prefix(prefix))
        .filter(|part| !part.is_empty())
        .ok_or(AuthError::InvalidCredentials)
}

fn component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn parse_timestamp(timestamp: &str) -> Result<DateTime<Utc>, AuthError> {
    if timestamp.len() > 64 {
        return Err(AuthError::InvalidCredentials);
    }
    let value = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| AuthError::InvalidCredentials)?
        .with_timezone(&Utc);
    // Leap-second encodings are accepted by chrono, but not JavaScript Date.
    if !(0..=9999).contains(&value.year()) || value.timestamp_subsec_nanos() >= 1_000_000_000 {
        return Err(AuthError::InvalidCredentials);
    }
    Ok(value)
}

fn payload(
    method: &str,
    path: &str,
    body: SignedBody<'_>,
    timestamp: DateTime<Utc>,
    nonce: &str,
) -> Result<String, AuthError> {
    if !component(method) || path.is_empty() || path.contains(['\r', '\n']) || !component(nonce) {
        return Err(AuthError::InvalidCredentials);
    }
    // TypeScript interpolates the literal "undefined" when bodyString is absent.
    // Preserve this existing wire format rather than silently changing signatures.
    let body = match body {
        SignedBody::Absent | SignedBody::Text("") => "undefined".to_owned(),
        SignedBody::Text(text) => format!("{text}\n"),
        SignedBody::Bytes(bytes) => format!("{}\n", String::from_utf8_lossy(bytes)),
    };
    Ok(format!(
        "{method}\n{path}\n{body}{}\n{nonce}",
        timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
    ))
}

#[cfg(test)]
mod tests;

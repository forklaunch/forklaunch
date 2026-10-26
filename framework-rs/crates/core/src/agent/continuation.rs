//! Sealed approval continuations: everything needed to resume a run, held by
//! the client. AES-256-GCM keeps tool results confidential and makes any change
//! unreadable; nothing is stored on the server, so it works across replicas.
//!
//! Token: `base64url(iv[12] || tag[16] || ciphertext)` with key
//! `SHA-256("forklaunch-agent-continuation:" + HMAC_SECRET_KEY)`, the same
//! layout the TypeScript framework uses.
use super::types::ToolCallRecord;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Largest encoded token.
pub const MAX_CONTINUATION_BYTES: usize = 256 * 1024;
/// Lifetime of a token.
pub const CONTINUATION_TTL_MS: i64 = 15 * 60_000;
const IV: usize = 12;
const TAG: usize = 16;

/// A tool call the model asked for that has not run yet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingToolCall {
    pub id: String,
    pub name: String,
    /// The raw JSON text the model sent.
    pub arguments: String,
}

/// Everything needed to resume a run after the user decides on a tool call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Continuation {
    pub v: u8,
    pub user_id: String,
    /// Unix milliseconds.
    pub expires_at: i64,
    pub steps: u32,
    /// Chat transcript in the gateway's `OpenAI` message shape.
    pub messages: Vec<Value>,
    /// The call awaiting approval first, then any later calls of the same turn.
    pub pending: Vec<PendingToolCall>,
    pub tool_calls: Vec<ToolCallRecord>,
}

/// Sealing failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContinuationError {
    /// The transcript is too long to pause for approval.
    TooLarge,
    Encoding,
}

fn cipher(secret: &str) -> Aes256Gcm {
    let mut hasher = Sha256::new();
    hasher.update(b"forklaunch-agent-continuation:");
    hasher.update(secret.as_bytes());
    let key: [u8; 32] = hasher.finalize().into();
    Aes256Gcm::new(&Key::<Aes256Gcm>::from(key))
}

/// Encrypts a continuation.
/// # Errors
/// Rejects tokens over [`MAX_CONTINUATION_BYTES`].
pub fn seal_continuation(value: &Continuation, secret: &str) -> Result<String, ContinuationError> {
    let plain = serde_json::to_vec(value).map_err(|_| ContinuationError::Encoding)?;
    // base64 of iv+tag+body must fit; reject early before encrypting huge payloads.
    if (plain.len() + IV + TAG).div_ceil(3) * 4 > MAX_CONTINUATION_BYTES {
        return Err(ContinuationError::TooLarge);
    }
    let mut iv = [0_u8; IV];
    getrandom::fill(&mut iv).map_err(|_| ContinuationError::Encoding)?;
    let sealed = cipher(secret)
        .encrypt(&Nonce::from(iv), plain.as_slice())
        .map_err(|_| ContinuationError::Encoding)?;
    // aes-gcm appends the tag; the token carries it before the ciphertext.
    let (body, tag) = sealed.split_at(sealed.len() - TAG);
    let mut raw = Vec::with_capacity(IV + TAG + body.len());
    raw.extend_from_slice(&iv);
    raw.extend_from_slice(tag);
    raw.extend_from_slice(body);
    let token = URL_SAFE_NO_PAD.encode(raw);
    if token.len() > MAX_CONTINUATION_BYTES {
        return Err(ContinuationError::TooLarge);
    }
    Ok(token)
}

/// Opens a token for `user_id` at `now_ms`. Tampered, foreign, expired,
/// oversized or malformed tokens all yield `None`.
#[must_use]
pub fn open_continuation(
    token: &str,
    secret: &str,
    user_id: &str,
    now_ms: i64,
) -> Option<Continuation> {
    if token.len() > MAX_CONTINUATION_BYTES {
        return None;
    }
    let raw = URL_SAFE_NO_PAD.decode(token.trim_end_matches('=')).ok()?;
    if raw.len() < IV + TAG {
        return None;
    }
    let (iv, rest) = raw.split_at(IV);
    let (tag, body) = rest.split_at(TAG);
    let mut sealed = Vec::with_capacity(body.len() + TAG);
    sealed.extend_from_slice(body);
    sealed.extend_from_slice(tag);
    // GCM verifies the tag in constant time before releasing plaintext.
    let plain = cipher(secret)
        .decrypt(&Nonce::try_from(iv).ok()?, sealed.as_slice())
        .ok()?;
    let value: Continuation = serde_json::from_slice(&plain).ok()?;
    (value.v == 1 && value.user_id == user_id && value.expires_at >= now_ms).then_some(value)
}

/// Current Unix time in milliseconds.
#[must_use]
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

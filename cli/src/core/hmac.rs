use anyhow::Result;
use chrono::{SecondsFormat, Utc};
// hmac 0.13 no longer surfaces `new_from_slice` through `Mac`; it comes from
// `KeyInit`, which must now be imported explicitly.
use hmac::{Hmac, KeyInit, Mac};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone)]
pub(crate) enum AuthMode {
    Jwt,
    Hmac { secret_key: String },
}

impl AuthMode {
    /// Detect auth mode from environment. If `FORKLAUNCH_HMAC_SECRET` is set
    /// and non-empty, returns `Hmac`; otherwise returns `Jwt`.
    pub(crate) fn detect() -> Self {
        match std::env::var("FORKLAUNCH_HMAC_SECRET") {
            Ok(secret) if !secret.is_empty() => AuthMode::Hmac { secret_key: secret },
            _ => AuthMode::Jwt,
        }
    }

    pub(crate) fn is_hmac(&self) -> bool {
        matches!(self, AuthMode::Hmac { .. })
    }
}

/// Generate an HMAC Authorization header value.
///
/// The signature message format matches the TypeScript implementation exactly:
///   `${method}\n${path}\n${bodyString}${timestamp.toISOString()}\n${nonce}`
///
/// Where `bodyString` is:
///   - `${safeStringify(body)}\n` when body is present
///   - `undefined` (literal string) when body is absent (JS template literal behavior)
pub(crate) fn generate_hmac_auth_header(
    secret_key: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<String> {
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let nonce = Uuid::new_v4().to_string();

    let body_string = match body {
        Some(b) => format!("{}\n", serde_json::to_string(b)?),
        None => "undefined".to_string(),
    };

    let message = format!(
        "{}\n{}\n{}{}\n{}",
        method, path, body_string, timestamp, nonce
    );

    let mut mac =
        HmacSha256::new_from_slice(secret_key.as_bytes()).expect("HMAC can take key of any size");
    mac.update(message.as_bytes());
    let result = mac.finalize();

    use base64::{Engine as _, engine::general_purpose};
    let signature = general_purpose::STANDARD.encode(result.into_bytes());

    Ok(format!(
        "HMAC keyId=default ts={} nonce={} signature={}",
        timestamp, nonce, signature
    ))
}

#[cfg(test)]
mod tests;

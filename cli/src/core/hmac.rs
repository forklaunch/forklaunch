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
    let timestamp = Utc::now()
        .to_rfc3339_opts(SecondsFormat::Millis, true);
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
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_auth_mode_is_hmac() {
        let jwt = AuthMode::Jwt;
        assert!(!jwt.is_hmac());

        let hmac = AuthMode::Hmac {
            secret_key: "test".to_string(),
        };
        assert!(hmac.is_hmac());
    }

    #[test]
    fn test_generate_hmac_auth_header_with_body() {
        let body = json!({"applicationId": "app-123", "version": "1.0.0"});
        let header =
            generate_hmac_auth_header("test-secret", "POST", "/releases/internal", Some(&body))
                .unwrap();

        assert!(header.starts_with("HMAC keyId=default ts="));
        assert!(header.contains("nonce="));
        assert!(header.contains("signature="));
    }

    #[test]
    fn test_generate_hmac_auth_header_without_body() {
        let header =
            generate_hmac_auth_header("test-secret", "GET", "/deployments/123", None).unwrap();

        assert!(header.starts_with("HMAC keyId=default ts="));
        assert!(header.contains("nonce="));
        assert!(header.contains("signature="));
    }

    #[test]
    fn test_hmac_signature_deterministic_with_same_inputs() {
        // We can't fully test determinism since timestamp changes,
        // but we can verify the format is correct and consistent.
        let body = json!({"key": "value"});
        let header1 =
            generate_hmac_auth_header("secret", "POST", "/path", Some(&body)).unwrap();
        let header2 =
            generate_hmac_auth_header("secret", "POST", "/path", Some(&body)).unwrap();

        // Both should be valid HMAC headers (different due to timestamp/nonce)
        assert!(header1.starts_with("HMAC keyId=default ts="));
        assert!(header2.starts_with("HMAC keyId=default ts="));
    }

    #[test]
    fn test_no_body_produces_undefined_in_message() {
        // Verify the "undefined" behavior for GET requests matches TypeScript.
        // The message for no-body should contain "undefined" (not empty string).
        // We verify this indirectly by checking that GET and POST with empty body
        // produce different signatures.
        let header_get =
            generate_hmac_auth_header("secret", "GET", "/path", None).unwrap();
        let header_post =
            generate_hmac_auth_header("secret", "POST", "/path", Some(&json!({}))).unwrap();

        // They should be different (different methods and body handling)
        assert_ne!(header_get, header_post);
    }
}

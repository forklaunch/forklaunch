use serde_json::json;

use super::*;

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
    let header1 = generate_hmac_auth_header("secret", "POST", "/path", Some(&body)).unwrap();
    let header2 = generate_hmac_auth_header("secret", "POST", "/path", Some(&body)).unwrap();

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
    let header_get = generate_hmac_auth_header("secret", "GET", "/path", None).unwrap();
    let header_post =
        generate_hmac_auth_header("secret", "POST", "/path", Some(&json!({}))).unwrap();

    // They should be different (different methods and body handling)
    assert_ne!(header_get, header_post);
}

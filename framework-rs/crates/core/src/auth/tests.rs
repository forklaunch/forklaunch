use jsonwebtoken::{EncodingKey, Header, encode};

use super::*;

fn future_exp() -> usize {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs();
    usize::try_from(now).unwrap_or(0) + 3600
}

#[test]
fn hs256_time_claims_match_jose_and_empty_keys_fail_closed() {
    let now = jsonwebtoken::get_current_timestamp();
    let secret = b"test-only-signing-key";
    let sign = |claims: &serde_json::Value, key: &[u8]| {
        encode(
            &Header::new(Algorithm::HS256),
            claims,
            &EncodingKey::from_secret(key),
        )
        .unwrap()
    };
    for claims in [
        serde_json::json!({"sub":"tester","exp":now}),
        serde_json::json!({"sub":"tester","exp":now-30}),
        serde_json::json!({"sub":"tester","exp":now+7200,"nbf":now+3600}),
        serde_json::json!({"sub":"tester","exp":now+7200,"nbf":null}),
        serde_json::json!({"sub":"tester","exp":now+7200,"nbf":"0"}),
        serde_json::json!({"sub":"tester","exp":now+7200,"iat":null}),
        serde_json::json!({"sub":"tester","exp":now+7200,"iat":"0"}),
    ] {
        assert!(
            verify_jwt_hs256(&sign(&claims, secret), secret).is_err(),
            "{claims}"
        );
    }
    let claims = serde_json::json!({"sub":"tester","exp":now+7200,"nbf":0.5,"iat":now+3600});
    assert!(verify_jwt_hs256(&sign(&claims, secret), secret).is_ok());
    assert!(verify_jwt_hs256(&sign(&claims, b""), b"").is_err());
}

#[test]
fn typed_jwt_issuance_preserves_grants_and_rejects_invalid_inputs() {
    let mut claims = JwtClaims {
        sub: "issuer-test".into(),
        exp: 4_102_444_800,
        roles: vec!["admin".into()],
        permissions: vec!["users:read".into()],
        scopes: vec!["profile".into()],
        features: vec!["reports".into()],
        active_subscription: true,
    };
    let token = issue_jwt_hs256(&claims, b"test-only-signing-key").unwrap();
    let principal = verify_jwt_hs256(&token, b"test-only-signing-key").unwrap();
    assert_eq!(principal, Principal::from_claims(claims.clone()));
    assert!(verify_jwt_hs256(&token, b"different-key").is_err());
    claims.exp = 1;
    let expired = issue_jwt_hs256(&claims, b"test-only-signing-key").unwrap();
    assert!(verify_jwt_hs256(&expired, b"test-only-signing-key").is_err());
    assert!(issue_jwt_hs256(&claims, b"").is_err());
    claims.sub.clear();
    assert!(issue_jwt_hs256(&claims, b"test-only-signing-key").is_err());
}

#[test]
fn hmac_round_trips_and_rejects_tampering() {
    let secret = b"super-secret";
    let signature = hmac_sign(secret, b"payload");

    assert!(hmac_verify(secret, b"payload", &signature));
    assert!(!hmac_verify(secret, b"tampered", &signature));
    assert!(!hmac_verify(b"wrong-secret", b"payload", &signature));
    assert!(!hmac_verify(secret, b"payload", "not-base64!!"));
}

#[test]
fn jwt_verifies_valid_tokens_and_rejects_wrong_keys() {
    let secret = b"jwt-signing-key";
    let claims = JwtClaims {
        sub: "user-1".to_owned(),
        exp: future_exp(),
        roles: vec!["admin".to_owned()],
        permissions: vec!["users:read".to_owned()],
        scopes: vec!["org".to_owned()],
        features: vec!["beta".to_owned()],
        active_subscription: true,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .expect("token should encode");

    let principal = verify_jwt_hs256(&token, secret).expect("token should verify");
    assert_eq!(principal.subject, "user-1");
    assert_eq!(principal.roles, vec!["admin".to_owned()]);
    assert!(principal.active_subscription);

    assert_eq!(
        verify_jwt_hs256(&token, b"other-key"),
        Err(AuthError::InvalidCredentials)
    );
}

#[test]
fn parses_authorization_headers() {
    assert_eq!(parse_bearer("Bearer abc.def"), Some("abc.def"));
    assert_eq!(parse_bearer("bearer  token "), Some("token"));
    assert_eq!(parse_bearer("Basic xyz"), None);

    // base64 of "user:pass"
    assert_eq!(
        parse_basic("Basic dXNlcjpwYXNz"),
        Some(("user".to_owned(), "pass".to_owned()))
    );
}

#[test]
fn rbac_enforces_permissions_roles_and_subscription() {
    let principal = Principal {
        subject: "user-1".to_owned(),
        roles: vec!["member".to_owned()],
        permissions: vec!["users:read".to_owned()],
        scopes: vec!["org".to_owned()],
        features: vec!["beta".to_owned()],
        active_subscription: true,
    };

    let allow = RbacConfig {
        allowed_permissions: vec!["users:read".to_owned()],
        allowed_roles: vec!["member".to_owned()],
        required_features: vec!["beta".to_owned()],
        require_active_subscription: true,
        ..RbacConfig::default()
    };
    assert!(authorize(&principal, &allow).is_ok());

    let forbidden_permission = RbacConfig {
        forbidden_permissions: vec!["users:read".to_owned()],
        ..RbacConfig::default()
    };
    assert!(authorize(&principal, &forbidden_permission).is_err());

    let missing_feature = RbacConfig {
        required_features: vec!["enterprise".to_owned()],
        ..RbacConfig::default()
    };
    assert!(authorize(&principal, &missing_feature).is_err());

    let needs_subscription = RbacConfig {
        require_active_subscription: true,
        ..RbacConfig::default()
    };
    let mut unsubscribed = principal.clone();
    unsubscribed.active_subscription = false;
    assert!(authorize(&unsubscribed, &needs_subscription).is_err());
}

#[test]
fn scope_hierarchy_allows_higher_privilege_scopes() {
    let hierarchy = vec!["admin".to_owned(), "org".to_owned(), "team".to_owned()];

    let admin = Principal {
        scopes: vec!["admin".to_owned()],
        ..Principal::new("admin-user")
    };
    let team = Principal {
        scopes: vec!["team".to_owned()],
        ..Principal::new("team-user")
    };

    assert!(scope_satisfied(&admin, "team", &hierarchy));
    assert!(!scope_satisfied(&team, "org", &hierarchy));
    assert!(scope_satisfied(&team, "team", &hierarchy));
}

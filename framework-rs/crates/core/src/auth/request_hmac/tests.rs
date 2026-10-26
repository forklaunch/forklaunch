use super::*;
const SECRET: &[u8] = b"request-hmac-test-secret";
const TIMESTAMP: &str = "2026-10-10T10:00:00.000Z";

#[test]
fn signature_binds_every_request_component_and_freshness_bounds() {
    let header = generate_hmac_auth_header(
        SECRET,
        "default",
        "POST",
        "/items",
        SignedBody::Text("{}"),
        TIMESTAMP,
        "nonce-1",
    )
    .unwrap();
    let credential = HmacAuthorization::parse(&header).unwrap();
    assert_eq!(credential.key_id(), "default");
    assert_eq!(credential.nonce(), "nonce-1");
    assert!(!format!("{credential:?}").contains("signature="));
    assert!(credential.verify(SECRET, "POST", "/items", SignedBody::Bytes(b"{}")));
    for (secret, method, path, body) in [
        (b"wrong".as_slice(), "POST", "/items", "{}"),
        (SECRET, "GET", "/items", "{}"),
        (SECRET, "POST", "/other", "{}"),
        (SECRET, "POST", "/items", "{ }"),
    ] {
        assert!(!credential.verify(secret, method, path, SignedBody::Text(body)));
    }
    let now = credential.timestamp_unix_millis();
    for delta in [-300_000, 0, 300_000] {
        assert!(credential.verify_at(
            SECRET,
            "POST",
            "/items",
            SignedBody::Text("{}"),
            now + delta,
            Duration::from_secs(300)
        ));
    }
    for delta in [-300_001, 300_001] {
        assert!(!credential.verify_at(
            SECRET,
            "POST",
            "/items",
            SignedBody::Text("{}"),
            now + delta,
            Duration::from_secs(300)
        ));
    }
}

#[test]
fn malformed_headers_and_newline_injection_fail_closed() {
    let header = generate_hmac_auth_header(
        SECRET,
        "default",
        "GET",
        "/",
        SignedBody::Absent,
        TIMESTAMP,
        "nonce",
    )
    .unwrap();
    for invalid in [
        format!("{header} extra=1"),
        header.replace("keyId=", "id="),
        header.replace("HMAC ", "HMAC  "),
        header.replace("ts=2026", "ts=invalid2026"),
        header.replace("nonce=nonce", "nonce="),
        header.replace("HMAC", "Basic"),
    ] {
        assert!(HmacAuthorization::parse(&invalid).is_err());
    }
    assert!(
        create_hmac_token(
            SECRET,
            "GET",
            "/\nother",
            SignedBody::Absent,
            TIMESTAMP,
            "nonce"
        )
        .is_err()
    );
    assert!(
        create_hmac_token(
            SECRET,
            "GET",
            "/",
            SignedBody::Absent,
            "2026-12-31T23:59:60Z",
            "nonce"
        )
        .is_err()
    );
    assert_ne!(
        create_hmac_token(SECRET, "GET", "/", SignedBody::Absent, TIMESTAMP, "n").unwrap(),
        create_hmac_token(SECRET, "GET", "/", SignedBody::Bytes(b""), TIMESTAMP, "n").unwrap()
    );
}

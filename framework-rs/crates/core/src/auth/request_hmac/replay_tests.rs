use super::*;
#[tokio::test]
async fn verifies_before_atomic_nonce_claim_and_rejects_replays_or_store_failure() {
    let secret = [7; 32];
    let timestamp = "2026-10-15T00:00:00.000Z";
    let now = DateTime::parse_from_rfc3339(timestamp)
        .unwrap()
        .timestamp_millis();
    let header = generate_hmac_auth_header(
        &secret,
        "worker",
        "POST",
        "/callback",
        SignedBody::Text("encrypted"),
        timestamp,
        "nonce",
    )
    .unwrap();
    let used = std::sync::atomic::AtomicBool::new(false);
    for expected in [true, false] {
        let result = verify_with_nonce_store(
            &header,
            &secret,
            "worker",
            "POST",
            "/callback",
            SignedBody::Text("encrypted"),
            now,
            Duration::from_secs(60),
            |key, expiry| {
                assert_eq!(key, "6:workernonce");
                assert_eq!(expiry, now + 60_000);
                std::future::ready(Ok::<_, ()>(
                    !used.swap(true, std::sync::atomic::Ordering::SeqCst),
                ))
            },
        )
        .await;
        assert_eq!(result.is_ok(), expected);
    }
    for (body, clock) in [
        ("tampered", now),
        ("encrypted", now + 60_001),
        ("encrypted", now - 60_001),
    ] {
        assert!(
            verify_with_nonce_store(
                &header,
                &secret,
                "worker",
                "POST",
                "/callback",
                SignedBody::Text(body),
                clock,
                Duration::from_secs(60),
                |_, _| {
                    panic!("invalid signature consumed a nonce");
                    #[allow(unreachable_code)]
                    std::future::ready(Ok::<_, ()>(true))
                }
            )
            .await
            .is_err()
        );
    }
    assert!(
        verify_with_nonce_store(
            &header,
            &secret,
            "worker",
            "POST",
            "/callback",
            SignedBody::Text("encrypted"),
            now,
            Duration::from_secs(60),
            |_, _| std::future::ready(Err::<bool, _>("store unavailable"))
        )
        .await
        .is_err()
    );
}

#[test]
fn in_memory_store_claims_once_until_expiry_and_stays_bounded() {
    let store = InMemoryNonceStore::with_max_entries(3);
    assert!(store.claim("a", 1_000, 0));
    assert!(!store.claim("a", 1_000, 500));
    assert!(store.claim("b", 1_000, 500));
    // Expired: the timestamp it belonged to can no longer verify anyway.
    assert!(store.claim("a", 2_000, 1_000));
    assert_eq!(store.len(), 1);
    for key in ["c", "d", "e", "f"] {
        assert!(store.claim(key, 60_000, 1_000));
        assert!(store.len() <= 3);
    }
    // The longest-lived are still remembered; the soonest to expire made room.
    assert!(!store.claim("f", 60_000, 1_001));
    assert!(!store.claim("e", 60_000, 1_001));
    assert!(store.claim("z", 120_000, 60_000));
    assert_eq!(store.len(), 1);
    assert!(InMemoryNonceStore::default().is_empty());
}

#[test]
fn nonce_claim_is_scoped_to_the_key_and_expires_with_the_signature() {
    let timestamp = "2026-10-15T00:00:00.000Z";
    let signed_at = DateTime::parse_from_rfc3339(timestamp)
        .unwrap()
        .timestamp_millis();
    let header = generate_hmac_auth_header(
        &[7; 32],
        "worker",
        "POST",
        "/callback",
        SignedBody::Absent,
        timestamp,
        "nonce",
    )
    .unwrap();
    let auth = HmacAuthorization::parse(&header).unwrap();
    assert_eq!(
        auth.nonce_claim(DEFAULT_MAX_SKEW).unwrap(),
        ("6:workernonce".to_owned(), signed_at + 300_000)
    );
}

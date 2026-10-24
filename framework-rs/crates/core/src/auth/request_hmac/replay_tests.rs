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

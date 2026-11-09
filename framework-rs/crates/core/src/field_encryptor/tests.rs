use super::*;
/// Deterministic envelopes are TypeScript-compatible ciphertexts: their bytes
/// must never change for a given key, tenant and plaintext.
#[test]
fn deterministic_envelopes_are_byte_stable() {
    let v2 = FieldEncryptor::new("golden-test-key").unwrap();
    assert_eq!(
        v2.encrypt("private value", "tenant-a").unwrap(),
        "v2:Yjsj9ylsnaq8Z+20:KSrAIhyO0hKdUoT2vphlUg==:bCyKoNNuE/8ildgrGw=="
    );
    let v3 = FieldEncryptor::new("golden-test-key")
        .unwrap()
        .with_format(CiphertextFormat::V3);
    assert_eq!(
        v3.encrypt("private value", "tenant-a").unwrap(),
        "v3:ae4f1c0385e5:Yjsj9ylsnaq8Z+20:KSrAIhyO0hKdUoT2vphlUg==:bCyKoNNuE/8ildgrGw=="
    );
}
#[test]
fn envelopes_are_authenticated_tenant_bound_rotatable_and_random_when_sealed() {
    let old = FieldEncryptor::new("old-test-key").unwrap();
    let new = FieldEncryptor::new("new-test-key")
        .unwrap()
        .with_previous_keys(&["old-test-key".to_owned()])
        .unwrap()
        .with_format(CiphertextFormat::V3);
    for ciphertext in [
        old.encrypt("private", "tenant-a").unwrap(),
        old.seal("private", "tenant-a", "anonymous").unwrap(),
    ] {
        let opened = new.open(&ciphertext, "tenant-a").unwrap();
        assert_eq!(opened.plaintext, "private");
        assert!(opened.stale);
        assert!(!opened.current);
        assert!(new.decrypt(&ciphertext, "tenant-b").is_err());
        let rotated = new.rotate(&ciphertext, "tenant-a").unwrap();
        assert!(new.open(&rotated, "tenant-a").unwrap().current);
        assert!(!new.needs_rotation(&rotated, "tenant-a").unwrap());
        assert_eq!(new.rotate(&rotated, "tenant-a").unwrap(), rotated);
    }
    assert_eq!(
        old.encrypt("private", "tenant-a").unwrap(),
        old.encrypt("private", "tenant-a").unwrap()
    );
    assert_ne!(
        old.seal("private", "tenant-a", "anon").unwrap(),
        old.seal("private", "tenant-a", "anon").unwrap()
    );
    assert_ne!(
        old.blind_index("private", "tenant-a").unwrap(),
        old.blind_index("private", "tenant-b").unwrap()
    );
    assert!(
        new.blind_indexes("private", "tenant-a")
            .unwrap()
            .contains(&old.blind_index("private", "tenant-a").unwrap())
    );
    for malformed in [
        "plain",
        "v2:bad:bad:bad",
        "v3:unknown:AAAA:AAAA:AAAA",
        "v4:unknown:bad:bad:bad:bad",
    ] {
        assert!(new.decrypt(malformed, "tenant-a").is_err());
    }
}

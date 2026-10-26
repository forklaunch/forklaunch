use super::*;
#[test]
fn salted_hashes_are_distinct_and_reject_wrong_passwords_and_bad_encodings() {
    let first = hash_password("correct-password").unwrap();
    let second = hash_password("correct-password").unwrap();
    assert!(first.starts_with("$argon2id$"));
    assert_ne!(first, second);
    assert!(!first.contains("correct-password"));
    assert!(verify_password("correct-password", &first));
    assert!(!verify_password("wrong", &first));
    assert!(!verify_password("correct-password", "correct-password"));
    assert!(!verify_password("correct-password", "$argon2id$malformed"));
}

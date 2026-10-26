#![cfg(feature = "encryption")]
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
use forklaunch_core::{
    common::FrameworkResult,
    persistence::{compliance::ComplianceLevel, protected::*},
};
use uuid::Uuid;
fn key() -> EnvelopeKey {
    EnvelopeKey::with_rotation(
        EnvelopeProfile::new("projection-test").unwrap(),
        "active".into(),
        [7; 32],
        vec![],
        None,
    )
    .unwrap()
}
fn context() -> ProtectedContext {
    ProtectedContext::new(Uuid::from_u128(1), Uuid::from_u128(2)).unwrap()
}
fn member(policy: DisclosurePolicy) -> ProtectedMember {
    ProtectedMember::new("Contact", "value", "contact-value", ComplianceLevel::Pii)
        .unwrap()
        .with_disclosure(policy)
}
struct Audit;
impl ComplianceAudit for Audit {
    fn record(&self, _: &ComplianceAccess<'_>) -> FrameworkResult<()> {
        Ok(())
    }
}
#[test]
fn explicit_projections_match_supported_ts_strategies_and_default_never_discloses() {
    let key = key();
    for (policy, input, expected) in [
        (DisclosurePolicy::Redact, "private-canary", "[redacted]"),
        (DisclosurePolicy::Year, "born 1999-12-30", "1999"),
        (DisclosurePolicy::Year, "no-date", "[redacted]"),
        (DisclosurePolicy::Zip3, "036-99", "000"),
        (DisclosurePolicy::Zip3, "94107", "941"),
        (DisclosurePolicy::Zip3, "12", "000"),
        (DisclosurePolicy::Last4, "4111 1111 1111 1234", "•••• 1234"),
        (DisclosurePolicy::Last4, "123", "[redacted]"),
    ] {
        let protected =
            ProtectedField::seal(&key, context(), member(policy), input.as_bytes()).unwrap();
        assert_eq!(protected.anonymous().as_str(), "[redacted]");
        let loaded =
            ProtectedField::from_ciphertext(member(policy), protected.ciphertext().to_owned())
                .unwrap();
        let projection = loaded.disclosed(&key, context()).unwrap();
        assert_eq!(projection.as_str(), expected);
        assert_eq!(projection.policy(), policy);
        assert_eq!(serde_json::to_string(&loaded).unwrap(), "{}");
        assert_eq!(format!("{projection:?}"), "DisclosureValue([redacted])");
        assert_eq!(
            loaded
                .reveal(&key, context(), "user", "read", &Audit)
                .unwrap()
                .as_slice(),
            input.as_bytes()
        );
    }
}
#[test]
fn random_tokens_are_stored_independent_and_authenticated_without_decryption() {
    let key = key();
    let policy = DisclosurePolicy::RandomToken;
    let first =
        ProtectedField::seal(&key, context(), member(policy), b"same-private-value").unwrap();
    let second =
        ProtectedField::seal(&key, context(), member(policy), b"same-private-value").unwrap();
    let token = first.disclosed(&key, context()).unwrap();
    assert!(token.as_str().starts_with("tok_"));
    assert_eq!(token.as_str().len(), 26);
    assert_ne!(
        token.as_str(),
        second.disclosed(&key, context()).unwrap().as_str()
    );
    let loaded =
        ProtectedField::from_ciphertext(member(policy), first.ciphertext().into()).unwrap();
    assert_eq!(
        token.as_str(),
        loaded.disclosed(&key, context()).unwrap().as_str()
    );
    for wrong in [
        ProtectedContext::new(Uuid::from_u128(3), Uuid::from_u128(2)).unwrap(),
        ProtectedContext::new(Uuid::from_u128(1), Uuid::from_u128(3)).unwrap(),
    ] {
        assert!(loaded.disclosed(&key, wrong).is_err());
    }
    let different = ProtectedMember::new("Contact", "other", "contact-value", ComplianceLevel::Pii)
        .unwrap()
        .with_disclosure(policy);
    assert!(
        ProtectedField::from_ciphertext(different, first.ciphertext().into())
            .unwrap()
            .disclosed(&key, context())
            .is_err()
    );
    let bytes = URL_SAFE_NO_PAD
        .decode(first.ciphertext().strip_prefix("pd1.").unwrap())
        .unwrap();
    let original: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    for field in ["value", "tag", "ciphertext"] {
        let mut changed = original.clone();
        changed[field] = format!("{}x", changed[field].as_str().unwrap()).into();
        let wire = format!(
            "pd1.{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&changed).unwrap())
        );
        let tampered = ProtectedField::from_ciphertext(member(policy), wire).unwrap();
        assert!(tampered.disclosed(&key, context()).is_err());
        assert!(
            tampered
                .reveal(&key, context(), "user", "read", &Audit)
                .is_err()
        );
    }
    let legacy =
        ProtectedField::seal(&key, context(), member(DisclosurePolicy::Redact), b"legacy").unwrap();
    let absent =
        ProtectedField::from_ciphertext(member(policy), legacy.ciphertext().into()).unwrap();
    assert_eq!(
        absent.disclosed(&key, context()).unwrap_err().code(),
        "protected_disclosure_migration_required"
    );
}

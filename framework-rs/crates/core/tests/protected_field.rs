#![cfg(feature = "encryption")]
use forklaunch_core::encryption::{EnvelopeKey, EnvelopeProfile};
use forklaunch_core::{
    common::FrameworkError,
    persistence::{compliance::ComplianceLevel, protected::*},
};
use std::cell::RefCell;
use uuid::Uuid;

#[derive(Default)]
struct Audit(RefCell<Vec<String>>);
impl ComplianceAudit for Audit {
    fn record(&self, event: &ComplianceAccess<'_>) -> Result<(), FrameworkError> {
        self.0.borrow_mut().push(format!("{event:?}"));
        Ok(())
    }
}
struct FailedAudit;
impl ComplianceAudit for FailedAudit {
    fn record(&self, _: &ComplianceAccess<'_>) -> Result<(), FrameworkError> {
        Err(FrameworkError::new(
            "unsafe_sink_error",
            "do not propagate audit implementation details",
        ))
    }
}
fn key(byte: u8) -> EnvelopeKey {
    EnvelopeKey::new(EnvelopeProfile::new("test").unwrap(), [byte; 32])
}
fn context(tenant: u128, resource: u128) -> ProtectedContext {
    ProtectedContext::new(Uuid::from_u128(tenant), Uuid::from_u128(resource)).unwrap()
}
fn member(purpose: &'static str) -> ProtectedMember {
    ProtectedMember::new("Note", "body", purpose, ComplianceLevel::Pii).unwrap()
}

#[test]
fn every_reveal_is_audited_and_no_plaintext_is_available_implicitly() {
    let key = key(7);
    let field = ProtectedField::seal(&key, context(1, 2), member("body"), b"secret-value").unwrap();
    assert_eq!(serde_json::to_string(&field).unwrap(), "{}");
    assert_eq!(format!("{field:?}"), "ProtectedField([redacted])");
    assert!(!field.ciphertext().contains("secret-value"));
    let loaded =
        ProtectedField::from_ciphertext(member("body"), field.ciphertext().into()).unwrap();
    let audit = Audit::default();
    for _ in 0..2 {
        assert_eq!(
            &*loaded
                .reveal(&key, context(1, 2), "actor", "read-note", &audit)
                .unwrap(),
            b"secret-value"
        );
    }
    assert_eq!(audit.0.borrow().len(), 2);
    assert!(!audit.0.borrow()[0].contains("secret-value"));
    assert!(
        loaded
            .reveal(&key, context(1, 2), "actor", "read-note", &FailedAudit)
            .is_err()
    );
}

#[test]
fn wrong_tenant_resource_purpose_keys_and_unclassified_data_fail_closed() {
    let key = key(7);
    let field = ProtectedField::seal(&key, context(1, 2), member("body"), b"secret-value").unwrap();
    let audit = Audit::default();
    assert!(
        field
            .reveal(&key, context(3, 2), "actor", "read", &audit)
            .is_err()
    );
    assert!(
        field
            .reveal(&key, context(1, 3), "actor", "read", &audit)
            .is_err()
    );
    assert!(
        field
            .reveal(&self::key(8), context(1, 2), "actor", "read", &audit)
            .is_err()
    );
    assert!(
        field
            .reveal(&key, context(1, 2), "", "read", &audit)
            .is_err()
    );
    let other =
        ProtectedField::from_ciphertext(member("title"), field.ciphertext().into()).unwrap();
    assert!(
        other
            .reveal(&key, context(1, 2), "actor", "read", &audit)
            .is_err()
    );
    assert!(audit.0.borrow().is_empty());
    assert!(ProtectedMember::new("Note", "body", "body", ComplianceLevel::None).is_err());
    assert!(ProtectedContext::new(Uuid::nil(), Uuid::from_u128(1)).is_err());
    assert!(ProtectedField::from_ciphertext(member("body"), "plaintext".into()).is_err());
}

#[test]
fn retained_envelopes_open_with_rotation_and_tampering_never_releases_plaintext() {
    let old = key(7);
    let active = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("test").unwrap(),
        "active".into(),
        [8; 32],
        vec![],
        Some([7; 32]),
    )
    .unwrap();
    let stored = ProtectedField::seal(&old, context(1, 2), member("body"), b"secret").unwrap();
    let audit = Audit::default();
    assert_eq!(
        &*stored
            .reveal(&active, context(1, 2), "actor", "read", &audit)
            .unwrap(),
        b"secret"
    );
    let current = ProtectedField::seal(&active, context(1, 2), member("body"), b"secret").unwrap();
    let mut corrupt = current.ciphertext().to_owned();
    // Mutate a middle ciphertext byte, not unencoded trailing padding bits.
    let position = corrupt.rfind('.').unwrap() + 2;
    corrupt.replace_range(
        position..=position,
        if &corrupt[position..=position] == "A" {
            "B"
        } else {
            "A"
        },
    );
    let corrupt = ProtectedField::from_ciphertext(member("body"), corrupt).unwrap();
    assert!(
        corrupt
            .reveal(&active, context(1, 2), "actor", "read", &audit)
            .is_err()
    );
    assert_eq!(audit.0.borrow().len(), 1);
}

#[test]
fn protected_member_resolves_entity_classification_instead_of_defaulting() {
    use forklaunch_core::persistence::compliance::ComplianceMetadata;
    #[allow(dead_code)]
    #[derive(ComplianceMetadata)]
    struct Example {
        #[compliance(level = "none")]
        id: String,
        #[compliance(level = "phi")]
        diagnosis: String,
    }
    assert!(ProtectedMember::from_entity::<Example>("diagnosis", "diagnosis").is_ok());
    assert!(ProtectedMember::from_entity::<Example>("id", "id").is_err());
    assert!(ProtectedMember::from_entity::<Example>("missing", "missing").is_err());
}

#[test]
fn asynchronous_audit_must_acknowledge_before_plaintext_is_released() {
    use std::{
        future::Future,
        pin::Pin,
        sync::atomic::{AtomicU8, Ordering},
        task::{Context, Poll, Waker},
    };
    struct Sink(AtomicU8);
    impl AsyncComplianceAudit for Sink {
        fn record<'a>(
            &'a self,
            _: ComplianceAccess<'a>,
        ) -> Pin<Box<dyn Future<Output = Result<(), FrameworkError>> + Send + 'a>> {
            Box::pin(std::future::poll_fn(move |_| {
                match self.0.load(Ordering::SeqCst) {
                    0 => Poll::Pending,
                    1 => Poll::Ready(Ok(())),
                    _ => Poll::Ready(Err(FrameworkError::new(
                        "sink_failed",
                        "private sink detail",
                    ))),
                }
            }))
        }
    }
    let key = key(7);
    let field = ProtectedField::seal(&key, context(1, 2), member("body"), b"private").unwrap();
    let sink = Sink(AtomicU8::new(0));
    let mut reveal = Box::pin(field.reveal_async(&key, context(1, 2), "actor", "read", &sink));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(reveal.as_mut().poll(&mut cx).is_pending());
    sink.0.store(1, Ordering::SeqCst);
    let Poll::Ready(Ok(plaintext)) = reveal.as_mut().poll(&mut cx) else {
        panic!("acknowledged audit must release plaintext")
    };
    assert_eq!(&*plaintext, b"private");
    sink.0.store(2, Ordering::SeqCst);
    let mut denied = Box::pin(field.reveal_async(&key, context(1, 2), "actor", "read", &sink));
    assert!(matches!(denied.as_mut().poll(&mut cx), Poll::Ready(Err(_))));
}

#[test]
fn equality_indexes_are_opt_in_tenant_member_scoped_and_rotation_aware() {
    let old = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("test").unwrap(),
        "old".into(),
        [7; 32],
        vec![],
        None,
    )
    .unwrap();
    let key = EnvelopeKey::with_rotation(
        EnvelopeProfile::new("test").unwrap(),
        "new".into(),
        [8; 32],
        vec![("old".into(), [7; 32])],
        None,
    )
    .unwrap();
    let configured = member("email").with_equality_index(IndexNormalization::TrimLowercase);
    let stored = ProtectedField::seal(
        &old,
        context(1, 2),
        configured.clone(),
        b" Alice@Example.test ",
    )
    .unwrap();
    let indexes = configured
        .query(
            &key,
            Uuid::from_u128(1),
            ProtectedQuery::Equal("alice@example.test"),
        )
        .unwrap();
    assert!(
        indexes
            .values()
            .iter()
            .any(|v| Some(v.as_str()) == stored.blind_index())
    );
    assert_eq!(indexes.values().len(), 2);
    let current = ProtectedField::seal(
        &key,
        context(1, 3),
        configured.clone(),
        b"alice@example.test",
    )
    .unwrap();
    assert_eq!(current.blind_index(), Some(indexes.values()[0].as_str()));
    assert_ne!(
        configured
            .query(
                &key,
                Uuid::from_u128(2),
                ProtectedQuery::Equal("alice@example.test")
            )
            .unwrap()
            .values(),
        indexes.values()
    );
    assert_ne!(
        member("other")
            .with_equality_index(IndexNormalization::TrimLowercase)
            .query(
                &key,
                Uuid::from_u128(1),
                ProtectedQuery::Equal("alice@example.test")
            )
            .unwrap()
            .values(),
        indexes.values()
    );
    for operation in [
        ProtectedQuery::Range,
        ProtectedQuery::Order,
        ProtectedQuery::NotEqual("alice@example.test"),
    ] {
        assert!(
            configured
                .query(&key, Uuid::from_u128(1), operation)
                .is_err()
        );
    }
    assert!(
        member("email")
            .query(
                &key,
                Uuid::from_u128(1),
                ProtectedQuery::Equal("alice@example.test")
            )
            .is_err()
    );
    assert_eq!(
        serde_json::to_string(&current.anonymous()).unwrap(),
        "\"[redacted]\""
    );
    assert_eq!(current.anonymous().as_str(), "[redacted]");
}

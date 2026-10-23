#![cfg(feature = "encryption")]
use forklaunch_core as renamed_core;
use renamed_core::{
    encryption::{EnvelopeKey, EnvelopeProfile},
    persistence::{
        compliance::{ComplianceEntity, ComplianceMetadata},
        protected::{ProtectedContext, ProtectedField, storage::ProtectedModel},
    },
};
use uuid::Uuid;

type Ciphertext = ProtectedField;
#[derive(Debug, serde::Serialize, ComplianceMetadata)]
#[compliance(entity = "StoredNote", crate_path = "renamed_core", persisted)]
struct StoredNote {
    #[compliance(level = "none")]
    id: String,
    #[compliance(level = "pii", name = "title_ciphertext", purpose = "note-title")]
    title: Ciphertext,
    #[compliance(level = "pii", nullable, purpose = "note-insight")]
    insight: Option<ProtectedField>,
}
#[allow(dead_code)]
#[derive(ComplianceMetadata)]
#[compliance(crate_path = "renamed_core")]
struct Descriptor {
    #[compliance(level = "pii")]
    title: String,
}

#[test]
fn actual_ciphertext_aliases_bind_policy_and_reject_member_substitution() {
    let key = EnvelopeKey::new(EnvelopeProfile::new("stored-model").unwrap(), [4; 32]);
    let id = Uuid::new_v4();
    let context = ProtectedContext::new(Uuid::new_v4(), id).unwrap();
    let seal =
        |member| ProtectedField::seal(&key, context, member, b"private-model-canary").unwrap();
    let mut note = StoredNote {
        id: id.to_string(),
        title: seal(StoredNote::protected_title().unwrap()),
        insight: None,
    };
    note.validate_protected_storage().unwrap();
    assert!(!format!("{note:?}").contains("private-model-canary"));
    assert!(
        !serde_json::to_string(&note)
            .unwrap()
            .contains("private-model-canary")
    );
    note.insight = Some(seal(StoredNote::protected_title().unwrap()));
    assert!(note.validate_protected_storage().is_err());
    note.insight = Some(seal(StoredNote::protected_insight().unwrap()));
    note.validate_protected_storage().unwrap();
    note.title = ProtectedField::from_ciphertext(
        StoredNote::protected_insight().unwrap(),
        note.title.ciphertext().to_owned(),
    )
    .unwrap();
    assert!(note.validate_protected_storage().is_err());
}

#[test]
fn artifact_distinguishes_persisted_bindings_from_plain_declarations() {
    assert!(Descriptor::PROTECTED_MEMBERS.is_none());
    assert_eq!(StoredNote::PROTECTED_MEMBERS.unwrap().len(), 2);
    let stored = ComplianceEntity::of::<StoredNote>("notes", "postgres");
    assert_eq!(stored.fields[1].storage_name, "title_ciphertext");
    assert_eq!(stored.protected_members.unwrap()[0].purpose, "note-title");
    #[cfg(feature = "compliance-artifact")]
    {
        let artifact = renamed_core::persistence::compliance::compliance_manifest(&[
            stored,
            ComplianceEntity::of::<Descriptor>("descriptor", "declaration"),
        ]);
        assert_eq!(
            artifact["entities"][0]["binding"]["kind"],
            "protected-model"
        );
        assert_eq!(
            artifact["entities"][1]["binding"]["kind"],
            "declarations-only"
        );
        assert_eq!(
            artifact["runtimeEnforcement"],
            "requires-independent-verification"
        );
    }
}

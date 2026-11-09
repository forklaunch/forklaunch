use forklaunch_core as renamed_core;
use renamed_core::persistence::compliance::{ComplianceField, ComplianceLevel, ComplianceMetadata};

#[allow(dead_code)]
#[derive(ComplianceMetadata)]
#[compliance(entity = "Customer", crate_path = "renamed_core")]
struct Customer<'a, T, const N: usize>
where
    T: Clone,
{
    #[compliance(level = "none")]
    id: &'a str,
    #[compliance(level = "pii", name = "email_address", nullable)]
    email: Option<T>,
    #[compliance(level = "phi")]
    diagnosis: [u8; N],
    #[compliance(level = "pci", nullable)]
    card: Option<String>,
    #[compliance(level = "none", nullable)]
    r#type: Option<String>,
    #[compliance(level = "pii")]
    optional_in_rust_only: Option<String>,
}

#[test]
fn explicit_classification_controls_anonymization_independently_of_rust_types() {
    type Entity = Customer<'static, String, 4>;
    assert_eq!(Entity::ENTITY, "Customer");
    let fields = Entity::FIELDS;
    assert_eq!(fields.len(), 6);
    assert_eq!(fields[0].level, ComplianceLevel::None);
    assert_eq!(fields[1].rust_name, "email");
    assert_eq!(fields[1].storage_name, "email_address");
    assert_eq!(fields[1].level, ComplianceLevel::Pii);
    assert_eq!(fields[2].level, ComplianceLevel::Phi);
    assert_eq!(fields[3].level, ComplianceLevel::Pci);
    assert_eq!(fields[4].rust_name, "type");
    assert_eq!(
        fields
            .iter()
            .map(ComplianceField::can_anonymize)
            .collect::<Vec<_>>(),
        [false, true, false, true, false, false]
    );
}

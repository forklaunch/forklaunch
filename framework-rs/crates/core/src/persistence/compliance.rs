//! Explicit entity metadata for application-owned compliance adapters.
//!
//! Classification does not enable encryption or authorize data access. Storage
//! adapters must verify these declarations against their schema and enforce
//! tenant boundaries. Every field requires an explicit classification; nullability
//! is explicit because Rust types and database nullability need not agree.
//!
//! ```
//! use forklaunch_core::persistence::compliance::{ComplianceLevel, ComplianceMetadata};
//! #[derive(ComplianceMetadata)]
//! #[compliance(entity = "Contact")]
//! struct Contact {
//!     #[compliance(level = "none")]
//!     id: String,
//!     #[compliance(level = "pii", nullable, name = "email_address")]
//!     email: Option<String>,
//! }
//! let field = &Contact::FIELDS[1];
//! assert_eq!(Contact::ENTITY, "Contact");
//! assert_eq!(field.storage_name, "email_address");
//! assert_eq!(field.level, ComplianceLevel::Pii);
//! assert!(field.can_anonymize());
//! ```
//!
//! Omitting a classification is a compile-time error, including for identifiers:
//! ```compile_fail
//! use forklaunch_core::persistence::compliance::ComplianceMetadata;
//! #[derive(ComplianceMetadata)]
//! struct Contact { email: String }
//! ```
//!
//! Misspelled classifications fail at compile time:
//! ```compile_fail
//! use forklaunch_core::persistence::compliance::ComplianceMetadata;
//! #[derive(ComplianceMetadata)]
//! struct Contact { #[compliance(level = "personal")] email: String }
//! ```
//!
//! Duplicate storage names also fail:
//! ```compile_fail
//! use forklaunch_core::persistence::compliance::ComplianceMetadata;
//! #[derive(ComplianceMetadata)]
//! struct Contact { #[compliance(level = "none")] id: String, #[compliance(level = "pii", name = "id")] email: String }
//! ```
pub use forklaunch_core_macros::ComplianceMetadata;

/// Classifications shared with TypeScript's explicit compliance registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplianceLevel {
    None,
    Pii,
    Phi,
    Pci,
}

/// A field's Rust name and explicitly declared storage metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComplianceField {
    pub rust_name: &'static str,
    pub storage_name: &'static str,
    pub level: ComplianceLevel,
    pub nullable: bool,
}

impl ComplianceField {
    /// Whether the TypeScript retention policy permits clearing this field.
    /// Adapters must still verify their storage schema before writing null.
    #[must_use]
    pub const fn can_anonymize(&self) -> bool {
        self.nullable && !matches!(self.level, ComplianceLevel::None)
    }
}

/// Metadata can be derived for named structs or implemented by an adapter.
/// Use `#[compliance(crate_path = "renamed_core")]` with a renamed dependency.
/// One explicit purpose binding; it does not itself encrypt or authorize access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtectedDeclaration {
    pub member: &'static str,
    pub purpose: &'static str,
}

pub trait ComplianceMetadata {
    const ENTITY: &'static str;
    /// None denotes declarations only; Some denotes actual protected field bindings.
    const PROTECTED_MEMBERS: Option<&'static [ProtectedDeclaration]> = None;
    const FIELDS: &'static [ComplianceField];
}

/// Machine-readable declarations for a concrete storage collection. This is
/// evidence of explicit classification, not certification of runtime enforcement.
#[derive(Debug, Clone, Copy)]
pub struct ComplianceEntity {
    pub entity: &'static str,
    pub storage: &'static str,
    pub kind: &'static str,
    pub fields: &'static [ComplianceField],
    pub protected_members: Option<&'static [ProtectedDeclaration]>,
}
impl ComplianceEntity {
    #[must_use]
    pub const fn of<T: ComplianceMetadata>(storage: &'static str, kind: &'static str) -> Self {
        Self {
            entity: T::ENTITY,
            storage,
            kind,
            fields: T::FIELDS,
            protected_members: T::PROTECTED_MEMBERS,
        }
    }
}

/// Export declarations without implying that encryption, query, or lifecycle
/// adapters have been verified. CLI audits must report these checks as unknown
/// until they have separate evidence.
#[cfg(feature = "compliance-artifact")]
#[must_use]
pub fn compliance_manifest(entities: &[ComplianceEntity]) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion": 1,
        "coverage": "declared-storage-members-only",
        "runtimeEnforcement": "requires-independent-verification",
        "lifecycleCoverage": "unknown",
        "entities": entities.iter().map(|entity| serde_json::json!({
            "entity": entity.entity, "storage": entity.storage, "kind": entity.kind,
            "binding": if let Some(members) = entity.protected_members {
                serde_json::json!({"kind":"protected-model", "protectedMembers":members.iter().map(|member| serde_json::json!({"member":member.member,"purpose":member.purpose})).collect::<Vec<_>>()})
            } else { serde_json::json!({"kind":"declarations-only"}) },
            "fields": entity.fields.iter().map(|field| serde_json::json!({
                "member": field.rust_name, "column": field.storage_name,
                "classification": match field.level { ComplianceLevel::None=>"none", ComplianceLevel::Pii=>"pii", ComplianceLevel::Phi=>"phi", ComplianceLevel::Pci=>"pci" },
                "nullable": field.nullable,
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    })
}

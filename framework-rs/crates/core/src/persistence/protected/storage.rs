//! Bind real ciphertext fields to their derived persisted-model declarations.
//! Validation checks policy identity, not ciphertext authenticity or authorization.
//!
//! Persisted models cannot substitute plaintext fields, even with classification:
//! ```compile_fail
//! use forklaunch_core::persistence::compliance::ComplianceMetadata;
//! #[derive(ComplianceMetadata)]
//! #[compliance(persisted)]
//! struct Invalid { #[compliance(level="pii", purpose="email")] email: String }
//! ```
//! Purpose is explicit and cannot be attached to unclassified fields:
//! ```compile_fail
//! use forklaunch_core::persistence::{compliance::ComplianceMetadata, protected::ProtectedField};
//! #[derive(ComplianceMetadata)]
//! #[compliance(persisted)]
//! struct Invalid { #[compliance(level="pii")] email: ProtectedField }
//! ```
//! ```compile_fail
//! use forklaunch_core::persistence::compliance::ComplianceMetadata;
//! #[derive(ComplianceMetadata)]
//! #[compliance(persisted)]
//! struct Invalid { #[compliance(level="none", purpose="email")] email: String }
//! ```
//! Classification descriptors cannot pretend to bind a protection purpose:
//! ```compile_fail
//! use forklaunch_core::persistence::compliance::ComplianceMetadata;
//! #[derive(ComplianceMetadata)]
//! struct Invalid { #[compliance(level="pii", purpose="email")] email: String }
//! ```
use super::{ProtectedField, ProtectedMember};
use crate::common::{FrameworkError, FrameworkResult};
use crate::persistence::compliance::ComplianceMetadata;

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ProtectedField {}
    impl Sealed for Option<super::ProtectedField> {}
}

/// Actual protected storage representations; plaintext types cannot implement this trait.
pub trait ProtectedStorage: sealed::Sealed {
    /// Check that a field was sealed/loaded with its model's declared member binding.
    /// # Errors
    /// Rejects a different entity, member, classification or encryption purpose.
    fn validate_binding(&self, member: &ProtectedMember) -> FrameworkResult<()>;
}
impl ProtectedStorage for ProtectedField {
    fn validate_binding(&self, member: &ProtectedMember) -> FrameworkResult<()> {
        if self.member.entity != member.entity
            || self.member.field != member.field
            || self.member.purpose != member.purpose
            || self.member.level != member.level
        {
            return Err(FrameworkError::new(
                "protected_storage_binding_mismatch",
                "Protected storage value does not match its declared member policy",
            ));
        }
        Ok(())
    }
}
impl ProtectedStorage for Option<ProtectedField> {
    fn validate_binding(&self, member: &ProtectedMember) -> FrameworkResult<()> {
        self.as_ref()
            .map_or(Ok(()), |field| field.validate_binding(member))
    }
}

/// Derived by `ComplianceMetadata` with `#[compliance(persisted)]`.
/// Storage adapters call this before exporting ciphertext into storage writes.
/// Keys, trusted tenant/resource context and authorized audit access stay explicit.
pub trait ProtectedModel: ComplianceMetadata {
    /// Check all protected member bindings, including nullable fields when present.
    /// # Errors
    /// Rejects any field constructed under a different member policy.
    fn validate_protected_storage(&self) -> FrameworkResult<()>;
}

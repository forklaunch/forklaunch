//! Persisted domain model: classification and protection are part of the stored type.
use forklaunch_core::persistence::{
    compliance::ComplianceMetadata,
    protected::{ProtectedField, storage::ProtectedModel},
};
use uuid::Uuid;
use forklaunch_core::common::FrameworkResult;

/// Plaintext cannot be assigned to a classified persisted field.
/// ```compile_fail
/// use {{crate_module}}::persistence::entities::StoredRecord;
/// let row = StoredRecord {
///     id: uuid::Uuid::nil(), tenant_id: "tenant".into(),
///     name_ciphertext: String::from("plaintext"),
/// };
/// ```
/// Protected members must declare their encryption purpose explicitly.
/// ```compile_fail
/// use forklaunch_core::persistence::{compliance::ComplianceMetadata, protected::ProtectedField};
/// #[derive(ComplianceMetadata)]
/// #[compliance(persisted)]
/// struct MissingPurpose {
///     #[compliance(level = "pii")]
///     name: ProtectedField,
/// }
/// ```
#[derive(Debug, ComplianceMetadata)]
#[compliance(entity = "Record", persisted)]
pub struct StoredRecord {
    #[compliance(level = "none")]
    pub id: Uuid,
    #[compliance(level = "none")]
    pub tenant_id: String,
    #[compliance(level = "pii", purpose = "record-name-v1")]
    pub name_ciphertext: ProtectedField,
}
impl StoredRecord {
    /// Check declared member bindings before SQL writes and after row hydration.
    pub fn validate(&self) -> FrameworkResult<()> {
        self.validate_protected_storage()
    }
}

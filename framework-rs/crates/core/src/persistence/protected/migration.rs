//! Explicit protected-member rewrites. Audit records describe preparation, not
//! successful storage mutation. The committed operation checkpoint is the write
//! receipt; stores must update ciphertext, index and checkpoint atomically.
use super::{AsyncComplianceAudit, ProtectedContext, ProtectedField, ProtectedMember};
use crate::common::{FrameworkError, FrameworkResult};
use crate::encryption::EnvelopeKey;
use std::{future::Future, pin::Pin};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationKind {
    Rotate,
    Reclassify,
}
/// Both contexts must describe the same verified tenant/resource. This API does
/// not authorize cross-tenant repair, plaintext import, or dropping encryption.
pub struct MigrationRequest<'a> {
    pub source_key: &'a EnvelopeKey,
    pub source_context: ProtectedContext,
    pub target_key: &'a EnvelopeKey,
    pub target_context: ProtectedContext,
    pub target_member: ProtectedMember,
    /// Persist this ID in the migration job/checkpoint; reuse only for retries of
    /// this exact policy/key transition, never for a different migration.
    pub operation_id: Uuid,
    pub kind: MigrationKind,
    pub actor: &'a str,
    pub reason: &'a str,
}
/// Metadata only. No plaintext, index values, ciphertext or key material.
#[derive(Debug)]
pub struct MigrationPrepared<'a> {
    pub operation_id: Uuid,
    pub kind: MigrationKind,
    pub tenant: Uuid,
    pub resource: Uuid,
    pub source_member: &'a ProtectedMember,
    pub target_member: &'a ProtectedMember,
    pub source_key_id: Option<&'a str>,
    pub target_key_id: Option<&'a str>,
    pub actor: &'a str,
    pub reason: &'a str,
}
/// Acknowledgment must precede release of a prepared rewrite. The event means
/// "prepared"; failed/CAS-conflicting mutations must not be labeled applied.
pub trait MigrationAudit: Sync {
    /// # Errors
    /// Fail unless the migration intent was durably acknowledged.
    fn prepared<'a>(
        &'a self,
        event: MigrationPrepared<'a>,
    ) -> Pin<Box<dyn Future<Output = FrameworkResult<()>> + Send + 'a>>;
}
pub struct PreparedMigration {
    expected: String,
    replacement: ProtectedField,
    context: ProtectedContext,
    operation_id: Uuid,
}
impl std::fmt::Debug for PreparedMigration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedMigration([redacted])")
    }
}
impl PreparedMigration {
    #[must_use]
    pub const fn operation_id(&self) -> Uuid {
        self.operation_id
    }
    #[must_use]
    pub const fn context(&self) -> ProtectedContext {
        self.context
    }
    #[must_use]
    pub fn expected_ciphertext(&self) -> &str {
        &self.expected
    }
    #[must_use]
    pub const fn replacement(&self) -> &ProtectedField {
        &self.replacement
    }
}
impl ProtectedField {
    /// Authenticate old ciphertext, acknowledge its access, re-seal under the
    /// target policy/key, recompute its active index, then acknowledge the intent.
    /// The returned plan contains no plaintext and is safe to retry with CAS.
    /// # Errors
    /// Rejects context moves, invalid IDs, changed policy during a rotation,
    /// absent keys, invalid ciphertext, or either failed audit acknowledgment.
    pub async fn prepare_migration(
        &self,
        request: MigrationRequest<'_>,
        access: &dyn AsyncComplianceAudit,
        audit: &dyn MigrationAudit,
    ) -> FrameworkResult<PreparedMigration> {
        if request.operation_id.is_nil()
            || request.source_context.tenant != request.target_context.tenant
            || request.source_context.resource != request.target_context.resource
            || (request.kind == MigrationKind::Rotate && self.member != request.target_member)
        {
            return Err(migration_error());
        }
        let plaintext = self
            .reveal_async(
                request.source_key,
                request.source_context,
                request.actor,
                request.reason,
                access,
            )
            .await?;
        let retained = if request.kind == MigrationKind::Rotate
            && self.member.disclosure != super::DisclosurePolicy::Redact
        {
            Some(self.disclosed(request.source_key, request.source_context)?)
        } else {
            None
        };
        let replacement = Self::seal_with_projection(
            request.target_key,
            request.target_context,
            request.target_member,
            &plaintext,
            retained.as_ref(),
        )?;
        drop(plaintext);
        let source_key_id = key_id(&self.ciphertext);
        let target_key_id = key_id(&replacement.ciphertext);
        audit
            .prepared(MigrationPrepared {
                operation_id: request.operation_id,
                kind: request.kind,
                tenant: request.target_context.tenant,
                resource: request.target_context.resource,
                source_member: &self.member,
                target_member: &replacement.member,
                source_key_id: source_key_id.as_deref(),
                target_key_id: target_key_id.as_deref(),
                actor: request.actor,
                reason: request.reason,
            })
            .await
            .map_err(|_| {
                FrameworkError::new(
                    "migration_audit_failed",
                    "Protected migration intent could not be recorded",
                )
            })?;
        Ok(PreparedMigration {
            expected: self.ciphertext.clone(),
            replacement,
            context: request.target_context,
            operation_id: request.operation_id,
        })
    }
}
fn key_id(value: &str) -> Option<String> {
    super::disclosure::stored_key_id(value)
}

fn migration_error() -> FrameworkError {
    FrameworkError::new(
        "invalid_protected_migration",
        "Protected migration requires a stable operation, identical tenant/resource and explicit policy transition",
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOutcome {
    Applied,
    AlreadyApplied,
    Conflict,
}

#[cfg(feature = "postgres")]
mod postgres;
#[cfg(feature = "postgres")]
pub use postgres::{MigrationColumns, PostgresMigrationStore};

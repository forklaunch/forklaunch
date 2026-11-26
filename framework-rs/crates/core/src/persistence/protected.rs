//! Ciphertext-only protected members with explicit, audited plaintext access.
//!
//! This is a persistence boundary, not authorization: callers must establish a
//! trusted tenant/resource and authorize the operation before calling `reveal`.
//! Neither diagnostics nor ordinary serialization expose the stored envelope or
//! plaintext. Only `ciphertext` exports storage bytes; it cannot expose plaintext.
//! Existing envelopes remain readable under the configured key ring.
mod disclosure;
pub mod migration;
pub mod storage;
pub use disclosure::{DisclosurePolicy, DisclosureValue};

use super::compliance::{ComplianceLevel, ComplianceMetadata};
use crate::common::{FrameworkError, FrameworkResult};
use crate::encryption::EnvelopeKey;
use serde::{Serialize, Serializer, ser::SerializeMap};
use uuid::Uuid;
use zeroize::Zeroizing;

/// Explicit mapping from a classified entity member to its encryption purpose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectedMember {
    entity: &'static str,
    field: &'static str,
    purpose: String,
    level: ComplianceLevel,
    index: Option<IndexNormalization>,
    disclosure: DisclosurePolicy,
}

/// Explicit normalization before keyed equality indexing. This does not enable
/// range/order queries or promise SQL collation semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexNormalization {
    Exact,
    TrimLowercase,
}
impl IndexNormalization {
    fn normalize(self, value: &str) -> Zeroizing<String> {
        Zeroizing::new(match self {
            Self::Exact => value.to_owned(),
            Self::TrimLowercase => value.trim().to_lowercase(),
        })
    }
}
/// Supported query algebra is deliberately narrower than ordinary SQL fields.
#[derive(Clone, Copy)]
pub enum ProtectedQuery<'a> {
    Equal(&'a str),
    NotEqual(&'a str),
    Range,
    Order,
}

/// Explicit storage/query values. Debug does not expose the equality tokens.
pub struct EqualityIndexes(Vec<String>);
impl std::fmt::Debug for EqualityIndexes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EqualityIndexes([redacted])")
    }
}
impl EqualityIndexes {
    /// Bind all values using the adapter's equality/ANY predicate. Always scope
    /// the SQL query to the same verified tenant; indexes are not authorization.
    #[must_use]
    pub fn values(&self) -> &[String] {
        &self.0
    }
}

/// A value-free redaction. Only protected fields can construct this wrapper.
/// This is not a legal de-identification certificate or a pseudonym.
#[derive(Debug)]
pub struct AnonymousValue(&'static str);
impl Serialize for AnonymousValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}
impl AnonymousValue {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        self.0
    }
}

impl ProtectedMember {
    /// Opt in to a separately persisted blind-index column. Existing data needs
    /// a deliberate backfill before indexed queries can find it.
    #[must_use]
    pub const fn with_equality_index(mut self, normalization: IndexNormalization) -> Self {
        self.index = Some(normalization);
        self
    }

    /// Return active and previous-key indexes for a tenant-scoped equality query.
    /// The adapter binds these tokens; it must never substitute plaintext SQL.
    /// # Errors
    /// Rejects unconfigured indexing, unsupported operators, nil tenant and absent keys.
    pub fn query(
        &self,
        key: &EnvelopeKey,
        tenant: Uuid,
        query: ProtectedQuery<'_>,
    ) -> FrameworkResult<EqualityIndexes> {
        let normalization = self.index.ok_or_else(query_error)?;
        let ProtectedQuery::Equal(value) = query else {
            return Err(query_error());
        };
        if tenant.is_nil() {
            return Err(query_error());
        }
        let value = normalization.normalize(value);
        key.blind_indexes(tenant, &self.purpose, value.as_bytes())
            .map(EqualityIndexes)
            .map_err(|_| unavailable())
    }

    /// Resolve a protected member from the entity's explicit classifications.
    /// # Errors
    /// Rejects missing or unprotected member declarations.
    pub fn from_entity<E: ComplianceMetadata>(
        field: &'static str,
        purpose: impl Into<String>,
    ) -> FrameworkResult<Self> {
        let declared = E::FIELDS
            .iter()
            .find(|item| item.rust_name == field)
            .ok_or_else(|| {
                FrameworkError::new(
                    "unknown_protected_member",
                    "Member is absent from entity metadata",
                )
            })?;
        Self::new(E::ENTITY, declared.rust_name, purpose, declared.level)
    }

    /// # Errors
    /// Rejects unclassified members and missing/invalid names or wire purposes.
    pub fn new(
        entity: &'static str,
        field: &'static str,
        purpose: impl Into<String>,
        level: ComplianceLevel,
    ) -> FrameworkResult<Self> {
        let purpose = purpose.into();
        if level == ComplianceLevel::None
            || [entity, field, purpose.as_str()]
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        {
            return Err(FrameworkError::new(
                "invalid_protected_member",
                "Protected member requires classification and explicit names",
            ));
        }
        Ok(Self {
            entity,
            field,
            purpose,
            level,
            index: None,
            disclosure: DisclosurePolicy::Redact,
        })
    }
}

/// Context must come from verified application identity and selected resource,
/// never from an untrusted envelope. Nil identifiers are rejected.
#[derive(Clone, Copy, Debug)]
pub struct ProtectedContext {
    tenant: Uuid,
    resource: Uuid,
}
impl ProtectedContext {
    /// # Errors
    /// Rejects absent tenant or resource identities.
    pub fn new(tenant: Uuid, resource: Uuid) -> FrameworkResult<Self> {
        if tenant.is_nil() || resource.is_nil() {
            return Err(FrameworkError::new(
                "invalid_protected_context",
                "Tenant and resource are required",
            ));
        }
        Ok(Self { tenant, resource })
    }
}

/// Contains only identity/classification metadata; never values or ciphertext.
#[derive(Debug)]
pub struct ComplianceAccess<'a> {
    pub entity: &'static str,
    pub field: &'static str,
    pub level: ComplianceLevel,
    pub tenant: Uuid,
    pub resource: Uuid,
    pub actor: &'a str,
    pub reason: &'a str,
}

/// Applications provide a durable audit sink according to their policy. A sink
/// error denies plaintext access; the framework never silently ignores it.
pub trait ComplianceAudit {
    /// # Errors
    /// Return an error if the access event cannot be recorded.
    fn record(&self, event: &ComplianceAccess<'_>) -> FrameworkResult<()>;
}

/// Asynchronous acknowledged audit sinks (for example committed database writes).
pub trait AsyncComplianceAudit: Sync {
    /// # Errors
    /// Return an error unless the audit event has been accepted durably.
    fn record<'a>(
        &'a self,
        event: ComplianceAccess<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = FrameworkResult<()>> + Send + 'a>>;
}

/// No plaintext is retained. Cloning copies only encrypted bytes and metadata.
#[derive(Clone)]
pub struct ProtectedField {
    ciphertext: String,
    member: ProtectedMember,
    blind_index: Option<String>,
}
impl std::fmt::Debug for ProtectedField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProtectedField([redacted])")
    }
}
impl Serialize for ProtectedField {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_map(Some(0))?.end()
    }
}
impl ProtectedField {
    /// Encrypt immediately; there is no plaintext or missing-key write mode.
    /// # Errors
    /// Fails when authenticated encryption is unavailable.
    pub fn seal(
        key: &EnvelopeKey,
        context: ProtectedContext,
        member: ProtectedMember,
        plaintext: &[u8],
    ) -> FrameworkResult<Self> {
        Self::seal_with_projection(key, context, member, plaintext, None)
    }
    fn seal_with_projection(
        key: &EnvelopeKey,
        context: ProtectedContext,
        member: ProtectedMember,
        plaintext: &[u8],
        retained: Option<&DisclosureValue>,
    ) -> FrameworkResult<Self> {
        let inner = key
            .seal(context.tenant, context.resource, &member.purpose, plaintext)
            .map_err(|_| unavailable())?;
        let ciphertext = disclosure::seal(key, context, &member, plaintext, inner, retained)?;
        let blind_index = if member.index.is_some() {
            let plaintext = std::str::from_utf8(plaintext).map_err(|_| query_error())?;
            Some(
                member
                    .query(key, context.tenant, ProtectedQuery::Equal(plaintext))?
                    .0
                    .into_iter()
                    .next()
                    .ok_or_else(unavailable)?,
            )
        } else {
            None
        };
        Ok(Self {
            ciphertext,
            member,
            blind_index,
        })
    }

    /// Wrap stored ciphertext lazily. Authenticity is checked on every reveal;
    /// plaintext legacy rows require a separate, explicit migration.
    /// # Errors
    /// Rejects obvious plaintext. A recognized prefix is not authentication.
    pub fn from_ciphertext(member: ProtectedMember, ciphertext: String) -> FrameworkResult<Self> {
        if !["v1.", "v2.", "v3.", "pd1."]
            .iter()
            .any(|p| ciphertext.starts_with(p))
        {
            return Err(unavailable());
        }
        disclosure::validate_storage(&ciphertext)?;
        Ok(Self {
            ciphertext,
            member,
            blind_index: None,
        })
    }

    #[must_use]
    pub fn ciphertext(&self) -> &str {
        &self.ciphertext
    }

    /// Active equality index for the just-sealed value. Loaded fields require
    /// explicit re-sealing/backfill; no plaintext is implicitly decrypted here.
    #[must_use]
    pub fn blind_index(&self) -> Option<&str> {
        self.blind_index.as_deref()
    }

    #[must_use]
    #[allow(clippy::unused_self)]
    pub const fn anonymous(&self) -> AnonymousValue {
        AnonymousValue("[redacted]")
    }

    /// Redaction never decrypts and carries no value-derived information.
    #[must_use]
    #[allow(clippy::unused_self)] // Keep redaction available on the protected value.
    pub const fn redacted(&self) -> &'static str {
        "[redacted]"
    }

    /// Asynchronously acknowledge the audit record before releasing plaintext.
    /// # Errors
    /// Rejects invalid access metadata, invalid ciphertext/context, and failed audits.
    pub async fn reveal_async(
        &self,
        key: &EnvelopeKey,
        context: ProtectedContext,
        actor: &str,
        reason: &str,
        audit: &dyn AsyncComplianceAudit,
    ) -> FrameworkResult<Zeroizing<Vec<u8>>> {
        if [actor, reason]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(FrameworkError::new(
                "invalid_compliance_access",
                "Actor and access reason are required",
            ));
        }
        let inner = disclosure::authenticated_inner(key, context, &self.member, &self.ciphertext)?;
        let plaintext = key
            .open(
                context.tenant,
                context.resource,
                &self.member.purpose,
                &inner,
            )
            .map_err(|_| unavailable())?;
        audit
            .record(ComplianceAccess {
                entity: self.member.entity,
                field: self.member.field,
                level: self.member.level,
                tenant: context.tenant,
                resource: context.resource,
                actor,
                reason,
            })
            .await
            .map_err(|_| {
                FrameworkError::new(
                    "compliance_audit_failed",
                    "Protected access audit could not be recorded",
                )
            })?;
        Ok(plaintext)
    }

    /// Decrypt and record every successful access before returning a zeroizing
    /// buffer. Audit metadata is required even for repeated reads. The caller
    /// owns any plaintext copies it deliberately creates from the buffer.
    /// # Errors
    /// Fails on missing access metadata, tenant/resource/purpose mismatch,
    /// malformed ciphertext, unavailable keys, or audit sink failure.
    pub fn reveal(
        &self,
        key: &EnvelopeKey,
        context: ProtectedContext,
        actor: &str,
        reason: &str,
        audit: &dyn ComplianceAudit,
    ) -> FrameworkResult<Zeroizing<Vec<u8>>> {
        if [actor, reason]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
        {
            return Err(FrameworkError::new(
                "invalid_compliance_access",
                "Actor and access reason are required",
            ));
        }
        let inner = disclosure::authenticated_inner(key, context, &self.member, &self.ciphertext)?;
        let plaintext = key
            .open(
                context.tenant,
                context.resource,
                &self.member.purpose,
                &inner,
            )
            .map_err(|_| unavailable())?;
        audit
            .record(&ComplianceAccess {
                entity: self.member.entity,
                field: self.member.field,
                level: self.member.level,
                tenant: context.tenant,
                resource: context.resource,
                actor,
                reason,
            })
            .map_err(|_| {
                FrameworkError::new(
                    "compliance_audit_failed",
                    "Protected access audit could not be recorded",
                )
            })?;
        Ok(plaintext)
    }
}
fn unavailable() -> FrameworkError {
    FrameworkError::new(
        "protected_field_unavailable",
        "Protected field could not be authenticated",
    )
}

fn query_error() -> FrameworkError {
    FrameworkError::new(
        "unsupported_protected_query",
        "Protected members require explicit equality indexing; this query is unsupported",
    )
}

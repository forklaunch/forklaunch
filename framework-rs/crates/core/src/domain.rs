//! Shared module types: errors, paging, audit and session context used by blueprint
//! implementations, as TypeScript implementations share them through framework packages.
use std::{collections::BTreeMap, error::Error, fmt};

/// A stable string identifier for a blueprint entity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Identifier(pub String);

impl Identifier {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Creation/update timestamps (UNIX seconds) shared by persisted entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamps {
    pub created_at_unix_seconds: u64,
    pub updated_at_unix_seconds: u64,
}

impl Timestamps {
    #[must_use]
    pub const fn created(at_unix_seconds: u64) -> Self {
        Self {
            created_at_unix_seconds: at_unix_seconds,
            updated_at_unix_seconds: at_unix_seconds,
        }
    }

    #[must_use]
    pub const fn touched(mut self, at_unix_seconds: u64) -> Self {
        self.updated_at_unix_seconds = at_unix_seconds;
        self
    }
}

/// A persisted blueprint entity addressable by a stable identifier.
pub trait Entity {
    /// Returns the entity's stable identifier.
    fn id(&self) -> &str;
}

/// The standard envelope for a single-resource API result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResult<T> {
    pub data: T,
    pub correlation_id: Option<String>,
}

impl<T> ApiResult<T> {
    #[must_use]
    pub const fn new(data: T) -> Self {
        Self {
            data,
            correlation_id: None,
        }
    }

    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }
}

pub type BlueprintResult<T> = Result<T, BlueprintError>;

/// Operation error with explicitly public wire details and opaque typed diagnostics.
/// Construct with [`Self::new`]; private diagnostics prohibit struct literals.
#[derive(Clone)]
pub struct BlueprintError {
    pub code: String,
    pub message: String,
    /// Structured operation details; transports explicitly select public fields.
    pub details: BTreeMap<String, serde_json::Value>,
    diagnostic_context: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
}

impl BlueprintError {
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: BTreeMap::new(),
            diagnostic_context: None,
        }
    }

    /// Attaches a typed diagnostic value, replacing any previous context.
    /// Context is excluded from details, formatting, equality and wire rendering.
    /// Private marker types let modules preserve provenance across error layers
    /// without trusting application-provided JSON metadata.
    #[must_use]
    pub fn with_diagnostic_context<T: std::any::Any + Send + Sync>(mut self, context: T) -> Self {
        self.diagnostic_context = Some(std::sync::Arc::new(context));
        self
    }

    /// Reads the context only when its exact concrete type matches.
    #[must_use]
    pub fn diagnostic_context<T: std::any::Any>(&self) -> Option<&T> {
        self.diagnostic_context.as_ref()?.downcast_ref()
    }

    #[must_use]
    pub fn with_detail(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }
}

// Diagnostic context is deliberately not part of the public value semantics.
// Preserve the established public Debug representation; opaque diagnostics may
// contain private data and must never be formatted.
#[allow(clippy::missing_fields_in_debug)]
impl fmt::Debug for BlueprintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BlueprintError")
            .field("code", &self.code)
            .field("message", &self.message)
            .field("details", &self.details)
            .finish()
    }
}
impl PartialEq for BlueprintError {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code && self.message == other.message && self.details == other.details
    }
}
impl Eq for BlueprintError {}

impl fmt::Display for BlueprintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for BlueprintError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueprintModule {
    pub name: String,
    pub description: String,
    pub version: String,
    pub tags: BTreeMap<String, String>,
}

impl BlueprintModule {
    #[must_use]
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            tags: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    #[must_use]
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    #[must_use]
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            next_cursor: None,
        }
    }

    #[must_use]
    pub fn with_next_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.next_cursor = Some(cursor.into());
        self
    }

    #[must_use]
    pub fn has_more(&self) -> bool {
        self.next_cursor.is_some()
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct PageRequest {
    pub limit: usize,
    pub cursor: Option<String>,
}

impl PageRequest {
    #[must_use]
    pub fn first_page(limit: usize) -> Self {
        Self {
            limit,
            cursor: None,
        }
    }

    #[must_use]
    pub fn after(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditContext {
    pub actor_id: Option<String>,
    pub correlation_id: String,
}

impl AuditContext {
    #[must_use]
    pub fn system(correlation_id: impl Into<String>) -> Self {
        Self {
            actor_id: None,
            correlation_id: correlation_id.into(),
        }
    }

    #[must_use]
    pub fn actor(actor_id: impl Into<String>, correlation_id: impl Into<String>) -> Self {
        Self {
            actor_id: Some(actor_id.into()),
            correlation_id: correlation_id.into(),
        }
    }
}

/// Common auth/session DTO shared across blueprints.
///
/// A minimal, domain-agnostic view of an authenticated caller that IAM and
/// downstream services can enrich from a verified credential.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionContext {
    pub subject: String,
    pub roles: Vec<String>,
    pub scopes: Vec<String>,
}

impl SessionContext {
    #[must_use]
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            subject: subject.into(),
            ..Self::default()
        }
    }

    /// Returns whether the session holds the given role.
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|held| held == role)
    }
}

/// Common billing DTO: a monetary amount in minor units with a currency code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoneyAmount {
    pub amount_minor: i64,
    pub currency_code: [u8; 3],
}

impl MoneyAmount {
    #[must_use]
    pub fn new(amount_minor: i64, currency_code: &str) -> Self {
        let mut code = *b"USD";
        for (slot, byte) in code.iter_mut().zip(currency_code.bytes()) {
            *slot = byte.to_ascii_uppercase();
        }
        Self {
            amount_minor,
            currency_code: code,
        }
    }

    /// Returns the ISO currency code as a string.
    #[must_use]
    pub fn currency(&self) -> String {
        String::from_utf8_lossy(&self.currency_code).into_owned()
    }
}

/// Common worker/job DTO: a stable reference to an enqueued job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobDescriptor {
    pub id: String,
    pub queue: String,
}

impl JobDescriptor {
    #[must_use]
    pub fn new(id: impl Into<String>, queue: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            queue: queue.into(),
        }
    }
}

impl Entity for JobDescriptor {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Deterministic helpers for blueprint unit and SDK tests.
pub mod testing {
    use super::{AuditContext, PageRequest};

    /// A fixed correlation id so test assertions stay deterministic.
    pub const TEST_CORRELATION_ID: &str = "test-correlation-id";

    /// Returns an audit context for a system actor with the test correlation id.
    #[must_use]
    pub fn system_audit_context() -> AuditContext {
        AuditContext::system(TEST_CORRELATION_ID)
    }

    /// Returns an audit context attributed to `actor_id` with the test correlation id.
    #[must_use]
    pub fn actor_audit_context(actor_id: &str) -> AuditContext {
        AuditContext::actor(actor_id, TEST_CORRELATION_ID)
    }

    /// Returns a first-page request with the given limit.
    #[must_use]
    pub fn first_page(limit: usize) -> PageRequest {
        PageRequest::first_page(limit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_diagnostics_are_typed_cloneable_and_excluded_from_public_semantics() {
        struct PrivateMarker(&'static str);
        let plain = BlueprintError::new("OWNED", "public message")
            .with_detail("table", serde_json::json!("owned_table"));
        let marked = plain
            .clone()
            .with_diagnostic_context(PrivateMarker("PRIVATE_CONTEXT_SECRET"));
        let copied = marked.clone();
        assert_eq!(
            copied.diagnostic_context::<PrivateMarker>().unwrap().0,
            "PRIVATE_CONTEXT_SECRET"
        );
        assert!(copied.diagnostic_context::<String>().is_none());
        assert_eq!(plain, marked);
        assert_eq!(format!("{plain:?}"), format!("{marked:?}"));
        assert_eq!(plain.to_string(), marked.to_string());
        assert_eq!(plain.details, marked.details);
        assert!(!format!("{copied:?}").contains("PRIVATE_CONTEXT_SECRET"));
        let replaced = copied.with_diagnostic_context(123u32);
        assert!(replaced.diagnostic_context::<PrivateMarker>().is_none());
        assert_eq!(replaced.diagnostic_context::<u32>(), Some(&123));
    }

    #[test]
    fn module_tracks_version_and_tags() {
        let module = BlueprintModule::new("iam", "IAM module")
            .with_version("1.2.3")
            .with_tag("kind", "identity");

        assert_eq!(module.version, "1.2.3");
        assert_eq!(
            module.tags.get("kind").map(String::as_str),
            Some("identity")
        );
    }

    #[test]
    fn pages_report_more_results() {
        let page = Page::new(vec![1, 2]).with_next_cursor("cursor-2");

        assert!(page.has_more());
    }

    #[test]
    fn shared_primitives_compose() {
        let timestamps = Timestamps::created(1000).touched(2000);
        assert_eq!(timestamps.created_at_unix_seconds, 1000);
        assert_eq!(timestamps.updated_at_unix_seconds, 2000);

        let result = ApiResult::new(Identifier::new("user_1")).with_correlation_id("corr-1");
        assert_eq!(result.data.as_str(), "user_1");
        assert_eq!(result.correlation_id.as_deref(), Some("corr-1"));
    }

    #[test]
    fn testing_helpers_are_deterministic() {
        assert_eq!(
            testing::system_audit_context().correlation_id,
            testing::TEST_CORRELATION_ID
        );
        assert_eq!(
            testing::actor_audit_context("admin").actor_id.as_deref(),
            Some("admin")
        );
    }

    #[test]
    fn common_dtos_expose_shared_shapes() {
        let session = SessionContext {
            roles: vec!["admin".to_owned()],
            ..SessionContext::new("user_1")
        };
        assert!(session.has_role("admin"));
        assert!(!session.has_role("guest"));

        let money = MoneyAmount::new(999, "usd");
        assert_eq!(money.amount_minor, 999);
        assert_eq!(money.currency(), "USD");

        let job = JobDescriptor::new("job_1", "email");
        assert_eq!(job.id(), "job_1");
    }
}

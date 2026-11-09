//! Tenant-bound IAM directory CRUD, independent of session authentication.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use serde_json::Value;
use std::future::Future;

pub mod compliance;
pub mod filter;
pub(crate) mod mapping;
pub mod memory;
#[cfg(feature = "postgres")]
pub mod postgres;
pub mod service;
pub mod unit_of_work;

/// Persisted directory entity families. Session users/roles remain separate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DirectoryKind {
    Permission,
    Role,
    User,
    Organization,
}
impl DirectoryKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Permission => "permission",
            Self::Role => "role",
            Self::User => "user",
            Self::Organization => "organization",
        }
    }
}

/// One atomic directory operation, including relationship hydration and audits.
/// Dropping an uncommitted transaction must roll back every mutation.
pub trait DirectoryTransaction: Send {
    fn get(
        &mut self,
        kind: DirectoryKind,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<Option<Value>>> + Send;
    fn list(
        &mut self,
        kind: DirectoryKind,
        ids: Option<&[String]>,
        organization: Option<&str>,
    ) -> impl Future<Output = BlueprintResult<Vec<Value>>> + Send;
    fn insert(
        &mut self,
        kind: DirectoryKind,
        value: Value,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn replace(
        &mut self,
        kind: DirectoryKind,
        value: Value,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn delete(
        &mut self,
        kind: DirectoryKind,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<()>> + Send;
    fn commit(self) -> impl Future<Output = BlueprintResult<()>> + Send;
}

/// A store is bound to one trusted tenant, never selected from request DTOs.
pub trait DirectoryStore: Send + Sync {
    type Transaction<'a>: DirectoryTransaction
    where
        Self: 'a;
    fn begin(
        &self,
        write: bool,
    ) -> impl Future<Output = BlueprintResult<Self::Transaction<'_>>> + Send;
}

pub(super) fn invalid() -> BlueprintError {
    BlueprintError::new("iam_directory_invalid", "Invalid IAM directory input")
}
pub(super) fn storage_error() -> BlueprintError {
    BlueprintError::new(
        "iam_directory_storage_error",
        "IAM directory storage operation failed",
    )
}
pub(super) fn not_found() -> BlueprintError {
    BlueprintError::new("iam_directory_not_found", "IAM directory entity not found")
}
pub(super) fn conflict() -> BlueprintError {
    BlueprintError::new(
        "iam_directory_conflict",
        "IAM directory entity conflicts with an existing record",
    )
}

pub(crate) fn record_id(value: &Value) -> BlueprintResult<&str> {
    value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .ok_or_else(invalid)
}

#[cfg(test)]
mod tests;

pub(super) fn scan_limit_error() -> BlueprintError {
    BlueprintError::new(
        "iam_directory_scan_limit",
        "Directory candidate scan exceeds 4096 rows",
    )
}

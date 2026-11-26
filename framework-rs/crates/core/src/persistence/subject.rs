//! Bounded data-subject operations with explicit application-owned relationships.
//!
//! Implementations decide which records belong to a subject, lawful holds,
//! projection/redaction, cascades and storage-specific deletion. The framework
//! never guesses that an email, creator or owner column implies data ownership.
#[cfg(all(feature = "postgres", feature = "encryption"))]
pub mod export;
#[cfg(all(feature = "postgres", feature = "encryption"))]
pub mod outbox;
#[cfg(all(feature = "postgres", feature = "encryption"))]
pub mod postgres;

use super::{TenantContext, compliance::ComplianceMetadata};
use crate::common::{FrameworkError, FrameworkResult};
use std::future::Future;

/// A registered entity and an explicitly selected storage relationship.
#[derive(Debug, Clone)]
pub struct SubjectEntity {
    entity: &'static str,
    subject_field: &'static str,
}
impl SubjectEntity {
    /// # Errors
    /// Rejects unknown storage members or entities without protected members.
    pub fn for_entity<T: ComplianceMetadata>(subject_field: &'static str) -> FrameworkResult<Self> {
        if !T::FIELDS.iter().any(|f| f.storage_name == subject_field)
            || !T::FIELDS
                .iter()
                .any(|f| f.level != super::compliance::ComplianceLevel::None)
        {
            return Err(invalid());
        }
        Ok(Self {
            entity: T::ENTITY,
            subject_field,
        })
    }
    #[must_use]
    pub const fn entity(&self) -> &'static str {
        self.entity
    }
    #[must_use]
    pub const fn subject_field(&self) -> &'static str {
        self.subject_field
    }
}
/// One authorized operation. Persist and reuse `operation_id` across retries;
/// never reuse it for a different tenant, subject, entity or action.
/// No Debug/Serialize implementation: subject identifiers may be sensitive.
pub struct SubjectRequest<'a> {
    pub tenant: &'a TenantContext,
    pub subject: &'a str,
    pub entity: &'a SubjectEntity,
    pub operation_id: &'a str,
    pub batch_size: usize,
}
impl SubjectRequest<'_> {
    fn validate(&self, max_batches: usize) -> FrameworkResult<()> {
        let bounded =
            |s: &str| !s.trim().is_empty() && s.len() <= 256 && !s.chars().any(char::is_control);
        if !bounded(self.subject)
            || !bounded(self.operation_id)
            || !(1..=10_000).contains(&self.batch_size)
            || !(1..=10_000).contains(&max_batches)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
/// Adapter-owned stable page identity. Retrying an unacknowledged page must
/// return identical contents/identity, including across application restarts.
/// Records are not logged or serialized by this module.
pub struct ExportBatch<T> {
    pub batch_id: String,
    pub records: Vec<T>,
}
/// Data-subject export storage boundary. Each operation must bind a stable,
/// authorized snapshot and tenant/subject scope; fresh concurrent records require
/// an explicitly new operation. All plaintext projection reads must be audited.
pub trait SubjectExportStore: Sync {
    type Record: Send + Sync;
    fn next_batch(
        &self,
        request: &SubjectRequest<'_>,
    ) -> impl Future<Output = FrameworkResult<Option<ExportBatch<Self::Record>>>> + Send;
    /// Persist progress only after the sink acknowledged this exact batch.
    /// Compare scope and batch identity atomically; false means already committed.
    fn acknowledge(
        &self,
        request: &SubjectRequest<'_>,
        batch_id: &str,
    ) -> impl Future<Output = FrameworkResult<bool>> + Send;
}
/// Authorized destination for exported data. Acknowledgment requires durable
/// delivery; retrying the same scoped operation/batch must be idempotent.
/// Implementations own encrypted delivery, recipient authorization and expiry.
pub trait SubjectExportSink<T>: Sync {
    fn deliver(
        &self,
        request: &SubjectRequest<'_>,
        batch: &ExportBatch<T>,
    ) -> impl Future<Output = FrameworkResult<()>> + Send;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraseBatch {
    pub deleted: usize,
    pub complete: bool,
}
/// Each batch must atomically enforce tenant+subject scope, holds and policy,
/// delete at most `batch_size` records, and persist its replay checkpoint/receipt.
/// Return committed counts only; a failure/cancellation rolls back the batch.
/// Deletion of external objects requires a durable outbox before declaring done.
/// Audit authorization/intent must succeed before mutation. There is no implicit
/// permission to erase records merely because they have classified members.
pub trait SubjectEraseStore: Sync {
    fn erase_batch(
        &self,
        request: &SubjectRequest<'_>,
    ) -> impl Future<Output = FrameworkResult<EraseBatch>> + Send;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncompleteReason {
    Storage,
    Destination,
    InvalidBatch,
    BatchLimit,
}
/// Counts describe acknowledged progress during this invocation, not historical
/// totals. Replays may report zero new records. Only complete=true is success.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubjectProgress {
    pub records: u64,
    pub complete: bool,
    pub incomplete: Option<IncompleteReason>,
}
impl SubjectProgress {
    const fn pending() -> Self {
        Self {
            records: 0,
            complete: false,
            incomplete: None,
        }
    }
    fn fail(mut self, reason: IncompleteReason) -> Self {
        self.incomplete = Some(reason);
        self
    }
}
/// Deliver bounded batches, then persist their acknowledgment. Sink or checkpoint
/// failure preserves retryability and returns explicit partial progress.
/// # Errors
/// Rejects invalid scope/bounds before invoking either adapter.
pub async fn export_subject<S: SubjectExportStore, D: SubjectExportSink<S::Record>>(
    store: &S,
    destination: &D,
    request: &SubjectRequest<'_>,
    max_batches: usize,
) -> FrameworkResult<SubjectProgress> {
    request.validate(max_batches)?;
    let mut result = SubjectProgress::pending();
    for _ in 0..max_batches {
        let batch = match store.next_batch(request).await {
            Ok(Some(batch)) => batch,
            Ok(None) => {
                result.complete = true;
                return Ok(result);
            }
            Err(_) => return Ok(result.fail(IncompleteReason::Storage)),
        };
        if batch.records.is_empty()
            || batch.records.len() > request.batch_size
            || batch.batch_id.is_empty()
            || batch.batch_id.len() > 256
            || batch.batch_id.chars().any(char::is_control)
        {
            return Ok(result.fail(IncompleteReason::InvalidBatch));
        }
        if destination.deliver(request, &batch).await.is_err() {
            return Ok(result.fail(IncompleteReason::Destination));
        }
        match store.acknowledge(request, &batch.batch_id).await {
            Ok(true) => result.records += batch.records.len() as u64,
            Ok(false) => (),
            Err(_) => return Ok(result.fail(IncompleteReason::Storage)),
        }
    }
    Ok(result.fail(IncompleteReason::BatchLimit))
}
/// Apply bounded committed erasure batches. An incomplete result must remain a
/// failed/pending request; retry it with the same operation identity.
/// # Errors
/// Rejects invalid scope/bounds before invoking storage.
pub async fn erase_subject<S: SubjectEraseStore>(
    store: &S,
    request: &SubjectRequest<'_>,
    max_batches: usize,
) -> FrameworkResult<SubjectProgress> {
    request.validate(max_batches)?;
    let mut result = SubjectProgress::pending();
    for _ in 0..max_batches {
        let Ok(batch) = store.erase_batch(request).await else {
            return Ok(result.fail(IncompleteReason::Storage));
        };
        if batch.deleted > request.batch_size || (batch.deleted == 0 && !batch.complete) {
            return Ok(result.fail(IncompleteReason::InvalidBatch));
        }
        result.records += batch.deleted as u64;
        if batch.complete {
            result.complete = true;
            return Ok(result);
        }
    }
    Ok(result.fail(IncompleteReason::BatchLimit))
}
fn invalid() -> FrameworkError {
    FrameworkError::new(
        "invalid_subject_operation",
        "Data-subject operation requires explicit valid scope, relationship and bounds",
    )
}

/// `SQLx` 0.9 durable materialized exports.
#[cfg(all(feature = "postgres09", feature = "encryption"))]
pub mod export09;
/// `SQLx` 0.9 external deletion outbox.
#[cfg(all(feature = "postgres09", feature = "encryption"))]
pub mod outbox09;
/// `SQLx` 0.9 subject erasure, with identical scope and hold policies.
#[cfg(all(feature = "postgres09", feature = "encryption"))]
pub mod postgres09;

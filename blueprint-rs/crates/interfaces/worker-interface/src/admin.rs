//! Explicit tenant-bound operations for dead letters and terminal retention.
use crate::{AsyncWorkerQueue, JobSnapshot, RetryPolicy};
use forklaunch_core::domain::BlueprintResult;
use std::{future::Future, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalStatus {
    Completed,
    Failed,
}
impl TerminalStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Administration acts only on the bound tenant and requested queue. Purging
/// removes identities permanently; those identities can subsequently be reused.
pub trait AsyncWorkerAdmin: AsyncWorkerQueue {
    /// Returns up to `limit` failed jobs, oldest terminal transition first.
    /// # Errors
    /// Rejects invalid queue names, limits outside 1..=1000, or storage failures.
    fn dead_letters(
        &self,
        queue: &str,
        limit: usize,
    ) -> impl Future<Output = BlueprintResult<Vec<JobSnapshot>>> + Send;

    /// Atomically requeues a failed job with an explicit new retry budget,
    /// clearing its attempts, failure reason, and obsolete lease capability.
    /// Returns false if the identity is missing or is not currently failed.
    /// # Errors
    /// Rejects invalid names, retry policies, or storage failures.
    fn retry_dead_letter(
        &self,
        queue: &str,
        id: &str,
        policy: RetryPolicy,
    ) -> impl Future<Output = BlueprintResult<bool>> + Send;

    /// Deletes at most `limit` jobs of the selected terminal status whose last
    /// terminal transition is at least `minimum_age` old. Never deletes queued
    /// or processing jobs. Repeat bounded calls to enforce a retention policy.
    /// # Errors
    /// Rejects invalid names, limits outside 1..=1000, ages below 1 ms or above
    /// 3650 days, and storage failures.
    fn purge_terminal(
        &self,
        queue: &str,
        status: TerminalStatus,
        minimum_age: Duration,
        limit: usize,
    ) -> impl Future<Output = BlueprintResult<usize>> + Send;
}

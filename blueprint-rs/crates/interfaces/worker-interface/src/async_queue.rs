//! Durable queues use leases to fence acknowledgments from expired consumers.
use crate::{Job, JobReceipt, JobStatus, QueueStats};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use std::{future::Future, time::Duration};

/// A claimed delivery. Treat the opaque token as a capability: only its current
/// holder may acknowledge, fail, or renew this delivery before it expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobLease {
    pub job: Job,
    pub token: String,
    pub expires_at_unix_ms: i64,
}

/// Durable state without the lease capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSnapshot {
    pub job: Job,
    pub status: JobStatus,
    pub last_error: Option<String>,
    pub lease_expires_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureDisposition {
    RetryScheduled,
    DeadLetter,
    /// The token is obsolete or its lease has expired; no state was changed.
    LeaseLost,
}

/// Asynchronous, tenant-bound queue with at-least-once delivery. Handlers must be
/// idempotent: a crash after an external side effect can cause redelivery.
/// Attempts count claims, including deliveries whose consumer crashes.
pub trait AsyncWorkerQueue: Send + Sync {
    /// # Errors
    /// Returns an error for invalid input, duplicate identity, or storage failure.
    fn enqueue(&self, job: Job) -> impl Future<Output = BlueprintResult<JobReceipt>> + Send;

    /// Enqueues the entire batch atomically, preserving input order.
    /// # Errors
    /// Returns an error if any job is invalid/duplicate or storage fails.
    fn enqueue_batch(
        &self,
        jobs: Vec<Job>,
    ) -> impl Future<Output = BlueprintResult<Vec<JobReceipt>>> + Send;

    /// Claims a ready job or reclaims an expired delivery with a fresh token.
    /// # Errors
    /// Returns an error for invalid queue/lease duration or storage failure.
    fn claim(
        &self,
        queue: &str,
        lease_duration: Duration,
    ) -> impl Future<Output = BlueprintResult<Option<JobLease>>> + Send;

    /// Atomically claims up to `limit` ready deliveries in queue order, each
    /// with an independent capability. Durable adapters support 1..=1000.
    /// The default supports one delivery so existing adapters remain compatible;
    /// implementations must override this method to offer atomic batches.
    /// # Errors
    /// Returns invalid input, unsupported batching, or storage errors. No
    /// delivery is acquired when the default rejects a larger batch.
    fn claim_batch(
        &self,
        queue: &str,
        lease_duration: Duration,
        limit: usize,
    ) -> impl Future<Output = BlueprintResult<Vec<JobLease>>> + Send {
        async move {
            if !(1..=1000).contains(&limit) {
                return Err(BlueprintError::new(
                    "worker_invalid_input",
                    "Invalid worker queue input",
                ));
            }
            if limit != 1 {
                return Err(BlueprintError::new(
                    "worker_batch_unsupported",
                    "This queue does not support atomic batch claims",
                ));
            }
            Ok(self
                .claim(queue, lease_duration)
                .await?
                .into_iter()
                .collect())
        }
    }

    /// Returns false for an expired, obsolete, or already completed lease.
    /// # Errors
    /// Returns an error on storage failure.
    fn ack(&self, lease: &JobLease) -> impl Future<Output = BlueprintResult<bool>> + Send;

    /// Schedules a retry or retains the exhausted job as a dead letter.
    /// # Errors
    /// Returns an error for invalid failure text or storage failure.
    fn fail(
        &self,
        lease: &JobLease,
        reason: &str,
    ) -> impl Future<Output = BlueprintResult<FailureDisposition>> + Send;

    /// Extends the current lease without shortening its existing deadline.
    /// # Errors
    /// Returns an error for invalid duration or storage failure.
    fn renew(
        &self,
        lease: &JobLease,
        lease_duration: Duration,
    ) -> impl Future<Output = BlueprintResult<bool>> + Send;

    /// Returns ready jobs in delivery order without acquiring a lease.
    /// # Errors
    /// Returns an error for invalid queue/limit or storage failure.
    fn peek(
        &self,
        queue: &str,
        limit: usize,
    ) -> impl Future<Output = BlueprintResult<Vec<Job>>> + Send;

    /// Queued includes delayed retries; processing includes expired leases until
    /// the next claim reclaims or dead-letters them.
    /// # Errors
    /// Returns an error for invalid queue or storage failure.
    fn stats(&self, queue: &str) -> impl Future<Output = BlueprintResult<QueueStats>> + Send;

    /// Reads a job, including completed jobs and dead letters, without its token.
    /// # Errors
    /// Returns an error for invalid identity or storage failure.
    fn inspect(
        &self,
        queue: &str,
        id: &str,
    ) -> impl Future<Output = BlueprintResult<Option<JobSnapshot>>> + Send;
}

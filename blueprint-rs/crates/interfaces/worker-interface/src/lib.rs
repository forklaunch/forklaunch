#![forbid(unsafe_code)]

#[cfg(feature = "encrypted")]
pub mod encrypted;
#[cfg(feature = "encrypted")]
pub use encrypted::*;

mod admin;
pub use admin::{AsyncWorkerAdmin, TerminalStatus};

mod async_queue;
pub use async_queue::{AsyncWorkerQueue, FailureDisposition, JobLease, JobSnapshot};

use forklaunch_core::domain::BlueprintResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub queue: String,
    pub payload_json: String,
    pub attempts: u32,
}

impl Job {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        queue: impl Into<String>,
        payload_json: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            queue: queue.into(),
            payload_json: payload_json.into(),
            attempts: 0,
        }
    }

    #[must_use]
    pub const fn with_attempts(mut self, attempts: u32) -> Self {
        self.attempts = attempts;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Processing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobReceipt {
    pub id: String,
    pub queue: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueStats {
    pub queue: String,
    pub queued: usize,
    pub processing: usize,
}

/// A retry policy applied to a failing job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_seconds: u64,
}

impl RetryPolicy {
    #[must_use]
    pub const fn new(max_attempts: u32, backoff_seconds: u64) -> Self {
        Self {
            max_attempts,
            backoff_seconds,
        }
    }

    /// Returns whether another attempt is permitted after `attempts` tries.
    #[must_use]
    pub const fn should_retry(self, attempts: u32) -> bool {
        attempts < self.max_attempts
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(3, 30)
    }
}

/// A recurring schedule for a job, expressed as a cron expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    pub queue: String,
    pub cron: String,
}

/// The terminal outcome of processing a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerResult {
    Completed { job_id: String },
    Failed { job_id: String, reason: String },
}

/// A distributed coordination signal broadcast to workers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributedSignal {
    Pause,
    Resume,
    Drain,
}

impl DistributedSignal {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Drain => "drain",
        }
    }
}

pub trait WorkerQueue {
    /// # Errors
    ///
    /// Returns an error when the queue backend rejects the enqueue operation.
    fn enqueue(&self, job: Job) -> BlueprintResult<JobReceipt>;

    /// # Errors
    ///
    /// Returns an error when the queue backend rejects the dequeue operation.
    fn dequeue(&self, queue: &str) -> BlueprintResult<Option<Job>>;

    /// # Errors
    ///
    /// Returns an error when the queue backend cannot acknowledge the job.
    fn ack(&self, job_id: &str) -> BlueprintResult<()>;

    /// # Errors
    ///
    /// Returns an error when the queue backend cannot mark the job as failed.
    fn fail(&self, job_id: &str, reason: &str) -> BlueprintResult<()>;

    /// # Errors
    ///
    /// Returns an error when the queue backend cannot read queue statistics.
    fn stats(&self, queue: &str) -> BlueprintResult<QueueStats>;
}

/// A coordination channel for broadcasting [`DistributedSignal`]s to workers.
pub trait SignalBus {
    /// # Errors
    ///
    /// Returns an error when the signal cannot be broadcast.
    fn broadcast(&self, signal: DistributedSignal) -> BlueprintResult<()>;

    /// Drains and returns all pending signals for this worker.
    ///
    /// # Errors
    ///
    /// Returns an error when pending signals cannot be read.
    fn drain(&self) -> BlueprintResult<Vec<DistributedSignal>>;
}

/// Asynchronous broadcast control with adapter-owned subscriptions.
/// Broadcast delivery counts subscribers, not completed control applications.
/// Adapters must document persistence, disconnect and overflow behavior.
pub trait AsyncSignalBus: Send {
    /// # Errors
    /// Returns an error when publishing fails or the subscription is unhealthy.
    fn broadcast(
        &self,
        signal: DistributedSignal,
    ) -> impl std::future::Future<Output = BlueprintResult<usize>> + Send;
    /// Waits for one signal; `None` means the bounded wait elapsed.
    /// # Errors
    /// Returns an error on disconnect, malformed messages or buffer overflow.
    fn next_signal(
        &mut self,
        wait: std::time::Duration,
    ) -> impl std::future::Future<Output = BlueprintResult<Option<DistributedSignal>>> + Send;
    /// Drains immediately available signals in publication order.
    /// # Errors
    /// Returns an error when subscription health cannot be established.
    fn drain_signals(&mut self) -> BlueprintResult<Vec<DistributedSignal>>;
}

/// Worker event contracts used to emit `AsyncAPI` documents for job channels.
pub mod events {
    use forklaunch_core::asyncapi::{AsyncApiInfo, generate_asyncapi_json};
    use forklaunch_ws::{EventContract, MessageDirection, WebSocketContract};

    /// Builds the worker job event channel contract.
    #[must_use]
    pub fn channel() -> WebSocketContract {
        WebSocketContract::new("worker/jobs")
            .with_event(EventContract::new(
                "job.submitted",
                MessageDirection::ClientToServer,
                "Job",
            ))
            .with_event(EventContract::new(
                "job.completed",
                MessageDirection::ServerToClient,
                "WorkerResult",
            ))
            .with_event(EventContract::new(
                "job.failed",
                MessageDirection::ServerToClient,
                "WorkerResult",
            ))
            .with_error_schema("WorkerError")
    }

    /// Generates the `AsyncAPI` 3.0 document for the worker job channel.
    #[must_use]
    pub fn asyncapi_json() -> String {
        generate_asyncapi_json(&AsyncApiInfo::new("worker", "0.1.0"), &channel())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_builder_tracks_attempts() {
        let job = Job::new("job_1", "email", "{\"ok\":true}").with_attempts(2);

        assert_eq!(job.id, "job_1");
        assert_eq!(job.attempts, 2);
    }

    #[test]
    fn retry_policy_caps_attempts() {
        let policy = RetryPolicy::new(3, 10);
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }

    #[test]
    fn worker_events_generate_asyncapi() {
        let document = events::asyncapi_json();
        assert!(document.contains("\"asyncapi\":\"3.0.0\""));
        assert!(document.contains("\"job.submitted\":{\"action\":\"receive\""));
        assert!(document.contains("\"job.completed\":{\"action\":\"send\""));
    }
}

//! Opt-in, bounded terminal retention coordinated with worker shutdown.
use crate::control::WorkerControl;
use crate::runner::{self, JobHandler, RunReport, RunnerConfig};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_interface::{AsyncWorkerAdmin, TerminalStatus};
use std::{future::Future, time::Duration};
use tokio::{
    sync::watch,
    time::{Instant, MissedTickBehavior, interval_at, timeout},
};

#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// None retains completed jobs indefinitely.
    pub completed_age: Option<Duration>,
    /// None retains failed jobs indefinitely.
    pub failed_age: Option<Duration>,
    pub interval: Duration,
    /// Maximum deletions per status on each tick (1..=1000).
    pub batch_limit: usize,
    pub operation_timeout: Duration,
}
impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            completed_age: None,
            failed_age: None,
            interval: Duration::from_secs(60),
            batch_limit: 100,
            operation_timeout: Duration::from_secs(5),
        }
    }
}
impl RetentionPolicy {
    /// # Errors
    /// Rejects invalid ages, limits, scheduling intervals, or operation timeouts.
    pub fn validate(&self) -> BlueprintResult<()> {
        if !(1..=1000).contains(&self.batch_limit)
            || self.interval < Duration::from_millis(10)
            || self.interval > Duration::from_hours(24)
            || self.operation_timeout < Duration::from_millis(1)
            || self.operation_timeout > Duration::from_secs(60)
            || [self.completed_age, self.failed_age]
                .into_iter()
                .flatten()
                .any(|age| age < Duration::from_millis(1) || age > Duration::from_hours(87600))
        {
            return Err(BlueprintError::new(
                "worker_invalid_retention",
                "Invalid worker retention policy",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RetentionReport {
    pub completed_purged: u64,
    pub failed_purged: u64,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RetainedRunReport {
    pub worker: RunReport,
    pub retention: RetentionReport,
}

/// Enforces only explicitly configured terminal ages, starting after the first
/// interval. Each tick performs at most one bounded purge per enabled status.
/// Shutdown cancels in-flight maintenance; it never waits for a full interval.
/// # Errors
/// Returns invalid configuration, storage failures, or bounded-operation errors.
pub async fn run<Q, F>(
    queue: &Q,
    queue_name: &str,
    policy: &RetentionPolicy,
    shutdown: F,
) -> BlueprintResult<RetentionReport>
where
    Q: AsyncWorkerAdmin,
    F: Future<Output = ()> + Send,
{
    policy.validate()?;
    RunnerConfig::new(queue_name).validate()?;
    tokio::pin!(shutdown);
    let mut ticks = interval_at(Instant::now() + policy.interval, policy.interval);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut report = RetentionReport::default();
    loop {
        tokio::select! {
            biased;
            () = &mut shutdown => return Ok(report),
            _ = ticks.tick() => {}
        }
        for (status, age) in [
            (TerminalStatus::Completed, policy.completed_age),
            (TerminalStatus::Failed, policy.failed_age),
        ] {
            let Some(age) = age else {
                continue;
            };
            let count = tokio::select! {
                biased;
                () = &mut shutdown => return Ok(report),
                result = timeout(policy.operation_timeout,queue.purge_terminal(queue_name,status,age,policy.batch_limit)) => {
                    result.map_err(|_| BlueprintError::new("worker_retention_timeout","Worker retention operation timed out"))??
                }
            };
            match status {
                TerminalStatus::Completed => report.completed_purged += count as u64,
                TerminalStatus::Failed => report.failed_purged += count as u64,
            }
        }
    }
}

async fn stopped(mut receiver: watch::Receiver<bool>) {
    if !*receiver.borrow() {
        let _ = receiver.changed().await;
    }
}

/// Runs retention alongside job consumers. Shutdown stops maintenance and drains
/// active handlers with lease renewal. A failure in either component stops the
/// other and drains handlers before returning the first observed error.
/// # Errors
/// Returns invalid configuration or worker/retention storage and timeout errors.
pub async fn run_worker<Q, H, F>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    policy: &RetentionPolicy,
    shutdown: F,
) -> BlueprintResult<RetainedRunReport>
where
    Q: AsyncWorkerAdmin,
    H: JobHandler,
    F: Future<Output = ()> + Send,
{
    run_worker_controlled(
        queue,
        handler,
        config,
        policy,
        &WorkerControl::new(),
        shutdown,
    )
    .await
}

/// Coordinates controlled consumers and retention. Pausing leaves maintenance
/// enabled; draining stops maintenance immediately, before handlers finish.
/// # Errors
/// Returns invalid configuration or worker/retention errors after handler drain.
pub async fn run_worker_controlled<Q, H, F>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    policy: &RetentionPolicy,
    control: &WorkerControl,
    shutdown: F,
) -> BlueprintResult<RetainedRunReport>
where
    Q: AsyncWorkerAdmin,
    H: JobHandler,
    F: Future<Output = ()> + Send,
{
    config.validate()?;
    policy.validate()?;
    let (stop, receiver) = watch::channel(false);
    let worker = runner::run_controlled(queue, handler, config, control, stopped(receiver.clone()));
    let maintenance = run(queue, &config.queue, policy, stopped(receiver));
    let shutdown = async {
        tokio::select! { () = shutdown => {}, () = control.draining() => {} }
    };
    tokio::pin!(worker, maintenance, shutdown);
    let mut report = RetainedRunReport::default();
    let mut first_error = None;
    let (mut worker_done, mut maintenance_done, mut stopping) = (false, false, false);
    while !worker_done || !maintenance_done {
        tokio::select! {
            biased;
            () = &mut shutdown, if !stopping => { stopping = true; control.drain(); stop.send_replace(true); }
            result = &mut worker, if !worker_done => {
                worker_done = true;
                stopping = true;
                control.drain();
                stop.send_replace(true);
                match result { Ok(value) => report.worker = value, Err(error) => { if first_error.is_none() { first_error = Some(error); } } }
            }
            result = &mut maintenance, if !maintenance_done => {
                maintenance_done = true;
                stopping = true;
                control.drain();
                stop.send_replace(true);
                match result { Ok(value) => report.retention = value, Err(error) => { if first_error.is_none() { first_error = Some(error); } } }
            }
        }
    }
    first_error.map_or(Ok(report), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_worker_interface::{
        AsyncWorkerQueue, FailureDisposition, Job, JobLease, JobReceipt, JobSnapshot, QueueStats,
        RetryPolicy,
    };
    struct StallingAdmin(tokio::sync::Notify);
    impl AsyncWorkerQueue for StallingAdmin {
        async fn enqueue(&self, _job: Job) -> BlueprintResult<JobReceipt> {
            panic!("unexpected enqueue")
        }
        async fn enqueue_batch(&self, _jobs: Vec<Job>) -> BlueprintResult<Vec<JobReceipt>> {
            panic!("unexpected enqueue")
        }
        async fn claim(
            &self,
            _queue: &str,
            _duration: Duration,
        ) -> BlueprintResult<Option<JobLease>> {
            panic!("unexpected claim")
        }
        async fn ack(&self, _lease: &JobLease) -> BlueprintResult<bool> {
            panic!("unexpected acknowledgment")
        }
        async fn fail(
            &self,
            _lease: &JobLease,
            _reason: &str,
        ) -> BlueprintResult<FailureDisposition> {
            panic!("unexpected failure")
        }
        async fn renew(&self, _lease: &JobLease, _duration: Duration) -> BlueprintResult<bool> {
            panic!("unexpected renewal")
        }
        async fn peek(&self, _queue: &str, _limit: usize) -> BlueprintResult<Vec<Job>> {
            panic!("unexpected peek")
        }
        async fn stats(&self, _queue: &str) -> BlueprintResult<QueueStats> {
            panic!("unexpected stats")
        }
        async fn inspect(&self, _queue: &str, _id: &str) -> BlueprintResult<Option<JobSnapshot>> {
            panic!("unexpected inspection")
        }
    }
    impl AsyncWorkerAdmin for StallingAdmin {
        async fn dead_letters(
            &self,
            _queue: &str,
            _limit: usize,
        ) -> BlueprintResult<Vec<JobSnapshot>> {
            panic!("unexpected listing")
        }
        async fn retry_dead_letter(
            &self,
            _queue: &str,
            _id: &str,
            _policy: RetryPolicy,
        ) -> BlueprintResult<bool> {
            panic!("unexpected retry")
        }
        async fn purge_terminal(
            &self,
            _queue: &str,
            _status: TerminalStatus,
            _age: Duration,
            _limit: usize,
        ) -> BlueprintResult<usize> {
            self.0.notify_one();
            std::future::pending().await
        }
    }
    #[tokio::test]
    async fn retention_bounds_stalled_storage_operations() {
        let queue = StallingAdmin(tokio::sync::Notify::new());
        let policy = RetentionPolicy {
            completed_age: Some(Duration::from_millis(1)),
            interval: Duration::from_millis(10),
            operation_timeout: Duration::from_millis(2),
            ..RetentionPolicy::default()
        };
        let result = timeout(
            Duration::from_secs(1),
            run(&queue, "jobs", &policy, std::future::pending()),
        )
        .await
        .unwrap();
        assert_eq!(result.unwrap_err().code, "worker_retention_timeout");
    }
    #[tokio::test]
    async fn shutdown_cancels_stalled_retention_without_waiting_for_its_timeout() {
        let queue = StallingAdmin(tokio::sync::Notify::new());
        let policy = RetentionPolicy {
            failed_age: Some(Duration::from_millis(1)),
            interval: Duration::from_millis(10),
            operation_timeout: Duration::from_secs(60),
            ..RetentionPolicy::default()
        };
        let report = timeout(
            Duration::from_secs(1),
            run(&queue, "jobs", &policy, async {
                queue.0.notified().await;
            }),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(report, RetentionReport::default());
    }
    #[test]
    fn retention_defaults_preserve_terminal_jobs_and_validate_bounds() {
        let mut policy = RetentionPolicy::default();
        assert!(policy.validate().is_ok());
        assert_eq!(policy.completed_age, None);
        assert_eq!(policy.failed_age, None);
        policy.completed_age = Some(Duration::ZERO);
        assert!(policy.validate().is_err());
        policy.completed_age = Some(Duration::MAX);
        assert!(policy.validate().is_err());
        policy = RetentionPolicy::default();
        policy.batch_limit = 0;
        assert!(policy.validate().is_err());
        policy.batch_limit = 1001;
        assert!(policy.validate().is_err());
        policy = RetentionPolicy::default();
        policy.interval = Duration::from_millis(9);
        assert!(policy.validate().is_err());
        policy = RetentionPolicy::default();
        policy.operation_timeout = Duration::ZERO;
        assert!(policy.validate().is_err());
    }
}

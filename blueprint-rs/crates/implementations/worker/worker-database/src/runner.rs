//! Polling and graceful drain for asynchronous, leased worker queues.
use crate::control::{WorkerControl, WorkerState};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_interface::{AsyncWorkerQueue, FailureDisposition, Job, JobLease};
use futures_util::{StreamExt, stream::FuturesUnordered};
use std::{collections::BTreeMap, future::Future, time::Duration};
use tokio::sync::watch;
use tokio::time::{Instant, MissedTickBehavior, interval_at, timeout};

/// Application handlers must be idempotent and keep all work inside this future
/// so cancellation stops processing when a lease is lost or drain times out.
pub trait JobHandler: Send + Sync {
    /// # Errors
    /// Returns an application failure; the runner records its code and retries.
    fn process(&self, job: Job) -> impl Future<Output = BlueprintResult<()>> + Send;

    /// Processes a claimed batch and returns only its failed job identities.
    /// The compatibility implementation invokes the single-job handler in order.
    /// Override this method for bulk application work; successes are omitted.
    /// # Errors
    /// A whole-batch error retries every delivery. Unknown or duplicate failure
    /// identities invalidate the result and retry every delivery as well.
    fn process_batch(
        &self,
        jobs: Vec<Job>,
    ) -> impl Future<Output = BlueprintResult<Vec<JobFailure>>> + Send {
        async move {
            let mut failures = Vec::new();
            for job in jobs {
                let id = job.id.clone();
                if let Err(error) = self.process(job).await {
                    failures.push(JobFailure { id, error });
                }
            }
            Ok(failures)
        }
    }
}

/// One failed job in a batch; messages are never persisted by the runner.
#[derive(Debug, Clone)]
pub struct JobFailure {
    pub id: String,
    pub error: BlueprintError,
}

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub queue: String,
    /// Maximum concurrent handler batches in this process (1..=256).
    pub concurrency: usize,
    /// Maximum deliveries per handler batch. Total in flight is capped at 1000.
    pub batch_size: usize,
    pub poll_interval: Duration,
    pub lease_duration: Duration,
    pub renew_interval: Duration,
    pub drain_timeout: Duration,
}

impl RunnerConfig {
    #[must_use]
    pub fn new(queue: impl Into<String>) -> Self {
        Self {
            queue: queue.into(),
            concurrency: 1,
            batch_size: 1,
            poll_interval: Duration::from_millis(250),
            lease_duration: Duration::from_secs(30),
            renew_interval: Duration::from_secs(10),
            drain_timeout: Duration::from_secs(30),
        }
    }

    /// # Errors
    /// Rejects invalid queue names, time bounds, and concurrency limits.
    pub fn validate(&self) -> BlueprintResult<()> {
        if !(1..=256).contains(&self.concurrency)
            || !(1..=1000).contains(&self.batch_size)
            || self.concurrency * self.batch_size > 1000
            || self.queue.trim().is_empty()
            || self.queue.len() > 256
            || self.queue.chars().any(char::is_control)
            || self.poll_interval.is_zero()
            || self.poll_interval > Duration::from_secs(60)
            || self.lease_duration < Duration::from_millis(3)
            || self.lease_duration > Duration::from_hours(24)
            || self.renew_interval < Duration::from_millis(1)
            || self.renew_interval > self.lease_duration / 3
            || self.drain_timeout.is_zero()
            || self.drain_timeout > Duration::from_hours(24)
        {
            return Err(BlueprintError::new(
                "worker_invalid_config",
                "Invalid worker runner configuration",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RunReport {
    pub completed: u64,
    pub retried: u64,
    pub dead_lettered: u64,
    pub lease_lost: u64,
    pub abandoned: u64,
}

#[derive(Clone, Copy)]
enum Outcome {
    Completed,
    Retried,
    DeadLettered,
    LeaseLost,
}

impl RunReport {
    fn merge(&mut self, other: Self) {
        self.completed += other.completed;
        self.retried += other.retried;
        self.dead_lettered += other.dead_lettered;
        self.lease_lost += other.lease_lost;
        self.abandoned += other.abandoned;
    }

    fn record(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Completed => self.completed += 1,
            Outcome::Retried => self.retried += 1,
            Outcome::DeadLettered => self.dead_lettered += 1,
            Outcome::LeaseLost => self.lease_lost += 1,
        }
    }
}

fn operation_timeout() -> BlueprintError {
    BlueprintError::new(
        "worker_operation_timeout",
        "Worker queue operation timed out",
    )
}

/// Runs up to the configured number of concurrent handler batches, renewing each
/// lease while its handler runs. Shutdown stops new claims and drains all active
/// handlers while continuing renewal. A drain timeout cancels the handler and
/// leaves its delivery recoverable after lease expiry. A queue error stops all
/// consumers and drains their active work before returning the first error.
///
/// # Errors
/// Returns an error for invalid configuration or queue failure/timeout. Handler
/// errors instead schedule retries or retain a failed job after exhaustion.
pub async fn run<Q, H, F>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    shutdown: F,
) -> BlueprintResult<RunReport>
where
    Q: AsyncWorkerQueue,
    H: JobHandler,
    F: Future<Output = ()> + Send,
{
    run_controlled(queue, handler, config, &WorkerControl::new(), shutdown).await
}

/// Runs with process-local pause/resume/drain control. Pause preserves active
/// handlers and claims already submitted to storage. Drain cannot be reversed.
/// # Errors
/// Returns invalid configuration or queue failures/timeouts after active drain.
pub async fn run_controlled<Q, H, F>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    control: &WorkerControl,
    shutdown: F,
) -> BlueprintResult<RunReport>
where
    Q: AsyncWorkerQueue,
    H: JobHandler,
    F: Future<Output = ()> + Send,
{
    config.validate()?;
    let (stop, receiver) = watch::channel(false);
    let mut consumers: FuturesUnordered<_> = (0..config.concurrency)
        .map(|_| {
            let mut receiver = receiver.clone();
            run_single(queue, handler, config, control, async move {
                if !*receiver.borrow() {
                    let _ = receiver.changed().await;
                }
            })
        })
        .collect();
    let shutdown = async {
        tokio::select! { () = shutdown => {}, () = control.draining() => {} }
    };
    tokio::pin!(shutdown);
    let mut stopping = false;
    let mut first_error = None;
    let mut report = RunReport::default();
    loop {
        tokio::select! {
            biased;
            () = &mut shutdown, if !stopping => {
                stopping = true;
                control.drain();
                stop.send_replace(true);
            }
            result = consumers.next() => {
                let Some(result) = result else { break; };
                match result {
                    Ok(partial) => {
                        report.merge(partial);
                    }
                    Err(error) => {
                        if first_error.is_none() { first_error = Some(error); }
                        stopping = true;
                        control.drain();
                        stop.send_replace(true);
                    }
                }
            }
        }
    }
    first_error.map_or(Ok(report), Err)
}

async fn run_single<Q, H, F>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    control: &WorkerControl,
    shutdown: F,
) -> BlueprintResult<RunReport>
where
    Q: AsyncWorkerQueue,
    H: JobHandler,
    F: Future<Output = ()> + Send,
{
    tokio::pin!(shutdown);
    let mut report = RunReport::default();
    let mut state = control.subscribe();
    loop {
        loop {
            let current = *state.borrow_and_update();
            match current {
                WorkerState::Running => break,
                WorkerState::Draining => return Ok(report),
                WorkerState::Paused => {
                    tokio::select! {
                        biased;
                        () = &mut shutdown => return Ok(report),
                        result = state.changed() => { if result.is_err() { return Ok(report); } }
                    }
                }
            }
        }
        let leases = tokio::select! {
            biased;
            () = &mut shutdown => return Ok(report),
            result = timeout(config.renew_interval, queue.claim_batch(&config.queue, config.lease_duration, config.batch_size)) => {
                result.map_err(|_| operation_timeout())??
            }
        };
        if leases.is_empty() {
            tokio::select! {
                biased;
                () = &mut shutdown => return Ok(report),
                _ = state.changed() => {},
                () = tokio::time::sleep(config.poll_interval) => {},
            }
            continue;
        }
        let processing = process(queue, handler, config, &leases);
        tokio::pin!(processing);
        tokio::select! {
            biased;
            () = &mut shutdown => {
                match timeout(config.drain_timeout, &mut processing).await {
                    Ok(result) => report.merge(result?),
                    Err(_) => report.abandoned += leases.len() as u64,
                }
                return Ok(report);
            }
            result = &mut processing => report.merge(result?),
        }
    }
}

fn safe_reason(error: &BlueprintError) -> String {
    error
        .code
        .chars()
        .filter(|ch| !ch.is_control())
        .take(256)
        .collect()
}

fn failure_reasons(
    leases: &[JobLease],
    result: BlueprintResult<Vec<JobFailure>>,
) -> Vec<Option<String>> {
    match result {
        Err(error) => vec![Some(safe_reason(&error)); leases.len()],
        Ok(failures) => {
            let mut reasons = BTreeMap::new();
            for failure in failures {
                if !leases.iter().any(|lease| lease.job.id == failure.id)
                    || reasons
                        .insert(failure.id, safe_reason(&failure.error))
                        .is_some()
                {
                    return vec![Some("worker_invalid_batch_result".to_owned()); leases.len()];
                }
            }
            leases
                .iter()
                .map(|lease| reasons.remove(&lease.job.id))
                .collect()
        }
    }
}

async fn renew_batch<Q: AsyncWorkerQueue>(
    queue: &Q,
    config: &RunnerConfig,
    leases: &[JobLease],
) -> BlueprintResult<u64> {
    let renewals = async {
        let mut pending: FuturesUnordered<_> = leases
            .iter()
            .map(|lease| queue.renew(lease, config.lease_duration))
            .collect();
        let mut lost = 0;
        while let Some(renewed) = pending.next().await {
            if !renewed? {
                lost += 1;
            }
        }
        Ok(lost)
    };
    timeout(config.renew_interval, renewals)
        .await
        .map_err(|_| operation_timeout())?
}

async fn process<Q: AsyncWorkerQueue, H: JobHandler>(
    queue: &Q,
    handler: &H,
    config: &RunnerConfig,
    leases: &[JobLease],
) -> BlueprintResult<RunReport> {
    let work = async {
        if config.batch_size == 1 {
            handler
                .process(leases[0].job.clone())
                .await
                .map(|()| Vec::new())
        } else {
            handler
                .process_batch(leases.iter().map(|lease| lease.job.clone()).collect())
                .await
        }
    };
    tokio::pin!(work);
    let mut renewal = interval_at(
        Instant::now() + config.renew_interval,
        config.renew_interval,
    );
    renewal.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let result = loop {
        tokio::select! {
            biased;
            result = &mut work => break result,
            _ = renewal.tick() => {
                let lost = renew_batch(queue, config, leases).await?;
                if lost>0 { return Ok(RunReport {lease_lost:lost,abandoned:leases.len() as u64-lost,..RunReport::default()}); }
            }
        }
    };
    // Validate the complete result before acknowledging any batch member.
    let reasons = failure_reasons(leases, result);
    let settlement = async {
        let mut pending: FuturesUnordered<_> = leases
            .iter()
            .zip(&reasons)
            .map(|(lease, reason)| async move {
                if let Some(reason) = reason {
                    Ok::<_, BlueprintError>(match queue.fail(lease, reason).await? {
                        FailureDisposition::RetryScheduled => Outcome::Retried,
                        FailureDisposition::DeadLetter => Outcome::DeadLettered,
                        FailureDisposition::LeaseLost => Outcome::LeaseLost,
                    })
                } else {
                    Ok(if queue.ack(lease).await? {
                        Outcome::Completed
                    } else {
                        Outcome::LeaseLost
                    })
                }
            })
            .collect();
        let mut report = RunReport::default();
        while let Some(result) = pending.next().await {
            report.record(result?);
        }
        Ok(report)
    };
    timeout(config.renew_interval, settlement)
        .await
        .map_err(|_| operation_timeout())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_timing_that_cannot_renew_safely() {
        let mut config = RunnerConfig::new("email");
        assert!(config.validate().is_ok());
        config.renew_interval = config.lease_duration;
        assert!(config.validate().is_err());
        config = RunnerConfig::new("email");
        config.drain_timeout = Duration::ZERO;
        assert!(config.validate().is_err());
        config = RunnerConfig::new("\n");
        assert!(config.validate().is_err());
        config = RunnerConfig::new("email");
        config.concurrency = 0;
        assert!(config.validate().is_err());
        config.concurrency = 257;
        assert!(config.validate().is_err());
        config = RunnerConfig::new("email");
        config.batch_size = 0;
        assert!(config.validate().is_err());
        config.batch_size = 1001;
        assert!(config.validate().is_err());
        config.batch_size = 1000;
        assert!(config.validate().is_ok());
        config.concurrency = 2;
        assert!(config.validate().is_err());
        config = RunnerConfig::new("email");
        config.poll_interval = Duration::ZERO;
        assert!(config.validate().is_err());
    }
}

#[cfg(test)]
mod batch_fencing_tests {
    use super::*;
    use forklaunch_worker_interface::{JobReceipt, JobSnapshot, QueueStats};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    struct FencedQueue {
        claimed: AtomicBool,
        acknowledgments: AtomicUsize,
        renewed: tokio::sync::Notify,
    }
    impl AsyncWorkerQueue for FencedQueue {
        async fn enqueue(&self, _job: Job) -> BlueprintResult<JobReceipt> {
            unreachable!()
        }
        async fn enqueue_batch(&self, _jobs: Vec<Job>) -> BlueprintResult<Vec<JobReceipt>> {
            unreachable!()
        }
        async fn claim(
            &self,
            _queue: &str,
            _duration: Duration,
        ) -> BlueprintResult<Option<JobLease>> {
            unreachable!()
        }
        async fn claim_batch(
            &self,
            _queue: &str,
            _duration: Duration,
            _limit: usize,
        ) -> BlueprintResult<Vec<JobLease>> {
            if self.claimed.swap(true, Ordering::SeqCst) {
                return Ok(Vec::new());
            }
            Ok(["lost", "valid"]
                .into_iter()
                .map(|id| JobLease {
                    job: Job::new(id, "jobs", "{}"),
                    token: id.into(),
                    expires_at_unix_ms: i64::MAX,
                })
                .collect())
        }
        async fn ack(&self, _lease: &JobLease) -> BlueprintResult<bool> {
            self.acknowledgments.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
        async fn fail(
            &self,
            _lease: &JobLease,
            _reason: &str,
        ) -> BlueprintResult<FailureDisposition> {
            unreachable!()
        }
        async fn renew(&self, lease: &JobLease, _duration: Duration) -> BlueprintResult<bool> {
            self.renewed.notify_one();
            Ok(lease.job.id != "lost")
        }
        async fn peek(&self, _queue: &str, _limit: usize) -> BlueprintResult<Vec<Job>> {
            unreachable!()
        }
        async fn stats(&self, _queue: &str) -> BlueprintResult<QueueStats> {
            unreachable!()
        }
        async fn inspect(&self, _queue: &str, _id: &str) -> BlueprintResult<Option<JobSnapshot>> {
            unreachable!()
        }
    }
    struct Handler(Arc<AtomicBool>);
    struct Active(Arc<AtomicBool>);
    impl Drop for Active {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    impl JobHandler for Handler {
        async fn process(&self, _job: Job) -> BlueprintResult<()> {
            unreachable!()
        }
        async fn process_batch(&self, _jobs: Vec<Job>) -> BlueprintResult<Vec<JobFailure>> {
            self.0.store(true, Ordering::SeqCst);
            let _active = Active(Arc::clone(&self.0));
            std::future::pending().await
        }
    }
    #[tokio::test]
    async fn one_lost_delivery_cancels_the_entire_batch_without_acknowledging_others() {
        let queue = FencedQueue {
            claimed: AtomicBool::new(false),
            acknowledgments: AtomicUsize::new(0),
            renewed: tokio::sync::Notify::new(),
        };
        let active = Arc::new(AtomicBool::new(false));
        let handler = Handler(Arc::clone(&active));
        let mut config = RunnerConfig::new("jobs");
        config.batch_size = 2;
        config.poll_interval = Duration::from_millis(1);
        config.lease_duration = Duration::from_millis(30);
        config.renew_interval = Duration::from_millis(10);
        let report = timeout(
            Duration::from_secs(1),
            run(&queue, &handler, &config, async {
                queue.renewed.notified().await;
            }),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(report.lease_lost, 1);
        assert_eq!(report.abandoned, 1);
        assert_eq!(report.completed, 0);
        assert_eq!(queue.acknowledgments.load(Ordering::SeqCst), 0);
        assert!(!active.load(Ordering::SeqCst));
    }
}

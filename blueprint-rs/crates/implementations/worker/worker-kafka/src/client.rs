//! Broker-backed, tenant-scoped Kafka delivery. Consumer offsets and retry/dead
//! letter publications commit together. External application effects remain
//! at-least-once and must be idempotent. No database lease semantics are implied.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::TenantContext;
use forklaunch_worker_database::{
    control::{WorkerControl, WorkerState},
    runner::JobHandler,
};
use forklaunch_worker_interface::{Job, JobReceipt, RetryPolicy};
use rdkafka::{
    ClientConfig, Message, Offset, TopicPartitionList,
    client::ClientContext,
    config::RDKafkaLogLevel,
    consumer::{
        BaseConsumer, Consumer, ConsumerContext, ConsumerGroupMetadata, Rebalance, StreamConsumer,
    },
    error::{KafkaError, KafkaResult, RDKafkaErrorCode},
    message::OwnedMessage,
    producer::{FutureProducer, FutureRecord, Producer},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

mod inspection;
pub use inspection::{KafkaDeadLetter, KafkaInspector, initialize_topics};

struct DrainOnExit<'a>(&'a WorkerControl);
impl Drop for DrainOnExit<'_> {
    fn drop(&mut self) {
        self.0.drain();
    }
}

/// Configuration is deliberately not printed, since client properties may hold secrets.
#[derive(Clone)]
pub struct KafkaOptions {
    pub brokers: String,
    pub namespace: String,
    pub tenant: TenantContext,
    pub queue: String,
    pub group: String,
    /// Stable, unique identity for one producer/worker replica; reuse on restart
    /// fences an earlier replica. Never share this identity across live replicas.
    pub transactional_id: String,
    pub operation_timeout: Duration,
    /// Total time `connect` keeps retrying retriable broker errors (for example a
    /// broker that is still starting, or the first transactional initialization
    /// that creates `__transaction_state`). Each attempt is bounded by
    /// `operation_timeout`.
    pub startup_timeout: Duration,
    pub session_timeout: Duration,
    pub max_payload_bytes: usize,
    /// librdkafka TLS/SASL and connection options. Delivery-critical settings are
    /// always applied after these properties and cannot be overridden.
    pub properties: BTreeMap<String, String>,
}
impl fmt::Debug for KafkaOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KafkaOptions").finish_non_exhaustive()
    }
}
impl KafkaOptions {
    #[must_use]
    pub fn new(
        brokers: impl Into<String>,
        namespace: impl Into<String>,
        tenant: TenantContext,
        queue: impl Into<String>,
        transactional_id: impl Into<String>,
    ) -> Self {
        Self {
            brokers: brokers.into(),
            namespace: namespace.into(),
            tenant,
            queue: queue.into(),
            group: "workers".into(),
            transactional_id: transactional_id.into(),
            operation_timeout: Duration::from_secs(5),
            startup_timeout: Duration::from_secs(60),
            session_timeout: Duration::from_secs(10),
            max_payload_bytes: 1024 * 1024,
            properties: BTreeMap::new(),
        }
    }
    /// # Errors
    /// Rejects invalid identities, unbounded limits, and credential-bearing broker URIs.
    pub fn validate(&self) -> BlueprintResult<()> {
        if !valid_name(&self.namespace)
            || !valid_name(&self.queue)
            || !valid_name(&self.group)
            || !valid_name(&self.transactional_id)
            || self.brokers.is_empty()
            || self.brokers.len() > 4096
            || self.brokers.contains('@')
            || self.brokers.contains("://")
            || self.brokers.chars().any(char::is_control)
            || self.operation_timeout < Duration::from_millis(100)
            || self.operation_timeout > Duration::from_secs(60)
            || self.startup_timeout < self.operation_timeout
            || self.startup_timeout > Duration::from_secs(600)
            || self.session_timeout < Duration::from_secs(6)
            || self.session_timeout > Duration::from_secs(300)
            || !(1..=8 * 1024 * 1024).contains(&self.max_payload_bytes)
        {
            return Err(invalid());
        }
        Ok(())
    }
    /// Deterministic names include namespace, trusted tenant and queue, with
    /// length-delimited input hashed to stay below Kafka's topic-name limit.
    #[must_use]
    pub fn topic(&self) -> String {
        scoped("flw", &[&self.namespace, self.tenant.id(), &self.queue])
    }
    #[must_use]
    pub fn dead_letter_topic(&self) -> String {
        format!("{}.dead", self.topic())
    }
    fn group_id(&self) -> String {
        scoped(
            "flg",
            &[&self.namespace, self.tenant.id(), &self.queue, &self.group],
        )
    }
    fn config(&self) -> ClientConfig {
        let mut config = ClientConfig::new();
        for (key, value) in &self.properties {
            config.set(key, value);
        }
        config
            .set("bootstrap.servers", &self.brokers)
            .set("client.id", "forklaunch-worker-interface")
            .set(
                "socket.timeout.ms",
                millis(self.operation_timeout).to_string(),
            )
            .set(
                "message.timeout.ms",
                millis(self.operation_timeout).to_string(),
            )
            .set("allow.auto.create.topics", "false");
        // On hosts where `localhost` resolves to `::1` first (macOS), an IPv4-only
        // broker port fails the first connection. Prefer IPv4 for loopback brokers
        // unless the caller chose an address family explicitly.
        if !self.properties.contains_key("broker.address.family") && self.loopback_only() {
            config.set("broker.address.family", "v4");
        }
        config
    }
    fn loopback_only(&self) -> bool {
        self.brokers.split(',').all(|broker| {
            let host = broker
                .trim()
                .rsplit_once(':')
                .map_or(broker.trim(), |(host, _)| host);
            host.eq_ignore_ascii_case("localhost")
        })
    }
}
fn scoped(prefix: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!(
        "{prefix}.{}",
        base16ct::lower::encode_string(&digest.finalize())
    )
}
fn valid_name(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn millis(value: Duration) -> u64 {
    u64::try_from(value.as_millis()).unwrap_or(u64::MAX)
}
fn invalid() -> BlueprintError {
    BlueprintError::new(
        "kafka_invalid_input",
        "Invalid Kafka worker configuration or message",
    )
}
fn storage() -> BlueprintError {
    BlueprintError::new("kafka_storage_error", "Kafka operation failed")
}
fn remaining(deadline: Instant) -> BlueprintResult<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or_else(storage)
}
fn rebalance_error() -> BlueprintError {
    BlueprintError::new(
        "kafka_assignment_lost",
        "Kafka assignment changed; restart the worker before processing more records",
    )
}

/// Routes librdkafka logs and client errors to `tracing` (target
/// `forklaunch_worker_kafka::librdkafka`). Messages are stripped of control
/// characters and truncated; librdkafka does not log credentials unless its
/// `debug` property is enabled, which callers opt into explicitly.
#[derive(Clone, Default)]
struct LoggingContext {
    epoch: Arc<AtomicU64>,
}
impl ClientContext for LoggingContext {
    fn log(&self, level: RDKafkaLogLevel, facility: &str, message: &str) {
        let message = sanitize(message);
        let facility = sanitize(facility);
        match level {
            RDKafkaLogLevel::Emerg | RDKafkaLogLevel::Alert | RDKafkaLogLevel::Critical => {
                tracing::error!(target: "forklaunch_worker_kafka::librdkafka", facility, "{message}");
            }
            // librdkafka reports recoverable connection failures at error level.
            RDKafkaLogLevel::Error | RDKafkaLogLevel::Warning => {
                tracing::warn!(target: "forklaunch_worker_kafka::librdkafka", facility, "{message}");
            }
            RDKafkaLogLevel::Notice => {
                tracing::info!(target: "forklaunch_worker_kafka::librdkafka", facility, "{message}");
            }
            RDKafkaLogLevel::Info => {
                tracing::debug!(target: "forklaunch_worker_kafka::librdkafka", facility, "{message}");
            }
            RDKafkaLogLevel::Debug => {
                tracing::trace!(target: "forklaunch_worker_kafka::librdkafka", facility, "{message}");
            }
        }
    }
    fn error(&self, error: KafkaError, reason: &str) {
        let reason = sanitize(reason);
        if is_transient(&error) {
            tracing::warn!(target: "forklaunch_worker_kafka::librdkafka", error = %error, "{reason}");
        } else {
            tracing::error!(target: "forklaunch_worker_kafka::librdkafka", error = %error, "{reason}");
        }
    }
}
impl ConsumerContext for LoggingContext {
    fn pre_rebalance(&self, _: &BaseConsumer<Self>, _: &Rebalance<'_>) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }
}
fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(1024)
        .collect()
}

/// Errors librdkafka recovers from by itself (reconnects, leader/coordinator
/// moves, topic metadata not yet propagated). The worker keeps polling instead
/// of exiting; everything else stops it.
fn is_transient(error: &KafkaError) -> bool {
    match error {
        KafkaError::MessageConsumptionFatal(_) => false,
        KafkaError::PartitionEOF(_) | KafkaError::NoMessageReceived => true,
        KafkaError::Transaction(error) => !error.is_fatal() && error.is_retriable(),
        other => other.rdkafka_error_code().is_some_and(transient_code),
    }
}
fn transient_code(code: RDKafkaErrorCode) -> bool {
    matches!(
        code,
        RDKafkaErrorCode::BrokerTransportFailure
            | RDKafkaErrorCode::AllBrokersDown
            | RDKafkaErrorCode::Resolve
            | RDKafkaErrorCode::OperationTimedOut
            | RDKafkaErrorCode::MessageTimedOut
            | RDKafkaErrorCode::RequestTimedOut
            | RDKafkaErrorCode::TimedOutQueue
            | RDKafkaErrorCode::NetworkException
            | RDKafkaErrorCode::WaitingForCoordinator
            | RDKafkaErrorCode::CoordinatorNotAvailable
            | RDKafkaErrorCode::NotCoordinator
            | RDKafkaErrorCode::CoordinatorLoadInProgress
            | RDKafkaErrorCode::LeaderNotAvailable
            | RDKafkaErrorCode::NotLeaderForPartition
            | RDKafkaErrorCode::BrokerNotAvailable
            | RDKafkaErrorCode::NotEnoughReplicas
            | RDKafkaErrorCode::NotEnoughReplicasAfterAppend
            | RDKafkaErrorCode::UnknownTopicOrPartition
            | RDKafkaErrorCode::UnknownTopic
            | RDKafkaErrorCode::UnknownPartition
            | RDKafkaErrorCode::RebalanceInProgress
            | RDKafkaErrorCode::ConcurrentTransactions
    )
}
const BACKOFF_START: Duration = Duration::from_millis(100);
const BACKOFF_MAX: Duration = Duration::from_secs(5);
fn next_backoff(current: Duration) -> Duration {
    (current * 2).min(BACKOFF_MAX)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireJob {
    id: String,
    queue: String,
    payload_json: String,
    attempts: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    tenant_id: String,
    job: WireJob,
}
impl Envelope {
    fn new(job: Job, tenant: &TenantContext) -> Self {
        Self {
            version: 1,
            tenant_id: tenant.id().into(),
            job: WireJob {
                id: job.id,
                queue: job.queue,
                payload_json: job.payload_json,
                attempts: job.attempts,
            },
        }
    }
    fn job(&self) -> Job {
        Job::new(&self.job.id, &self.job.queue, &self.job.payload_json)
            .with_attempts(self.job.attempts)
    }
    fn validate(&self, options: &KafkaOptions) -> BlueprintResult<()> {
        if self.version != 1
            || self.tenant_id != options.tenant.id()
            || self.job.queue != options.queue
            || !valid_name(&self.job.id)
            || self.job.payload_json.len() > options.max_payload_bytes
            || serde_json::from_str::<serde_json::Value>(&self.job.payload_json).is_err()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeadEnvelope {
    envelope: Envelope,
    error_code: String,
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> KafkaResult<T> + Send + 'static,
) -> BlueprintResult<T> {
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|_| storage())?
        .map_err(|_| storage())
}

/// The first transactional producer on a fresh broker creates
/// `__transaction_state`, and a broker may still be starting; both surface as
/// retriable errors. librdkafka allows `init_transactions` to be called again
/// after a retriable failure, so retry with backoff within `startup_timeout`.
async fn init_transactions(
    producer: &FutureProducer<LoggingContext>,
    options: &KafkaOptions,
) -> BlueprintResult<()> {
    let deadline = Instant::now() + options.startup_timeout;
    let mut backoff = BACKOFF_START;
    loop {
        let owned = producer.clone();
        let attempt = remaining(deadline)?.min(options.operation_timeout);
        let result = tokio::task::spawn_blocking(move || owned.init_transactions(attempt))
            .await
            .map_err(|_| storage())?;
        match result {
            Ok(()) => return Ok(()),
            Err(error) if is_transient(&error) => {
                let wait = backoff.min(remaining(deadline)?);
                tracing::warn!(
                    target: "forklaunch_worker_kafka",
                    error = %error,
                    retry_in_ms = millis(wait),
                    "Kafka transactional producer not ready; retrying"
                );
                tokio::time::sleep(wait).await;
                backoff = next_backoff(backoff);
            }
            Err(error) => {
                tracing::error!(target: "forklaunch_worker_kafka", error = %error, "Kafka transactional producer initialization failed");
                return Err(storage());
            }
        }
    }
}

/// Serialized, transactional publisher. A failed transaction makes this handle
/// unusable: construct a fresh one to fence/resolve the earlier transaction.
/// Duplicate job IDs are permitted by Kafka; use idempotent application handlers.
pub struct KafkaProducer {
    options: KafkaOptions,
    producer: FutureProducer<LoggingContext>,
    lock: Mutex<()>,
    healthy: AtomicBool,
}
impl KafkaProducer {
    /// # Errors
    /// Returns sanitized configuration, broker, or transaction initialization errors.
    pub async fn connect(options: KafkaOptions) -> BlueprintResult<Self> {
        options.validate()?;
        let producer: FutureProducer<LoggingContext> = options
            .config()
            .set(
                "transactional.id",
                scoped(
                    "flt",
                    &[
                        &options.namespace,
                        options.tenant.id(),
                        &options.queue,
                        &options.transactional_id,
                    ],
                ),
            )
            .set("partitioner", "murmur2_random")
            .set("enable.idempotence", "true")
            .set("acks", "all")
            .set("transaction.timeout.ms", "60000")
            .create_with_context(LoggingContext::default())
            .map_err(|_| invalid())?;
        init_transactions(&producer, &options).await?;
        Ok(Self {
            options,
            producer,
            lock: Mutex::new(()),
            healthy: AtomicBool::new(true),
        })
    }
    /// # Errors
    /// Rejects invalid jobs and returns bounded transaction/publication failures.
    pub async fn enqueue(&self, job: Job) -> BlueprintResult<JobReceipt> {
        self.enqueue_batch(vec![job])
            .await?
            .into_iter()
            .next()
            .ok_or_else(storage)
    }
    /// Publishes an entire validated batch atomically in input order. A timeout
    /// may leave the commit outcome uncertain; do not blindly retry side effects.
    /// # Errors
    /// Returns invalid input or sanitized transaction failures; empty batches succeed.
    pub async fn enqueue_batch(&self, jobs: Vec<Job>) -> BlueprintResult<Vec<JobReceipt>> {
        if jobs.len() > 1000 {
            return Err(invalid());
        }
        let mut records = Vec::with_capacity(jobs.len());
        let mut total = 0_usize;
        for job in jobs {
            if job.attempts != 0 {
                return Err(invalid());
            }
            let envelope = Envelope::new(job, &self.options.tenant);
            envelope.validate(&self.options)?;
            let encoded = serde_json::to_vec(&envelope).map_err(|_| invalid())?;
            if encoded.len() > self.options.max_payload_bytes + 4096 {
                return Err(invalid());
            }
            total = total.checked_add(encoded.len()).ok_or_else(invalid)?;
            if total > 8 * 1024 * 1024 {
                return Err(invalid());
            }
            records.push((envelope.job.id, encoded));
        }
        if records.is_empty() {
            return Ok(Vec::new());
        }
        let deadline = Instant::now() + self.options.operation_timeout;
        let _guard = tokio::time::timeout(self.options.operation_timeout, self.lock.lock())
            .await
            .map_err(|_| storage())?;
        if !self.healthy.swap(false, Ordering::SeqCst) {
            return Err(storage());
        }
        self.producer.begin_transaction().map_err(|_| storage())?;
        // Cancellation leaves the handle unhealthy, preventing a later caller
        // from appending to an abandoned or uncertain transaction.
        let transaction = async {
            let topic = self.options.topic();
            for (key, payload) in &records {
                let wait = remaining(deadline)?;
                tokio::time::timeout(
                    wait,
                    self.producer
                        .send(FutureRecord::to(&topic).key(key).payload(payload), wait),
                )
                .await
                .map_err(|_| storage())?
                .map_err(|_| storage())?;
            }
            let producer = self.producer.clone();
            let wait = remaining(deadline)?;
            blocking(move || producer.commit_transaction(wait)).await
        }
        .await;
        if let Err(error) = transaction {
            self.abort().await;
            return Err(error);
        }
        self.healthy.store(true, Ordering::SeqCst);
        Ok(records
            .into_iter()
            .map(|(id, _)| JobReceipt {
                id,
                queue: self.options.queue.clone(),
            })
            .collect())
    }
    async fn abort(&self) {
        let producer = self.producer.clone();
        let deadline = self.options.operation_timeout;
        let _ = blocking(move || producer.abort_transaction(deadline)).await;
    }
}

#[derive(Debug, Clone)]
pub struct KafkaRunnerConfig {
    pub retry: RetryPolicy,
    pub handler_timeout: Duration,
    pub drain_timeout: Duration,
}
impl Default for KafkaRunnerConfig {
    fn default() -> Self {
        Self {
            retry: RetryPolicy::default(),
            handler_timeout: Duration::from_secs(300),
            drain_timeout: Duration::from_secs(30),
        }
    }
}
impl KafkaRunnerConfig {
    /// # Errors
    /// Rejects unbounded handler/backoff/drain durations and retry budgets.
    pub fn validate(&self) -> BlueprintResult<()> {
        if !(1..=1000).contains(&self.retry.max_attempts)
            || self.retry.backoff_seconds > 3600
            || self.handler_timeout.is_zero()
            || self.handler_timeout > Duration::from_secs(3600)
            || self.drain_timeout.is_zero()
            || self.drain_timeout > self.handler_timeout
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct KafkaRunReport {
    pub completed: u64,
    pub retried: u64,
    pub dead_lettered: u64,
    pub abandoned: u64,
}
struct Delivery {
    message: OwnedMessage,
    metadata: ConsumerGroupMetadata,
    epoch: u64,
}

/// One sequential consumer in a broker-managed group. Scale across replicas or
/// partitions. Transactions fence assignment changes; errors stop the consumer
/// without committing its current record. Restart explicitly after a failure.
pub struct KafkaWorker {
    options: KafkaOptions,
    producer: KafkaProducer,
    consumer: StreamConsumer<LoggingContext>,
}
impl KafkaWorker {
    /// # Errors
    /// Returns sanitized configuration, transaction, and subscription errors.
    pub async fn connect(
        options: KafkaOptions,
        config: &KafkaRunnerConfig,
    ) -> BlueprintResult<Self> {
        options.validate()?;
        config.validate()?;
        let producer = KafkaProducer::connect(options.clone()).await?;
        let max_poll = config.handler_timeout
            + Duration::from_secs(config.retry.backoff_seconds)
            + options.operation_timeout * 6
            + config.drain_timeout
            + Duration::from_secs(30);
        let consumer: StreamConsumer<LoggingContext> = options
            .config()
            .set("group.id", options.group_id())
            .set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false")
            .set("auto.offset.reset", "earliest")
            .set("isolation.level", "read_committed")
            .set("partition.assignment.strategy", "range")
            .set(
                "session.timeout.ms",
                millis(options.session_timeout).to_string(),
            )
            .set(
                "heartbeat.interval.ms",
                millis(options.session_timeout / 3).to_string(),
            )
            .set(
                "max.poll.interval.ms",
                millis(max_poll.max(options.session_timeout * 2)).to_string(),
            )
            .set("queued.max.messages.kbytes", "8192")
            .set(
                "fetch.message.max.bytes",
                (options.max_payload_bytes + 4096).to_string(),
            )
            .create_with_context(LoggingContext::default())
            .map_err(|_| invalid())?;
        consumer
            .subscribe(&[&options.topic()])
            .map_err(|_| storage())?;
        Ok(Self {
            options,
            producer,
            consumer,
        })
    }
    /// Current assigned partition count; a connected worker may still be joining.
    /// # Errors
    /// Returns a sanitized broker error if assignment cannot be read.
    pub fn assigned_partitions(&self) -> BlueprintResult<usize> {
        Ok(self.consumer.assignment().map_err(|_| storage())?.count())
    }
    /// # Errors
    /// Returns broker, invalid-message or assignment errors after stopping claims.
    /// Shutdown drains the active handler within its deadline; cancellation leaves
    /// the offset uncommitted for another replica or a subsequent restart.
    #[allow(clippy::too_many_lines)]
    pub async fn run<H, F>(
        &self,
        handler: &H,
        config: &KafkaRunnerConfig,
        control: &WorkerControl,
        shutdown: F,
    ) -> BlueprintResult<KafkaRunReport>
    where
        H: JobHandler,
        F: Future<Output = ()> + Send,
    {
        config.validate()?;
        let _drain_on_exit = DrainOnExit(control);
        tokio::pin!(shutdown);
        let mut report = KafkaRunReport::default();
        let mut buffered = VecDeque::<Delivery>::new();
        let mut buffered_bytes = 0_usize;
        let mut backoff = BACKOFF_START;
        loop {
            if control.state() == WorkerState::Draining {
                break;
            }
            let assignment = self.consumer.assignment().map_err(|_| storage())?;
            if assignment.count() > 0 {
                if control.state() == WorkerState::Paused {
                    self.consumer.pause(&assignment)
                } else {
                    self.consumer.resume(&assignment)
                }
                .map_err(|_| storage())?;
            }
            let delivery = if control.state() == WorkerState::Running && !buffered.is_empty() {
                let delivery = buffered.pop_front().ok_or_else(invalid)?;
                buffered_bytes -= delivery.message.payload().map_or(0, <[u8]>::len);
                delivery
            } else {
                // Poll even while paused to keep group membership healthy. Records
                // already fetched during a pause race stay bounded and uncommitted.
                let received = tokio::select! {
                    () = &mut shutdown => { control.drain(); break; }
                    () = control.draining() => break,
                    () = tokio::time::sleep(Duration::from_millis(100)) => continue,
                    message = self.consumer.recv() => message,
                };
                let message = match received {
                    Ok(message) => {
                        backoff = BACKOFF_START;
                        message
                    }
                    Err(error) if is_transient(&error) => {
                        // librdkafka reconnects on its own; keep the group
                        // membership alive and back off instead of exiting.
                        tracing::warn!(
                            target: "forklaunch_worker_kafka",
                            error = %error,
                            retry_in_ms = millis(backoff),
                            "transient Kafka consumer error; retrying"
                        );
                        tokio::select! {
                            () = &mut shutdown => { control.drain(); break; }
                            () = control.draining() => break,
                            () = tokio::time::sleep(backoff) => (),
                        }
                        backoff = next_backoff(backoff);
                        continue;
                    }
                    Err(error) => {
                        tracing::error!(target: "forklaunch_worker_kafka", error = %error, "Kafka consumer failed");
                        return Err(storage());
                    }
                };
                let epoch = self.consumer.context().epoch.load(Ordering::SeqCst);
                let metadata = self.consumer.group_metadata().ok_or_else(rebalance_error)?;
                let delivery = Delivery {
                    message: message.detach(),
                    metadata,
                    epoch,
                };
                if control.state() != WorkerState::Running {
                    buffered_bytes = buffered_bytes
                        .checked_add(delivery.message.payload().map_or(0, <[u8]>::len))
                        .ok_or_else(invalid)?;
                    if buffered.len() >= 1000 || buffered_bytes > 8 * 1024 * 1024 {
                        return Err(invalid());
                    }
                    buffered.push_back(delivery);
                    continue;
                }
                delivery
            };
            if delivery.epoch != self.consumer.context().epoch.load(Ordering::SeqCst) {
                return Err(rebalance_error());
            }
            let bytes = delivery.message.payload().ok_or_else(invalid)?;
            if bytes.len() > self.options.max_payload_bytes + 4096 {
                return Err(invalid());
            }
            let mut envelope: Envelope = serde_json::from_slice(bytes).map_err(|_| invalid())?;
            envelope.validate(&self.options)?;
            envelope.job.attempts = envelope.job.attempts.checked_add(1).ok_or_else(invalid)?;
            let process =
                tokio::time::timeout(config.handler_timeout, handler.process(envelope.job()));
            tokio::pin!(process);
            let outcome = tokio::select! {
                result = &mut process => Some(result.unwrap_or_else(|_| Err(BlueprintError::new("kafka_handler_timeout", "Kafka handler timed out")))),
                () = &mut shutdown => { control.drain(); tokio::time::timeout(config.drain_timeout, &mut process).await.ok().map(|result| result.unwrap_or_else(|_| Err(BlueprintError::new("kafka_handler_timeout", "Kafka handler timed out")))) },
                () = control.draining() => tokio::time::timeout(config.drain_timeout, &mut process).await.ok().map(|result| result.unwrap_or_else(|_| Err(BlueprintError::new("kafka_handler_timeout", "Kafka handler timed out")))),
            };
            let Some(outcome) = outcome else {
                report.abandoned += 1;
                break;
            };
            let retry = outcome.is_err() && config.retry.should_retry(envelope.job.attempts);
            if retry && config.retry.backoff_seconds > 0 && control.state() != WorkerState::Draining
            {
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_secs(config.retry.backoff_seconds)) => (),
                    () = &mut shutdown => control.drain(),
                    () = control.draining() => (),
                }
            }
            if delivery.epoch != self.consumer.context().epoch.load(Ordering::SeqCst) {
                return Err(rebalance_error());
            }
            self.commit_delivery(delivery, &envelope, outcome.as_ref().err(), retry)
                .await?;
            if outcome.is_ok() {
                report.completed += 1;
            } else if retry {
                report.retried += 1;
            } else {
                report.dead_lettered += 1;
            }
        }
        Ok(report)
    }
    async fn commit_delivery(
        &self,
        delivery: Delivery,
        envelope: &Envelope,
        failure: Option<&BlueprintError>,
        retry: bool,
    ) -> BlueprintResult<()> {
        let deadline = Instant::now() + self.options.operation_timeout;
        let _guard =
            tokio::time::timeout(self.options.operation_timeout, self.producer.lock.lock())
                .await
                .map_err(|_| storage())?;
        if !self.producer.healthy.swap(false, Ordering::SeqCst) {
            return Err(storage());
        }
        self.producer
            .producer
            .begin_transaction()
            .map_err(|_| storage())?;
        let transaction = async {
            if let Some(error) = failure {
                let (topic, payload) = if retry {
                    (
                        self.options.topic(),
                        serde_json::to_vec(envelope).map_err(|_| invalid())?,
                    )
                } else {
                    let error_code: String = error
                        .code
                        .chars()
                        .filter(char::is_ascii_graphic)
                        .take(256)
                        .collect();
                    (
                        self.options.dead_letter_topic(),
                        serde_json::to_vec(&DeadEnvelope {
                            envelope: envelope.clone(),
                            error_code,
                        })
                        .map_err(|_| invalid())?,
                    )
                };
                let wait = remaining(deadline)?;
                tokio::time::timeout(
                    wait,
                    self.producer.producer.send(
                        FutureRecord::to(&topic)
                            .key(&envelope.job.id)
                            .payload(&payload),
                        wait,
                    ),
                )
                .await
                .map_err(|_| storage())?
                .map_err(|_| storage())?;
            }
            let mut offsets = TopicPartitionList::new();
            offsets
                .add_partition_offset(
                    delivery.message.topic(),
                    delivery.message.partition(),
                    Offset::Offset(
                        delivery
                            .message
                            .offset()
                            .checked_add(1)
                            .ok_or_else(invalid)?,
                    ),
                )
                .map_err(|_| storage())?;
            let producer = self.producer.producer.clone();
            let timeout = remaining(deadline)?;
            let metadata = delivery.metadata;
            if delivery.epoch != self.consumer.context().epoch.load(Ordering::SeqCst) {
                return Err(rebalance_error());
            }
            blocking(move || producer.send_offsets_to_transaction(&offsets, &metadata, timeout))
                .await?;
            let producer = self.producer.producer.clone();
            let timeout = remaining(deadline)?;
            blocking(move || producer.commit_transaction(timeout)).await
        }
        .await;
        if transaction.is_err() {
            self.producer.abort().await;
        } else {
            self.producer.healthy.store(true, Ordering::SeqCst);
        }
        transaction
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options() -> KafkaOptions {
        KafkaOptions::new(
            "localhost:9092",
            "test",
            TenantContext::new("tenant-a").unwrap(),
            "jobs",
            "replica-a",
        )
    }
    #[test]
    fn topics_and_groups_are_scoped_and_configuration_never_prints_credentials() {
        let first = options();
        assert_eq!(first.topic().len(), 68);
        let mut other = first.clone();
        other.tenant = TenantContext::new("tenant-b").unwrap();
        assert_ne!(first.topic(), other.topic());
        other = first.clone();
        other.namespace = "other".into();
        assert_ne!(first.topic(), other.topic());
        other = first.clone();
        other.queue = "other".into();
        assert_ne!(first.topic(), other.topic());
        other = first.clone();
        other.group = "other".into();
        assert_eq!(first.topic(), other.topic());
        assert_ne!(first.group_id(), other.group_id());
        assert_ne!(scoped("flw", &["ab", "c"]), scoped("flw", &["a", "bc"]));
        other
            .properties
            .insert("sasl.password".into(), "secret-value".into());
        assert!(!format!("{other:?}").contains("secret-value"));
    }
    #[test]
    fn rejects_wrong_tenants_payloads_retry_budgets_and_unsafe_connection_options() {
        let mut options = options();
        let envelope = Envelope::new(Job::new("job", "jobs", "{}"), &options.tenant);
        envelope.validate(&options).unwrap();
        options.tenant = TenantContext::new("other").unwrap();
        assert!(envelope.validate(&options).is_err());
        options = super::tests::options();
        options.brokers = "user:password@broker".into();
        assert!(options.validate().is_err());
        options = super::tests::options();
        options.max_payload_bytes = 0;
        assert!(options.validate().is_err());
        assert!(
            KafkaRunnerConfig {
                retry: RetryPolicy::new(0, 0),
                ..KafkaRunnerConfig::default()
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn transient_broker_errors_are_retried_and_others_stop_the_worker() {
        for code in [
            RDKafkaErrorCode::BrokerTransportFailure,
            RDKafkaErrorCode::AllBrokersDown,
            RDKafkaErrorCode::UnknownTopicOrPartition,
            RDKafkaErrorCode::CoordinatorLoadInProgress,
        ] {
            assert!(
                is_transient(&KafkaError::MessageConsumption(code)),
                "{code:?}"
            );
        }
        assert!(!is_transient(&KafkaError::MessageConsumptionFatal(
            RDKafkaErrorCode::BrokerTransportFailure
        )));
        for code in [
            RDKafkaErrorCode::Authentication,
            RDKafkaErrorCode::TopicAuthorizationFailed,
            RDKafkaErrorCode::Fatal,
        ] {
            assert!(
                !is_transient(&KafkaError::MessageConsumption(code)),
                "{code:?}"
            );
        }
        assert_eq!(next_backoff(BACKOFF_START), Duration::from_millis(200));
        assert_eq!(next_backoff(BACKOFF_MAX), BACKOFF_MAX);
    }
    #[test]
    fn loopback_brokers_prefer_ipv4_unless_configured() {
        let mut options = options();
        assert_eq!(options.config().get("broker.address.family"), Some("v4"));
        options.brokers = "localhost:9092,kafka:9092".into();
        assert_eq!(options.config().get("broker.address.family"), None);
        options.brokers = "localhost:9092".into();
        options
            .properties
            .insert("broker.address.family".into(), "any".into());
        assert_eq!(options.config().get("broker.address.family"), Some("any"));
        assert_eq!(options.config().get("log_level"), None);
        options.startup_timeout = Duration::from_secs(1);
        assert!(options.validate().is_err());
    }
}

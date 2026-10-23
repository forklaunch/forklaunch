use super::{DeadEnvelope, Envelope, KafkaOptions, LoggingContext, invalid, storage};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_interface::Job;
use rdkafka::{
    Message, Offset, TopicPartitionList,
    admin::{AdminClient, AdminOptions, NewTopic, TopicReplication},
    consumer::{BaseConsumer, Consumer},
    error::RDKafkaErrorCode,
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

/// Explicitly creates queue/dead-letter topics; ordinary publishing and consuming
/// never enable topic auto-creation. Existing topics are left unchanged.
/// # Errors
/// Rejects invalid configuration and returns bounded administrative broker errors.
pub async fn initialize_topics(
    options: &KafkaOptions,
    partitions: i32,
    replication: i32,
) -> BlueprintResult<()> {
    options.validate()?;
    if !(1..=128).contains(&partitions) || !(1..=16).contains(&replication) {
        return Err(invalid());
    }
    let admin: AdminClient<LoggingContext> = options
        .config()
        .create_with_context(LoggingContext::default())
        .map_err(|_| invalid())?;
    let topic = options.topic();
    let dead = options.dead_letter_topic();
    let topics = [
        NewTopic::new(&topic, partitions, TopicReplication::Fixed(replication)),
        NewTopic::new(&dead, partitions, TopicReplication::Fixed(replication)),
    ];
    let opts = AdminOptions::new()
        .operation_timeout(Some(options.operation_timeout))
        .request_timeout(Some(options.operation_timeout));
    let results = tokio::time::timeout(
        options.operation_timeout,
        admin.create_topics(&topics, &opts),
    )
    .await
    .map_err(|_| storage())?
    .map_err(|_| storage())?;
    for result in results {
        if let Err((_, code)) = result
            && code != RDKafkaErrorCode::TopicAlreadyExists
        {
            return Err(storage());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KafkaDeadLetter {
    pub job: Job,
    pub error_code: String,
}
/// Reads without joining or advancing the worker group. Inspection is a bounded
/// view of retained log records, not a global per-job database or lease registry.
pub struct KafkaInspector {
    options: KafkaOptions,
}
impl KafkaInspector {
    /// # Errors
    /// Rejects invalid configuration before accessing a broker.
    pub fn new(options: KafkaOptions) -> BlueprintResult<Self> {
        options.validate()?;
        Ok(Self { options })
    }
    /// Reads pending records from the group's committed offsets, preserving order
    /// within each partition. There is no total order across partitions.
    /// Open transactions and broker delays may produce a shorter result.
    /// # Errors
    /// Rejects invalid limits/messages and returns sanitized broker failures.
    pub async fn peek(&self, limit: usize) -> BlueprintResult<Vec<Job>> {
        let options = self.options.clone();
        let records = self.read(false, limit).await?;
        records
            .into_iter()
            .map(|bytes| {
                let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
                envelope.validate(&options)?;
                Ok(envelope.job())
            })
            .collect()
    }
    /// Reads retained dead letters without changing consumer offsets. Kafka topic
    /// retention governs deletion; this operation never purges individual records.
    /// # Errors
    /// Rejects invalid limits/messages and returns sanitized broker failures.
    pub async fn dead_letters(&self, limit: usize) -> BlueprintResult<Vec<KafkaDeadLetter>> {
        let options = self.options.clone();
        self.read(true, limit)
            .await?
            .into_iter()
            .map(|bytes| {
                let dead: DeadEnvelope = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
                dead.envelope.validate(&options)?;
                Ok(KafkaDeadLetter {
                    job: dead.envelope.job(),
                    error_code: dead.error_code,
                })
            })
            .collect()
    }
    async fn read(&self, dead: bool, limit: usize) -> BlueprintResult<Vec<Vec<u8>>> {
        if !(1..=1000).contains(&limit) {
            return Err(invalid());
        }
        let options = self.options.clone();
        tokio::task::spawn_blocking(move || read_records(&options, dead, limit))
            .await
            .map_err(|_| storage())?
    }
}
fn remaining(deadline: Instant) -> BlueprintResult<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| BlueprintError::new("kafka_operation_timeout", "Kafka inspection timed out"))
}
fn read_records(options: &KafkaOptions, dead: bool, limit: usize) -> BlueprintResult<Vec<Vec<u8>>> {
    let deadline = Instant::now() + options.operation_timeout;
    let topic = if dead {
        options.dead_letter_topic()
    } else {
        options.topic()
    };
    let consumer: BaseConsumer<LoggingContext> = options
        .config()
        .set("group.id", options.group_id())
        .set("enable.auto.commit", "false")
        .set("enable.auto.offset.store", "false")
        .set("isolation.level", "read_committed")
        .set("enable.partition.eof", "true")
        .set("queued.max.messages.kbytes", "8192")
        .create_with_context(LoggingContext::default())
        .map_err(|_| invalid())?;
    let metadata = consumer
        .fetch_metadata(Some(&topic), remaining(deadline)?)
        .map_err(|_| storage())?;
    let partitions = metadata
        .topics()
        .first()
        .filter(|metadata| metadata.error().is_none())
        .ok_or_else(storage)?
        .partitions();
    if partitions.is_empty() || partitions.len() > 128 {
        return Err(invalid());
    }
    let mut requested = TopicPartitionList::new();
    for partition in partitions {
        requested.add_partition(&topic, partition.id());
    }
    let committed = consumer
        .committed_offsets(requested, remaining(deadline)?)
        .map_err(|_| storage())?;
    let mut assignment = TopicPartitionList::new();
    let mut ends = BTreeMap::new();
    for partition in partitions {
        let (low, high) = consumer
            .fetch_watermarks(&topic, partition.id(), remaining(deadline)?)
            .map_err(|_| storage())?;
        let start = if dead {
            low
        } else {
            match committed
                .find_partition(&topic, partition.id())
                .map(|element| element.offset())
            {
                Some(Offset::Offset(offset)) => offset.max(low),
                _ => low,
            }
        };
        if start < high {
            assignment
                .add_partition_offset(&topic, partition.id(), Offset::Offset(start))
                .map_err(|_| storage())?;
            ends.insert(partition.id(), high);
        }
    }
    if ends.is_empty() {
        return Ok(Vec::new());
    }
    consumer.assign(&assignment).map_err(|_| storage())?;
    let mut bytes_read = 0_usize;
    let mut records = Vec::new();
    while records.len() < limit && !ends.is_empty() {
        let Ok(wait) = remaining(deadline) else {
            break;
        };
        match consumer.poll(wait.min(Duration::from_millis(100))) {
            Some(Ok(message)) => {
                let Some(end) = ends.get(&message.partition()).copied() else {
                    continue;
                };
                if message.offset() >= end {
                    ends.remove(&message.partition());
                    continue;
                }
                let payload = message.payload().ok_or_else(invalid)?;
                bytes_read = bytes_read.checked_add(payload.len()).ok_or_else(invalid)?;
                if payload.len() > options.max_payload_bytes + 8192 || bytes_read > 8 * 1024 * 1024
                {
                    return Err(invalid());
                }
                records.push(payload.to_vec());
                if message.offset() + 1 >= end {
                    ends.remove(&message.partition());
                }
            }
            Some(Err(rdkafka::error::KafkaError::PartitionEOF(partition))) => {
                ends.remove(&partition);
            }
            Some(Err(_)) => return Err(storage()),
            None => (),
        }
    }
    Ok(records)
}

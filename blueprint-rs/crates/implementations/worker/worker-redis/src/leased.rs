//! Asynchronous Redis jobs with atomic claims and fenced delivery leases.
use crate::tenant_isolation::{RedisKeyScope, isolation_error};
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::TenantContext;
use forklaunch_worker_interface::{
    AsyncWorkerAdmin, AsyncWorkerQueue, FailureDisposition, Job, JobLease, JobReceipt, JobSnapshot,
    JobStatus, QueueStats, RetryPolicy, TerminalStatus,
};
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt::Write, sync::LazyLock, time::Duration};
use uuid::Uuid;

const TIMEOUT: Duration = Duration::from_secs(5);
static SCRIPT: LazyLock<redis::Script> =
    LazyLock::new(|| redis::Script::new(include_str!("leased.lua")));

fn storage_error(_: impl std::fmt::Display) -> BlueprintError {
    BlueprintError::new("worker_storage_error", "Worker storage operation failed")
}
fn redis_error(error: redis::RedisError) -> BlueprintError {
    isolation_error(&error).unwrap_or_else(|| storage_error(error))
}
fn invalid_input() -> BlueprintError {
    BlueprintError::new("worker_invalid_input", "Invalid worker queue input")
}
fn valid_name(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn duration_ms(duration: Duration) -> BlueprintResult<u64> {
    if duration < Duration::from_millis(1) || duration > Duration::from_hours(24) {
        return Err(invalid_input());
    }
    u64::try_from(duration.as_millis()).map_err(|_| invalid_input())
}
fn encoded(value: &str) -> String {
    value.bytes().fold(
        String::with_capacity(value.len() * 2),
        |mut output, byte| {
            let _ = write!(&mut output, "{byte:02x}");
            output
        },
    )
}

#[derive(Serialize, Deserialize)]
struct StoredJob {
    id: String,
    queue: String,
    payload_json: String,
    attempts: u32,
    max_attempts: u32,
    backoff_ms: u64,
    status: String,
    member: String,
    available_at: i64,
    token: Option<String>,
    lease_until: Option<i64>,
    last_error: Option<String>,
    #[serde(default)]
    terminal_at: Option<i64>,
}
impl StoredJob {
    fn job(&self) -> Job {
        Job {
            id: self.id.clone(),
            queue: self.queue.clone(),
            payload_json: self.payload_json.clone(),
            attempts: self.attempts,
        }
    }
    fn snapshot(self) -> BlueprintResult<JobSnapshot> {
        let status = match self.status.as_str() {
            "queued" => JobStatus::Queued,
            "processing" => JobStatus::Processing,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            _ => return Err(storage_error("invalid status")),
        };
        Ok(JobSnapshot {
            job: self.job(),
            status,
            last_error: self.last_error,
            lease_expires_at_unix_ms: self.lease_until,
        })
    }
}

/// A tenant-bound Redis queue. The namespace and tenant are hex-encoded into a
/// shared hash tag so mixed-queue batches stay in one Redis Cluster slot. Bind
/// tenants through trusted application code; configure Redis ACLs separately.
/// Server persistence/replication settings determine durability across a Redis
/// server restart. Completed and failed jobs are retained until administration.
#[derive(Clone)]
pub struct RedisLeasedQueue {
    connection: ConnectionManager,
    namespace: String,
    tenant: TenantContext,
    retry_policy: RetryPolicy,
    key_scope: RedisKeyScope,
}

impl RedisLeasedQueue {
    /// Connects with bounded connection, response, and reconnection waits.
    ///
    /// # Errors
    /// Returns a sanitized error for invalid input or a failed Redis connection.
    pub async fn connect(
        url: &str,
        namespace: &str,
        tenant: TenantContext,
        retry_policy: RetryPolicy,
    ) -> BlueprintResult<Self> {
        Self::connect_scoped(
            url,
            namespace,
            tenant,
            retry_policy,
            RedisKeyScope::from_env()?,
        )
        .await
    }

    /// Connects using an explicitly injected application key scope.
    /// # Errors
    /// Rejects invalid queue configuration and returns sanitized connection failures.
    pub async fn connect_scoped(
        url: &str,
        namespace: &str,
        tenant: TenantContext,
        retry_policy: RetryPolicy,
        key_scope: RedisKeyScope,
    ) -> BlueprintResult<Self> {
        if !valid_name(namespace)
            || retry_policy.max_attempts == 0
            || retry_policy.max_attempts > i32::MAX as u32
            || retry_policy.backoff_seconds > 604_800
        {
            return Err(invalid_input());
        }
        let client = redis::Client::open(url).map_err(redis_error)?;
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(Some(TIMEOUT))
            .set_response_timeout(Some(TIMEOUT))
            .set_number_of_retries(2);
        let connection =
            tokio::time::timeout(TIMEOUT, ConnectionManager::new_with_config(client, config))
                .await
                .map_err(storage_error)?
                .map_err(redis_error)?;
        Ok(Self {
            connection,
            namespace: namespace.to_owned(),
            tenant,
            retry_policy,
            key_scope,
        })
    }

    /// Reuses the connection with another tenant selected by trusted code.
    #[must_use]
    pub fn for_tenant(&self, tenant: TenantContext) -> Self {
        Self {
            connection: self.connection.clone(),
            namespace: self.namespace.clone(),
            tenant,
            retry_policy: self.retry_policy,
            key_scope: self.key_scope.clone(),
        }
    }

    fn keys(&self, queue: &str) -> BlueprintResult<[String; 7]> {
        if !valid_name(queue) {
            return Err(invalid_input());
        }
        let prefix = format!(
            "fl_worker:{{{}_{}}}:{}",
            encoded(&self.namespace),
            encoded(self.tenant.id()),
            encoded(queue)
        );
        Ok([
            "jobs",
            "ready",
            "leased",
            "sequence",
            "completed",
            "failed",
            "admin-index",
        ]
        .map(|suffix| self.key_scope.qualify(&format!("{prefix}:{suffix}"))))
    }

    async fn execute(
        &self,
        keys: &[String],
        operation: &str,
        input: Value,
    ) -> BlueprintResult<Value> {
        tokio::time::timeout(TIMEOUT, async {
            loop {
                let mut invocation = SCRIPT.prepare_invoke();
                for key in keys {
                    invocation.key(key);
                }
                invocation.arg(operation).arg(input.to_string());
                let mut connection = self.connection.clone();
                let response: String = invocation
                    .invoke_async(&mut connection)
                    .await
                    .map_err(redis_error)?;
                let response: Value = serde_json::from_str(&response).map_err(storage_error)?;
                if let Some(error) = response.get("error").and_then(Value::as_str) {
                    if error == "indexing" {
                        continue;
                    }
                    return Err(if error == "duplicate" {
                        BlueprintError::new(
                            "worker_duplicate_job",
                            "Worker job identity already exists",
                        )
                    } else {
                        storage_error(error)
                    });
                }
                return response
                    .get("value")
                    .cloned()
                    .ok_or_else(|| storage_error("missing response"));
            }
        })
        .await
        .map_err(storage_error)?
    }

    async fn lease_operation(
        &self,
        lease: &JobLease,
        operation: &str,
        mut input: Value,
    ) -> BlueprintResult<Value> {
        if !valid_name(&lease.job.id) || Uuid::parse_str(&lease.token).is_err() {
            return Err(invalid_input());
        }
        input["id"] = json!(lease.job.id);
        input["token"] = json!(lease.token);
        self.execute(&self.keys(&lease.job.queue)?, operation, input)
            .await
    }
}

impl AsyncWorkerQueue for RedisLeasedQueue {
    async fn enqueue(&self, job: Job) -> BlueprintResult<JobReceipt> {
        self.enqueue_batch(vec![job])
            .await?
            .pop()
            .ok_or_else(invalid_input)
    }
    async fn enqueue_batch(&self, jobs: Vec<Job>) -> BlueprintResult<Vec<JobReceipt>> {
        if jobs.len() > 1000 {
            return Err(invalid_input());
        }
        if jobs.is_empty() {
            return Ok(Vec::new());
        }
        let mut queues = BTreeMap::new();
        let mut keys = Vec::new();
        let mut stored = Vec::new();
        let mut receipts = Vec::new();
        for job in jobs {
            if !valid_name(&job.id)
                || job.attempts != 0
                || job.payload_json.len() > 1024 * 1024
                || serde_json::from_str::<Value>(&job.payload_json).is_err()
            {
                return Err(invalid_input());
            }
            let new_index = queues.len();
            let index = *queues.entry(job.queue.clone()).or_insert_with(|| new_index);
            if index == keys.len() / 7 {
                keys.extend(self.keys(&job.queue)?);
            }
            receipts.push(JobReceipt {
                id: job.id.clone(),
                queue: job.queue.clone(),
            });
            let mut record = serde_json::to_value(StoredJob {
                id: job.id,
                queue: job.queue,
                payload_json: job.payload_json,
                attempts: 0,
                max_attempts: self.retry_policy.max_attempts,
                backoff_ms: self.retry_policy.backoff_seconds * 1000,
                status: "queued".into(),
                member: String::new(),
                available_at: 0,
                token: None,
                lease_until: None,
                last_error: None,
                terminal_at: None,
            })
            .map_err(storage_error)?;
            record["key_index"] = json!(index);
            stored.push(record);
        }
        self.execute(&keys, "enqueue", json!({"jobs":stored}))
            .await?;
        Ok(receipts)
    }
    async fn claim(
        &self,
        queue: &str,
        lease_duration: Duration,
    ) -> BlueprintResult<Option<JobLease>> {
        Ok(self.claim_batch(queue, lease_duration, 1).await?.pop())
    }
    async fn claim_batch(
        &self,
        queue: &str,
        lease_duration: Duration,
        limit: usize,
    ) -> BlueprintResult<Vec<JobLease>> {
        if !(1..=1000).contains(&limit) {
            return Err(invalid_input());
        }
        let tokens: Vec<_> = (0..limit).map(|_| Uuid::new_v4().to_string()).collect();
        let value = self
            .execute(
                &self.keys(queue)?,
                "claim_batch",
                json!({"duration":duration_ms(lease_duration)?,"limit":limit,"tokens":tokens}),
            )
            .await?;
        let stored: Vec<StoredJob> = serde_json::from_value(value).map_err(storage_error)?;
        stored
            .into_iter()
            .map(|stored| {
                Ok(JobLease {
                    job: stored.job(),
                    token: stored.token.ok_or_else(|| storage_error("missing token"))?,
                    expires_at_unix_ms: stored
                        .lease_until
                        .ok_or_else(|| storage_error("missing deadline"))?,
                })
            })
            .collect()
    }
    async fn ack(&self, lease: &JobLease) -> BlueprintResult<bool> {
        self.lease_operation(lease, "ack", json!({}))
            .await?
            .as_bool()
            .ok_or_else(|| storage_error("invalid acknowledgment"))
    }
    async fn fail(&self, lease: &JobLease, reason: &str) -> BlueprintResult<FailureDisposition> {
        if reason.len() > 4096 || reason.contains('\0') {
            return Err(invalid_input());
        }
        let value = self
            .lease_operation(lease, "fail", json!({"reason":reason}))
            .await?;
        match value {
            Value::Bool(false) => Ok(FailureDisposition::LeaseLost),
            Value::String(status) if status == "queued" => Ok(FailureDisposition::RetryScheduled),
            Value::String(status) if status == "failed" => Ok(FailureDisposition::DeadLetter),
            _ => Err(storage_error("invalid failure response")),
        }
    }
    async fn renew(&self, lease: &JobLease, lease_duration: Duration) -> BlueprintResult<bool> {
        self.lease_operation(
            lease,
            "renew",
            json!({"duration":duration_ms(lease_duration)?}),
        )
        .await?
        .as_bool()
        .ok_or_else(|| storage_error("invalid renewal"))
    }
    async fn peek(&self, queue: &str, limit: usize) -> BlueprintResult<Vec<Job>> {
        if !(1..=1000).contains(&limit) {
            return Err(invalid_input());
        }
        let value = self
            .execute(&self.keys(queue)?, "peek", json!({"limit":limit}))
            .await?;
        let jobs: Vec<StoredJob> = serde_json::from_value(value).map_err(storage_error)?;
        Ok(jobs.iter().map(StoredJob::job).collect())
    }
    async fn stats(&self, queue: &str) -> BlueprintResult<QueueStats> {
        let value = self.execute(&self.keys(queue)?, "stats", json!({})).await?;
        let count = |name| {
            value
                .get(name)
                .and_then(Value::as_u64)
                .and_then(|count| usize::try_from(count).ok())
                .ok_or_else(|| storage_error("invalid count"))
        };
        Ok(QueueStats {
            queue: queue.to_owned(),
            queued: count("queued")?,
            processing: count("processing")?,
        })
    }
    async fn inspect(&self, queue: &str, id: &str) -> BlueprintResult<Option<JobSnapshot>> {
        if !valid_name(id) {
            return Err(invalid_input());
        }
        let value = self
            .execute(&self.keys(queue)?, "inspect", json!({"id":id}))
            .await?;
        if value.is_null() {
            return Ok(None);
        }
        let stored: StoredJob = serde_json::from_value(value).map_err(storage_error)?;
        stored.snapshot().map(Some)
    }
}

impl AsyncWorkerAdmin for RedisLeasedQueue {
    async fn dead_letters(&self, queue: &str, limit: usize) -> BlueprintResult<Vec<JobSnapshot>> {
        if !(1..=1000).contains(&limit) {
            return Err(invalid_input());
        }
        let value = self
            .execute(&self.keys(queue)?, "dead_letters", json!({"limit":limit}))
            .await?;
        let jobs: Vec<StoredJob> = serde_json::from_value(value).map_err(storage_error)?;
        jobs.into_iter().map(StoredJob::snapshot).collect()
    }
    async fn retry_dead_letter(
        &self,
        queue: &str,
        id: &str,
        policy: RetryPolicy,
    ) -> BlueprintResult<bool> {
        if !valid_name(id)
            || policy.max_attempts == 0
            || policy.max_attempts > i32::MAX as u32
            || policy.backoff_seconds > 604_800
        {
            return Err(invalid_input());
        }
        self.execute(&self.keys(queue)?, "retry_dead_letter", json!({"id":id,"max_attempts":policy.max_attempts,"backoff_ms":policy.backoff_seconds*1000})).await?
            .as_bool().ok_or_else(|| storage_error("invalid retry response"))
    }
    async fn purge_terminal(
        &self,
        queue: &str,
        status: TerminalStatus,
        minimum_age: Duration,
        limit: usize,
    ) -> BlueprintResult<usize> {
        if !(1..=1000).contains(&limit)
            || minimum_age < Duration::from_millis(1)
            || minimum_age > Duration::from_hours(87600)
        {
            return Err(invalid_input());
        }
        let age_ms = u64::try_from(minimum_age.as_millis()).map_err(|_| invalid_input())?;
        self.execute(
            &self.keys(queue)?,
            "purge_terminal",
            json!({"status":status.as_str(),"age_ms":age_ms,"limit":limit}),
        )
        .await?
        .as_u64()
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| storage_error("invalid purge response"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoding_is_unambiguous_and_lease_duration_is_bounded() {
        assert_ne!(encoded("a:b"), encoded("ab:"));
        assert_ne!(encoded("{tenant}"), encoded("tenant"));
        assert!(duration_ms(Duration::ZERO).is_err());
        assert!(duration_ms(Duration::MAX).is_err());
        assert_eq!(duration_ms(Duration::from_millis(1)).unwrap(), 1);
    }
}

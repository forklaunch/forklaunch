//! Durable, tenant-bound `PostgreSQL` jobs with fenced, renewable leases.
//!
//! Apply migrations with an owner pool. Runtime connections require a restricted
//! role with USAGE on `fl_worker`, table DML, and sequence USAGE privileges.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::{TenantContext, begin_tenant_transaction};
use forklaunch_worker_interface::{
    AsyncWorkerAdmin, AsyncWorkerQueue, FailureDisposition, Job, JobLease, JobReceipt, JobSnapshot,
    JobStatus, QueueStats, RetryPolicy, TerminalStatus,
};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow, types::Json};
use std::time::Duration;
use uuid::Uuid;

const MAX_BATCH: usize = 1000;
const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const MAX_LEASE: Duration = Duration::from_hours(24);
const MAX_BACKOFF_SECONDS: u64 = 7 * 24 * 60 * 60;

fn storage_error(_: impl std::fmt::Display) -> BlueprintError {
    BlueprintError::new("worker_storage_error", "Worker storage operation failed")
}
fn invalid_input() -> BlueprintError {
    BlueprintError::new("worker_invalid_input", "Invalid worker queue input")
}
fn valid_name(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
fn validate_policy(policy: RetryPolicy) -> BlueprintResult<()> {
    if policy.max_attempts == 0
        || policy.max_attempts > i32::MAX as u32
        || policy.backoff_seconds > MAX_BACKOFF_SECONDS
    {
        return Err(invalid_input());
    }
    Ok(())
}
fn lease_ms(duration: Duration) -> BlueprintResult<i64> {
    if duration < Duration::from_millis(1) || duration > MAX_LEASE {
        return Err(invalid_input());
    }
    i64::try_from(duration.as_millis()).map_err(|_| invalid_input())
}
fn payload(job: &Job) -> BlueprintResult<serde_json::Value> {
    if !valid_name(&job.id)
        || !valid_name(&job.queue)
        || job.attempts != 0
        || job.payload_json.len() > MAX_PAYLOAD_BYTES
    {
        return Err(invalid_input());
    }
    serde_json::from_str(&job.payload_json).map_err(|_| invalid_input())
}
fn decode_job(row: &PgRow) -> BlueprintResult<Job> {
    let Json(payload): Json<serde_json::Value> = row.try_get("payload").map_err(storage_error)?;
    let attempts: i32 = row.try_get("attempts").map_err(storage_error)?;
    Ok(Job {
        id: row.try_get("id").map_err(storage_error)?,
        queue: row.try_get("queue").map_err(storage_error)?,
        payload_json: payload.to_string(),
        attempts: u32::try_from(attempts).map_err(storage_error)?,
    })
}
fn decode_snapshot(row: &PgRow) -> BlueprintResult<JobSnapshot> {
    let status: String = row.try_get("status").map_err(storage_error)?;
    Ok(JobSnapshot {
        job: decode_job(row)?,
        status: match status.as_str() {
            "queued" => JobStatus::Queued,
            "processing" => JobStatus::Processing,
            "completed" => JobStatus::Completed,
            "failed" => JobStatus::Failed,
            _ => return Err(storage_error("invalid job status")),
        },
        last_error: row.try_get("last_error").map_err(storage_error)?,
        lease_expires_at_unix_ms: row.try_get("lease_ms").map_err(storage_error)?,
    })
}
fn lease_token(lease: &JobLease) -> BlueprintResult<Uuid> {
    if !valid_name(&lease.job.id) || !valid_name(&lease.job.queue) {
        return Err(invalid_input());
    }
    Uuid::parse_str(&lease.token).map_err(|_| invalid_input())
}

/// Cloneable pool handle bound to a trusted tenant. Retry policy is persisted
/// when a job is enqueued; subsequent consumers cannot change that job's budget.
#[derive(Clone)]
pub struct PostgresWorkerQueue {
    pool: PgPool,
    tenant: TenantContext,
    retry_policy: RetryPolicy,
}

impl PostgresWorkerQueue {
    /// Applies versioned, checksummed `SQLx` migrations in a private migration
    /// namespace. The transaction restores `search_path` even on failure, and its
    /// advisory lock serializes simultaneous worker migration runners.
    ///
    /// # Errors
    /// Returns a sanitized storage error if schema setup or migrations fail.
    pub async fn migrate(pool: &PgPool) -> BlueprintResult<()> {
        let mut tx = pool.begin().await.map_err(storage_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(0x464C_5752_4B52_i64)
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        sqlx::query("CREATE SCHEMA IF NOT EXISTS fl_worker")
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        sqlx::query("SET LOCAL search_path = fl_worker, pg_catalog")
            .execute(&mut *tx)
            .await
            .map_err(storage_error)?;
        let mut migrator = sqlx::migrate!("./migrations");
        migrator.set_locking(false);
        // Same arguments `Migrator::run` passes: every pending migration.
        migrator
            .run_direct(None, &mut *tx, false)
            .await
            .map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)
    }

    /// # Errors
    /// Returns an error for invalid retry policy, storage failure, or a runtime
    /// role with SUPERUSER/BYPASSRLS privileges.
    pub async fn new(
        pool: PgPool,
        tenant: TenantContext,
        retry_policy: RetryPolicy,
    ) -> BlueprintResult<Self> {
        validate_policy(retry_policy)?;
        let bypass: (bool,) = sqlx::query_as(
            "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
        )
        .fetch_one(&pool)
        .await
        .map_err(storage_error)?;
        if bypass.0 {
            return Err(BlueprintError::new(
                "worker_unsafe_database_role",
                "Worker runtime requires a role without SUPERUSER or BYPASSRLS",
            ));
        }
        Ok(Self {
            pool,
            tenant,
            retry_policy,
        })
    }

    /// Rebinds the pool to another tenant selected by trusted application code.
    #[must_use]
    pub fn for_tenant(&self, tenant: TenantContext) -> Self {
        Self {
            pool: self.pool.clone(),
            tenant,
            retry_policy: self.retry_policy,
        }
    }

    /// Adds a job to an application transaction without committing it. The
    /// transaction must already be bound to this queue's trusted tenant. This
    /// lets a business-state change and its work item commit or roll back together.
    /// # Errors
    /// Rejects invalid jobs, missing/mismatched tenant binding and storage errors.
    pub async fn enqueue_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job: Job,
    ) -> BlueprintResult<JobReceipt> {
        let payload = payload(&job)?;
        let (tenant,): (Option<String>,) =
            sqlx::query_as("SELECT current_setting('forklaunch.tenant_id',true)")
                .fetch_one(&mut **tx)
                .await
                .map_err(storage_error)?;
        if tenant.as_deref() != Some(self.tenant.id()) {
            return Err(invalid_input());
        }
        self.insert_job(tx, job, payload).await
    }
    async fn insert_job(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job: Job,
        payload: serde_json::Value,
    ) -> BlueprintResult<JobReceipt> {
        sqlx::query("INSERT INTO fl_worker.jobs (tenant_id,queue,id,payload,max_attempts,backoff_ms) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(self.tenant.id()).bind(&job.queue).bind(&job.id).bind(Json(payload))
            .bind(i32::try_from(self.retry_policy.max_attempts).map_err(|_| invalid_input())?)
            .bind(i64::try_from(self.retry_policy.backoff_seconds * 1000).map_err(|_| invalid_input())?)
            .execute(&mut **tx).await.map_err(storage_error)?;
        Ok(JobReceipt {
            id: job.id,
            queue: job.queue,
        })
    }

    async fn transaction(&self) -> BlueprintResult<Transaction<'_, Postgres>> {
        begin_tenant_transaction(&self.pool, &self.tenant)
            .await
            .map_err(storage_error)
    }
}

impl AsyncWorkerQueue for PostgresWorkerQueue {
    async fn enqueue(&self, job: Job) -> BlueprintResult<JobReceipt> {
        let mut receipts = self.enqueue_batch(vec![job]).await?;
        receipts.pop().ok_or_else(invalid_input)
    }

    async fn enqueue_batch(&self, jobs: Vec<Job>) -> BlueprintResult<Vec<JobReceipt>> {
        if jobs.len() > MAX_BATCH {
            return Err(invalid_input());
        }
        let payloads = jobs
            .iter()
            .map(payload)
            .collect::<BlueprintResult<Vec<_>>>()?;
        let mut tx = self.transaction().await?;
        let mut receipts = Vec::with_capacity(jobs.len());
        for (job, payload) in jobs.into_iter().zip(payloads) {
            receipts.push(self.insert_job(&mut tx, job, payload).await?);
        }
        tx.commit().await.map_err(storage_error)?;
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
        if !valid_name(queue) || !(1..=MAX_BATCH).contains(&limit) {
            return Err(invalid_input());
        }
        let duration = lease_ms(lease_duration)?;
        let mut tx = self.transaction().await?;
        // Exhausted consumers may crash without calling fail. Retain those jobs
        // as dead letters rather than reclaiming them beyond their retry budget.
        sqlx::query("UPDATE fl_worker.jobs SET status='failed',lease_token=NULL,lease_until=NULL,last_error='lease expired after final attempt',updated_at=clock_timestamp() WHERE tenant_id=$1 AND queue=$2 AND status='processing' AND lease_until<=clock_timestamp() AND attempts>=max_attempts")
            .bind(self.tenant.id()).bind(queue).execute(&mut *tx).await.map_err(storage_error)?;
        let rows = sqlx::query("WITH candidate AS (SELECT tenant_id,queue,id FROM fl_worker.jobs WHERE tenant_id=$1 AND queue=$2 AND attempts<max_attempts AND ((status='queued' AND available_at<=clock_timestamp()) OR (status='processing' AND lease_until<=clock_timestamp())) ORDER BY available_at,sequence FOR UPDATE SKIP LOCKED LIMIT $4), updated AS (UPDATE fl_worker.jobs AS jobs SET status='processing',attempts=jobs.attempts+1,lease_token=gen_random_uuid(),lease_until=clock_timestamp()+$3*interval '1 millisecond',updated_at=clock_timestamp() FROM candidate WHERE jobs.tenant_id=candidate.tenant_id AND jobs.queue=candidate.queue AND jobs.id=candidate.id RETURNING jobs.*,floor(extract(epoch FROM jobs.lease_until)*1000)::bigint AS lease_ms) SELECT * FROM updated ORDER BY available_at,sequence")
            .bind(self.tenant.id()).bind(queue).bind(duration).bind(i64::try_from(limit).map_err(|_| invalid_input())?)
            .fetch_all(&mut *tx).await.map_err(storage_error)?;
        let leases = rows
            .iter()
            .map(|row| {
                Ok(JobLease {
                    job: decode_job(row)?,
                    token: row
                        .try_get::<Uuid, _>("lease_token")
                        .map_err(storage_error)?
                        .to_string(),
                    expires_at_unix_ms: row.try_get("lease_ms").map_err(storage_error)?,
                })
            })
            .collect::<BlueprintResult<Vec<_>>>()?;
        tx.commit().await.map_err(storage_error)?;
        Ok(leases)
    }

    async fn ack(&self, lease: &JobLease) -> BlueprintResult<bool> {
        let token = lease_token(lease)?;
        let mut tx = self.transaction().await?;
        let result = sqlx::query("UPDATE fl_worker.jobs SET status='completed',lease_token=NULL,lease_until=NULL,updated_at=clock_timestamp() WHERE tenant_id=$1 AND queue=$2 AND id=$3 AND status='processing' AND lease_token=$4 AND lease_until>clock_timestamp()")
            .bind(self.tenant.id()).bind(&lease.job.queue).bind(&lease.job.id).bind(token)
            .execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() == 1)
    }

    async fn fail(&self, lease: &JobLease, reason: &str) -> BlueprintResult<FailureDisposition> {
        if reason.len() > 4096 || reason.contains('\0') {
            return Err(invalid_input());
        }
        let token = lease_token(lease)?;
        let mut tx = self.transaction().await?;
        let status: Option<(String,)> = sqlx::query_as("UPDATE fl_worker.jobs SET status=CASE WHEN attempts<max_attempts THEN 'queued' ELSE 'failed' END,available_at=clock_timestamp()+backoff_ms*interval '1 millisecond',lease_token=NULL,lease_until=NULL,last_error=$5,updated_at=clock_timestamp() WHERE tenant_id=$1 AND queue=$2 AND id=$3 AND status='processing' AND lease_token=$4 AND lease_until>clock_timestamp() RETURNING status")
            .bind(self.tenant.id()).bind(&lease.job.queue).bind(&lease.job.id).bind(token).bind(reason)
            .fetch_optional(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(match status.as_ref().map(|(status,)| status.as_str()) {
            Some("queued") => FailureDisposition::RetryScheduled,
            Some(_) => FailureDisposition::DeadLetter,
            None => FailureDisposition::LeaseLost,
        })
    }

    async fn renew(&self, lease: &JobLease, lease_duration: Duration) -> BlueprintResult<bool> {
        let duration = lease_ms(lease_duration)?;
        let token = lease_token(lease)?;
        let mut tx = self.transaction().await?;
        let result = sqlx::query("UPDATE fl_worker.jobs SET lease_until=GREATEST(lease_until,clock_timestamp()+$5*interval '1 millisecond'),updated_at=clock_timestamp() WHERE tenant_id=$1 AND queue=$2 AND id=$3 AND status='processing' AND lease_token=$4 AND lease_until>clock_timestamp()")
            .bind(self.tenant.id()).bind(&lease.job.queue).bind(&lease.job.id).bind(token).bind(duration)
            .execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() == 1)
    }

    async fn peek(&self, queue: &str, limit: usize) -> BlueprintResult<Vec<Job>> {
        if !valid_name(queue) || !(1..=MAX_BATCH).contains(&limit) {
            return Err(invalid_input());
        }
        let mut tx = self.transaction().await?;
        let rows = sqlx::query("SELECT * FROM fl_worker.jobs WHERE tenant_id=$1 AND queue=$2 AND attempts<max_attempts AND ((status='queued' AND available_at<=clock_timestamp()) OR (status='processing' AND lease_until<=clock_timestamp())) ORDER BY available_at,sequence LIMIT $3")
            .bind(self.tenant.id()).bind(queue).bind(i64::try_from(limit).map_err(|_| invalid_input())?)
            .fetch_all(&mut *tx).await.map_err(storage_error)?;
        let jobs = rows
            .iter()
            .map(decode_job)
            .collect::<BlueprintResult<_>>()?;
        tx.commit().await.map_err(storage_error)?;
        Ok(jobs)
    }

    async fn stats(&self, queue: &str) -> BlueprintResult<QueueStats> {
        if !valid_name(queue) {
            return Err(invalid_input());
        }
        let mut tx = self.transaction().await?;
        let counts: (i64, i64) = sqlx::query_as("SELECT count(*) FILTER (WHERE status='queued'),count(*) FILTER (WHERE status='processing') FROM fl_worker.jobs WHERE tenant_id=$1 AND queue=$2")
            .bind(self.tenant.id()).bind(queue).fetch_one(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(QueueStats {
            queue: queue.to_owned(),
            queued: usize::try_from(counts.0).map_err(storage_error)?,
            processing: usize::try_from(counts.1).map_err(storage_error)?,
        })
    }

    async fn inspect(&self, queue: &str, id: &str) -> BlueprintResult<Option<JobSnapshot>> {
        if !valid_name(queue) || !valid_name(id) {
            return Err(invalid_input());
        }
        let mut tx = self.transaction().await?;
        let row = sqlx::query("SELECT *,floor(extract(epoch FROM lease_until)*1000)::bigint AS lease_ms FROM fl_worker.jobs WHERE tenant_id=$1 AND queue=$2 AND id=$3")
            .bind(self.tenant.id()).bind(queue).bind(id).fetch_optional(&mut *tx).await.map_err(storage_error)?;
        let snapshot = row.as_ref().map(decode_snapshot).transpose()?;
        tx.commit().await.map_err(storage_error)?;
        Ok(snapshot)
    }
}

impl AsyncWorkerAdmin for PostgresWorkerQueue {
    async fn dead_letters(&self, queue: &str, limit: usize) -> BlueprintResult<Vec<JobSnapshot>> {
        if !valid_name(queue) || !(1..=MAX_BATCH).contains(&limit) {
            return Err(invalid_input());
        }
        let mut tx = self.transaction().await?;
        let rows = sqlx::query("SELECT *,NULL::bigint AS lease_ms FROM fl_worker.jobs WHERE tenant_id=$1 AND queue=$2 AND status='failed' ORDER BY updated_at,sequence LIMIT $3")
            .bind(self.tenant.id()).bind(queue).bind(i64::try_from(limit).map_err(|_| invalid_input())?)
            .fetch_all(&mut *tx).await.map_err(storage_error)?;
        let jobs = rows
            .iter()
            .map(decode_snapshot)
            .collect::<BlueprintResult<_>>()?;
        tx.commit().await.map_err(storage_error)?;
        Ok(jobs)
    }
    async fn retry_dead_letter(
        &self,
        queue: &str,
        id: &str,
        policy: RetryPolicy,
    ) -> BlueprintResult<bool> {
        if !valid_name(queue) || !valid_name(id) {
            return Err(invalid_input());
        }
        validate_policy(policy)?;
        let mut tx = self.transaction().await?;
        let result = sqlx::query("UPDATE fl_worker.jobs SET status='queued',attempts=0,max_attempts=$4,backoff_ms=$5,available_at=clock_timestamp(),updated_at=clock_timestamp(),last_error=NULL,lease_token=NULL,lease_until=NULL WHERE tenant_id=$1 AND queue=$2 AND id=$3 AND status='failed'")
            .bind(self.tenant.id()).bind(queue).bind(id).bind(i32::try_from(policy.max_attempts).map_err(|_| invalid_input())?)
            .bind(i64::try_from(policy.backoff_seconds*1000).map_err(|_| invalid_input())?)
            .execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        Ok(result.rows_affected() == 1)
    }
    async fn purge_terminal(
        &self,
        queue: &str,
        status: TerminalStatus,
        minimum_age: Duration,
        limit: usize,
    ) -> BlueprintResult<usize> {
        if !valid_name(queue)
            || !(1..=MAX_BATCH).contains(&limit)
            || minimum_age < Duration::from_millis(1)
            || minimum_age > Duration::from_hours(87600)
        {
            return Err(invalid_input());
        }
        let age_ms = i64::try_from(minimum_age.as_millis()).map_err(|_| invalid_input())?;
        let mut tx = self.transaction().await?;
        let result = sqlx::query("WITH expired AS (SELECT tenant_id,queue,id FROM fl_worker.jobs WHERE tenant_id=$1 AND queue=$2 AND status=$3 AND updated_at<=clock_timestamp()-$4*interval '1 millisecond' ORDER BY updated_at,sequence FOR UPDATE SKIP LOCKED LIMIT $5) DELETE FROM fl_worker.jobs AS jobs USING expired WHERE jobs.tenant_id=expired.tenant_id AND jobs.queue=expired.queue AND jobs.id=expired.id")
            .bind(self.tenant.id()).bind(queue).bind(status.as_str()).bind(age_ms)
            .bind(i64::try_from(limit).map_err(|_| invalid_input())?)
            .execute(&mut *tx).await.map_err(storage_error)?;
        tx.commit().await.map_err(storage_error)?;
        usize::try_from(result.rows_affected()).map_err(storage_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_retry_budgets_payloads_and_lease_bounds() {
        assert!(validate_policy(RetryPolicy::new(0, 0)).is_err());
        assert!(validate_policy(RetryPolicy::new(u32::MAX, 0)).is_err());
        assert!(validate_policy(RetryPolicy::new(1, u64::MAX)).is_err());
        assert!(lease_ms(Duration::ZERO).is_err());
        assert!(lease_ms(Duration::MAX).is_err());
        assert_eq!(lease_ms(Duration::from_millis(1)).unwrap(), 1);
        assert!(payload(&Job::new("id", "queue", "not-json")).is_err());
        assert!(payload(&Job::new("id", "queue", "{}").with_attempts(1)).is_err());
        assert!(payload(&Job::new("", "queue", "{}")).is_err());
        assert!(payload(&Job::new("id", "queue", "null")).is_ok());
    }
}

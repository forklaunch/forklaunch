#![cfg(feature = "postgres")]
use forklaunch_core::persistence::TenantContext;
use forklaunch_iam_base::postgres::PostgresIamService;
use forklaunch_worker_database::postgres::PostgresWorkerQueue;
use forklaunch_worker_interface::{
    AsyncWorkerAdmin, AsyncWorkerQueue, FailureDisposition, Job, JobStatus, RetryPolicy,
    TerminalStatus,
};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{collections::HashSet, time::Duration};

/// Timing multiplier for the runner, control and retention windows. Runner queue
/// operations are bounded by `renew_interval`, so a loaded CI runner needs
/// headroom; the assertions depend only on the ratios between lease, renewal,
/// handler and sleep durations, which scaling preserves.
const T: u32 = 8;

struct Fixture {
    owner: PgPool,
    pool: PgPool,
    role: String,
    queue: PostgresWorkerQueue,
}
impl Fixture {
    async fn new() -> Self {
        let url = std::env::var("FORKLAUNCH_TEST_POSTGRES_URL").expect("PostgreSQL URL required");
        let owner = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .unwrap();
        // Both components must migrate repeatedly in the same database without
        // sharing SQLx's migration ledger or leaking a changed search_path.
        PostgresIamService::migrate(&owner).await.unwrap();
        let before: (String,) = sqlx::query_as("SHOW search_path")
            .fetch_one(&owner)
            .await
            .unwrap();
        PostgresWorkerQueue::migrate(&owner).await.unwrap();
        PostgresWorkerQueue::migrate(&owner).await.unwrap();
        PostgresIamService::migrate(&owner).await.unwrap();
        let after: (String,) = sqlx::query_as("SHOW search_path")
            .fetch_one(&owner)
            .await
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            PostgresWorkerQueue::new(
                owner.clone(),
                TenantContext::new("unsafe").unwrap(),
                RetryPolicy::default()
            )
            .await
            .err()
            .unwrap()
            .code,
            "worker_unsafe_database_role"
        );
        let role = format!(
            "fl_jobs_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        for statement in [
            format!("CREATE ROLE {role} NOLOGIN NOSUPERUSER NOBYPASSRLS"),
            format!("GRANT USAGE ON SCHEMA fl_worker TO {role}"),
            format!("GRANT SELECT,INSERT,UPDATE,DELETE ON fl_worker.jobs TO {role}"),
            format!("GRANT USAGE ON ALL SEQUENCES IN SCHEMA fl_worker TO {role}"),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&owner)
                .await
                .unwrap();
        }
        let set_role = format!("SET ROLE {role}");
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .after_connect(move |connection, _| {
                let statement = set_role.clone();
                Box::pin(async move {
                    sqlx::query(sqlx::AssertSqlSafe(statement))
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        let queue = PostgresWorkerQueue::new(
            pool.clone(),
            TenantContext::new(&role).unwrap(),
            RetryPolicy::new(2, 1),
        )
        .await
        .unwrap();
        Self {
            owner,
            pool,
            role,
            queue,
        }
    }
    async fn close(self) {
        self.pool.close().await;
        sqlx::query("DELETE FROM fl_worker.jobs WHERE tenant_id=$1 OR tenant_id=$2")
            .bind(&self.role)
            .bind(format!("{}-b", self.role))
            .execute(&self.owner)
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP OWNED BY {}", self.role)))
            .execute(&self.owner)
            .await
            .unwrap();
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP ROLE {}", self.role)))
            .execute(&self.owner)
            .await
            .unwrap();
        self.owner.close().await;
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
#[ignore = "requires isolated PostgreSQL and a schema-owner test URL"]
async fn postgres_worker_leases_retries_isolation_and_concurrency() {
    let fixture = Fixture::new().await;
    let queue = &fixture.queue;
    let other = queue.for_tenant(TenantContext::new(format!("{}-b", fixture.role)).unwrap());
    queue
        .enqueue_batch(vec![
            Job::new("first", "email", "{\"message\":\"héllo\"}"),
            Job::new("second", "email", "{}"),
        ])
        .await
        .unwrap();
    other
        .enqueue(Job::new("first", "email", "{\"tenant\":\"other\"}"))
        .await
        .unwrap();
    let peek = queue.peek("email", 10).await.unwrap();
    assert_eq!(
        peek.iter().map(|job| job.id.as_str()).collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(queue.stats("email").await.unwrap().queued, 2);
    let first = queue
        .claim("email", Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.job.id, "first");
    assert_eq!(first.job.attempts, 1);
    assert!(!other.ack(&first).await.unwrap());
    assert_eq!(
        other.fail(&first, "forged").await.unwrap(),
        FailureDisposition::LeaseLost
    );
    assert!(!other.renew(&first, Duration::from_secs(30)).await.unwrap());
    let snapshot = queue.inspect("email", "first").await.unwrap().unwrap();
    assert_eq!(snapshot.status, JobStatus::Processing);
    assert!(queue.renew(&first, Duration::from_secs(60)).await.unwrap());
    let renewed = queue.inspect("email", "first").await.unwrap().unwrap();
    assert!(renewed.lease_expires_at_unix_ms > snapshot.lease_expires_at_unix_ms);
    assert!(queue.renew(&first, Duration::from_millis(1)).await.unwrap());
    assert_eq!(
        queue
            .inspect("email", "first")
            .await
            .unwrap()
            .unwrap()
            .lease_expires_at_unix_ms,
        renewed.lease_expires_at_unix_ms
    );
    // New consumer handle observes the persistent delivery and can acknowledge.
    let restarted = PostgresWorkerQueue::new(
        fixture.pool.clone(),
        TenantContext::new(&fixture.role).unwrap(),
        RetryPolicy::new(100, 0),
    )
    .await
    .unwrap();
    assert!(restarted.ack(&first).await.unwrap());
    assert!(!queue.ack(&first).await.unwrap());
    assert_eq!(
        queue.fail(&first, "late").await.unwrap(),
        FailureDisposition::LeaseLost
    );
    assert_eq!(
        queue
            .inspect("email", "first")
            .await
            .unwrap()
            .unwrap()
            .status,
        JobStatus::Completed
    );
    let second = queue
        .claim("email", Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        queue.fail(&second, "transient").await.unwrap(),
        FailureDisposition::RetryScheduled
    );
    assert!(queue.peek("email", 10).await.unwrap().is_empty());
    assert!(
        queue
            .claim("email", Duration::from_secs(30))
            .await
            .unwrap()
            .is_none()
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let retry = restarted
        .claim("email", Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry.job.id, "second");
    assert_eq!(retry.job.attempts, 2);
    assert_ne!(retry.token, second.token);
    assert!(!queue.ack(&second).await.unwrap());
    assert_eq!(
        restarted.fail(&retry, "permanent").await.unwrap(),
        FailureDisposition::DeadLetter
    );
    let failed = queue.inspect("email", "second").await.unwrap().unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.last_error.as_deref(), Some("permanent"));
    assert!(
        queue
            .claim("email", Duration::from_secs(30))
            .await
            .unwrap()
            .is_none()
    );

    queue
        .enqueue(Job::new("crash", "crashes", "null"))
        .await
        .unwrap();
    let abandoned = queue
        .claim("crashes", Duration::from_millis(20))
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(!queue.ack(&abandoned).await.unwrap());
    assert!(
        !queue
            .renew(&abandoned, Duration::from_secs(1))
            .await
            .unwrap()
    );
    let recovered = queue
        .claim("crashes", Duration::from_millis(20))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.job.attempts, 2);
    assert_ne!(abandoned.token, recovered.token);
    assert_eq!(
        queue.fail(&abandoned, "stale").await.unwrap(),
        FailureDisposition::LeaseLost
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(
        queue
            .claim("crashes", Duration::from_secs(30))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        queue
            .inspect("crashes", "crash")
            .await
            .unwrap()
            .unwrap()
            .status,
        JobStatus::Failed
    );

    // Duplicate batches roll back every insert, including earlier valid jobs.
    assert!(
        queue
            .enqueue_batch(vec![
                Job::new("rollback", "email", "{}"),
                Job::new("first", "email", "{}")
            ])
            .await
            .is_err()
    );
    assert!(queue.inspect("email", "rollback").await.unwrap().is_none());
    assert!(
        queue
            .enqueue(Job::new("bad", "email", "not-json"))
            .await
            .is_err()
    );
    assert!(queue.peek("email", 0).await.is_err());
    assert!(queue.claim("email", Duration::ZERO).await.is_err());
    assert!(queue.enqueue_batch(Vec::new()).await.unwrap().is_empty());

    queue
        .enqueue_batch(
            (0..50)
                .map(|id| Job::new(format!("job-{id}"), "concurrent", "{}"))
                .collect(),
        )
        .await
        .unwrap();
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..10 {
        let consumer = queue.clone();
        tasks.spawn(async move {
            let mut ids = Vec::new();
            while let Some(lease) = consumer
                .claim("concurrent", Duration::from_secs(30))
                .await
                .unwrap()
            {
                assert_eq!(lease.job.attempts, 1);
                assert!(consumer.ack(&lease).await.unwrap());
                ids.push(lease.job.id);
            }
            ids
        });
    }
    let mut ids = HashSet::new();
    while let Some(result) = tasks.join_next().await {
        for id in result.unwrap() {
            assert!(ids.insert(id), "duplicate live delivery");
        }
    }
    assert_eq!(ids.len(), 50);
    assert_eq!(queue.stats("concurrent").await.unwrap().queued, 0);
    assert_eq!(queue.stats("concurrent").await.unwrap().processing, 0);
    let admin_queue = PostgresWorkerQueue::new(
        fixture.pool.clone(),
        TenantContext::new(&fixture.role).unwrap(),
        RetryPolicy::new(1, 0),
    )
    .await
    .unwrap();
    let admin_other =
        admin_queue.for_tenant(TenantContext::new(format!("{}-b", fixture.role)).unwrap());
    verify_admin_contract(&admin_queue, &admin_other).await;
    verify_batch_claim_contract(&admin_queue, &admin_other).await;
    #[cfg(feature = "runtime")]
    verify_retention_contract(&admin_queue, &admin_other).await;
    #[cfg(feature = "runtime")]
    verify_control_contract(&admin_queue).await;
    #[cfg(feature = "runtime")]
    verify_batch_runner_contract(&admin_queue).await;
    // RLS also protects direct queries without a tenant context, and pooled
    // transaction-local settings do not escape to a subsequent borrower.
    let count: (i64,) = sqlx::query_as("SELECT count(*) FROM fl_worker.jobs")
        .fetch_one(&fixture.pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
    let context: (Option<String>,) =
        sqlx::query_as("SELECT nullif(current_setting('forklaunch.tenant_id', true),'')")
            .fetch_one(&fixture.pool)
            .await
            .unwrap();
    assert!(context.0.is_none());
    let denied = sqlx::query("INSERT INTO fl_worker.jobs (tenant_id,queue,id,payload,max_attempts,backoff_ms) VALUES ('foreign','email','foreign','{}',1,0)").execute(&fixture.pool).await;
    assert!(denied.is_err());
    assert_eq!(other.stats("email").await.unwrap().queued, 1);
    fixture.pool.close().await;
    let error = queue.stats("email").await.unwrap_err();
    assert_eq!(error.code, "worker_storage_error");
    assert_eq!(error.message, "Worker storage operation failed");
    fixture.close().await;
}

#[cfg(feature = "runtime")]
mod runner_tests {
    use super::*;
    use forklaunch_core::domain::BlueprintError;
    use forklaunch_core::domain::BlueprintResult;
    use forklaunch_worker_database::runner::{self, JobHandler, RunnerConfig};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::sync::Notify;

    struct Active(Arc<AtomicBool>);
    impl Drop for Active {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    struct Handler {
        started: Arc<AtomicBool>,
        active: Arc<AtomicBool>,
        duration: Duration,
        fails: bool,
    }
    impl JobHandler for Handler {
        async fn process(&self, _job: Job) -> BlueprintResult<()> {
            self.active.store(true, Ordering::SeqCst);
            let _active = Active(Arc::clone(&self.active));
            self.started.store(true, Ordering::SeqCst);
            tokio::time::sleep(self.duration).await;
            if self.fails {
                Err(BlueprintError::new(
                    "test_failure",
                    "secret application message",
                ))
            } else {
                Ok(())
            }
        }
    }
    async fn until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(5) * T, async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    fn config(queue: &str) -> RunnerConfig {
        RunnerConfig {
            queue: queue.into(),
            concurrency: 1,
            batch_size: 1,
            poll_interval: Duration::from_millis(5),
            lease_duration: Duration::from_millis(600) * T,
            renew_interval: Duration::from_millis(150) * T,
            drain_timeout: Duration::from_secs(2) * T,
        }
    }

    #[tokio::test]
    #[ignore = "requires isolated PostgreSQL"]
    #[allow(clippy::too_many_lines)]
    async fn postgres_worker_runner_renews_drains_cancels_and_retries() {
        let fixture = Fixture::new().await;
        let queue = fixture.queue.clone();
        queue
            .enqueue_batch(vec![
                Job::new("active", "runner", "{}"),
                Job::new("pending", "runner", "{}"),
            ])
            .await
            .unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(Notify::new());
        let task = {
            let consumer = queue.clone();
            let shutdown = Arc::clone(&shutdown);
            let handler = Handler {
                started: Arc::clone(&started),
                active: Arc::clone(&active),
                duration: Duration::from_millis(1400) * T,
                fails: false,
            };
            tokio::spawn(async move {
                runner::run(&consumer, &handler, &config("runner"), shutdown.notified())
                    .await
                    .unwrap()
            })
        };
        until(|| started.load(Ordering::SeqCst)).await;
        tokio::time::sleep(Duration::from_millis(800) * T).await;
        assert!(active.load(Ordering::SeqCst));
        assert!(
            queue
                .claim("runner", Duration::from_secs(30))
                .await
                .unwrap()
                .is_some_and(|lease| lease.job.id == "pending")
        );
        // The active job remains leased after more than its original duration.
        assert!(
            queue
                .claim("runner", Duration::from_secs(30))
                .await
                .unwrap()
                .is_none()
        );
        shutdown.notify_one();
        let report = task.await.unwrap();
        assert_eq!(report.completed, 1);
        assert_eq!(report.abandoned, 0);
        assert!(!active.load(Ordering::SeqCst));
        assert_eq!(
            queue
                .inspect("runner", "active")
                .await
                .unwrap()
                .unwrap()
                .job
                .attempts,
            1
        );

        queue
            .enqueue(Job::new("cancel", "cancel", "{}"))
            .await
            .unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(Notify::new());
        let task = {
            let consumer = queue.clone();
            let shutdown = Arc::clone(&shutdown);
            let handler = Handler {
                started: Arc::clone(&started),
                active: Arc::clone(&active),
                duration: Duration::from_secs(10),
                fails: false,
            };
            let mut config = config("cancel");
            config.drain_timeout = Duration::from_millis(100);
            tokio::spawn(async move {
                runner::run(&consumer, &handler, &config, shutdown.notified())
                    .await
                    .unwrap()
            })
        };
        until(|| started.load(Ordering::SeqCst)).await;
        shutdown.notify_one();
        let report = tokio::time::timeout(Duration::from_secs(2) * T, task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(report.abandoned, 1);
        assert!(!active.load(Ordering::SeqCst));
        assert_eq!(
            queue
                .inspect("cancel", "cancel")
                .await
                .unwrap()
                .unwrap()
                .status,
            JobStatus::Processing
        );
        tokio::time::sleep(Duration::from_millis(650) * T).await;
        assert_eq!(
            queue
                .claim("cancel", Duration::from_secs(30))
                .await
                .unwrap()
                .unwrap()
                .job
                .attempts,
            2
        );

        queue.enqueue(Job::new("lost", "lost", "{}")).await.unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(Notify::new());
        let task = {
            let consumer = queue.clone();
            let shutdown = Arc::clone(&shutdown);
            let handler = Handler {
                started: Arc::clone(&started),
                active: Arc::clone(&active),
                duration: Duration::from_secs(10),
                fails: false,
            };
            tokio::spawn(async move {
                runner::run(&consumer, &handler, &config("lost"), shutdown.notified())
                    .await
                    .unwrap()
            })
        };
        until(|| started.load(Ordering::SeqCst)).await;
        // Replace the capability while work is pending. Renewal must cancel
        // the obsolete handler instead of letting it acknowledge this delivery.
        sqlx::query("UPDATE fl_worker.jobs SET lease_token=$2 WHERE tenant_id=$1 AND queue='lost'")
            .bind(&fixture.role)
            .bind(uuid::Uuid::new_v4())
            .execute(&fixture.owner)
            .await
            .unwrap();
        until(|| !active.load(Ordering::SeqCst)).await;
        shutdown.notify_one();
        let report = task.await.unwrap();
        assert_eq!(report.lease_lost, 1);
        assert_eq!(report.completed, 0);

        let queue = PostgresWorkerQueue::new(
            fixture.pool.clone(),
            TenantContext::new(&fixture.role).unwrap(),
            RetryPolicy::new(2, 0),
        )
        .await
        .unwrap();
        queue
            .enqueue(Job::new("failure", "failure", "{}"))
            .await
            .unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(Notify::new());
        let task = {
            let consumer = queue.clone();
            let shutdown = Arc::clone(&shutdown);
            let handler = Handler {
                started: Arc::clone(&started),
                active: Arc::clone(&active),
                duration: Duration::from_millis(5),
                fails: true,
            };
            tokio::spawn(async move {
                runner::run(
                    &consumer,
                    &handler,
                    &RunnerConfig::new("failure"),
                    shutdown.notified(),
                )
                .await
                .unwrap()
            })
        };
        tokio::time::timeout(Duration::from_secs(5) * T, async {
            loop {
                if queue
                    .inspect("failure", "failure")
                    .await
                    .unwrap()
                    .is_some_and(|job| job.status == JobStatus::Failed)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        shutdown.notify_one();
        let report = task.await.unwrap();
        assert_eq!(report.retried, 1);
        assert_eq!(report.dead_lettered, 1);
        assert_eq!(
            queue
                .inspect("failure", "failure")
                .await
                .unwrap()
                .unwrap()
                .last_error
                .as_deref(),
            Some("test_failure")
        );
        // An already requested shutdown never claims a queued job.
        queue
            .enqueue(Job::new("untouched", "idle", "{}"))
            .await
            .unwrap();
        let handler = Handler {
            started,
            active,
            duration: Duration::ZERO,
            fails: false,
        };
        let report = runner::run(&queue, &handler, &config("idle"), async {})
            .await
            .unwrap();
        assert_eq!(report, runner::RunReport::default());
        assert_eq!(queue.stats("idle").await.unwrap().queued, 1);
        fixture.close().await;
    }
}

#[allow(clippy::too_many_lines)]
async fn verify_admin_contract<Q: AsyncWorkerAdmin>(queue: &Q, other: &Q) {
    let name = "admin";
    queue
        .enqueue_batch(
            [
                "failed-1",
                "failed-2",
                "completed-1",
                "completed-2",
                "active",
            ]
            .into_iter()
            .map(|id| Job::new(id, name, "{}"))
            .collect(),
        )
        .await
        .unwrap();
    let mut failed_lease = None;
    for id in ["failed-1", "failed-2", "completed-1", "completed-2"] {
        let lease = queue
            .claim(name, Duration::from_secs(60))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(lease.job.id, id);
        if id.starts_with("failed") {
            assert_eq!(
                queue.fail(&lease, "test_failure").await.unwrap(),
                FailureDisposition::DeadLetter
            );
            if id == "failed-1" {
                failed_lease = Some(lease);
            }
        } else {
            assert!(queue.ack(&lease).await.unwrap());
        }
    }
    let active = queue
        .claim(name, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.job.id, "active");
    other
        .enqueue(Job::new("failed-1", name, "{}"))
        .await
        .unwrap();
    let foreign = other
        .claim(name, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        other.fail(&foreign, "foreign_failure").await.unwrap(),
        FailureDisposition::DeadLetter
    );
    let failed = queue.dead_letters(name, 1).await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].status, JobStatus::Failed);
    assert!(failed[0].lease_expires_at_unix_ms.is_none());
    assert_eq!(queue.dead_letters(name, 1000).await.unwrap().len(), 2);
    assert!(
        !queue
            .retry_dead_letter(name, "missing", RetryPolicy::default())
            .await
            .unwrap()
    );
    assert!(
        !queue
            .retry_dead_letter(name, "active", RetryPolicy::default())
            .await
            .unwrap()
    );
    assert!(
        !queue
            .retry_dead_letter(name, "completed-1", RetryPolicy::default())
            .await
            .unwrap()
    );
    let policy = RetryPolicy::new(2, 0);
    let (first, second) = tokio::join!(
        queue.retry_dead_letter(name, "failed-1", policy),
        queue.retry_dead_letter(name, "failed-1", policy)
    );
    assert_eq!(
        usize::from(first.unwrap()) + usize::from(second.unwrap()),
        1
    );
    let reset = queue.inspect(name, "failed-1").await.unwrap().unwrap();
    assert_eq!(reset.status, JobStatus::Queued);
    assert_eq!(reset.job.attempts, 0);
    assert!(reset.last_error.is_none());
    let old = failed_lease.unwrap();
    assert!(!queue.ack(&old).await.unwrap());
    assert!(!queue.renew(&old, Duration::from_secs(60)).await.unwrap());
    assert_eq!(
        queue.fail(&old, "stale").await.unwrap(),
        FailureDisposition::LeaseLost
    );
    let retry = queue
        .claim(name, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry.job.id, "failed-1");
    assert_eq!(retry.job.attempts, 1);
    assert_eq!(
        queue.fail(&retry, "retry_failure").await.unwrap(),
        FailureDisposition::RetryScheduled
    );
    let retry = queue
        .claim(name, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry.job.attempts, 2);
    assert_eq!(
        queue.fail(&retry, "retry_failure").await.unwrap(),
        FailureDisposition::DeadLetter
    );
    queue.enqueue(Job::new("queued", name, "{}")).await.unwrap();
    assert!(
        !queue
            .retry_dead_letter(name, "queued", policy)
            .await
            .unwrap()
    );
    assert_eq!(
        queue
            .purge_terminal(name, TerminalStatus::Failed, Duration::from_hours(24), 1000)
            .await
            .unwrap(),
        0
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    for status in [TerminalStatus::Completed, TerminalStatus::Failed] {
        assert_eq!(
            queue
                .purge_terminal(name, status, Duration::from_millis(1), 1)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            queue
                .purge_terminal(name, status, Duration::from_millis(1), 1)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            queue
                .purge_terminal(name, status, Duration::from_millis(1), 1)
                .await
                .unwrap(),
            0
        );
    }
    assert!(queue.dead_letters(name, 1000).await.unwrap().is_empty());
    assert_eq!(
        queue.inspect(name, "queued").await.unwrap().unwrap().status,
        JobStatus::Queued
    );
    assert_eq!(
        queue.inspect(name, "active").await.unwrap().unwrap().status,
        JobStatus::Processing
    );
    assert_eq!(other.dead_letters(name, 1000).await.unwrap().len(), 1);
    queue
        .enqueue(Job::new("completed-1", name, "{}"))
        .await
        .unwrap();
    assert_eq!(
        queue
            .inspect(name, "completed-1")
            .await
            .unwrap()
            .unwrap()
            .status,
        JobStatus::Queued
    );
    for limit in [0, 1001] {
        assert!(queue.dead_letters(name, limit).await.is_err());
        assert!(
            queue
                .purge_terminal(name, TerminalStatus::Failed, Duration::from_secs(1), limit)
                .await
                .is_err()
        );
    }
    for age in [Duration::ZERO, Duration::MAX] {
        assert!(
            queue
                .purge_terminal(name, TerminalStatus::Completed, age, 1)
                .await
                .is_err()
        );
    }
    assert!(
        queue
            .retry_dead_letter(name, "queued", RetryPolicy::new(0, 0))
            .await
            .is_err()
    );
    assert!(
        queue
            .retry_dead_letter(name, "queued", RetryPolicy::new(1, u64::MAX))
            .await
            .is_err()
    );
    assert!(queue.dead_letters("", 1).await.is_err());
    assert!(queue.retry_dead_letter(name, "", policy).await.is_err());
    // Requeue and retention serialize; exactly one operation wins.
    queue
        .enqueue(Job::new("race", "admin-race", "{}"))
        .await
        .unwrap();
    let lease = queue
        .claim("admin-race", Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        queue.fail(&lease, "race").await.unwrap(),
        FailureDisposition::DeadLetter
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
    let (purged, requeued) = tokio::join!(
        queue.purge_terminal(
            "admin-race",
            TerminalStatus::Failed,
            Duration::from_millis(1),
            1
        ),
        queue.retry_dead_letter("admin-race", "race", policy)
    );
    assert_eq!(purged.unwrap() + usize::from(requeued.unwrap()), 1);
    assert!(!queue.ack(&lease).await.unwrap());
}

#[cfg(feature = "runtime")]
#[allow(clippy::too_many_lines)]
async fn verify_retention_contract<Q: AsyncWorkerAdmin>(queue: &Q, other: &Q) {
    use forklaunch_worker_database::retention::{self, RetentionPolicy, RetentionReport};
    let name = "retention";
    queue
        .enqueue_batch(
            ["completed", "failed", "active", "queued"]
                .into_iter()
                .map(|id| Job::new(id, name, "{}"))
                .collect(),
        )
        .await
        .unwrap();
    let completed = queue
        .claim(name, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert!(queue.ack(&completed).await.unwrap());
    let failed = queue
        .claim(name, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        queue.fail(&failed, "retained_failure").await.unwrap(),
        FailureDisposition::DeadLetter
    );
    let active = queue
        .claim(name, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.job.id, "active");
    other
        .enqueue(Job::new("completed", name, "{}"))
        .await
        .unwrap();
    let foreign = other
        .claim(name, Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert!(other.ack(&foreign).await.unwrap());
    let mut policy = RetentionPolicy {
        interval: Duration::from_millis(25),
        batch_limit: 1,
        ..RetentionPolicy::default()
    };
    let report = retention::run(queue, name, &policy, async {
        tokio::time::sleep(Duration::from_millis(75)).await;
    })
    .await
    .unwrap();
    assert_eq!(report, RetentionReport::default());
    policy.completed_age = Some(Duration::from_millis(1));
    let report = retention::run(queue, name, &policy, async {})
        .await
        .unwrap();
    assert_eq!(report, RetentionReport::default());
    assert!(queue.inspect(name, "completed").await.unwrap().is_some());
    let report = retention::run(queue, name, &policy, async {
        tokio::time::sleep(Duration::from_millis(150) * T).await;
    })
    .await
    .unwrap();
    assert_eq!(report.completed_purged, 1);
    assert_eq!(report.failed_purged, 0);
    assert!(queue.inspect(name, "completed").await.unwrap().is_none());
    assert_eq!(
        queue.inspect(name, "failed").await.unwrap().unwrap().status,
        JobStatus::Failed
    );
    assert!(other.inspect(name, "completed").await.unwrap().is_some());
    policy.completed_age = None;
    policy.failed_age = Some(Duration::from_millis(1));
    let report = retention::run(queue, name, &policy, async {
        tokio::time::sleep(Duration::from_millis(150) * T).await;
    })
    .await
    .unwrap();
    assert_eq!(report.completed_purged, 0);
    assert_eq!(report.failed_purged, 1);
    assert!(queue.inspect(name, "failed").await.unwrap().is_none());
    assert_eq!(
        queue.inspect(name, "active").await.unwrap().unwrap().status,
        JobStatus::Processing
    );
    assert_eq!(
        queue.inspect(name, "queued").await.unwrap().unwrap().status,
        JobStatus::Queued
    );
}

#[cfg(feature = "runtime")]
#[allow(clippy::too_many_lines)]
async fn verify_control_contract<Q: AsyncWorkerQueue>(queue: &Q) {
    use forklaunch_worker_database::{
        control::{WorkerControl, WorkerState},
        runner::{self, JobHandler, RunnerConfig},
    };
    struct ControlledHandler;
    impl JobHandler for ControlledHandler {
        async fn process(&self, _job: Job) -> forklaunch_core::domain::BlueprintResult<()> {
            tokio::time::sleep(Duration::from_millis(600) * T).await;
            Ok(())
        }
    }
    async fn wait<Q: AsyncWorkerQueue>(queue: &Q, id: &str, status: JobStatus) {
        tokio::time::timeout(Duration::from_secs(5) * T, async {
            loop {
                if queue
                    .inspect("controlled", id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status
                    == status
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    queue
        .enqueue_batch(vec![
            Job::new("first", "controlled", "{}"),
            Job::new("second", "controlled", "{}"),
            Job::new("third", "controlled", "{}"),
        ])
        .await
        .unwrap();
    let control = WorkerControl::new();
    assert!(control.pause());
    let config = RunnerConfig {
        queue: "controlled".into(),
        concurrency: 1,
        batch_size: 1,
        poll_interval: Duration::from_millis(5),
        lease_duration: Duration::from_millis(300) * T,
        renew_interval: Duration::from_millis(75) * T,
        drain_timeout: Duration::from_secs(2) * T,
    };
    let (report, ()) = tokio::join!(
        runner::run_controlled(
            queue,
            &ControlledHandler,
            &config,
            &control,
            std::future::pending()
        ),
        async {
            tokio::time::sleep(Duration::from_millis(40)).await;
            assert_eq!(queue.stats("controlled").await.unwrap().queued, 3);
            assert!(control.resume());
            wait(queue, "first", JobStatus::Processing).await;
            assert!(control.pause());
            tokio::time::sleep(Duration::from_millis(350) * T).await;
            assert_eq!(
                queue
                    .inspect("controlled", "first")
                    .await
                    .unwrap()
                    .unwrap()
                    .job
                    .attempts,
                1
            );
            assert_eq!(queue.stats("controlled").await.unwrap().processing, 1);
            wait(queue, "first", JobStatus::Completed).await;
            assert_eq!(queue.stats("controlled").await.unwrap().queued, 2);
            assert!(control.resume());
            wait(queue, "second", JobStatus::Processing).await;
            assert!(control.pause());
            control.drain();
            assert!(!control.resume());
            assert!(!control.pause());
        }
    );
    assert_eq!(report.unwrap().completed, 2);
    assert_eq!(control.state(), WorkerState::Draining);
    assert_eq!(
        queue
            .inspect("controlled", "second")
            .await
            .unwrap()
            .unwrap()
            .status,
        JobStatus::Completed
    );
    assert_eq!(
        queue
            .inspect("controlled", "third")
            .await
            .unwrap()
            .unwrap()
            .status,
        JobStatus::Queued
    );
}

#[allow(clippy::too_many_lines)]
async fn verify_batch_claim_contract<Q: AsyncWorkerQueue>(queue: &Q, other: &Q) {
    let jobs = (0..7)
        .map(|index| Job::new(format!("batch-{index}"), "batch-claims", "{}"))
        .collect();
    queue.enqueue_batch(jobs).await.unwrap();
    other
        .enqueue(Job::new("batch-0", "batch-claims", "{}"))
        .await
        .unwrap();
    for limit in [0, 1001] {
        assert_eq!(
            queue
                .claim_batch("batch-claims", Duration::from_secs(1), limit)
                .await
                .unwrap_err()
                .code,
            "worker_invalid_input"
        );
    }
    assert!(
        queue
            .claim_batch("batch-claims", Duration::ZERO, 3)
            .await
            .is_err()
    );
    assert_eq!(queue.stats("batch-claims").await.unwrap().queued, 7);
    let first = queue
        .claim_batch("batch-claims", Duration::from_secs(30), 2)
        .await
        .unwrap();
    assert_eq!(
        first
            .iter()
            .map(|lease| lease.job.id.as_str())
            .collect::<Vec<_>>(),
        ["batch-0", "batch-1"]
    );
    let (second, third) = tokio::join!(
        queue.claim_batch("batch-claims", Duration::from_secs(30), 3),
        queue.claim_batch("batch-claims", Duration::from_secs(30), 3)
    );
    let leases: Vec<_> = first
        .into_iter()
        .chain(second.unwrap())
        .chain(third.unwrap())
        .collect();
    assert_eq!(leases.len(), 7);
    assert_eq!(
        leases
            .iter()
            .map(|lease| &lease.job.id)
            .collect::<HashSet<_>>()
            .len(),
        7
    );
    assert_eq!(
        leases
            .iter()
            .map(|lease| &lease.token)
            .collect::<HashSet<_>>()
            .len(),
        7
    );
    assert!(
        queue
            .claim_batch("batch-claims", Duration::from_secs(30), 1000)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(other.stats("batch-claims").await.unwrap().queued, 1);
    for lease in &leases {
        assert_eq!(lease.job.attempts, 1);
        assert!(!other.ack(lease).await.unwrap());
        assert!(queue.renew(lease, Duration::from_secs(30)).await.unwrap());
        assert!(queue.ack(lease).await.unwrap());
        assert!(!queue.ack(lease).await.unwrap());
    }
    // Final-attempt expired deliveries become retained failures in the same
    // transaction/script, rather than being handed out beyond their budget.
    queue
        .enqueue_batch(vec![
            Job::new("expired-a", "batch-expired", "{}"),
            Job::new("expired-b", "batch-expired", "{}"),
        ])
        .await
        .unwrap();
    let expired = queue
        .claim_batch("batch-expired", Duration::from_millis(1), 2)
        .await
        .unwrap();
    assert_eq!(expired.len(), 2);
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert!(
        queue
            .claim_batch("batch-expired", Duration::from_secs(30), 2)
            .await
            .unwrap()
            .is_empty()
    );
    for lease in expired {
        assert!(!queue.ack(&lease).await.unwrap());
        assert_eq!(
            queue
                .inspect("batch-expired", &lease.job.id)
                .await
                .unwrap()
                .unwrap()
                .status,
            JobStatus::Failed
        );
    }
}

#[cfg(feature = "runtime")]
#[allow(clippy::too_many_lines)]
async fn verify_batch_runner_contract<Q: AsyncWorkerQueue>(queue: &Q) {
    use forklaunch_core::domain::BlueprintError;
    use forklaunch_core::domain::BlueprintResult;
    use forklaunch_worker_database::{
        control::WorkerControl,
        runner::{self, JobFailure, JobHandler, RunnerConfig},
    };
    struct Handler;
    impl JobHandler for Handler {
        async fn process(&self, _job: Job) -> BlueprintResult<()> {
            panic!("single handler used for a configured batch");
        }
        async fn process_batch(&self, jobs: Vec<Job>) -> BlueprintResult<Vec<JobFailure>> {
            tokio::time::sleep(
                Duration::from_millis(if jobs[0].queue == "batch-drain" {
                    700
                } else {
                    400
                }) * T,
            )
            .await;
            let failure = |id: String| JobFailure {
                id,
                error: BlueprintError::new("batch_failure", "secret payload details"),
            };
            match jobs[0].queue.as_str() {
                "batch-invalid-unknown" => Ok(vec![failure("foreign-job".into())]),
                "batch-invalid-duplicate" => Ok(vec![
                    failure(jobs[0].id.clone()),
                    failure(jobs[0].id.clone()),
                ]),
                "batch-error" => Err(BlueprintError::new("batch_error", "secret batch details")),
                _ => Ok(jobs
                    .into_iter()
                    .filter(|job| job.id == "fail")
                    .map(|job| failure(job.id))
                    .collect()),
            }
        }
    }
    for name in [
        "batch-mixed",
        "batch-invalid-unknown",
        "batch-invalid-duplicate",
        "batch-error",
        "batch-drain",
    ] {
        let drain = name == "batch-drain";
        let ids: Vec<_> = if drain {
            (0..7).map(|id| format!("job-{id}")).collect()
        } else {
            vec!["ok-a".into(), "fail".into(), "ok-b".into()]
        };
        queue
            .enqueue_batch(ids.iter().map(|id| Job::new(id, name, "{}")).collect())
            .await
            .unwrap();
        let control = WorkerControl::new();
        let config = RunnerConfig {
            queue: name.into(),
            concurrency: if drain { 2 } else { 1 },
            batch_size: 3,
            poll_interval: Duration::from_millis(5),
            lease_duration: Duration::from_millis(300) * T,
            renew_interval: Duration::from_millis(75) * T,
            drain_timeout: Duration::from_secs(2) * T,
        };
        let (report, ()) = tokio::time::timeout(Duration::from_secs(5) * T, async {
            tokio::join!(
                runner::run_controlled(queue, &Handler, &config, &control, std::future::pending()),
                async {
                    loop {
                        let stats = queue.stats(name).await.unwrap();
                        if (drain && stats.processing == 6)
                            || (!drain && stats.processing == 0 && stats.queued == 0)
                        {
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    control.drain();
                }
            )
        })
        .await
        .unwrap();
        let report = report.unwrap();
        if drain {
            assert_eq!(report.completed, 6);
            assert_eq!(
                queue.inspect(name, "job-6").await.unwrap().unwrap().status,
                JobStatus::Queued
            );
        } else if name == "batch-mixed" {
            assert_eq!(report.completed, 2);
            assert_eq!(report.dead_lettered, 1);
            let failure = queue.inspect(name, "fail").await.unwrap().unwrap();
            assert_eq!(failure.last_error.as_deref(), Some("batch_failure"));
            assert_eq!(failure.job.attempts, 1);
        } else {
            assert_eq!(report.completed, 0);
            assert_eq!(report.dead_lettered, 3);
            for id in ids {
                let snapshot = queue.inspect(name, &id).await.unwrap().unwrap();
                assert_eq!(snapshot.status, JobStatus::Failed);
                assert_eq!(
                    snapshot.last_error.as_deref(),
                    Some(if name == "batch-error" {
                        "batch_error"
                    } else {
                        "worker_invalid_batch_result"
                    })
                );
            }
        }
        assert_eq!(report.lease_lost, 0);
        assert_eq!(report.abandoned, 0);
    }
}

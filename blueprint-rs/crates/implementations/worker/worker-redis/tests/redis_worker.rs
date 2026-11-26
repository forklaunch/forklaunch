#![cfg(feature = "redis-client")]
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::TenantContext;
use forklaunch_worker_database::runner::{self, JobHandler, RunnerConfig};
use forklaunch_worker_interface::{
    AsyncWorkerAdmin, AsyncWorkerQueue, FailureDisposition, Job, JobStatus, RetryPolicy,
    TerminalStatus,
};
use forklaunch_worker_redis::leased::RedisLeasedQueue;
use redis::aio::MultiplexedConnection;
use std::{
    collections::HashSet,
    fmt::Write,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

fn encoded(value: &str) -> String {
    value.bytes().fold(String::new(), |mut output, byte| {
        let _ = write!(&mut output, "{byte:02x}");
        output
    })
}
fn prefix(namespace: &str, tenant: &str, queue: &str) -> String {
    format!(
        "fl_worker:{{{}_{}}}:{}",
        encoded(namespace),
        encoded(tenant),
        encoded(queue)
    )
}
async fn clean(connection: &mut MultiplexedConnection, namespace: &str) {
    let mut cursor = 0_u64;
    loop {
        let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(format!("fl_worker:{{{}_*", encoded(namespace)))
            .arg("COUNT")
            .arg(100)
            .query_async(connection)
            .await
            .unwrap();
        if !keys.is_empty() {
            let _: usize = redis::cmd("DEL")
                .arg(keys)
                .query_async(connection)
                .await
                .unwrap();
        }
        cursor = next;
        if cursor == 0 {
            break;
        }
    }
}

struct SlowHandler;
impl JobHandler for SlowHandler {
    async fn process(&self, _job: Job) -> BlueprintResult<()> {
        tokio::time::sleep(Duration::from_millis(1800)).await;
        Ok(())
    }
}

struct ErrorDrainHandler(Arc<AtomicUsize>);
impl JobHandler for ErrorDrainHandler {
    async fn process(&self, job: Job) -> BlueprintResult<()> {
        tokio::time::sleep(Duration::from_millis(if job.id == "fault-0" {
            1000
        } else {
            1500
        }))
        .await;
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
#[ignore = "requires isolated Redis and FORKLAUNCH_TEST_REDIS_URL"]
#[allow(clippy::too_many_lines)]
async fn redis_worker_leases_batches_retries_isolation_and_runner() {
    let url = std::env::var("FORKLAUNCH_TEST_REDIS_URL").expect("Redis URL required");
    let namespace = format!("worker-{}", uuid::Uuid::new_v4());
    let queue = RedisLeasedQueue::connect(
        &url,
        &namespace,
        TenantContext::new("tenant-a").unwrap(),
        RetryPolicy::new(2, 1),
    )
    .await
    .unwrap();
    let other = queue.for_tenant(TenantContext::new("tenant-a:other").unwrap());
    let mut admin = redis::Client::open(url.as_str())
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    queue
        .enqueue_batch(vec![
            Job::new("z", "email", "{\"message\":\"héllo\"}"),
            Job::new("a", "email", "{}"),
            Job::new("z", "secondary", "{}"),
        ])
        .await
        .unwrap();
    other.enqueue(Job::new("z", "email", "null")).await.unwrap();
    assert_eq!(
        queue
            .peek("email", 10)
            .await
            .unwrap()
            .iter()
            .map(|job| job.id.as_str())
            .collect::<Vec<_>>(),
        ["z", "a"]
    );
    assert_eq!(queue.stats("email").await.unwrap().queued, 2);
    // A mixed-queue duplicate aborts the entire batch before the first write.
    assert!(
        queue
            .enqueue_batch(vec![
                Job::new("rollback", "secondary", "{}"),
                Job::new("z", "email", "{}")
            ])
            .await
            .is_err()
    );
    assert!(
        queue
            .inspect("secondary", "rollback")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        queue
            .enqueue_batch(vec![
                Job::new("duplicate", "email", "{}"),
                Job::new("duplicate", "email", "{}")
            ])
            .await
            .is_err()
    );
    assert!(queue.inspect("email", "duplicate").await.unwrap().is_none());

    let first = queue
        .claim("email", Duration::from_secs(30))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.job.id, "z");
    assert_eq!(first.job.attempts, 1);
    assert!(first.job.payload_json.contains("héllo"));
    assert!(!other.ack(&first).await.unwrap());
    assert!(!other.renew(&first, Duration::from_secs(30)).await.unwrap());
    assert_eq!(
        other.fail(&first, "forged").await.unwrap(),
        FailureDisposition::LeaseLost
    );
    assert!(queue.renew(&first, Duration::from_secs(60)).await.unwrap());
    let deadline = queue
        .inspect("email", "z")
        .await
        .unwrap()
        .unwrap()
        .lease_expires_at_unix_ms;
    assert!(queue.renew(&first, Duration::from_millis(1)).await.unwrap());
    assert_eq!(
        deadline,
        queue
            .inspect("email", "z")
            .await
            .unwrap()
            .unwrap()
            .lease_expires_at_unix_ms
    );
    let restarted = RedisLeasedQueue::connect(
        &url,
        &namespace,
        TenantContext::new("tenant-a").unwrap(),
        RetryPolicy::new(100, 0),
    )
    .await
    .unwrap();
    assert!(restarted.ack(&first).await.unwrap());
    assert!(!queue.ack(&first).await.unwrap());
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
    assert_eq!(retry.job.attempts, 2);
    assert_ne!(retry.token, second.token);
    assert!(!queue.ack(&second).await.unwrap());
    assert_eq!(
        restarted.fail(&retry, "permanent").await.unwrap(),
        FailureDisposition::DeadLetter
    );
    let failed = queue.inspect("email", "a").await.unwrap().unwrap();
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.last_error.as_deref(), Some("permanent"));

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
    assert_ne!(recovered.token, abandoned.token);
    assert_eq!(
        queue.fail(&abandoned, "obsolete").await.unwrap(),
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
    assert_eq!(queue.stats("concurrent").await.unwrap().processing, 0);

    // Wrong key types are rejected before writes, including another queue in
    // the same batch. Script errors must not leave partially inserted jobs.
    let broken = prefix(&namespace, "tenant-a", "broken");
    let _: () = redis::cmd("SET")
        .arg(format!("{broken}:ready"))
        .arg("wrong-type")
        .query_async(&mut admin)
        .await
        .unwrap();
    assert!(
        queue
            .enqueue_batch(vec![
                Job::new("rollback", "healthy", "{}"),
                Job::new("broken", "broken", "{}")
            ])
            .await
            .is_err()
    );
    assert!(
        queue
            .inspect("healthy", "rollback")
            .await
            .unwrap()
            .is_none()
    );
    let absent: Option<String> = redis::cmd("HGET")
        .arg(format!("{broken}:jobs"))
        .arg("broken")
        .query_async(&mut admin)
        .await
        .unwrap();
    assert!(absent.is_none());
    let _: usize = redis::cmd("DEL")
        .arg(format!("{broken}:ready"))
        .query_async(&mut admin)
        .await
        .unwrap();
    assert_eq!(queue.stats("broken").await.unwrap().queued, 0);
    assert!(
        queue
            .enqueue(Job::new("bad", "email", "not-json"))
            .await
            .is_err()
    );
    assert!(queue.peek("email", 0).await.is_err());
    assert!(queue.claim("email", Duration::ZERO).await.is_err());
    assert_eq!(other.stats("email").await.unwrap().queued, 1);

    queue
        .enqueue(Job::new("runner", "runner", "{}"))
        .await
        .unwrap();
    let config = RunnerConfig {
        queue: "runner".into(),
        concurrency: 1,
        batch_size: 1,
        poll_interval: Duration::from_millis(25),
        lease_duration: Duration::from_millis(1200),
        renew_interval: Duration::from_millis(300),
        drain_timeout: Duration::from_secs(3),
    };
    let (report, ()) = tokio::join!(
        runner::run(&queue, &SlowHandler, &config, async {
            tokio::time::sleep(Duration::from_millis(1500)).await;
        }),
        async {
            tokio::time::sleep(Duration::from_millis(1400)).await;
            assert!(
                queue
                    .claim("runner", Duration::from_secs(30))
                    .await
                    .unwrap()
                    .is_none()
            );
        }
    );
    assert_eq!(report.unwrap().completed, 1);
    assert_eq!(
        queue
            .inspect("runner", "runner")
            .await
            .unwrap()
            .unwrap()
            .job
            .attempts,
        1
    );
    queue
        .enqueue_batch(
            (0..5)
                .map(|index| Job::new(format!("parallel-{index}"), "parallel", "{}"))
                .collect(),
        )
        .await
        .unwrap();
    let config = RunnerConfig {
        queue: "parallel".into(),
        concurrency: 4,
        batch_size: 1,
        ..config
    };
    let (report, ()) = tokio::join!(
        runner::run(&queue, &SlowHandler, &config, async {
            tokio::time::sleep(Duration::from_millis(400)).await;
        }),
        async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let stats = queue.stats("parallel").await.unwrap();
            assert_eq!(stats.processing, 4);
            assert_eq!(stats.queued, 1);
        }
    );
    assert_eq!(report.unwrap().completed, 4);
    let pending = queue
        .inspect("parallel", "parallel-4")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.status, JobStatus::Queued);
    assert_eq!(pending.job.attempts, 0);
    for index in 0..4 {
        let completed = queue
            .inspect("parallel", &format!("parallel-{index}"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(completed.status, JobStatus::Completed);
        assert_eq!(completed.job.attempts, 1);
    }
    // A storage error in one consumer must drain the others, not drop their
    // handler futures immediately. Corrupt only this test's isolated queue.
    queue
        .enqueue_batch(
            (0..4)
                .map(|index| Job::new(format!("fault-{index}"), "fault", "{}"))
                .collect(),
        )
        .await
        .unwrap();
    let completions = Arc::new(AtomicUsize::new(0));
    let handler = ErrorDrainHandler(Arc::clone(&completions));
    let config = RunnerConfig {
        concurrency: 4,
        batch_size: 1,
        ..RunnerConfig::new("fault")
    };
    let ready_key = format!("{}:ready", prefix(&namespace, "tenant-a", "fault"));
    let (result, ()) = tokio::join!(
        tokio::time::timeout(
            Duration::from_secs(5),
            runner::run(&queue, &handler, &config, std::future::pending())
        ),
        async {
            tokio::time::timeout(Duration::from_secs(2), async {
                while queue.stats("fault").await.unwrap().processing != 4 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            let _: () = redis::cmd("SET")
                .arg(&ready_key)
                .arg("wrong-type")
                .query_async(&mut admin)
                .await
                .unwrap();
        }
    );
    assert_eq!(result.unwrap().unwrap_err().code, "worker_storage_error");
    assert_eq!(completions.load(Ordering::SeqCst), 4);
    let _: usize = redis::cmd("DEL")
        .arg(&ready_key)
        .query_async(&mut admin)
        .await
        .unwrap();
    // Retention failures stop claims and drain handlers before surfacing.
    queue
        .enqueue_batch(
            (0..4)
                .map(|index| Job::new(format!("fault-{index}"), "retention-fault", "{}"))
                .collect(),
        )
        .await
        .unwrap();
    completions.store(0, Ordering::SeqCst);
    let config = RunnerConfig {
        concurrency: 4,
        batch_size: 1,
        ..RunnerConfig::new("retention-fault")
    };
    let policy = forklaunch_worker_database::retention::RetentionPolicy {
        completed_age: Some(Duration::from_millis(1)),
        interval: Duration::from_millis(25),
        ..forklaunch_worker_database::retention::RetentionPolicy::default()
    };
    let ready_key = format!(
        "{}:ready",
        prefix(&namespace, "tenant-a", "retention-fault")
    );
    let (result, ()) = tokio::join!(
        tokio::time::timeout(
            Duration::from_secs(5),
            forklaunch_worker_database::retention::run_worker(
                &queue,
                &handler,
                &config,
                &policy,
                std::future::pending()
            )
        ),
        async {
            tokio::time::timeout(Duration::from_secs(2), async {
                while queue.stats("retention-fault").await.unwrap().processing != 4 {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            let _: () = redis::cmd("SET")
                .arg(&ready_key)
                .arg("wrong-type")
                .query_async(&mut admin)
                .await
                .unwrap();
        }
    );
    assert_eq!(result.unwrap().unwrap_err().code, "worker_storage_error");
    assert_eq!(completions.load(Ordering::SeqCst), 4);
    let _: usize = redis::cmd("DEL")
        .arg(&ready_key)
        .query_async(&mut admin)
        .await
        .unwrap();
    let error = RedisLeasedQueue::connect(
        "secret-invalid-url",
        &namespace,
        TenantContext::new("tenant-a").unwrap(),
        RetryPolicy::default(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.message, "Worker storage operation failed");
    let admin_queue = RedisLeasedQueue::connect(
        &url,
        &namespace,
        TenantContext::new("tenant-a").unwrap(),
        RetryPolicy::new(1, 0),
    )
    .await
    .unwrap();
    let admin_other = admin_queue.for_tenant(TenantContext::new("tenant-a:other").unwrap());
    verify_admin_contract(&admin_queue, &admin_other).await;
    verify_batch_claim_contract(&admin_queue, &admin_other).await;
    verify_retention_contract(&admin_queue, &admin_other).await;
    verify_control_contract(&admin_queue).await;
    verify_batch_runner_contract(&admin_queue).await;
    // A corrupt later record must not leave earlier batch members claimed:
    // Redis cannot roll back a script's earlier writes on a runtime error.
    admin_queue
        .enqueue_batch(vec![
            Job::new("first", "corrupt-batch", "{}"),
            Job::new("last", "corrupt-batch", "{}"),
        ])
        .await
        .unwrap();
    let jobs_key = format!("{}:jobs", prefix(&namespace, "tenant-a", "corrupt-batch"));
    let original: String = redis::cmd("HGET")
        .arg(&jobs_key)
        .arg("last")
        .query_async(&mut admin)
        .await
        .unwrap();
    let mut corrupt: serde_json::Value = serde_json::from_str(&original).unwrap();
    corrupt["status"] = serde_json::json!("completed");
    for bad in [
        serde_json::to_string(&corrupt).unwrap(),
        "not-json".to_owned(),
    ] {
        let _: usize = redis::cmd("HSET")
            .arg(&jobs_key)
            .arg("last")
            .arg(bad)
            .query_async(&mut admin)
            .await
            .unwrap();
        assert_eq!(
            admin_queue
                .claim_batch("corrupt-batch", Duration::from_secs(30), 2)
                .await
                .unwrap_err()
                .code,
            "worker_storage_error"
        );
        let first = admin_queue
            .inspect("corrupt-batch", "first")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.status, JobStatus::Queued);
        assert_eq!(first.job.attempts, 0);
    }
    let _: usize = redis::cmd("HSET")
        .arg(&jobs_key)
        .arg("last")
        .arg(original)
        .query_async(&mut admin)
        .await
        .unwrap();
    for lease in admin_queue
        .claim_batch("corrupt-batch", Duration::from_secs(30), 2)
        .await
        .unwrap()
    {
        assert!(admin_queue.ack(&lease).await.unwrap());
    }
    // Legacy retained jobs have no terminal timestamp or indexes. Index them
    // incrementally and start their retention age at first observation.
    admin_queue
        .enqueue(Job::new("legacy", "legacy", "{}"))
        .await
        .unwrap();
    let lease = admin_queue
        .claim("legacy", Duration::from_secs(60))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        admin_queue.fail(&lease, "legacy_failure").await.unwrap(),
        FailureDisposition::DeadLetter
    );
    let legacy_prefix = prefix(&namespace, "tenant-a", "legacy");
    let stored: String = redis::cmd("HGET")
        .arg(format!("{legacy_prefix}:jobs"))
        .arg("legacy")
        .query_async(&mut admin)
        .await
        .unwrap();
    let mut record: serde_json::Value = serde_json::from_str(&stored).unwrap();
    record.as_object_mut().unwrap().remove("terminal_at");
    let mut pipeline = redis::pipe();
    for index in 0..250 {
        let id = format!("legacy-{index}");
        record["id"] = serde_json::json!(id);
        pipeline
            .cmd("HSET")
            .arg(format!("{legacy_prefix}:jobs"))
            .arg(&id)
            .arg(record.to_string())
            .ignore();
    }
    pipeline
        .cmd("HSET")
        .arg(format!("{legacy_prefix}:jobs"))
        .arg("legacy")
        .arg(stored.replace("\"terminal_at\":", "\"ignored_terminal_at\":"))
        .ignore();
    pipeline
        .cmd("DEL")
        .arg(format!("{legacy_prefix}:failed"))
        .arg(format!("{legacy_prefix}:admin-index"))
        .ignore();
    let _: () = pipeline.query_async(&mut admin).await.unwrap();
    assert_eq!(
        admin_queue
            .purge_terminal(
                "legacy",
                TerminalStatus::Failed,
                Duration::from_hours(24),
                1000
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        admin_queue
            .dead_letters("legacy", 1000)
            .await
            .unwrap()
            .len(),
        251
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        admin_queue
            .purge_terminal(
                "legacy",
                TerminalStatus::Failed,
                Duration::from_millis(1),
                1000
            )
            .await
            .unwrap(),
        251
    );
    clean(&mut admin, &namespace).await;
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
        tokio::time::sleep(Duration::from_millis(150)).await;
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
        tokio::time::sleep(Duration::from_millis(150)).await;
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

#[allow(clippy::too_many_lines)]
async fn verify_control_contract<Q: AsyncWorkerQueue>(queue: &Q) {
    use forklaunch_worker_database::{
        control::{WorkerControl, WorkerState},
        runner::{self, JobHandler, RunnerConfig},
    };
    struct ControlledHandler;
    impl JobHandler for ControlledHandler {
        async fn process(&self, _job: Job) -> forklaunch_core::domain::BlueprintResult<()> {
            tokio::time::sleep(Duration::from_millis(600)).await;
            Ok(())
        }
    }
    async fn wait<Q: AsyncWorkerQueue>(queue: &Q, id: &str, status: JobStatus) {
        tokio::time::timeout(Duration::from_secs(5), async {
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
        lease_duration: Duration::from_millis(300),
        renew_interval: Duration::from_millis(75),
        drain_timeout: Duration::from_secs(2),
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
            tokio::time::sleep(Duration::from_millis(350)).await;
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
            tokio::time::sleep(Duration::from_millis(if jobs[0].queue == "batch-drain" {
                700
            } else {
                400
            }))
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
            lease_duration: Duration::from_millis(300),
            renew_interval: Duration::from_millis(75),
            drain_timeout: Duration::from_secs(2),
        };
        let (report, ()) = tokio::time::timeout(Duration::from_secs(5), async {
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

#[tokio::test]
#[ignore = "requires isolated Redis and FORKLAUNCH_TEST_REDIS_URL"]
async fn redis_worker_distributed_control_shares_http_and_runner() {
    use forklaunch_worker_database::{
        control::{WorkerControl, WorkerState, forward_signals},
        http,
    };
    use forklaunch_worker_interface::DistributedSignal;
    use forklaunch_worker_redis::signals::{RedisSignalBus, RedisSignalPublisher, SignalOptions};
    use tower::ServiceExt;
    let url = std::env::var("FORKLAUNCH_TEST_REDIS_URL").unwrap();
    let namespace = format!("shared-control-{}", uuid::Uuid::new_v4());
    let tenant = TenantContext::new("tenant").unwrap();
    let queue = RedisLeasedQueue::connect(&url, &namespace, tenant.clone(), RetryPolicy::default())
        .await
        .unwrap();
    queue
        .enqueue(Job::new("controlled-job", "jobs", "{}"))
        .await
        .unwrap();
    let control = WorkerControl::new();
    assert!(control.pause());
    let running = spawn_shared_control_runner(queue.clone(), control.clone());
    let router = http::router(
        &control,
        &http::HttpConfig {
            address: "127.0.0.1:0".parse().unwrap(),
            control_token: None,
        },
    )
    .unwrap();
    let ready = || {
        router.clone().oneshot(
            axum::http::Request::builder()
                .uri("/ready")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        queue
            .inspect("jobs", "controlled-job")
            .await
            .unwrap()
            .unwrap()
            .status,
        JobStatus::Queued
    );
    assert_eq!(
        ready().await.unwrap().status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    let mut bus =
        RedisSignalBus::connect(&url, &namespace, &tenant, "jobs", SignalOptions::default())
            .await
            .unwrap();
    let publisher =
        RedisSignalPublisher::connect(&url, &namespace, &tenant, "jobs", &SignalOptions::default())
            .await
            .unwrap();
    let owned_control = control.clone();
    let forwarding = tokio::spawn(async move {
        forward_signals(&mut bus, &owned_control, std::future::pending()).await
    });
    assert_eq!(
        publisher
            .broadcast(DistributedSignal::Resume)
            .await
            .unwrap(),
        1
    );
    wait_for_controlled_completion(&queue).await;
    assert_eq!(ready().await.unwrap().status(), axum::http::StatusCode::OK);
    publisher.broadcast(DistributedSignal::Pause).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while control.state() != WorkerState::Paused {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        ready().await.unwrap().status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    publisher.broadcast(DistributedSignal::Drain).await.unwrap();
    let report = tokio::time::timeout(Duration::from_secs(3), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(report.worker.completed, 1);
    forwarding.await.unwrap().unwrap();
    assert_eq!(control.state(), WorkerState::Draining);
    let mut connection = redis::Client::open(url)
        .unwrap()
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    clean(&mut connection, &namespace).await;
}

fn spawn_shared_control_runner(
    queue: RedisLeasedQueue,
    control: forklaunch_worker_database::control::WorkerControl,
) -> tokio::task::JoinHandle<
    BlueprintResult<forklaunch_worker_database::retention::RetainedRunReport>,
> {
    use forklaunch_worker_database::http;
    struct Handler;
    impl JobHandler for Handler {
        async fn process(&self, _: Job) -> BlueprintResult<()> {
            Ok(())
        }
    }
    tokio::spawn(async move {
        let config = RunnerConfig {
            poll_interval: Duration::from_millis(10),
            ..RunnerConfig::new("jobs")
        };
        http::run_with_control(
            &queue,
            &Handler,
            &config,
            None,
            None,
            &control,
            std::future::pending(),
        )
        .await
    })
}
async fn wait_for_controlled_completion(queue: &RedisLeasedQueue) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if queue
                .inspect("jobs", "controlled-job")
                .await
                .unwrap()
                .unwrap()
                .status
                == JobStatus::Completed
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

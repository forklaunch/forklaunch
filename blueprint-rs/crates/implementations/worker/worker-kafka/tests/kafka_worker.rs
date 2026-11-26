#![cfg(feature = "kafka-client")]
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::TenantContext;
use forklaunch_worker_database::{control::WorkerControl, runner::JobHandler};
use forklaunch_worker_interface::{Job, RetryPolicy};
use forklaunch_worker_kafka::client::{
    KafkaInspector, KafkaOptions, KafkaProducer, KafkaRunnerConfig, KafkaWorker, initialize_topics,
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Notify;

fn options(queue: &str) -> KafkaOptions {
    let mut options = KafkaOptions::new(
        std::env::var("FORKLAUNCH_TEST_KAFKA_BROKERS").unwrap(),
        format!(
            "parity-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ),
        TenantContext::new("tenant-a").unwrap(),
        queue,
        "replica-a",
    );
    options.operation_timeout = Duration::from_secs(15);
    options
}
fn config() -> KafkaRunnerConfig {
    KafkaRunnerConfig {
        retry: RetryPolicy::new(2, 0),
        handler_timeout: Duration::from_secs(5),
        drain_timeout: Duration::from_millis(100),
    }
}
#[derive(Default)]
struct Handler {
    seen: Mutex<Vec<(String, u32)>>,
    entered: Notify,
    block: bool,
}
impl JobHandler for Handler {
    async fn process(&self, job: Job) -> BlueprintResult<()> {
        self.seen
            .lock()
            .unwrap()
            .push((job.id.clone(), job.attempts));
        self.entered.notify_one();
        if self.block {
            std::future::pending::<()>().await;
        }
        if job.id == "dead" || (job.id == "retry" && job.attempts == 1) {
            Err(BlueprintError::new(
                "test_handler_failure",
                "private failure details",
            ))
        } else {
            Ok(())
        }
    }
}
async fn wait_until(mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(45), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated Kafka broker and FORKLAUNCH_TEST_KAFKA_BROKERS"]
async fn kafka_transactions_retries_dead_letters_peek_and_group_offsets() {
    let options = options("jobs");
    initialize_topics(&options, 4, 1).await.unwrap();
    initialize_topics(&options, 4, 1).await.unwrap();
    let mut publisher_options = options.clone();
    publisher_options.transactional_id = "publisher".into();
    publisher_options.max_payload_bytes = 8192;
    let publisher = KafkaProducer::connect(publisher_options).await.unwrap();
    assert!(
        publisher
            .enqueue_batch(vec![
                Job::new("must-not-write", "jobs", "{}"),
                Job::new("wrong-queue", "other", "{}")
            ])
            .await
            .is_err()
    );
    let escaped = serde_json::to_string(&"\"".repeat(4000)).unwrap();
    assert!(
        publisher
            .enqueue(Job::new("oversized-wire", "jobs", escaped))
            .await
            .is_err()
    );
    let mut jobs: Vec<_> = (0..20)
        .map(|id| Job::new(format!("job-{id}"), "jobs", "{}"))
        .collect();
    jobs.extend([
        Job::new("retry", "jobs", "{}"),
        Job::new("dead", "jobs", "{}"),
    ]);
    assert_eq!(publisher.enqueue_batch(jobs).await.unwrap().len(), 22);
    let inspector = KafkaInspector::new(options.clone()).unwrap();
    let first = inspector.peek(100).await.unwrap();
    assert_eq!(first.len(), 22);
    assert_eq!(
        inspector.peek(100).await.unwrap().len(),
        22,
        "peek advanced the group offset"
    );
    let mut isolated = options.clone();
    isolated.tenant = TenantContext::new("tenant-b").unwrap();
    initialize_topics(&isolated, 1, 1).await.unwrap();
    assert!(
        KafkaInspector::new(isolated)
            .unwrap()
            .peek(100)
            .await
            .unwrap()
            .is_empty()
    );
    let worker = Arc::new(
        KafkaWorker::connect(options.clone(), &config())
            .await
            .unwrap(),
    );
    let handler = Arc::new(Handler::default());
    let control = WorkerControl::new();
    let running = {
        let worker = worker.clone();
        let handler = handler.clone();
        let control = control.clone();
        tokio::spawn(async move {
            worker
                .run(
                    handler.as_ref(),
                    &config(),
                    &control,
                    std::future::pending(),
                )
                .await
        })
    };
    wait_until(|| handler.seen.lock().unwrap().len() == 24).await;
    control.drain();
    let report = running.await.unwrap().unwrap();
    assert_eq!(report.completed, 21);
    assert_eq!(report.retried, 2);
    assert_eq!(report.dead_lettered, 1);
    assert!(inspector.peek(100).await.unwrap().is_empty());
    let dead = inspector.dead_letters(100).await.unwrap();
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].job.id, "dead");
    assert_eq!(dead[0].job.attempts, 2);
    assert_eq!(dead[0].error_code, "test_handler_failure");
    drop(worker);
    // A fresh replica sees committed group offsets, without local deduplication.
    let restarted = KafkaWorker::connect(options, &config()).await.unwrap();
    let fresh = Handler::default();
    let control = WorkerControl::new();
    let report = restarted
        .run(
            &fresh,
            &config(),
            &control,
            tokio::time::sleep(Duration::from_secs(2)),
        )
        .await
        .unwrap();
    assert_eq!(report.completed, 0);
    assert!(fresh.seen.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated Kafka broker and FORKLAUNCH_TEST_KAFKA_BROKERS"]
async fn kafka_pause_drain_cancellation_restart_and_replica_fencing() {
    let options = options("recovery");
    initialize_topics(&options, 1, 1).await.unwrap();
    let mut publisher_options = options.clone();
    publisher_options.transactional_id = "publisher".into();
    KafkaProducer::connect(publisher_options)
        .await
        .unwrap()
        .enqueue(Job::new("recover", "recovery", "{}"))
        .await
        .unwrap();
    let worker = Arc::new(
        KafkaWorker::connect(options.clone(), &config())
            .await
            .unwrap(),
    );
    let handler = Arc::new(Handler {
        block: true,
        ..Handler::default()
    });
    let control = WorkerControl::new();
    assert!(control.pause());
    let running = {
        let worker = worker.clone();
        let handler = handler.clone();
        let control = control.clone();
        tokio::spawn(async move {
            worker
                .run(
                    handler.as_ref(),
                    &config(),
                    &control,
                    std::future::pending(),
                )
                .await
        })
    };
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(handler.seen.lock().unwrap().is_empty());
    assert!(control.resume());
    tokio::time::timeout(Duration::from_secs(30), handler.entered.notified())
        .await
        .unwrap();
    control.drain();
    let report = running.await.unwrap().unwrap();
    assert_eq!(report.abandoned, 1);
    drop(worker);
    let worker = Arc::new(
        KafkaWorker::connect(options.clone(), &config())
            .await
            .unwrap(),
    );
    let handler = Arc::new(Handler::default());
    let control = WorkerControl::new();
    let running = {
        let worker = worker.clone();
        let handler = handler.clone();
        let control = control.clone();
        tokio::spawn(async move {
            worker
                .run(
                    handler.as_ref(),
                    &config(),
                    &control,
                    std::future::pending(),
                )
                .await
        })
    };
    wait_until(|| handler.seen.lock().unwrap().len() == 1).await;
    control.drain();
    assert_eq!(running.await.unwrap().unwrap().completed, 1);
    // A replacement producer with the same stable identity fences the old one.
    let old = KafkaProducer::connect(options.clone()).await.unwrap();
    let _replacement = KafkaProducer::connect(options).await.unwrap();
    assert!(
        old.enqueue(Job::new("fenced", "recovery", "{}"))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires isolated Kafka broker and FORKLAUNCH_TEST_KAFKA_BROKERS"]
async fn kafka_group_fanout_processes_each_committed_record_once() {
    let options = options("fanout");
    initialize_topics(&options, 4, 1).await.unwrap();
    let mut second_options = options.clone();
    second_options.transactional_id = "replica-b".into();
    let first = Arc::new(
        KafkaWorker::connect(options.clone(), &config())
            .await
            .unwrap(),
    );
    let second = Arc::new(
        KafkaWorker::connect(second_options, &config())
            .await
            .unwrap(),
    );
    let handler = Arc::new(Handler::default());
    let controls = [WorkerControl::new(), WorkerControl::new()];
    for control in &controls {
        assert!(control.pause());
    }
    let mut runs = Vec::new();
    for (worker, control) in [
        (first.clone(), controls[0].clone()),
        (second.clone(), controls[1].clone()),
    ] {
        let handler = handler.clone();
        runs.push(tokio::spawn(async move {
            worker
                .run(
                    handler.as_ref(),
                    &config(),
                    &control,
                    std::future::pending(),
                )
                .await
        }));
    }
    wait_until(|| {
        first.assigned_partitions().unwrap() == 2 && second.assigned_partitions().unwrap() == 2
    })
    .await;
    let mut publisher_options = options.clone();
    publisher_options.transactional_id = "publisher".into();
    let publisher = KafkaProducer::connect(publisher_options).await.unwrap();
    publisher
        .enqueue_batch(
            (0..40)
                .map(|id| Job::new(format!("job-{id}"), "fanout", "{}"))
                .collect(),
        )
        .await
        .unwrap();
    for control in &controls {
        assert!(control.resume());
    }
    wait_until(|| handler.seen.lock().unwrap().len() == 40).await;
    for control in &controls {
        control.drain();
    }
    let mut completed = 0;
    for run in runs {
        completed += run.await.unwrap().unwrap().completed;
    }
    assert_eq!(completed, 40);
    assert_eq!(
        handler
            .seen
            .lock()
            .unwrap()
            .iter()
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>()
            .len(),
        40
    );
    assert!(
        KafkaInspector::new(options)
            .unwrap()
            .peek(100)
            .await
            .unwrap()
            .is_empty()
    );
}

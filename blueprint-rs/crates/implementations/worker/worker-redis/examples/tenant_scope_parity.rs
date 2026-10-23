use forklaunch_core::domain::BlueprintResult;
use forklaunch_core::persistence::TenantContext;
use forklaunch_worker_database::runner::{self, JobHandler, RunnerConfig};
use forklaunch_worker_interface::{
    AsyncSignalBus, AsyncWorkerQueue, DistributedSignal, Job, RetryPolicy,
};
use forklaunch_worker_redis::{
    leased::RedisLeasedQueue,
    signals::{RedisSignalBus, SignalOptions},
    tenant_isolation::RedisKeyScope,
};
use serde_json::{Value, json};
use std::time::Duration;

struct UnusedHandler;
impl JobHandler for UnusedHandler {
    async fn process(&self, _: Job) -> BlueprintResult<()> {
        panic!("ACL-denied runner must never process a job")
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<Value> = serde_json::from_reader(std::io::stdin())?;
    let cases = cases
        .iter()
        .map(|input| {
            match RedisKeyScope::from_vars(|name| {
                input.get(name).and_then(Value::as_str).map(str::to_owned)
            }) {
                Ok(scope) => {
                    json!({"prefix":scope.prefix(),"key":scope.qualify("fl_worker:{slot}:jobs")})
                }
                Err(error) => json!({"error":error.code}),
            }
        })
        .collect::<Vec<_>>();
    backend().await?;
    println!("{}", json!({"cases":cases,"backend":true}));
    Ok(())
}

async fn backend() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("FORKLAUNCH_TEST_REDIS_URL")?;
    let namespace = format!("acl-{}", uuid::Uuid::new_v4());
    let tenant = TenantContext::new("tenant-a")?;
    let policy = RetryPolicy::new(2, 1);
    let queue = RedisLeasedQueue::connect(&url, &namespace, tenant.clone(), policy).await?;
    queue
        .enqueue_batch(vec![
            Job::new("first", "jobs", "{}"),
            Job::new("second", "jobs", "{}"),
        ])
        .await?;
    assert_eq!(queue.stats("jobs").await?.queued, 2);
    let leases = queue
        .claim_batch("jobs", Duration::from_secs(30), 2)
        .await?;
    assert_eq!(leases.len(), 2);
    for lease in &leases {
        assert!(queue.ack(lease).await?);
    }
    assert_eq!(queue.stats("jobs").await?.queued, 0);
    let other = queue.for_tenant(TenantContext::new("tenant-other")?);
    assert!(other.inspect("jobs", "first").await?.is_none());
    let mut signals =
        RedisSignalBus::connect(&url, &namespace, &tenant, "jobs", SignalOptions::default())
            .await?;
    assert_eq!(
        signals
            .publisher()
            .broadcast(DistributedSignal::Pause)
            .await?,
        1
    );
    assert_eq!(
        tokio::time::timeout(
            Duration::from_secs(2),
            signals.next_signal(Duration::from_secs(1))
        )
        .await??,
        Some(DistributedSignal::Pause)
    );
    let denied = RedisLeasedQueue::connect_scoped(
        &url,
        &namespace,
        tenant.clone(),
        policy,
        RedisKeyScope::default(),
    )
    .await?;
    let error = denied
        .enqueue(Job::new("denied", "jobs", "{}"))
        .await
        .unwrap_err();
    assert_eq!(error.code, "worker_tenant_isolation_error");
    assert!(!error.to_string().contains(&url));
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        runner::run(
            &denied,
            &UnusedHandler,
            &RunnerConfig::new("jobs"),
            std::future::pending(),
        ),
    )
    .await?
    .unwrap_err();
    assert_eq!(error.code, "worker_tenant_isolation_error");
    let denied_signal = RedisSignalBus::connect_scoped(
        &url,
        &namespace,
        &tenant,
        "jobs",
        SignalOptions::default(),
        &RedisKeyScope::default(),
    )
    .await;
    assert_eq!(
        denied_signal.unwrap_err().code,
        "worker_tenant_isolation_error"
    );
    Ok(())
}

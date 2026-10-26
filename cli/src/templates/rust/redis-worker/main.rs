mod signals;
use std::{env, time::Duration, error::Error};
use forklaunch_worker_database::http::HttpConfig;
use forklaunch_core::{config, persistence::TenantContext, telemetry::otlp::TelemetryRuntime};
use forklaunch_worker_database::runner::RunnerConfig;
use forklaunch_worker_redis::leased::RedisLeasedQueue;
use forklaunch_worker_database::retention::RetentionPolicy;
use forklaunch_worker_interface::{AsyncWorkerAdmin, AsyncWorkerQueue, Job, JobSnapshot, RetryPolicy, TerminalStatus};
use {{crate_module}}::{handler::ApplicationHandler, events, WORKER_NAME, write_contract_artifacts};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    if matches!(config::optional::<String>("FORKLAUNCH_MODE")?.as_deref(), Some("openapi" | "contract")) {
        write_contract_artifacts(
            config::required::<String>("FORKLAUNCH_CONTRACT_OUTPUT")?,
            config::required::<String>("FORKLAUNCH_OPENAPI_OUTPUT")?,
        )?;
        return Ok(());
    }
    let telemetry = TelemetryRuntime::start(
        WORKER_NAME, env!("CARGO_PKG_VERSION"),
    ).await?;
    let result = run().await;
    let flushed = telemetry.shutdown().await;
    result?;
    flushed?;
    Ok(())
}

async fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.is_empty() || args.first().map(String::as_str) == Some("--enqueue") { events::validate_configuration()?; }
    // Redis queues have no schema; `--migrate` is accepted so every worker binary shares one contract.
    if args == ["--migrate"] { return Ok(()); }
    if !args.is_empty() && args != ["--control-service"] && !(args.len() == 3 && args[0] == "--enqueue")
        && !(args.len() == 2 && matches!(args[0].as_str(), "--inspect" | "--dead-letters" | "--retry" | "--signal"))
        && !(args.len() == 3 && matches!(args[0].as_str(), "--purge-completed" | "--purge-failed")) {
        return Err("Expected --signal pause/resume/drain, no arguments, --control-service, --migrate, --enqueue ID JSON, --inspect ID, --dead-letters LIMIT, --retry ID, or --purge-completed/--purge-failed AGE_SECONDS LIMIT".into());
    }
    if args == ["--control-service"] {
        let http = http_config()?.ok_or("--control-service requires WORKER_HTTP_ADDR")?;
        signals::serve_management(&http, shutdown()).await?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--signal") {
        signals::publish(&args[1]).await?;
        return Ok(());
    }
    let url = config::required::<String>("REDIS_URL")?;
    events::transport_policy()?.redis(&url)?;
    let namespace = config::required::<String>("WORKER_NAMESPACE")?;
    let tenant = TenantContext::new(config::required::<String>("TENANT_ID")?)?;
    let policy = RetryPolicy::new(
        config::required("WORKER_MAX_ATTEMPTS")?,
        config::required("WORKER_BACKOFF_SECONDS")?,
    );
    let queue = RedisLeasedQueue::connect(&url, &namespace, tenant, policy).await?;
    let queue_name = config::required::<String>("WORKER_QUEUE")?;
    if args.first().map(String::as_str) == Some("--enqueue") {
        queue.enqueue(Job::new(&args[1], &queue_name, events::seal(&args[1], &args[2])?)).await?;
        println!("job enqueued");
    } else if args.first().map(String::as_str) == Some("--inspect") {
        let snapshot = queue.inspect(&queue_name, &args[1]).await?;
        println!("{}", snapshot.map_or(Value::Null, snapshot_json));
    } else if args.first().map(String::as_str) == Some("--dead-letters") {
        let jobs = queue.dead_letters(&queue_name, args[1].parse()?).await?;
        println!("{}", serde_json::to_string(&jobs.into_iter().map(snapshot_json).collect::<Vec<_>>())?);
    } else if args.first().map(String::as_str) == Some("--retry") {
        let requeued = queue.retry_dead_letter(&queue_name, &args[1], policy).await?;
        println!("{}", serde_json::json!({"requeued":requeued}));
    } else if args.first().is_some_and(|arg| matches!(arg.as_str(), "--purge-completed" | "--purge-failed")) {
        let status = if args[0] == "--purge-completed" { TerminalStatus::Completed } else { TerminalStatus::Failed };
        let purged = queue.purge_terminal(&queue_name, status, Duration::from_secs(args[1].parse()?), args[2].parse()?).await?;
        println!("{}", serde_json::json!({"purged":purged}));
    } else {
        let config = RunnerConfig {
            queue: queue_name,
            concurrency: config::required("WORKER_CONCURRENCY")?,
            batch_size: config::required("WORKER_BATCH_SIZE")?,
            poll_interval: Duration::from_millis(config::required("WORKER_POLL_MS")?),
            lease_duration: Duration::from_millis(config::required("WORKER_LEASE_MS")?),
            renew_interval: Duration::from_millis(config::required("WORKER_RENEW_MS")?),
            drain_timeout: Duration::from_millis(config::required("WORKER_DRAIN_MS")?),
        };
        config.validate()?;
        let retention = retention_policy()?;
        let http = http_config()?;
        println!("{} worker ready", WORKER_NAME);
        let result = signals::run(&queue, &ApplicationHandler, &config, retention.as_ref(), http.as_ref(), shutdown()).await?;
        if retention.is_some() {
            println!("retention stopped: completed_purged={} failed_purged={}", result.retention.completed_purged, result.retention.failed_purged);
        }
        let report = result.worker;
        println!("worker stopped: completed={} retried={} dead_lettered={} lease_lost={} abandoned={}",
            report.completed, report.retried, report.dead_lettered, report.lease_lost, report.abandoned);
    }
    Ok(())
}

fn snapshot_json(snapshot: JobSnapshot) -> Value {
    serde_json::json!({"id":snapshot.job.id, "queue":snapshot.job.queue,
        "attempts":snapshot.job.attempts, "status":format!("{:?}",snapshot.status)})
}

fn http_config() -> Result<Option<HttpConfig>, Box<dyn Error>> {
    let control_token: Option<String> = config::optional("WORKER_CONTROL_TOKEN")?;
    let Some(address) = config::optional("WORKER_HTTP_ADDR")? else {
        if control_token.is_some() { return Err("WORKER_CONTROL_TOKEN requires WORKER_HTTP_ADDR".into()); }
        return Ok(None);
    };
    let config = HttpConfig { address, control_token };
    config.validate()?;
    Ok(Some(config))
}

/// Retention is off unless an age is set; enabled retention requires its
/// interval, batch limit and timeout settings.
fn retention_policy() -> Result<Option<RetentionPolicy>, Box<dyn Error>> {
    let completed_age = config::optional("WORKER_COMPLETED_RETENTION_SECONDS")?.map(Duration::from_secs);
    let failed_age = config::optional("WORKER_FAILED_RETENTION_SECONDS")?.map(Duration::from_secs);
    if completed_age.is_none() && failed_age.is_none() {
        return Ok(None);
    }
    let policy = RetentionPolicy {
        completed_age,
        failed_age,
        interval: Duration::from_millis(config::required("WORKER_RETENTION_INTERVAL_MS")?),
        batch_limit: config::required("WORKER_RETENTION_BATCH_LIMIT")?,
        operation_timeout: Duration::from_millis(config::required("WORKER_RETENTION_TIMEOUT_MS")?),
    };
    policy.validate()?;
    Ok(Some(policy))
}

async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = signal.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

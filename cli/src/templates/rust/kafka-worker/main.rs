mod signals;
use std::{env, time::Duration, error::Error};
use forklaunch_core::{config, persistence::TenantContext, telemetry::otlp::TelemetryRuntime};
use forklaunch_worker_database::http::HttpConfig;
use forklaunch_worker_interface::{Job, RetryPolicy};
use forklaunch_worker_kafka::client::{KafkaInspector, KafkaOptions, KafkaProducer, KafkaRunnerConfig, KafkaWorker, initialize_topics};
use {{crate_module}}::{handler::ApplicationHandler, events, WORKER_NAME, write_contract_artifacts};

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
    if args == ["--migrate"] { return migrate().await; }
    if args.is_empty() || args.first().map(String::as_str) == Some("--enqueue") { events::validate_configuration()?; }
    if !args.is_empty() && args != ["--migrate"] && args != ["--control-service"]
        && !(args.len() == 3 && args[0] == "--enqueue")
        && !(args.len() == 2 && matches!(args[0].as_str(), "--peek" | "--dead-letters" | "--signal")) {
        return Err("Expected no arguments, --control-service, --migrate, --enqueue ID JSON, --peek LIMIT, --dead-letters LIMIT, or --signal pause/resume/drain".into());
    }
    if args == ["--control-service"] {
        let http = http_config()?.ok_or("--control-service requires WORKER_HTTP_ADDR")?;
        signals::serve_management(&http, shutdown()).await?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--signal") { signals::publish(&args[1]).await?; return Ok(()); }
    let mut options = KafkaOptions::new(
        config::required::<String>("KAFKA_BROKERS")?,
        config::required::<String>("WORKER_NAMESPACE")?,
        TenantContext::new(config::required::<String>("TENANT_ID")?)?,
        config::required::<String>("WORKER_QUEUE")?,
        if args.is_empty() { config::required::<String>("KAFKA_TRANSACTIONAL_ID")? }
        else if args[0] == "--enqueue" { config::required::<String>("KAFKA_PUBLISHER_ID")? }
        else { "inspection".to_owned() },
    );
    options.group = config::required("KAFKA_GROUP_ID")?;
    options.operation_timeout = Duration::from_millis(config::required("KAFKA_OPERATION_TIMEOUT_MS")?);
    options.session_timeout = Duration::from_millis(config::required("KAFKA_SESSION_TIMEOUT_MS")?);
    // Optional: how long startup retries a broker that is still starting or the first
    // transactional initialization on a fresh broker. Unset keeps the 60 s default.
    if let Some(value) = config::optional::<u64>("KAFKA_STARTUP_TIMEOUT_MS")? {
        options.startup_timeout = Duration::from_millis(value);
    }
    options.max_payload_bytes = config::required("WORKER_MAX_PAYLOAD_BYTES")?;
    // Optional librdkafka properties (for example SASL); unset uses none.
    if let Some(value) = config::optional::<String>("KAFKA_CLIENT_PROPERTIES")? {
        options.properties = serde_json::from_str(&value).map_err(|_| "KAFKA_CLIENT_PROPERTIES must be a JSON object of string properties")?;
    }
    events::transport_policy()?.kafka(&options.brokers, &options.properties)?;
    options.validate()?;
    match args.first().map(String::as_str) {
        Some("--enqueue") => {
            let queue = options.queue.clone();
            KafkaProducer::connect(options).await?.enqueue(Job::new(&args[1], queue, events::seal(&args[1], &args[2])?)).await?;
            println!("job enqueued");
        }
        Some("--peek") => {
            let jobs = KafkaInspector::new(options)?.peek(args[1].parse()?).await?;
            println!("{}", serde_json::to_string(&jobs.into_iter().map(|job| serde_json::json!({"id":job.id,"queue":job.queue,"attempts":job.attempts})).collect::<Vec<_>>())?);
        }
        Some("--dead-letters") => {
            let jobs = KafkaInspector::new(options)?.dead_letters(args[1].parse()?).await?;
            println!("{}", serde_json::to_string(&jobs.into_iter().map(|dead| serde_json::json!({"id":dead.job.id,"queue":dead.job.queue,"attempts":dead.job.attempts})).collect::<Vec<_>>())?);
        }
        None => {
            let config = KafkaRunnerConfig {
                retry: RetryPolicy::new(config::required("WORKER_MAX_ATTEMPTS")?, config::required("WORKER_BACKOFF_SECONDS")?),
                handler_timeout: Duration::from_millis(config::required("WORKER_HANDLER_TIMEOUT_MS")?),
                drain_timeout: Duration::from_millis(config::required("WORKER_DRAIN_MS")?),
            };
            config.validate()?;
            let http = http_config()?;
            let worker = KafkaWorker::connect(options, &config).await?;
            println!("{} worker ready", WORKER_NAME);
            let report = signals::run(&worker, &ApplicationHandler, &config, http.as_ref(), shutdown()).await?;
            println!("worker stopped: completed={} retried={} dead_lettered={} abandoned={}", report.completed, report.retried, report.dead_lettered, report.abandoned);
        }
        _ => unreachable!("arguments validated"),
    }
    Ok(())
}

/// Create the queue topics. Needs only the broker, queue identity and topic
/// shape, not the runner settings a consumer requires.
async fn migrate() -> Result<(), Box<dyn Error>> {
    let mut options = KafkaOptions::new(
        config::required::<String>("KAFKA_BROKERS")?,
        config::required::<String>("WORKER_NAMESPACE")?,
        TenantContext::new(config::required::<String>("TENANT_ID")?)?,
        config::required::<String>("WORKER_QUEUE")?,
        "migration".to_owned(),
    );
    if let Some(timeout) = config::optional::<u64>("KAFKA_OPERATION_TIMEOUT_MS")? { options.operation_timeout = Duration::from_millis(timeout); }
    if let Some(value) = config::optional::<String>("KAFKA_CLIENT_PROPERTIES")? {
        options.properties = serde_json::from_str(&value).map_err(|_| "KAFKA_CLIENT_PROPERTIES must be a JSON object of string properties")?;
    }
    events::transport_policy()?.kafka(&options.brokers, &options.properties)?;
    options.validate()?;
    initialize_topics(&options, config::required("KAFKA_PARTITIONS")?, config::required("KAFKA_REPLICATION_FACTOR")?).await?;
    Ok(())
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

async fn shutdown() {
    #[cfg(unix)] {
        if let Ok(mut signal) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = signal.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

// Optional scoped Pub/Sub control. Disconnects drain; restarts begin running.
use std::{future::{self, Future}, sync::Arc, error};
use forklaunch_core::{config, persistence::TenantContext};
use forklaunch_worker_database::{
    control::{WorkerControl, forward_signals},
    http::{self, HttpConfig, SignalPublisher},
    runner::JobHandler,
};
use forklaunch_worker_interface::{DistributedSignal};
use forklaunch_worker_redis::signals::{RedisSignalBus, RedisSignalPublisher, SignalOptions};
use crate::events;
use forklaunch_worker_kafka::client::{KafkaRunReport, KafkaRunnerConfig, KafkaWorker};

type Error = Box<dyn error::Error>;

fn configuration() -> Result<Option<(String, String, TenantContext, String)>, Error> {
    let Some(url) = config::optional::<String>("WORKER_SIGNAL_REDIS_URL")? else {
        return Ok(None);
    };
    events::transport_policy()?.redis(&url)?;
    let namespace = config::required("WORKER_SIGNAL_NAMESPACE")?;
    let tenant = TenantContext::new(config::required::<String>("TENANT_ID")?)?;
    let queue = config::required("WORKER_QUEUE")?;
    Ok(Some((url, namespace, tenant, queue)))
}

pub async fn publish(action: &str) -> Result<(), Error> {
    let signal = match action {
        "pause" => DistributedSignal::Pause,
        "resume" => DistributedSignal::Resume,
        "drain" => DistributedSignal::Drain,
        _ => return Err("Expected --signal pause, resume, or drain".into()),
    };
    let (url, namespace, tenant, queue) = configuration()?
        .ok_or("--signal requires WORKER_SIGNAL_REDIS_URL and WORKER_SIGNAL_NAMESPACE")?;
    let publisher = RedisSignalPublisher::connect(&url, &namespace, &tenant, &queue, &SignalOptions::default()).await?;
    let subscribers = publisher.broadcast(signal).await?;
    println!("{}", serde_json::json!({"subscribers": subscribers}));
    Ok(())
}

pub async fn run<H, F>(worker: &KafkaWorker, handler: &H, config: &KafkaRunnerConfig, http_config: Option<&HttpConfig>, shutdown: F) -> Result<KafkaRunReport, Error>
where H: JobHandler, F: Future<Output = ()> + Send {
    let mut bus = if let Some((url, namespace, tenant, queue)) = configuration()? {
        Some(RedisSignalBus::connect(&url, &namespace, &tenant, &queue, SignalOptions::default()).await?)
    } else { None };
    let publish: Option<SignalPublisher> = bus.as_ref().map(|bus| {
        let publisher = bus.publisher();
        let callback: SignalPublisher = Arc::new(move |signal| {
            let publisher = publisher.clone();
            Box::pin(async move { publisher.broadcast(signal).await })
        });
        callback
    });
    let control = WorkerControl::new();
    let consuming = worker.run(handler, config, &control, future::pending());
    let work = http::serve_controlled(&control, http_config, publish, consuming, shutdown);
    let forwarding = async {
        if let Some(bus) = bus.as_mut() { forward_signals(bus, &control, future::pending()).await }
        else { future::pending().await }
    };
    tokio::pin!(work, forwarding);
    tokio::select! {
        result = &mut work => Ok(result?),
        result = &mut forwarding => {
            let work_result = work.await;
            result?;
            Ok(work_result?)
        }
    }
}

// This role never connects to the job backend or subscribes as a consumer.
pub async fn serve_management<F>(config: &HttpConfig, shutdown: F) -> Result<(), Error>
where F: Future<Output = ()> + Send {
    let publisher: Option<SignalPublisher> = if let Some((url, namespace, tenant, queue)) = configuration()? {
        let publisher = RedisSignalPublisher::connect(&url, &namespace, &tenant, &queue, &SignalOptions::default()).await?;
        Some(Arc::new(move |signal| {
            let publisher = publisher.clone();
            Box::pin(async move { publisher.broadcast(signal).await })
        }))
    } else { None };
    http::serve_management(config, publisher, shutdown).await?;
    Ok(())
}

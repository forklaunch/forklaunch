//! Application-owned native backend, migration and handler assembly.
use apalis::prelude::*;
use apalis_postgres::{Config, PostgresStorage};
use std::time::Duration;
use forklaunch_worker_interface::native::{RuntimeError, SupervisorOptions};
pub async fn migrate() -> Result<(), RuntimeError> {
    let pool=crate::registrations::pool(true).await?;
    let result=crate::migrations::run(&pool).await;
    pool.close().await;
    result
}
pub async fn run() -> Result<(), RuntimeError> {
    let pool=crate::registrations::pool(false).await?;
    // Shared job resources (pool, SDK clients, keys) are built once and injected per job.
    let state=crate::domain::services::handler::JobState::new(pool.clone()).await?;
    let backend=PostgresStorage::<forklaunch_worker_interface::SealedEvent>::new(&pool).with_config(Config::default()
        .queue(env!("CARGO_PKG_NAME")).batch_size(4).persist_results(true)
        .heartbeat_interval(Duration::from_secs(5)).missed_heartbeats(6));
    let context=WorkerContext::new(&format!("{}-{}",env!("CARGO_PKG_NAME"),uuid::Uuid::new_v4()));
    let worker=WorkerBuilder::new(&context).backend(backend).data(state)
        .retry(forklaunch_worker_interface::apalis::retry_policy(forklaunch_worker_interface::RetrySettings::default())?)
        .build(crate::domain::services::handler::process_job);
    forklaunch_worker_interface::native::supervise(worker.run(),forklaunch_worker_interface::native::shutdown_signal(),
        ||context.stop().map_err(|_|()), ||async{Ok::<_,()>(())}, ||async{Ok::<_,()>(())},
        SupervisorOptions::new(Duration::from_millis(500),Duration::from_secs(60),Duration::from_secs(10),Duration::from_secs(20))?)
        .await?;
    Ok(())
}

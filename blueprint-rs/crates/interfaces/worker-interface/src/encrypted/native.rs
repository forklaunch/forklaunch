//! Shared native worker entrypoint and bounded backend supervision.
use std::{future::Future, time::Duration};
pub type RuntimeError = Box<dyn std::error::Error>;

/// Application-owned database registration, migrations and typed backend assembly.
/// The framework owns telemetry lifetime and command admission.
#[allow(async_fn_in_trait)]
pub trait WorkerApplication {
    async fn migrate(&self) -> Result<(), RuntimeError>;
    async fn run(&self) -> Result<(), RuntimeError>;
}

/// # Errors
/// Rejects unknown commands, telemetry failures and application errors. Flushes
/// telemetry after successful startup even when application work fails.
pub async fn run_application(
    name: &str,
    version: &str,
    application: &impl WorkerApplication,
) -> Result<(), RuntimeError> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let migrate = match args.as_slice() {
        [] => false,
        [argument] if matches!(argument.as_str(), "--migrate" | "--migrate-only") => true,
        _ => return Err("Expected no arguments, --migrate or --migrate-only".into()),
    };
    let telemetry =
        forklaunch_core::telemetry::otlp::TelemetryRuntime::start(name, version).await?;
    let result = if migrate {
        application.migrate().await
    } else {
        application.run().await
    };
    let flushed = telemetry.shutdown().await;
    result?;
    flushed?;
    Ok(())
}

#[derive(Clone, Copy)]
pub struct SupervisorOptions {
    dispatch: Duration,
    maintenance: Duration,
    operation: Duration,
    drain: Duration,
}
impl SupervisorOptions {
    /// # Errors
    /// Rejects zero intervals or operation/drain bounds exceeding five minutes.
    pub fn new(
        dispatch: Duration,
        maintenance: Duration,
        operation: Duration,
        drain: Duration,
    ) -> Result<Self, crate::encrypted::ProcessorError> {
        if [dispatch, maintenance, operation, drain]
            .iter()
            .any(Duration::is_zero)
            || operation > Duration::from_secs(300)
            || drain > Duration::from_secs(300)
        {
            return Err(crate::encrypted::ProcessorError::Configuration);
        }
        Ok(Self {
            dispatch,
            maintenance,
            operation,
            drain,
        })
    }
}

/// Drive a native backend without tying its payload or lease implementation to
/// the framework. Dispatch failure also stops admission and drains active jobs.
/// Maintenance is best-effort and bounded; applications retain retry/idempotency.
/// # Errors
/// Returns only fixed framework errors on backend, dispatch, stop or drain failure.
pub async fn supervise<W, Shutdown, Stop, Dispatch, Maintenance, D, M, WE, SE, DE, ME>(
    worker: W,
    shutdown: Shutdown,
    stop: Stop,
    mut dispatch: Dispatch,
    mut maintenance: Maintenance,
    options: SupervisorOptions,
) -> Result<(), crate::encrypted::ProcessorError>
where
    W: Future<Output = Result<(), WE>>,
    Shutdown: Future<Output = ()>,
    Stop: FnOnce() -> Result<(), SE>,
    Dispatch: FnMut() -> D,
    Maintenance: FnMut() -> M,
    D: Future<Output = Result<(), DE>>,
    M: Future<Output = Result<(), ME>>,
{
    tokio::pin!(worker, shutdown);
    let mut dispatch_tick = tokio::time::interval(options.dispatch);
    let mut maintenance_tick = tokio::time::interval(options.maintenance);
    dispatch_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    maintenance_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dispatch_work: Option<std::pin::Pin<Box<tokio::time::Timeout<D>>>> = None;
    let mut maintenance_work: Option<std::pin::Pin<Box<tokio::time::Timeout<M>>>> = None;
    let failed = loop {
        tokio::select! { biased;
            ()=&mut shutdown => break false,
            result=&mut worker => return result.map_err(|_|crate::encrypted::ProcessorError::Failed),
            result=async {
                if let Some(operation)=dispatch_work.as_mut() { operation.await }
                else { std::future::pending().await }
            } => {
                dispatch_work=None;
                if !matches!(result,Ok(Ok(()))) {break true;}
            },
            _=async {
                if let Some(operation)=maintenance_work.as_mut() { operation.await }
                else { std::future::pending().await }
            } => {maintenance_work=None;},
            _=dispatch_tick.tick(), if dispatch_work.is_none()=> {
                dispatch_work=Some(Box::pin(tokio::time::timeout(options.operation,dispatch())));
            },
            _=maintenance_tick.tick(), if maintenance_work.is_none()=> {
                maintenance_work=Some(Box::pin(tokio::time::timeout(options.operation,maintenance())));
            }
        }
    };
    // Cancel background operations before draining; they must not hold resources
    // needed by already admitted jobs. No untracked tasks survive this supervisor.
    drop(dispatch_work);
    drop(maintenance_work);
    stop().map_err(|_| crate::encrypted::ProcessorError::Failed)?;
    tokio::time::timeout(options.drain, &mut worker)
        .await
        .map_err(|_| crate::encrypted::ProcessorError::Timeout)?
        .map_err(|_| crate::encrypted::ProcessorError::Failed)?;
    if failed {
        Err(crate::encrypted::ProcessorError::Failed)
    } else {
        Ok(())
    }
}

/// SIGINT and SIGTERM share the same drain path.
pub async fn shutdown_signal() {
    #[cfg(unix)]
    if let Ok(mut signal) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    {
        tokio::select! {_=signal.recv()=>{},_=tokio::signal::ctrl_c()=>{}}
        return;
    }
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    fn options() -> SupervisorOptions {
        SupervisorOptions::new(
            Duration::from_millis(10),
            Duration::from_secs(1),
            Duration::from_millis(20),
            Duration::from_millis(30),
        )
        .unwrap()
    }
    #[tokio::test(start_paused = true)]
    async fn shutdown_drains_and_failed_dispatch_also_stops_admission() {
        for failure in [false, true] {
            let stopped = Arc::new(AtomicBool::new(false));
            let running = stopped.clone();
            let stopping = stopped.clone();
            let worker = async move {
                while !running.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                Ok::<_, ()>(())
            };
            let shutdown = async move {
                if failure {
                    std::future::pending::<()>().await;
                }
            };
            let result = supervise(
                worker,
                shutdown,
                move || {
                    stopping.store(true, Ordering::SeqCst);
                    Ok::<_, ()>(())
                },
                move || async move { if failure { Err(()) } else { Ok::<_, ()>(()) } },
                || async { Ok::<_, ()>(()) },
                options(),
            )
            .await;
            assert!(stopped.load(Ordering::SeqCst));
            assert_eq!(result.is_err(), failure);
        }
    }
    #[tokio::test(start_paused = true)]
    async fn stalled_backend_drain_is_bounded() {
        let result = supervise(
            std::future::pending::<Result<(), ()>>(),
            async {},
            || Ok::<_, ()>(()),
            || async { Ok::<_, ()>(()) },
            || async { Ok::<_, ()>(()) },
            options(),
        )
        .await;
        assert!(matches!(
            result,
            Err(crate::encrypted::ProcessorError::Timeout)
        ));
    }
    #[tokio::test(start_paused = true)]
    async fn stalled_background_work_does_not_block_shutdown_or_backend_progress() {
        use std::sync::atomic::AtomicUsize;
        for stalled_dispatch in [false, true] {
            let stopped = Arc::new(AtomicBool::new(false));
            let progress = Arc::new(AtomicUsize::new(0));
            let running = stopped.clone();
            let stopping = stopped.clone();
            let count = progress.clone();
            let worker = async move {
                while !running.load(Ordering::SeqCst) {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    count.fetch_add(1, Ordering::SeqCst);
                }
                Ok::<_, ()>(())
            };
            let start = tokio::time::Instant::now();
            supervise(
                worker,
                tokio::time::sleep(Duration::from_millis(5)),
                move || {
                    stopping.store(true, Ordering::SeqCst);
                    Ok::<_, ()>(())
                },
                move || async move {
                    if stalled_dispatch {
                        std::future::pending::<()>().await;
                    }
                    Ok::<_, ()>(())
                },
                std::future::pending::<Result<(), ()>>,
                options(),
            )
            .await
            .unwrap();
            assert!(progress.load(Ordering::SeqCst) >= 4);
            assert!(start.elapsed() < Duration::from_millis(10));
        }
    }
}

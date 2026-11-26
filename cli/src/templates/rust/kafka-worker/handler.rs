use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_worker_database::runner::JobHandler;
use forklaunch_worker_interface::Job;

pub struct ApplicationHandler;

impl JobHandler for ApplicationHandler {
    async fn process(&self, _job: Job) -> BlueprintResult<()> {
        // Install authorization and an acknowledged audit sink before decrypting.
        // Implement idempotent application work here. Return Ok(()) only after
        // that work succeeds. The runner commits the Kafka offset after successful processing.
        Err(BlueprintError::new(
            "worker_handler_not_implemented",
            "Implement handler::ApplicationHandler before consuming jobs",
        ))
    }
}

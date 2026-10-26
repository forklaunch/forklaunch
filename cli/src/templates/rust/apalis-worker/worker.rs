//! Generated runtime delegation. Application backend assembly belongs in worker_hooks.
use forklaunch_worker_interface::native::RuntimeError;

pub async fn run() -> Result<(), RuntimeError> {
    crate::worker_hooks::run().await
}

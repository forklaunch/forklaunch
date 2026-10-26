use forklaunch_worker_interface::native::RuntimeError;

#[tokio::main]
async fn main() -> Result<(), RuntimeError> {
    {{crate_module}}::bootstrapper::run().await
}

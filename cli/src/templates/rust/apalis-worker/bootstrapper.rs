use forklaunch_worker_interface::native::{RuntimeError, WorkerApplication};
struct Application;
impl WorkerApplication for Application {
    async fn migrate(&self) -> Result<(), RuntimeError> { crate::worker_hooks::migrate().await }
    async fn run(&self) -> Result<(), RuntimeError> { crate::worker::run().await }
}
pub async fn run() -> Result<(), RuntimeError> {
    // Migrations need only the owner database URL, not runtime telemetry settings.
    if std::env::args().skip(1).eq(["--migrate"]) { return crate::worker_hooks::migrate().await; }
    forklaunch_worker_interface::native::run_application(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), &Application).await
}

pub mod health;
use axum::Router;
use forklaunch_axum::native::ContractRouter;
use crate::registrations::AppState;
pub fn router() -> Result<Router<AppState>, String> {
    health::mount(ContractRouter::new(contracts::registry()))?.finish()
}

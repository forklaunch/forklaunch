use crate::{api::controllers::health::health, registrations::AppState};
use forklaunch_axum::native::ContractRouter;
pub fn mount(router: ContractRouter<AppState>) -> Result<ContractRouter<AppState>, String> {
    contracts::health::Bindings {
        health: contracts::health::Health::bind(health),
    }
    .mount(router)
}

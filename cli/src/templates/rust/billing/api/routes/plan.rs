//! Plans routes; the billing
//! interface declares each operation's contract.
use crate::{api::controllers::plan, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createPlan", plan::create_plan(state))
        .route("listPlans", plan::list_plans(state))
        .route("getPlan", plan::get_plan(state))
        .route("updatePlan", plan::update_plan(state))
        .route("deletePlan", plan::delete_plan(state))
}

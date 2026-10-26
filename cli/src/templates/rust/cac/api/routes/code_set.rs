//! Code-set description and procedure-code lookup routes; the CAC
//! interface declares each operation's contract.
use crate::{api::controllers::code_set, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("describeCodeSet", code_set::describe_code_set(state))
        .route("lookupProcedureCode", code_set::lookup_procedure_code(state))
}

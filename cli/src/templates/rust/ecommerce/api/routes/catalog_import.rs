//! Catalog imports by external ID routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::catalog_import, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("importCatalog", catalog_import::import_catalog(state))
}

//! Inventory and stock routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::inventory, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createInventory", inventory::create_inventory(state))
        .route("getInventory", inventory::get_inventory(state))
        .route("adjustStock", inventory::adjust_stock(state))
        .route("checkStock", inventory::check_stock(state))
}

//! Product variants routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::variant, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createVariant", variant::create_variant(state))
        .route("listVariants", variant::list_variants(state))
        .route("listVariantsByProduct", variant::list_variants_by_product(state))
        .route("getVariant", variant::get_variant(state))
        .route("updateVariant", variant::update_variant(state))
        .route("deleteVariant", variant::delete_variant(state))
}

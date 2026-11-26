//! Products and the public catalog routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::product, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createProduct", product::create_product(state))
        .route("listProducts", product::list_products(state))
        .route("listCatalog", product::list_catalog(state))
        .route("getProductByHandle", product::get_product_by_handle(state))
        .route("getProduct", product::get_product(state))
        .route("updateProduct", product::update_product(state))
        .route("deleteProduct", product::delete_product(state))
}

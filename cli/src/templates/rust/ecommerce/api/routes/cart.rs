//! Carts routes; the ecommerce
//! interface declares each operation's contract.
use crate::{api::controllers::cart, registrations::{AppState, Routes}};

pub fn mount(routes: Routes, state: &AppState) -> Routes {
    routes
        .route("createCart", cart::create_cart(state))
        .route("getCart", cart::get_cart(state))
        .route("addCartItem", cart::add_cart_item(state))
        .route("removeCartItem", cart::remove_cart_item(state))
        .route("clearCart", cart::clear_cart(state))
}

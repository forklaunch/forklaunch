//! Products and the public catalog.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, RawQuery, State},
    routing::any,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_ecommerce_interface::{
    CreateProductDto, ProductDto, ProductService, UpdateProductDto, catalog::CatalogPage,
};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Failure, Handler, JsonBody, query};

/// Creates a product.
pub fn create_product(state: &AppState) -> Handler {
    any(create).with_state(state.ecommerce.clone())
}

async fn create(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<CreateProductDto>,
) -> Result<Json<ProductDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.create_product(input).await?))
}

/// Searches products by ID, title, price, stock and option.
pub fn list_products(state: &AppState) -> Handler {
    any(search).with_state(state.ecommerce.clone())
}

async fn search(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    RawQuery(raw): RawQuery,
) -> Result<Json<Vec<ProductDto>>, Failure> {
    let filters = query::search(raw.as_deref())?;
    Ok(Json(ecommerce.service(&session)?.search_products(filters).await?))
}

/// Pages the catalog with each product's variants and stock.
pub fn list_catalog(state: &AppState) -> Handler {
    any(catalog).with_state(state.ecommerce.clone())
}

async fn catalog(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    RawQuery(raw): RawQuery,
) -> Result<Json<CatalogPage>, Failure> {
    let page = query::catalog(raw.as_deref())?;
    Ok(Json(ecommerce.service(&session)?.list_catalog(page).await?))
}

/// Reads a product by its handle.
pub fn get_product_by_handle(state: &AppState) -> Handler {
    any(by_handle).with_state(state.ecommerce.clone())
}

async fn by_handle(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(handle): Path<String>,
) -> Result<Json<ProductDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.get_product_by_handle(&handle).await?))
}

/// Reads a product.
pub fn get_product(state: &AppState) -> Handler {
    any(get).with_state(state.ecommerce.clone())
}

async fn get(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Result<Json<ProductDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.get_product(&id).await?))
}

/// Updates a product.
pub fn update_product(state: &AppState) -> Handler {
    any(update).with_state(state.ecommerce.clone())
}

async fn update(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<UpdateProductDto>,
) -> Result<Json<ProductDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.update_product(input).await?))
}

/// Deletes a product.
pub fn delete_product(state: &AppState) -> Handler {
    any(delete).with_state(state.ecommerce.clone())
}

async fn delete(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Result<&'static str, Failure> {
    ecommerce.service(&session)?.delete_product(&id).await?;
    Ok("Deleted product")
}

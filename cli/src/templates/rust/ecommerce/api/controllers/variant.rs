//! Product variants.
use crate::registrations::AppState;
use axum::{
    Extension, Json,
    extract::{Path, RawQuery, State},
    routing::any,
};
use forklaunch_axum::session::AuthenticatedSession;
use forklaunch_ecommerce_interface::{CreateVariantDto, UpdateVariantDto, VariantDto, VariantService};
use forklaunch_ecommerce_stripe::http::{EcommerceHttp, Failure, Handler, JsonBody, query};

/// Creates a variant.
pub fn create_variant(state: &AppState) -> Handler {
    any(create).with_state(state.ecommerce.clone())
}

async fn create(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<CreateVariantDto>,
) -> Result<Json<VariantDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.create_variant(input).await?))
}

/// Lists variants, optionally by a JSON `ids` array.
pub fn list_variants(state: &AppState) -> Handler {
    any(list).with_state(state.ecommerce.clone())
}

async fn list(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    RawQuery(raw): RawQuery,
) -> Result<Json<Vec<VariantDto>>, Failure> {
    let ids = query::ids(raw.as_deref())?;
    Ok(Json(ecommerce.service(&session)?.list_variants(ids).await?))
}

/// Lists a product's variants.
pub fn list_variants_by_product(state: &AppState) -> Handler {
    any(by_product).with_state(state.ecommerce.clone())
}

async fn by_product(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(product_id): Path<String>,
) -> Result<Json<Vec<VariantDto>>, Failure> {
    Ok(Json(ecommerce.service(&session)?.list_variants_by_product(&product_id).await?))
}

/// Reads a variant.
pub fn get_variant(state: &AppState) -> Handler {
    any(get).with_state(state.ecommerce.clone())
}

async fn get(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Result<Json<VariantDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.get_variant(&id).await?))
}

/// Updates a variant.
pub fn update_variant(state: &AppState) -> Handler {
    any(update).with_state(state.ecommerce.clone())
}

async fn update(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    JsonBody(input): JsonBody<UpdateVariantDto>,
) -> Result<Json<VariantDto>, Failure> {
    Ok(Json(ecommerce.service(&session)?.update_variant(input).await?))
}

/// Deletes a variant.
pub fn delete_variant(state: &AppState) -> Handler {
    any(delete).with_state(state.ecommerce.clone())
}

async fn delete(
    State(ecommerce): State<EcommerceHttp>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<String>,
) -> Result<&'static str, Failure> {
    ecommerce.service(&session)?.delete_variant(&id).await?;
    Ok("Deleted variant")
}

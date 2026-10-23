//! Sequential, external-ID-based imports preserving live stock on every re-import.
use crate::{BaseEcommerceService, store::EcommerceStore};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{
    CreateInventoryDto, CreateProductDto, CreateVariantDto, InventoryService, ProductService,
    UpdateProductDto, UpdateVariantDto, VariantService,
    import::{CatalogImport, CatalogImportResult},
};
impl<S: EcommerceStore> BaseEcommerceService<S> {
    /// Imports normalized products/variants and seeds missing inventory only.
    /// # Errors
    /// Propagates validation, uniqueness and storage errors; prior successful rows remain imported.
    pub async fn import_catalog(
        &self,
        input: CatalogImport,
    ) -> BlueprintResult<CatalogImportResult> {
        let mut result = CatalogImportResult {
            products_imported: 0,
            variants_imported: 0,
        };
        for product in input.products {
            let imported = match self.get_product_by_external_id(&product.external_id).await {
                Ok(existing) => {
                    self.update_product(UpdateProductDto {
                        id: existing.id,
                        external_id: Some(product.external_id.clone()),
                        handle: Some(product.handle.clone()),
                        source_url: product.source_url.clone(),
                        title: Some(product.title.clone()),
                        description_html: product.description_html.clone(),
                        vendor: product.vendor.clone(),
                        product_type: product.product_type.clone(),
                        tags: product.tags.clone(),
                        options: product.options.clone(),
                        images: product.images.clone(),
                    })
                    .await?
                }
                Err(error) if error.code == "ecommerce_not_found" => {
                    self.create_product(CreateProductDto {
                        id: None,
                        external_id: product.external_id.clone(),
                        handle: product.handle.clone(),
                        source_url: product.source_url.clone(),
                        title: product.title.clone(),
                        description_html: product.description_html.clone(),
                        vendor: product.vendor.clone(),
                        product_type: product.product_type.clone(),
                        tags: product.tags.clone(),
                        options: product.options.clone(),
                        images: product.images.clone(),
                    })
                    .await?
                }
                Err(error) => return Err(error),
            };
            result.products_imported += 1;
            for variant in product.variants {
                let imported_variant =
                    match self.get_variant_by_external_id(&variant.external_id).await {
                        Ok(existing) => {
                            self.update_variant(UpdateVariantDto {
                                id: existing.id,
                                product_id: Some(imported.id.clone()),
                                external_id: Some(variant.external_id.clone()),
                                sku: variant.sku.clone(),
                                title: Some(variant.title.clone()),
                                option_values: variant.option_values.clone(),
                                price_cents: Some(variant.price_cents),
                                compare_at_price_cents: variant.compare_at_price_cents,
                                requires_shipping: variant.requires_shipping,
                            })
                            .await?
                        }
                        Err(error) if error.code == "ecommerce_not_found" => {
                            self.create_variant(CreateVariantDto {
                                id: None,
                                product_id: imported.id.clone(),
                                external_id: variant.external_id.clone(),
                                sku: variant.sku.clone(),
                                title: variant.title.clone(),
                                option_values: variant.option_values.clone(),
                                price_cents: variant.price_cents,
                                compare_at_price_cents: variant.compare_at_price_cents,
                                requires_shipping: variant.requires_shipping,
                            })
                            .await?
                        }
                        Err(error) => return Err(error),
                    };
                match self.get_inventory(&imported_variant.id).await {
                    Ok(_) => {}
                    Err(error) if error.code == "ecommerce_not_found" => {
                        self.create_inventory(CreateInventoryDto {
                            id: None,
                            variant_id: imported_variant.id,
                            stock: variant.initial_stock.unwrap_or(0),
                        })
                        .await?;
                    }
                    Err(error) => return Err(error),
                }
                result.variants_imported += 1;
            }
        }
        Ok(result)
    }
}

//! TypeScript storefront controller behavior shared by HTTP and MCP adapters.
use crate::{
    BaseEcommerceService,
    store::{EcommerceStore, Field, Query, RecordKind},
};
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{
    InventoryDto, ProductDto, ProductSearchDto, ProductService, VariantDto, VariantService,
    catalog::{CatalogPage, CatalogProduct, CatalogQuery, CatalogVariant},
};
use std::collections::{BTreeMap, BTreeSet};
impl<S: EcommerceStore> BaseEcommerceService<S> {
    /// Applies variant-level filters before querying products. An explicit empty
    /// result must not turn into the base service's unfiltered `ids: []` query.
    /// # Errors
    /// Propagates storage failures. Missing inventory is out of stock.
    pub async fn search_products(
        &self,
        search: ProductSearchDto,
    ) -> BlueprintResult<Vec<ProductDto>> {
        let has_variant_filter = search.min_price_cents.is_some()
            || search.max_price_cents.is_some()
            || search.in_stock.is_some()
            || (search.option_name.is_some() && search.option_value.is_some());
        let mut ids = search.ids;
        if has_variant_filter {
            let variants = self.list_variants(None).await?;
            let stock = if search.in_stock.is_some() {
                self.stock_by_variant_ids(variants.iter().map(|v| v.id.clone()).collect())
                    .await?
            } else {
                BTreeMap::new()
            };
            let candidates: BTreeSet<_> = variants
                .into_iter()
                .filter(|variant| {
                    search
                        .min_price_cents
                        .is_none_or(|min| variant.price_cents >= min)
                        && search
                            .max_price_cents
                            .is_none_or(|max| variant.price_cents <= max)
                        && search
                            .option_name
                            .as_ref()
                            .zip(search.option_value.as_ref())
                            .is_none_or(|(name, value)| {
                                variant
                                    .option_values
                                    .as_ref()
                                    .and_then(|values| values.get(name))
                                    == Some(value)
                            })
                        && search.in_stock.is_none_or(|expected| {
                            (stock.get(&variant.id).copied().unwrap_or(0) > 0) == expected
                        })
                })
                .map(|variant| variant.product_id)
                .collect();
            ids = Some(match ids.filter(|values| !values.is_empty()) {
                Some(values) => values
                    .into_iter()
                    .filter(|id| candidates.contains(id))
                    .collect(),
                None => candidates.into_iter().collect(),
            });
        }
        if ids.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Vec::new());
        }
        self.list_products(Some(ProductSearchDto {
            ids,
            title: search.title,
            ..Default::default()
        }))
        .await
    }
    /// Pages in the storage adapter before fetching the page's variants/stock.
    /// # Errors
    /// Returns storage failures; empty/out-of-range pages preserve the total.
    pub async fn list_catalog(&self, query: CatalogQuery) -> BlueprintResult<CatalogPage> {
        let limit = u32::try_from(query.limit.unwrap_or(24).clamp(1, 100)).unwrap_or(100);
        let offset = query.offset.unwrap_or(0).max(0).unsigned_abs();
        let page = self
            .store()
            .product_page(query.ids, query.title, limit, offset)
            .await?;
        let mut variants: BTreeMap<String, Vec<CatalogVariant>> = BTreeMap::new();
        if !page.products.is_empty() {
            let product_ids = page
                .products
                .iter()
                .map(|product| product.id.clone())
                .collect();
            let rows: Vec<VariantDto> = self
                .many(Query {
                    equals_many: Some((Field::ProductId, product_ids)),
                    ..Query::all(RecordKind::Variant)
                })
                .await?;
            let stock = self
                .stock_by_variant_ids(rows.iter().map(|variant| variant.id.clone()).collect())
                .await?;
            for variant in rows {
                let quantity = stock.get(&variant.id).copied();
                variants
                    .entry(variant.product_id.clone())
                    .or_default()
                    .push(CatalogVariant {
                        variant,
                        stock: quantity,
                    });
            }
        }
        Ok(CatalogPage {
            products: page
                .products
                .into_iter()
                .map(|product| CatalogProduct {
                    variants: variants.remove(&product.id).unwrap_or_default(),
                    product,
                })
                .collect(),
            total: page.total,
            limit,
            offset,
        })
    }
    async fn stock_by_variant_ids(
        &self,
        ids: Vec<String>,
    ) -> BlueprintResult<BTreeMap<String, i64>> {
        if ids.is_empty() {
            return Ok(BTreeMap::new());
        }
        let rows: Vec<InventoryDto> = self
            .many(Query {
                equals_many: Some((Field::VariantId, ids)),
                ..Query::all(RecordKind::Inventory)
            })
            .await?;
        Ok(rows
            .into_iter()
            .map(|record| (record.variant_id, record.stock))
            .collect())
    }
}

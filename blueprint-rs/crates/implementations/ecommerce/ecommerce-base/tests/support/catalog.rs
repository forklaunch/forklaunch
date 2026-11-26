use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    store::{EcommerceStore, Record},
};
use forklaunch_ecommerce_interface::{ProductSearchDto, catalog::CatalogQuery};
use serde_json::json;
use std::sync::Arc;

#[allow(clippy::too_many_lines)]
pub async fn catalog_rules<S: EcommerceStore>(store: Arc<S>) {
    // IDs deliberately disagree with chronology; ties resolve by ID.
    for (id, title, date) in [
        ("cat-z", "Coffee Beans", "2026-01-01"),
        ("cat-a", "Tea Leaves", "2026-01-01"),
        ("cat-b", "Coffee Cups", "2026-01-02"),
    ] {
        store.insert(Record::Product(serde_json::from_value(json!({"id":id,"externalId":id,"handle":id,"title":title,"createdAt":format!("{date}T00:00:00.000Z")})).unwrap())).await.unwrap();
    }
    for (id, product, price) in [
        ("cat-v1", "cat-z", 100),
        ("cat-v2", "cat-z", 200),
        ("cat-v3", "cat-b", 50),
    ] {
        store.insert(Record::Variant(serde_json::from_value(json!({"id":id,"externalId":id,"productId":product,"title":id,"priceCents":price,"requiresShipping":true})).unwrap())).await.unwrap();
    }
    for (id, variant, stock) in [("cat-i1", "cat-v1", 4), ("cat-i2", "cat-v2", 0)] {
        store
            .insert(Record::Inventory(
                serde_json::from_value(json!({"id":id,"variantId":variant,"stock":stock})).unwrap(),
            ))
            .await
            .unwrap();
    }
    let service = BaseEcommerceService::new(Arc::clone(&store));
    let ids = vec!["cat-a".into(), "cat-z".into(), "cat-b".into()];
    let page = service
        .list_catalog(CatalogQuery {
            ids: Some(ids.clone()),
            limit: Some(1),
            offset: Some(1),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.products.len(), 1);
    assert_eq!(page.products[0].product.id, "cat-z");
    assert_eq!(page.products[0].variants.len(), 2);
    assert_eq!(page.products[0].variants[0].stock, Some(4));
    assert_eq!(page.products[0].variants[1].stock, Some(0));
    let page = service
        .list_catalog(CatalogQuery {
            ids: Some(ids.clone()),
            limit: Some(200),
            offset: Some(-50),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.limit, 100);
    assert_eq!(page.offset, 0);
    assert_eq!(
        page.products
            .iter()
            .map(|p| p.product.id.as_str())
            .collect::<Vec<_>>(),
        vec!["cat-a", "cat-z", "cat-b"]
    );
    assert_eq!(page.products[2].variants[0].stock, None);
    assert!(
        serde_json::to_value(&page.products[2].variants[0])
            .unwrap()
            .get("stock")
            .is_none()
    );
    for offset in [1000, i64::MAX] {
        let empty = service
            .list_catalog(CatalogQuery {
                ids: Some(ids.clone()),
                offset: Some(offset),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(empty.total, 3);
        assert!(empty.products.is_empty());
    }
    let page = service
        .list_catalog(CatalogQuery {
            title: Some("COFFEE".into()),
            ids: Some(ids.clone()),
            limit: Some(0),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.limit, 1);
    assert_eq!(page.products[0].product.id, "cat-z");
    let missing = service
        .list_catalog(CatalogQuery {
            ids: Some(vec!["no-cat".into()]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(missing.total, 0);
    assert!(missing.products.is_empty());
    let results = service
        .search_products(ProductSearchDto {
            ids: Some(ids.clone()),
            in_stock: Some(false),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        results.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        vec!["cat-b", "cat-z"]
    );
    let results = service
        .search_products(ProductSearchDto {
            ids: Some(ids),
            in_stock: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "cat-z");
    assert!(
        service
            .search_products(ProductSearchDto {
                min_price_cents: Some(i64::MAX),
                ..Default::default()
            })
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        service
            .search_products(ProductSearchDto {
                ids: Some(Vec::new()),
                ..Default::default()
            })
            .await
            .unwrap()
            .is_empty()
    );
    assert!(store.product_page(None, None, 0, 0).await.is_err());
    assert!(store.product_page(None, None, 101, 0).await.is_err());
}

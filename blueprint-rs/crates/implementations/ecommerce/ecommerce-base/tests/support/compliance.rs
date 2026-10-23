use forklaunch_ecommerce_base::{
    compliance::{EcommerceComplianceService, EcommerceComplianceStore, OrderSubjectField},
    store::{EcommerceStore, Query, Record, RecordKind},
};
use serde_json::json;
use std::sync::Arc;

pub fn order(id: &str, customer: &str) -> Record {
    serde_json::from_value(json!({"kind":"order","record":{
        "id":id,"customerId":customer,"status":"pending","items":[],
        "shippingAddress":{"name":"Compliance Person","line1":"Private Street","city":"Oakland","state":"CA","postalCode":"94601","country":"US"},
        "subtotalCents":0,"discountCents":0,"taxCents":0,"taxBreakdown":[],"shippingCents":0,"giftCardCents":0,"totalCents":0
    }})).unwrap()
}

pub async fn exercise<S: EcommerceStore + EcommerceComplianceStore>(store: Arc<S>) {
    for (id, owner) in [
        ("compliance-a", "compliance-owner"),
        ("compliance-b", "compliance-owner"),
        ("compliance-other", "foreign-owner"),
    ] {
        store.insert(order(id, owner)).await.unwrap();
    }
    let by_id = EcommerceComplianceService::new(store.clone(), OrderSubjectField::Id);
    assert!(
        by_id
            .export("compliance-owner")
            .await
            .unwrap()
            .entities
            .is_empty()
    );
    assert_eq!(
        by_id.export("compliance-a").await.unwrap().entities["Order"].len(),
        1
    );
    let service = EcommerceComplianceService::new(store.clone(), OrderSubjectField::CustomerId);
    let exported = service.export("compliance-owner").await.unwrap();
    assert_eq!(exported.entities["Order"].len(), 2);
    assert!(exported.failures.is_empty());
    let mut audited = false;
    let plaintext = exported.entities["Order"][0]
        .shipping_address
        .deanon(|access| {
            assert_eq!(access.field, "Order.shippingAddress");
            assert_eq!(access.level, "pii");
            assert_eq!(access.order_id, "compliance-a");
            audited = true;
        })
        .unwrap();
    assert!(audited);
    assert_eq!(plaintext.name, "Compliance Person");
    let serialized = serde_json::to_value(exported).unwrap();
    assert_eq!(
        serialized["entities"]["Order"][0]["shippingAddress"],
        json!({})
    );
    assert!(!serialized.to_string().contains("Private Street"));
    assert_eq!(
        serialized["entities"]["Order"][0]
            .as_object()
            .unwrap()
            .len(),
        2
    );
    assert!(service.export("").await.is_err());
    let erased = service.erase("compliance-owner").await.unwrap();
    assert_eq!(erased.records_deleted, 2);
    assert_eq!(erased.entities_affected, ["Order"]);
    assert!(erased.failures.is_empty());
    assert_eq!(
        service
            .erase("compliance-owner")
            .await
            .unwrap()
            .records_deleted,
        0
    );
    assert_eq!(
        service.export("foreign-owner").await.unwrap().entities["Order"].len(),
        1
    );
    assert_eq!(
        store
            .find(Query {
                ids: Some(vec!["compliance-other".into()]),
                ..Query::all(RecordKind::Order)
            })
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn memory_scoped_classified_export_and_erasure() {
    exercise(Arc::new(
        forklaunch_ecommerce_base::store::InMemoryEcommerceStore::default(),
    ))
    .await;
}

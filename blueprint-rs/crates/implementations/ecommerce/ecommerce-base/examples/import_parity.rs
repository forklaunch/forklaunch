//! Actual controller comparison, with IDs normalized to external ownership keys.
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    store::{EcommerceStore, InMemoryEcommerceStore, Query, Record, RecordKind},
};
use forklaunch_ecommerce_interface::{AdjustStockDto, InventoryService, VariantService};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    sync::Arc,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = String::new();
    io::stdin().read_to_string(&mut raw)?;
    let input: Value = serde_json::from_str(&raw)?;
    let store = Arc::new(InMemoryEcommerceStore::default());
    let service = BaseEcommerceService::new(Arc::clone(&store));
    let mut results = Vec::new();
    for action in input.as_array().ok_or("missing actions")? {
        let result = if action["operation"] == "adjust" {
            let variant = service
                .get_variant_by_external_id(action["externalId"].as_str().unwrap())
                .await?;
            service
                .adjust_stock(AdjustStockDto {
                    variant_id: variant.id,
                    delta: action["delta"].as_i64().unwrap(),
                })
                .await?;
            Value::Null
        } else {
            serde_json::to_value(
                service
                    .import_catalog(serde_json::from_value(action["body"].clone())?)
                    .await?,
            )?
        };
        let mut rows = store.find(Query::all(RecordKind::Product)).await?;
        rows.extend(store.find(Query::all(RecordKind::Variant)).await?);
        let ids: BTreeMap<_, _> = rows
            .iter()
            .map(|record| match record {
                Record::Product(value) => (value.id.clone(), value.external_id.clone()),
                Record::Variant(value) => (value.id.clone(), value.external_id.clone()),
                _ => unreachable!(),
            })
            .collect();
        rows.extend(store.find(Query::all(RecordKind::Inventory)).await?);
        let mut normalized = Vec::new();
        for row in rows {
            let mut value = serde_json::to_value(&row)?;
            let record = value["record"].as_object_mut().unwrap();
            record.remove("createdAt");
            record.remove("updatedAt");
            if row.kind() == RecordKind::Inventory {
                record.remove("id");
            }
            for key in ["id", "productId", "variantId"] {
                if let Some(value) = record.get_mut(key) {
                    *value = json!(ids[value.as_str().unwrap()]);
                }
            }
            normalized.push(value);
        }
        normalized.sort_by_key(Value::to_string);
        results.push(json!({"result":result,"records":normalized}));
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}

//! TypeScript inventory-worker behavior against native atomic stock updates.
use forklaunch_ecommerce_base::{BaseEcommerceService, store::InMemoryEcommerceStore};
use forklaunch_ecommerce_interface::{AdjustStockDto, InventoryService, events::OrderEvent};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    sync::Arc,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let fixture: Value = serde_json::from_str(&input)?;
    let service = BaseEcommerceService::new(Arc::new(InMemoryEcommerceStore::default()));
    let inventory = fixture["inventory"]
        .as_object()
        .ok_or("missing inventory")?;
    for (variant, stock) in inventory {
        service
            .create_inventory(serde_json::from_value(
                json!({"variantId":variant,"stock":stock}),
            )?)
            .await?;
    }
    let mut results = Vec::new();
    for step in fixture["steps"].as_array().ok_or("missing steps")? {
        if step.get("adjust").is_some() {
            service
                .adjust_stock(serde_json::from_value::<AdjustStockDto>(
                    step["adjust"].clone(),
                )?)
                .await?;
            continue;
        }
        let mut event: OrderEvent = serde_json::from_value(step["event"].clone())?;
        let failed = service.apply_order_event(&event).await.is_err();
        if !failed {
            event.processed = true;
        }
        let mut stock = BTreeMap::new();
        for variant in inventory.keys() {
            stock.insert(variant, service.get_inventory(variant).await?.stock);
        }
        results.push(json!({"failed":failed,"processed":event.processed,"stock":stock}));
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}

//! Actual TypeScript catalog controller comparison with owned rows.
use forklaunch_ecommerce_base::{
    BaseEcommerceService,
    store::{EcommerceStore, InMemoryEcommerceStore, Record},
};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    sync::Arc,
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let fixture: Value = serde_json::from_str(&input)?;
    let store = Arc::new(InMemoryEcommerceStore::default());
    for row in fixture["records"].as_array().ok_or("missing rows")? {
        store
            .insert(serde_json::from_value::<Record>(row.clone())?)
            .await?;
    }
    let service = BaseEcommerceService::new(store);
    let mut results = Vec::new();
    for case in fixture["cases"].as_array().ok_or("missing cases")? {
        let result = match case["operation"].as_str().ok_or("missing operation")? {
            "search" => serde_json::to_value(
                service
                    .search_products(serde_json::from_value(case["query"].clone())?)
                    .await?,
            ),
            "catalog" => serde_json::to_value(
                service
                    .list_catalog(serde_json::from_value(case["query"].clone())?)
                    .await?,
            ),
            _ => return Err("unknown operation".into()),
        }?;
        results.push(json!({"ok":result}));
    }
    println!("{}", serde_json::to_string(&results)?);
    Ok(())
}

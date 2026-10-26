use forklaunch_core::persistence::{
    TenantContext,
    retention::{self, EnforcementOptions, RetentionAction, RetentionPolicy},
};
use forklaunch_ecommerce_base::{
    retention::OrderCompliance,
    store::{EcommerceStore, InMemoryEcommerceStore, Query, Record, RecordKind},
};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    time::SystemTime,
};
fn date(value: &Value) -> SystemTime {
    chrono::DateTime::parse_from_rfc3339(value.as_str().unwrap())
        .unwrap()
        .into()
}
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let tenant = TenantContext::new("ecommerce-retention-fixture").unwrap();
    let store = InMemoryEcommerceStore::for_tenant(tenant.clone());
    for row in input["records"].as_array().unwrap() {
        store
            .insert(serde_json::from_value::<Record>(row.clone()).unwrap())
            .await
            .unwrap();
    }
    let mut output = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let action = if case["action"] == "delete" {
            RetentionAction::Delete
        } else {
            RetentionAction::Anonymize
        };
        let policies = if case["enabled"] == false {
            vec![]
        } else {
            vec![
                RetentionPolicy::for_entity::<OrderCompliance>(date(&input["cutoff"]), action)
                    .unwrap(),
            ]
        };
        let options = EnforcementOptions {
            dry_run: case["dryRun"] == true,
            batch_size: 1,
            entities: case
                .get("entities")
                .map(|value| serde_json::from_value(value.clone()).unwrap()),
            ..EnforcementOptions::default()
        };
        let result = retention::enforce(
            &store,
            std::slice::from_ref(&tenant),
            &policies,
            date(&input["now"]),
            &options,
        )
        .await
        .unwrap();
        let by_entity: serde_json::Map<String, Value> = result.by_entity.into_iter().map(|(name,count)| (name,json!({"deleted":count.deleted,"anonymized":count.anonymized,"errors":count.errors}))).collect();
        let rows: Vec<Value> = store
            .find(Query::all(RecordKind::Order))
            .await
            .unwrap()
            .into_iter()
            .map(|record| {
                let Record::Order(order) = record else {
                    unreachable!()
                };
                json!({"id":order.id,"address":order.shipping_address})
            })
            .collect();
        output.push(json!({"result":{"processed":result.processed,"deleted":result.deleted,"anonymized":result.anonymized,"errors":result.errors,"byEntity":by_entity},"rows":rows}));
    }
    println!("{}", json!(output));
}

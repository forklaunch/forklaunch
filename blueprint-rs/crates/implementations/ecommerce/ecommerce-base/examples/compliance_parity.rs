use forklaunch_ecommerce_base::{
    compliance::{EcommerceComplianceService, OrderSubjectField},
    store::{EcommerceStore, InMemoryEcommerceStore, Record},
};
use serde_json::{Value, json};
use std::{
    io::{self, Read},
    sync::Arc,
};
#[tokio::main]
async fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let input: Value = serde_json::from_str(&input).unwrap();
    let store = Arc::new(InMemoryEcommerceStore::default());
    for row in input["records"].as_array().unwrap() {
        store
            .insert(serde_json::from_value::<Record>(row.clone()).unwrap())
            .await
            .unwrap();
    }
    let mut output = Vec::new();
    for case in input["cases"].as_array().unwrap() {
        let field = if case["customer"] == true {
            OrderSubjectField::CustomerId
        } else {
            OrderSubjectField::Id
        };
        let service = EcommerceComplianceService::new(store.clone(), field);
        let subject = case["subject"].as_str().unwrap();
        output.push(match case["operation"].as_str().unwrap() {
            "export" => json!(service.export(subject).await.unwrap()),
            "erase" => json!(service.erase(subject).await.unwrap()),
            _ => panic!("unknown operation"),
        });
    }
    println!("{}", json!(output));
}

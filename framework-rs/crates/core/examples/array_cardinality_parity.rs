//! Reports compile admission separately from instance validity for cardinalities.
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let result = match forklaunch_core::contract::schema_validation::compile(&input["schema"]) {
            Ok(validate) => {
                json!({"compiled":true,"valid":input["values"].as_array().unwrap().iter().map(|value|validate(value).is_ok()).collect::<Vec<_>>()})
            }
            Err(error) => json!({"compiled":false,"error":error}),
        };
        println!("{result}");
    }
}

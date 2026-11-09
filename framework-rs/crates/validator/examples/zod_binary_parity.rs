use forklaunch_validator::typescript_coerce::zod_binary;
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let before = input.clone();
        let result = match zod_binary(input.get("value")) {
            Ok(bytes) => json!({"ok":true,"bytes":bytes}),
            Err(errors) => json!({"ok":false,"errors":errors}),
        };
        assert_eq!(input, before);
        println!("{result}");
    }
}

use forklaunch_validator::typescript_coerce;
use serde_json::{Value, json};
use std::io::{self, BufRead};

fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let raw = input.get("raw").and_then(Value::as_str);
        let value: Option<Value> = raw.map(|raw| serde_json::from_str(raw).unwrap());
        let number = if input["kind"] == "integer" {
            typescript_coerce::integer_number(value.as_ref())
        } else {
            typescript_coerce::number(value.as_ref())
        };
        let output = if input["kind"] == "boolean" {
            match typescript_coerce::boolean(value.as_ref()) {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(error) => json!({"ok":false,"errors":[error]}),
            }
        } else {
            match number {
                Ok(value) => json!({"ok":true,"bits":format!("{:016x}",value.to_bits())}),
                Err(error) => json!({"ok":false,"errors":[error]}),
            }
        };
        println!("{output}");
    }
}

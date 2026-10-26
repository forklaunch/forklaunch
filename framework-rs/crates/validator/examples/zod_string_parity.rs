use forklaunch_validator::typescript_coerce::{ZodStringStep, zod_string};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let steps: Vec<_> = input["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| {
                let limit = || u32::try_from(step[1].as_u64().unwrap()).unwrap();
                match step[0].as_str().unwrap() {
                    "min" => ZodStringStep::Min(limit()),
                    "max" => ZodStringStep::Max(limit()),
                    "length" => ZodStringStep::Length(limit()),
                    "trim" => ZodStringStep::Trim,
                    _ => panic!("unknown fixture step"),
                }
            })
            .collect();
        let output = match zod_string(input.get("value"), &steps) {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(errors) => json!({"ok":false,"errors":errors}),
        };
        println!("{output}");
    }
}

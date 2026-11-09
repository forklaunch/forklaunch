use forklaunch_validator::typescript_coerce::{ZodIntegerBound, ZodNumberStep, zod_number};
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
                let bound = || ZodIntegerBound::new(step[1].as_i64().unwrap()).unwrap();
                match step[0].as_str().unwrap() {
                    "int" => ZodNumberStep::Int,
                    "finite" => ZodNumberStep::Finite,
                    "safe" => ZodNumberStep::Safe,
                    "min" => ZodNumberStep::Min(bound()),
                    "max" => ZodNumberStep::Max(bound()),
                    "gt" => ZodNumberStep::Gt(bound()),
                    "lt" => ZodNumberStep::Lt(bound()),
                    _ => panic!("unknown fixture constraint"),
                }
            })
            .collect();
        let result = match zod_number(input.get("value"), &steps) {
            Ok(value) => json!({"ok":true,"bits":format!("{:016x}",value.to_bits())}),
            Err(errors) => json!({"ok":false,"errors":errors}),
        };
        println!("{result}");
    }
}

use forklaunch_validator::typescript_coerce::{ZodMultipleOf, ZodNumberStep, zod_number};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let divisor = input["divisor"]
            .as_f64()
            .unwrap_or_else(|| input["divisor"].as_str().unwrap().parse().unwrap());
        let multiple = ZodNumberStep::MultipleOf(ZodMultipleOf::new(divisor));
        let steps = match input["sequence"].as_str().unwrap() {
            "multiple" => vec![multiple],
            "intFirst" => vec![ZodNumberStep::Int, multiple],
            "intLast" => vec![multiple, ZodNumberStep::Int],
            "finiteFirst" => vec![ZodNumberStep::Finite, multiple],
            _ => panic!("unknown fixture sequence"),
        };
        let result = match zod_number(input.get("value"), &steps) {
            Ok(value) => json!({"ok":true,"bits":format!("{:016x}",value.to_bits())}),
            Err(errors) => json!({"ok":false,"errors":errors}),
        };
        println!("{result}");
    }
}

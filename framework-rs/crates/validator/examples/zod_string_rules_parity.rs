use forklaunch_validator::typescript_coerce::{ZodStringRule, ZodStringStep, zod_string_rules};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let rules: Vec<_> = input["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rule| match rule[0].as_str().unwrap() {
                "trim" => ZodStringRule::Step(ZodStringStep::Trim),
                "min" => ZodStringRule::Step(ZodStringStep::Min(
                    rule[1].as_u64().unwrap().try_into().unwrap(),
                )),
                "startsWith" => ZodStringRule::StartsWith(rule[1].as_str().unwrap()),
                "endsWith" => ZodStringRule::EndsWith(rule[1].as_str().unwrap()),
                "includes" => ZodStringRule::Includes {
                    needle: rule[1].as_str().unwrap(),
                    position: rule.get(2).map(|value| {
                        value
                            .as_f64()
                            .unwrap_or_else(|| value.as_str().unwrap().parse().unwrap())
                    }),
                },
                _ => panic!("unknown string rule"),
            })
            .collect();
        let result = match zod_string_rules(input.get("value"), &rules) {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(errors) => json!({"ok":false,"errors":errors}),
        };
        println!("{result}");
    }
}

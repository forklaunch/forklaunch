use forklaunch_validator::typescript_coerce::{
    ZodFloatBound, ZodMultipleOf, ZodNumberCheck, ZodNumberStep, ZodStringCheck, ZodStringRule,
    ZodStringStep, zod_number_checks, zod_string_checks,
};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let rules = input["checks"].as_array().unwrap();
        let result = if input["type"] == "number" {
            let checks: Vec<_> = rules
                .iter()
                .map(|rule| {
                    let bound = || ZodFloatBound::new(rule["argument"].as_f64().unwrap());
                    ZodNumberCheck {
                        message: rule["message"].as_str(),
                        step: match rule["name"].as_str().unwrap() {
                            "min" => ZodNumberStep::MinFloat(bound()),
                            "max" => ZodNumberStep::MaxFloat(bound()),
                            "gt" => ZodNumberStep::GtFloat(bound()),
                            "lt" => ZodNumberStep::LtFloat(bound()),
                            "int" => ZodNumberStep::Int,
                            "finite" => ZodNumberStep::Finite,
                            "safe" => ZodNumberStep::Safe,
                            "multipleOf" => ZodNumberStep::MultipleOf(ZodMultipleOf::new(
                                rule["argument"].as_f64().unwrap(),
                            )),
                            _ => panic!("unknown number check"),
                        },
                    }
                })
                .collect();
            match zod_number_checks(input.get("value"), &checks) {
                Ok(value) => json!({"ok":true,"bits":format!("{:016x}",value.to_bits())}),
                Err(errors) => json!({"ok":false,"errors":errors}),
            }
        } else {
            let checks: Vec<_> = rules
                .iter()
                .map(|rule| {
                    let length = || rule["argument"].as_u64().unwrap().try_into().unwrap();
                    let needle = || rule["argument"].as_str().unwrap();
                    ZodStringCheck {
                        message: rule["message"].as_str(),
                        rule: match rule["name"].as_str().unwrap() {
                            "min" => ZodStringRule::Step(ZodStringStep::Min(length())),
                            "max" => ZodStringRule::Step(ZodStringStep::Max(length())),
                            "length" => ZodStringRule::Step(ZodStringStep::Length(length())),
                            "trim" => ZodStringRule::Step(ZodStringStep::Trim),
                            "startsWith" => ZodStringRule::StartsWith(needle()),
                            "endsWith" => ZodStringRule::EndsWith(needle()),
                            "includes" => ZodStringRule::Includes {
                                needle: needle(),
                                position: rule["position"].as_f64(),
                            },
                            _ => panic!("unknown string check"),
                        },
                    }
                })
                .collect();
            match zod_string_checks(input.get("value"), &checks) {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(errors) => json!({"ok":false,"errors":errors}),
            }
        };
        println!("{result}");
    }
}

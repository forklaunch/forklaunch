use forklaunch_validator::{
    typescript_coerce::{
        ZodFloatBound, ZodJsonOutcome, ZodJsonSchema, ZodStringCheck, ZodStringRule, ZodStringStep,
        zod_json,
    },
    zod_array::{ZodArrayCheck, ZodArrayRule, zod_array_checks},
};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn schema(checks: Vec<ZodArrayCheck<'_>>, element: bool) -> ZodJsonSchema<'_> {
    ZodJsonSchema::CheckedArray {
        element: Box::new(if element {
            ZodJsonSchema::String(vec![ZodStringCheck {
                rule: ZodStringRule::Step(ZodStringStep::Trim),
                message: None,
            }])
        } else {
            ZodJsonSchema::Unknown
        }),
        checks,
    }
}
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let checks: Vec<_> = input["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|check| {
                let bound =
                    || ZodFloatBound::new(check["bound"].as_str().unwrap().parse().unwrap());
                ZodArrayCheck {
                    message: check["message"].as_str(),
                    rule: match check["kind"].as_str().unwrap() {
                        "min" => ZodArrayRule::Min(bound()),
                        "max" => ZodArrayRule::Max(bound()),
                        "length" => ZodArrayRule::Length(bound()),
                        "nonempty" => ZodArrayRule::Nonempty,
                        _ => panic!("unknown check"),
                    },
                }
            })
            .collect();
        let result = if input["mode"] == "borrowed" {
            match zod_array_checks(input.get("value"), &checks) {
                Ok(value) => json!({"ok":true,"value":value}),
                Err(errors) => json!({"ok":false,"errors":errors}),
            }
        } else {
            let element = input["element"] == true;
            let schema = match input["mode"].as_str().unwrap() {
                "union" => {
                    ZodJsonSchema::Union(vec![schema(checks, element), ZodJsonSchema::Unknown])
                }
                "intersection" => ZodJsonSchema::Intersection {
                    left: Box::new(schema(checks.clone(), element)),
                    right: Box::new(schema(checks, element)),
                },
                _ => schema(checks, element),
            };
            match zod_json(input.get("value"), &schema).unwrap() {
                ZodJsonOutcome::Valid(Some(value)) => json!({"ok":true,"value":value}),
                ZodJsonOutcome::Valid(None) => json!({"ok":true}),
                ZodJsonOutcome::Invalid(errors) => json!({"ok":false,"errors":errors}),
            }
        };
        println!("{result}");
    }
}

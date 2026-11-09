use forklaunch_validator::typescript_coerce::{
    ZodStringCheck, ZodStringFormat, ZodStringRule, ZodStringStep, zod_string_checks,
};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let format = match input["format"].as_str().unwrap() {
            "uuid" => ZodStringFormat::Uuid,
            "email" => ZodStringFormat::Email,
            #[cfg(feature = "zod-uri")]
            "uri" => ZodStringFormat::Uri,
            _ => panic!("unknown format"),
        };
        let plain = |rule| ZodStringCheck {
            rule,
            message: None,
        };
        let mut checks = Vec::new();
        if input["order"] == "trimFirst" {
            checks.push(plain(ZodStringRule::Step(ZodStringStep::Trim)));
        }
        checks.push(ZodStringCheck {
            rule: ZodStringRule::Format(format),
            message: input["message"].as_str(),
        });
        if input["order"] == "mixed" {
            checks.push(plain(ZodStringRule::Step(ZodStringStep::Trim)));
            checks.push(plain(ZodStringRule::EndsWith("z")));
        }
        let result = match zod_string_checks(input.get("value"), &checks) {
            Ok(value) => json!({"ok":true,"value":value}),
            Err(errors) => json!({"ok":false,"errors":errors}),
        };
        println!("{result}");
    }
}

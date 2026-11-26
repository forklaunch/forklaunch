use forklaunch_validator::typescript_coerce::{
    ZodIpVersion, ZodJsonField, ZodJsonOutcome, ZodJsonSchema, ZodStringCheck, ZodStringFormat,
    ZodStringFormatParameterError as ParameterError, ZodStringRule, ZodStringStep,
    ZodTimePrecision, zod_json,
};
use serde_json::{Value, json};
use std::io::{self, BufRead};

fn precision(value: &Value) -> Result<ZodTimePrecision, ParameterError> {
    ZodTimePrecision::new(match value.get("precision") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_i64().ok_or(ParameterError::TimePrecision)?),
    })
}
fn version(value: &Value) -> Result<Option<ZodIpVersion>, ParameterError> {
    match value.get("version") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            ZodIpVersion::try_from(value.as_str().ok_or(ParameterError::IpVersion)?).map(Some)
        }
    }
}
fn rule(value: &Value) -> Result<ZodStringRule<'_>, ParameterError> {
    use ZodStringFormat as Format;
    Ok(match value["kind"].as_str().unwrap() {
        "trim" => ZodStringRule::Step(ZodStringStep::Trim),
        "jwt" => ZodStringRule::Jwt {
            algorithm: value.get("algorithm").and_then(Value::as_str),
        },
        kind => ZodStringRule::Format(match kind {
            "uuid" => Format::Uuid,
            "email" => Format::Email,
            "cuid" => Format::Cuid,
            "cuid2" => Format::Cuid2,
            "ulid" => Format::Ulid,
            "nanoid" => Format::Nanoid,
            "base64" => Format::Base64,
            "base64url" => Format::Base64Url,
            "date" => Format::Date,
            "duration" => Format::Duration,
            "time" => Format::Time {
                precision: precision(value)?,
            },
            "datetime" => Format::DateTime {
                precision: precision(value)?,
                local: value["local"].as_bool().unwrap_or(false),
                offset: value["offset"].as_bool().unwrap_or(false),
            },
            "ip" => Format::Ip(version(value)?),
            "cidr" => Format::Cidr(version(value)?),
            "emoji" => Format::Emoji,
            _ => panic!("unknown fixture format"),
        }),
    })
}
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let checks: Result<Vec<_>, _> = input["checks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                rule(value).map(|rule| ZodStringCheck {
                    rule,
                    message: value.get("message").and_then(Value::as_str),
                })
            })
            .collect();
        let output = match checks {
            Err(error) => json!({"unsupported":error.to_string()}),
            Ok(checks) => {
                let leaf = ZodJsonSchema::String(checks);
                let schema = match input["shape"].as_str().unwrap_or("direct") {
                    "object" => ZodJsonSchema::Object(vec![ZodJsonField {
                        name: "value",
                        schema: leaf,
                    }]),
                    "array" => ZodJsonSchema::Array {
                        element: Box::new(leaf),
                        checks: vec![],
                    },
                    _ => leaf,
                };
                match zod_json(input.get("value"), &schema) {
                    Ok(ZodJsonOutcome::Valid(Some(value))) => json!({"ok":true,"value":value}),
                    Ok(ZodJsonOutcome::Valid(None)) => json!({"ok":true}),
                    Ok(ZodJsonOutcome::Invalid(errors)) => json!({"ok":false,"errors":errors}),
                    Err(error) => json!({"unsupported":format!("{error:?}")}),
                }
            }
        };
        println!("{output}");
    }
}

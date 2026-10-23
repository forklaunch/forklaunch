use forklaunch_validator::{
    typescript_coerce::{
        OrderedJsonDocument, ZodFloatBound, ZodJsonField, ZodJsonLiteral, ZodJsonOutcome,
        ZodJsonSchema, ZodNumberCheck, ZodNumberStep, ZodStringCheck, ZodStringRule, ZodStringStep,
        zod_json_at_path, zod_json_ordered_at_path,
    },
    zod_array::ZodArrayStep,
};
use serde_json::{Value, json};
use std::io::{self, BufRead};
fn literal(spec: &Value) -> ZodJsonLiteral<'_> {
    match spec["kind"].as_str().unwrap() {
        "string" => ZodJsonLiteral::String(spec["value"].as_str().unwrap()),
        "boolean" => ZodJsonLiteral::Boolean(spec["value"].as_bool().unwrap()),
        "number" => ZodJsonLiteral::Number(
            spec["value"]
                .as_f64()
                .unwrap_or_else(|| spec["value"].as_str().unwrap().parse().unwrap()),
        ),
        _ => panic!("unsupported literal kind"),
    }
}
fn string_schema(spec: &Value) -> ZodJsonSchema<'_> {
    ZodJsonSchema::String(
        spec["checks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|check| ZodStringCheck {
                message: check["message"].as_str(),
                rule: match check["name"].as_str().unwrap() {
                    "trim" => ZodStringRule::Step(ZodStringStep::Trim),
                    #[cfg(feature = "zod-uri")]
                    "url" => ZodStringRule::Format(
                        forklaunch_validator::typescript_coerce::ZodStringFormat::Uri,
                    ),
                    "min" => ZodStringRule::Step(ZodStringStep::Min(
                        check["argument"].as_u64().unwrap().try_into().unwrap(),
                    )),
                    "max" => ZodStringRule::Step(ZodStringStep::Max(
                        check["argument"].as_u64().unwrap().try_into().unwrap(),
                    )),
                    "endsWith" => ZodStringRule::EndsWith(check["argument"].as_str().unwrap()),
                    _ => panic!("unknown string check"),
                },
            })
            .collect(),
    )
}
fn object_policy(spec: &Value) -> ZodJsonSchema<'_> {
    ZodJsonSchema::ObjectWithPolicy {
        fields: spec["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| ZodJsonField {
                name: field[0].as_str().unwrap(),
                schema: schema(&field[1]),
            })
            .collect(),
        unknown: match spec["unknown"].as_str().unwrap() {
            "strip" => forklaunch_validator::typescript_coerce::ZodJsonUnknownKeys::Strip,
            "strict" => forklaunch_validator::typescript_coerce::ZodJsonUnknownKeys::Strict,
            "passthrough" => {
                forklaunch_validator::typescript_coerce::ZodJsonUnknownKeys::Passthrough
            }
            _ => panic!("unknown object policy"),
        },
        catchall: spec.get("catchall").map(|value| Box::new(schema(value))),
    }
}
fn registered_schema(spec: &Value) -> ZodJsonSchema<'_> {
    match spec["kind"].as_str().unwrap() {
        "enum" => ZodJsonSchema::Enum(
            spec["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect(),
        ),
        "nativeEnum" => ZodJsonSchema::NativeEnum(
            spec["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(
                    |entry| forklaunch_validator::typescript_coerce::ZodJsonNativeEnumEntry {
                        name: entry[0].as_str().unwrap(),
                        value: if entry[1]["kind"] == "string" {
                            forklaunch_validator::typescript_coerce::ZodJsonEnumValue::String(
                                entry[1]["value"].as_str().unwrap(),
                            )
                        } else {
                            forklaunch_validator::typescript_coerce::ZodJsonEnumValue::Number(
                                entry[1]["value"].as_f64().unwrap_or_else(|| {
                                    entry[1]["value"].as_str().unwrap().parse().unwrap()
                                }),
                            )
                        },
                    },
                )
                .collect(),
        ),
        "discriminated" => ZodJsonSchema::DiscriminatedUnion {
            discriminator: spec["discriminator"].as_str().unwrap(),
            options: spec["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(schema)
                .collect(),
        },
        _ => unreachable!(),
    }
}
fn number_schema(spec: &Value) -> ZodJsonSchema<'_> {
    ZodJsonSchema::Number(
        spec["checks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|check| ZodNumberCheck {
                message: check["message"].as_str(),
                step: match check["name"].as_str().unwrap() {
                    "int" => ZodNumberStep::Int,
                    "finite" => ZodNumberStep::Finite,
                    "min" => ZodNumberStep::MinFloat(ZodFloatBound::new(
                        check["argument"].as_f64().unwrap(),
                    )),
                    _ => panic!("unknown number check"),
                },
            })
            .collect(),
    )
}
fn schema(spec: &Value) -> ZodJsonSchema<'_> {
    let inner = || Box::new(schema(&spec["inner"]));
    match spec["kind"].as_str().unwrap() {
        "enum" | "nativeEnum" | "discriminated" => registered_schema(spec),
        "string" => string_schema(spec),
        "policy" => object_policy(spec),
        "number" => number_schema(spec),
        "record" => ZodJsonSchema::Record {
            key: Box::new(schema(&spec["key"])),
            value: Box::new(schema(&spec["valueSchema"])),
        },
        "intersection" => ZodJsonSchema::Intersection {
            left: Box::new(schema(&spec["left"])),
            right: Box::new(schema(&spec["right"])),
        },
        "union" => ZodJsonSchema::Union(
            spec["branches"]
                .as_array()
                .unwrap()
                .iter()
                .map(schema)
                .collect(),
        ),
        "literal" => ZodJsonSchema::Literal(literal(&spec["literal"])),
        "boolean" => ZodJsonSchema::Boolean,
        "unknown" => ZodJsonSchema::Unknown,
        "null" => ZodJsonSchema::Null,
        "undefined" => ZodJsonSchema::Undefined,
        "void" => ZodJsonSchema::Void,
        "never" => ZodJsonSchema::Never,
        "nullish" => ZodJsonSchema::Union(vec![
            ZodJsonSchema::Void,
            ZodJsonSchema::Null,
            ZodJsonSchema::Undefined,
        ]),
        "optional" => ZodJsonSchema::Optional(inner()),
        "nullable" => ZodJsonSchema::Nullable(inner()),
        "default" => ZodJsonSchema::Default {
            schema: inner(),
            value: &spec["value"],
        },
        "tuple" => ZodJsonSchema::Tuple {
            items: spec["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(schema)
                .collect(),
            rest: spec.get("rest").map(|value| Box::new(schema(value))),
        },
        "array" => ZodJsonSchema::Array {
            element: inner(),
            checks: spec["checks"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|check| {
                    let length = || check["argument"].as_u64().unwrap().try_into().unwrap();
                    match check["name"].as_str().unwrap() {
                        "min" => ZodArrayStep::Min(length()),
                        "max" => ZodArrayStep::Max(length()),
                        "length" => ZodArrayStep::Length(length()),
                        _ => panic!("unknown array check"),
                    }
                })
                .collect(),
        },
        "strict" | "object" => {
            let fields = spec["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|field| ZodJsonField {
                    name: field[0].as_str().unwrap(),
                    schema: schema(&field[1]),
                })
                .collect();
            if spec["kind"] == "strict" {
                ZodJsonSchema::StrictObject(fields)
            } else {
                ZodJsonSchema::Object(fields)
            }
        }
        _ => panic!("unknown schema"),
    }
}
fn tag(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            json!({"$number":format!("{:016x}",number.as_f64().unwrap().to_bits())})
        }
        Value::Array(values) => Value::Array(values.into_iter().map(tag).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, tag(value)))
                .collect(),
        ),
        other => other,
    }
}
fn main() {
    for line in io::stdin().lock().lines() {
        let input: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let before = input.clone();
        let raw = input["raw"]
            .as_str()
            .map(|value| serde_json::from_str::<Value>(value).unwrap());
        let path: Vec<_> = input["path"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|part| part.as_str().unwrap())
            .collect();
        let schema = schema(&input["schema"]);
        let evaluated = if input["ordered"] == true {
            let document =
                OrderedJsonDocument::parse(input["raw"].as_str().unwrap().as_bytes()).unwrap();
            zod_json_ordered_at_path(&document, &schema, &path)
        } else {
            zod_json_at_path(raw.as_ref().or_else(|| input.get("value")), &schema, &path)
        };
        let result = match evaluated {
            Ok(ZodJsonOutcome::Valid(value)) => match value {
                Some(value) => json!({"ok":true,"absent":false,"value":tag(value)}),
                None => json!({"ok":true,"absent":true}),
            },
            Ok(ZodJsonOutcome::Invalid(errors)) => json!({"ok":false,"errors":errors}),
            Err(reason) => json!({"unsupported":format!("{reason:?}")}),
        };
        assert_eq!(input, before);
        println!("{result}");
    }
}

//! Conformance bridge for typed versioned request fields.
use forklaunch_core::contract::{FieldContract, RequestContract, RouteContract, VersionedContract};
use forklaunch_core::http::{
    ContractValidationLocation as Location, FrameworkRequest, HttpMethod, RequestContext,
    VersionBodyHooksRegistry, VersionFieldHooks, validate_framework_request,
};
use serde_json::{Value, json};
use std::io::{self, Read};

fn field(name: &str, schema: &Value) -> FieldContract {
    let mut field = FieldContract::required(name, "Field");
    field.schema.json_schema = Some(schema.to_string());
    field
}
fn number(last: bool, header: bool) -> VersionFieldHooks {
    VersionFieldHooks::default()
        .with_optional_preprocess(move |value| {
            let number = match value {
                None => {
                    if last {
                        10
                    } else {
                        0
                    }
                }
                Some(Value::String(value)) => value.trim().parse::<i64>().map_err(|_| "number")?,
                _ => return Err("number".into()),
            };
            Ok(Some(json!(number)))
        })
        .with_optional_transform(move |value| {
            let number = value.and_then(Value::as_i64).ok_or("number")?;
            if (header && number == 66) || (!header && last && number == 13) {
                return Err("private transform error".into());
            }
            if number == 42 {
                return Ok(None);
            }
            Ok(Some(if header {
                match number {
                    0 => json!(0),
                    10 => Value::Null,
                    11 => json!(false),
                    12 => json!(""),
                    13 => json!([]),
                    14 => json!({}),
                    _ => json!(number * 2),
                }
            } else if number == 0 {
                Value::Null
            } else {
                json!(number + if last { 100 } else { 0 })
            }))
        })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let cases: Vec<Value> = serde_json::from_str(&input)?;
    let mut route = RouteContract::get("Fields", "Fields", "/fields/:id")
        .with_param(field("id", &json!({"type":"string","minLength":1})));
    for (version, minimum) in [("first", 0), ("last", 10)] {
        route = route.with_version(VersionedContract {
            version: version.into(),
            request: RequestContract {
                query: vec![field("count", &json!({"type":"integer","minimum":minimum}))],
                headers: vec![field(
                    "x-count",
                    &json!({"type":"integer","minimum":minimum}),
                )],
                ..Default::default()
            },
            responses: vec![],
        });
    }
    let mut hooks = VersionBodyHooksRegistry::default();
    for version in ["first", "last"] {
        hooks.insert_field(
            &route,
            version,
            Location::PathParameter,
            "id",
            VersionFieldHooks::default()
                .with_preprocess(|value| Ok(json!(value.as_str().ok_or("string")?.trim())))
                .with_transform(|value| {
                    let value = value.as_str().ok_or("string")?;
                    if value == "deny" {
                        Err("private parameter error".into())
                    } else {
                        Ok(json!(value.to_uppercase()))
                    }
                }),
        )?;
        hooks.insert_field(
            &route,
            version,
            Location::Query,
            "count",
            number(version == "last", false),
        )?;
        hooks.insert_field(
            &route,
            version,
            Location::Header,
            "X-Count",
            number(version == "last", true),
        )?;
    }
    hooks.validate_routes(&[route.clone()])?;
    let plan = hooks.compile(&route)?;
    let mut output = Vec::new();
    for case in cases {
        let mut request = FrameworkRequest::new(
            HttpMethod::Get,
            "/fields/value",
            RequestContext::new("fields"),
        );
        for (name, values) in [
            ("params", &mut request.params),
            ("query", &mut request.query),
            ("headers", &mut request.headers),
        ] {
            for (key, value) in case[name].as_object().unwrap() {
                values.insert(key.clone(), value.as_str().unwrap().into());
            }
        }
        let original = request.clone();
        output.push(match plan.select(&request) {
            Ok(Some((contract, selected))) => {
                let result=json!({"status":200,"version":selected.version,"matched":selected.matched_versions,
                    "params":selected.parsed_fields.params,"query":selected.parsed_fields.query,"headers":selected.parsed_fields.headers});
                request.version=Some(selected);
                assert!(validate_framework_request(&contract,&request).is_empty());
                assert_eq!(request.params,original.params); assert_eq!(request.query,original.query); assert_eq!(request.headers,original.headers);
                result
            },
            Err(_) => json!({"status":400}),
            _ => unreachable!(),
        });
    }
    println!("{}", json!(output));
    Ok(())
}

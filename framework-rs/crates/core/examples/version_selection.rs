//! Conformance bridge for the real HTTP version matcher.
use forklaunch_core::contract::{
    BodyContract, BodyKind, RequestContract, ResponseContract, RouteContract, VersionedContract,
};
use forklaunch_core::http::{
    Body, FrameworkRequest, HttpMethod, RequestContext, VersionedRoutePlan,
};
use serde_json::{Value, json};
use std::io::{self, Read};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let fixtures: Vec<Value> = serde_json::from_str(&input)?;
    let mut output = Vec::new();
    for fixture in fixtures {
        let mut contract = RouteContract::post("Versioned", "Versioned", "/versioned");
        if let Some(kinds) = fixture["parserKinds"].as_array() {
            output.push(parser_registration(kinds));
            continue;
        }
        configure_parsers(&mut contract, &fixture["parserOptions"]);
        let text = fixture["kind"] == "text";
        let file = fixture["kind"] == "file";
        let form = fixture["kind"] == "form";
        let multipart = fixture["kind"] == "multipart";
        let wire_bytes = fixture["wireBytes"].as_bool().unwrap_or(false);
        contract = fixture_versions(contract, &fixture);
        let mut plan = VersionedRoutePlan::compile(&contract)
            .map_err(|error| format!("fixture {}: {error}", output.len()))?;
        for version in fixture["versions"].as_array().unwrap() {
            let mode = version["hook"].as_str().unwrap_or("");
            if !mode.is_empty() {
                plan =
                    plan.with_body_hooks(version["name"].as_str().unwrap(), fixture_hooks(mode))?;
            }
        }
        if fixture.get("verify").is_some() || fixture.get("reviver").is_some() {
            plan = plan.with_parser_hooks(fixture_parser_hooks(&fixture))?;
        }
        let mut cases = Vec::new();
        for case in fixture["cases"].as_array().unwrap() {
            let request = FrameworkRequest::new(
                HttpMethod::Post,
                "/versioned",
                RequestContext::new("fixture"),
            )
            .with_header(
                "content-type",
                if let Some(content_type) = fixture["contentType"].as_str() {
                    content_type
                } else if multipart {
                    "multipart/form-data; boundary=boundary"
                } else if form {
                    "application/x-www-form-urlencoded"
                } else if file {
                    "application/octet-stream"
                } else if text {
                    "text/plain"
                } else {
                    "application/json"
                },
            )
            .with_header(
                "content-encoding",
                fixture["contentEncoding"].as_str().unwrap_or("identity"),
            )
            .with_body(if file || multipart || wire_bytes {
                Body::Binary(
                    case.as_array()
                        .unwrap()
                        .iter()
                        .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
                        .collect(),
                )
            } else {
                Body::Text(if text || form {
                    case.as_str().unwrap().into()
                } else {
                    case.to_string()
                })
            });
            cases.push(match plan.select(&request) {
                Ok(Some((_,version)))=> {
                    let mut output=json!({"version":version.version,"matched":version.matched_versions,"status":200});
                    if fixture["includeBody"] == true { output["body"]=version.parsed_body.as_deref().cloned().unwrap_or(Value::Null); }
                    if fixture["includePresence"] == true { output["bodyPresent"]=json!(version.parsed_body.is_some()); }
                    // Handler-visible member order: every object key path, depth first.
                    if fixture["includeKeyOrder"] == true { let mut keys=Vec::new(); if let Some(body)=version.parsed_body.as_deref() { key_paths(body,"",&mut keys); } output["bodyKeys"]=json!(keys); }
                    output
                },
                Err(errors)=>json!({"status":forklaunch_core::http::validation_error_response(&errors,"fixture").status_code}),
                _=>json!({"status":400})
            });
        }
        output.push(json!({"order":contract.versions.keys().collect::<Vec<_>>(),"cases":cases}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn parser_registration(kinds: &[Value]) -> Value {
    let mut contract = RouteContract::post("Versioned", "Versioned", "/versioned");
    for (index, kind) in kinds.iter().enumerate() {
        let kind = match kind.as_str().unwrap() {
            "none" => None,
            "json" => Some(BodyKind::Json),
            "text" => Some(BodyKind::Text),
            "file" => Some(BodyKind::File),
            "binary" => Some(BodyKind::Binary),
            "urlEncoded" => Some(BodyKind::UrlEncoded),
            "multipart" => Some(BodyKind::Multipart),
            _ => unreachable!(),
        };
        let body = kind.map(|kind| BodyContract {
            kind,
            ..BodyContract::json("Input")
        });
        contract = contract.with_version(VersionedContract {
            version: format!("v{index}"),
            request: RequestContract {
                body,
                ..Default::default()
            },
            responses: vec![],
        });
    }
    json!({"accepted":VersionedRoutePlan::compile(&contract).is_ok()})
}

fn configure_parsers(contract: &mut RouteContract, options: &Value) {
    let parsers = &mut contract.options.body_parser;
    for (name, wire) in [
        ("json", &mut parsers.json_wire),
        ("text", &mut parsers.text_wire),
        ("urlencoded", &mut parsers.form_wire),
        ("raw", &mut parsers.raw_wire),
    ] {
        if let Some(limit) = options[name]["limit"].as_u64() {
            wire.limit_bytes = usize::try_from(limit).unwrap();
        }
        if let Some(inflate) = options[name]["inflate"].as_bool() {
            wire.inflate = inflate;
        }
    }

    if let Some(value) = options["json"]["strict"].as_bool() {
        parsers.json_strict = value;
    }
    if let Some(value) = options["json"]["defaultCharset"].as_str() {
        parsers.json_default_charset = value.into();
    }
    if let Some(value) = options["text"]["defaultCharset"].as_str() {
        parsers.text_default_charset = value.into();
    }
    if let Some(value) = options["urlencoded"]["defaultCharset"].as_str() {
        parsers.form_default_charset = value.into();
    }
    if let Some(value) = options["urlencoded"]["extended"].as_bool() {
        parsers.form_extended = value;
    }
    if let Some(value) = options["urlencoded"]["charsetSentinel"].as_bool() {
        parsers.form_charset_sentinel = value;
    }
    if let Some(value) = options["urlencoded"]["interpretNumericEntities"].as_bool() {
        parsers.form_interpret_numeric_entities = value;
    }
    if let Some(value) = options["urlencoded"]["depth"].as_u64() {
        parsers.form_depth = usize::try_from(value).unwrap();
    }
    if let Some(value) = options["urlencoded"]["parameterLimit"].as_u64() {
        parsers.form_parameter_limit = usize::try_from(value).unwrap();
    }
}

fn fixture_hooks(mode: &str) -> forklaunch_core::http::VersionBodyHooks {
    if mode.starts_with("optional") {
        let first = mode == "optional-first";
        return forklaunch_core::http::VersionBodyHooks::default()
            .with_optional_preprocess(move |value| {
                Ok(Some(value.cloned().unwrap_or(json!(if first {
                    5
                } else {
                    10
                }))))
            })
            .with_optional_transform(move |value| {
                let number = value.and_then(Value::as_i64).ok_or("not integer")?;
                if number == 13 && !first {
                    return Err("private transform detail".into());
                }
                Ok((number != 99).then(|| json!({"value":number})))
            });
    }
    let mode = mode.to_owned();
    let preprocess_mode = mode.clone();
    forklaunch_core::http::VersionBodyHooks::default()
        .with_preprocess(move |value| {
            if preprocess_mode.starts_with("trim") {
                Ok(value
                    .as_str()
                    .map_or_else(|| value.clone(), |value| json!(value.trim())))
            } else if let Some(value) = value.as_str() {
                value
                    .trim()
                    .parse::<i64>()
                    .map(Value::from)
                    .map_err(|_| "invalid integer".into())
            } else {
                Ok(value.clone())
            }
        })
        .with_transform(move |value| {
            if mode.starts_with("trim") {
                let text = value.as_str().ok_or("not text")?;
                if text == "deny" || (mode == "trim-last" && text == "blocked") {
                    return Err("rejected transform".into());
                }
                Ok(if mode == "trim-first" {
                    json!({"value":text.to_uppercase(),"source":"first"})
                } else {
                    json!({"value":format!("{text}!"),"source":"last"})
                })
            } else {
                let number = value.as_i64().ok_or("not integer")?;
                if number == 99 || (mode == "number-last" && number == 13) {
                    return Err("rejected transform".into());
                }
                Ok(json!({"value":if mode == "number-first" {number+10} else {number*2}}))
            }
        })
}

fn fixture_versions(mut contract: RouteContract, fixture: &Value) -> RouteContract {
    for version in fixture["versions"].as_array().unwrap() {
        let mut body = BodyContract::json("Input");
        if fixture["kind"] == "text" {
            body.kind = BodyKind::Text;
            body.content_type = Some("text/plain".into());
        }
        if fixture["kind"] == "file" {
            body.kind = BodyKind::File;
            body.content_type = Some("application/octet-stream".into());
        }
        if fixture["kind"] == "form" {
            body.kind = BodyKind::UrlEncoded;
            body.content_type = Some("application/x-www-form-urlencoded".into());
        }
        if fixture["kind"] == "multipart" {
            body.kind = BodyKind::Multipart;
            body.content_type = Some("multipart/form-data".into());
        }
        body.schema = body.schema.with_json_schema(version["schema"].to_string());
        contract = contract.with_version(VersionedContract {
            version: version["name"].as_str().unwrap().into(),
            request: RequestContract::default().with_body(body),
            responses: vec![ResponseContract::new(200, None)],
        });
    }
    contract
}

fn fixture_parser_hooks(fixture: &Value) -> forklaunch_core::http::BodyParserHooks {
    let mut hooks = forklaunch_core::http::BodyParserHooks::default();
    if let Some(verify) = fixture.get("verify") {
        let verify = verify.clone();
        hooks = hooks.with_verify(move |_, bytes, charset| {
            let received = json!({"bytes":bytes,"charset":charset});
            if verify["reject"] == true || received != verify["expected"] {
                return Err("verification rejected".into());
            }
            Ok(())
        });
    }
    if fixture.get("reviver").is_some() {
        if fixture["reviver"] == "missing" {
            return hooks.with_json_reviver(|key, value| {
                Ok((!key.is_empty() || value != "missing").then_some(value))
            });
        }
        let trace = fixture["reviver"] == "trace";
        let count = std::sync::atomic::AtomicI64::new(0);
        hooks = hooks.with_json_reviver(move |key, value| {
            let ordinal = count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if key == "reject" {
                return Err("private reviver detail".into());
            }
            if key == "drop" {
                return Ok(None);
            }
            Ok(Some(match value {
                Value::Number(value) => json!(if trace {
                    ordinal
                } else {
                    value.as_i64().ok_or("integer")? * 2
                }),
                Value::String(value) => json!(value.trim().to_uppercase()),
                value => value,
            }))
        });
    }
    hooks
}

fn key_paths(value: &Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(members) => {
            for (key, member) in members {
                let path = format!("{prefix}/{key}");
                out.push(path.clone());
                key_paths(member, &path, out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                key_paths(item, &format!("{prefix}/{index}"), out);
            }
        }
        _ => {}
    }
}

//! Checked `AsyncAPI` for raw typed sockets and TypeScript channel enrichment.
use super::{AsyncApiInfo, invalid, pointer_segment};
use forklaunch_common::FrameworkResult;
use forklaunch_core::contract::relocate_schema_json_unscoped;
use forklaunch_core::ws::{SocketContract, SocketSchemaEntry};
use serde_json::{Map, Value, json};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SocketAsyncApiOptions {
    pub description: Option<String>,
    pub id: Option<String>,
    pub default_content_type: Option<String>,
}
/// Export raw socket payload schemas without inventing an event envelope.
/// Channel fan-out and operation naming follow the TypeScript generator.
/// Error metadata is documented without send/receive operations; ping/pong and
/// close reasons are protocol controls and are omitted, matching TypeScript.
///
/// # Errors
/// Rejects missing names, conflicting message/operation definitions, malformed
/// schemas, unresolved/external references and unsupported resource/anchor scopes.
pub fn generate_raw_socket_asyncapi_json(
    info: &AsyncApiInfo,
    contract: &SocketContract,
) -> FrameworkResult<String> {
    generate_raw_socket_asyncapi_json_with_options(
        info,
        contract,
        &SocketAsyncApiOptions::default(),
    )
}
/// Export raw socket schemas with document metadata.
///
/// # Errors
/// Returns the same checked contract/schema errors as the default exporter.
pub fn generate_raw_socket_asyncapi_json_with_options(
    info: &AsyncApiInfo,
    contract: &SocketContract,
    options: &SocketAsyncApiOptions,
) -> FrameworkResult<String> {
    if info.title.is_empty() || info.version.is_empty() {
        return Err(invalid("AsyncAPI title and version are required"));
    }
    let mut channels = Map::new();
    let mut operations = Map::new();
    for (collection, action) in [
        (&contract.client_messages, Some("receive")),
        (&contract.server_messages, Some("send")),
        (&contract.errors, None),
    ] {
        for (key, entry) in collection {
            let name = entry.message_name(key);
            if key.is_empty() || name.is_empty() {
                return Err(invalid(
                    "socket message keys and operation names are required",
                ));
            }
            for channel in entry.channel_names(key) {
                append_message(&mut channels, channel, name, entry)?;
                if let Some(action) = action {
                    let operation = json!({"action":action,"channel":{"$ref":format!("#/channels/{}",pointer_segment(channel))},"messages":[{"$ref":format!("#/channels/{}/messages/{}",pointer_segment(channel),pointer_segment(name))}]});
                    let id = format!("{action}-{channel}-{name}");
                    if operations
                        .get(&id)
                        .is_some_and(|previous| previous != &operation)
                    {
                        return Err(invalid(
                            "socket operation identifiers have conflicting definitions",
                        ));
                    }
                    operations.insert(id, operation);
                }
            }
        }
    }
    let mut document =
        json!({"asyncapi":"3.0.0","info":{"title":info.title,"version":info.version}});
    if !channels.is_empty() {
        document["channels"] = Value::Object(channels);
    }
    if !operations.is_empty() {
        document["operations"] = Value::Object(operations);
    }
    if let Some(description) = &options.description {
        document["info"]["description"] = json!(description);
    }
    if let Some(id) = &options.id {
        document["id"] = json!(id);
    }
    if let Some(content_type) = &options.default_content_type {
        document["defaultContentType"] = json!(content_type);
    }
    Ok(document.to_string())
}
fn append_message(
    channels: &mut Map<String, Value>,
    channel: &str,
    name: &str,
    entry: &SocketSchemaEntry,
) -> FrameworkResult<()> {
    if channel.is_empty() || channel.contains(['?', '#']) {
        return Err(invalid(
            "socket channel addresses must be nonempty and exclude query/fragment delimiters",
        ));
    }
    if !entry.valid_binary_shape() {
        return Err(invalid(
            "Binary socket entries require the canonical binary shape",
        ));
    }
    let format = schema_format(&entry.shape);
    let mut location = vec!["channels", channel, "messages", name, "payload"];
    if format.is_some() {
        location.push("schema");
    }
    let payload = embedded_schema(&entry.shape, &location)?;
    let payload = match format {
        Some(format) => json!({"schemaFormat":format,"schema":payload}),
        None => payload,
    };
    let mut message = json!({"name":name,"payload":payload});
    if entry.binary {
        message["contentType"] = json!("application/octet-stream");
    } else if entry.json_encoding {
        message["contentType"] = json!("application/json");
    }
    let channel = channels
        .entry(channel)
        .or_insert_with(|| json!({"address":channel,"messages":{}}));
    let messages = channel["messages"]
        .as_object_mut()
        .ok_or_else(|| invalid("channel messages must be an object"))?;
    if messages
        .get(name)
        .is_some_and(|previous| previous != &message)
    {
        return Err(invalid(
            "socket message identifiers have conflicting payload schemas",
        ));
    }
    messages.insert(name.into(), message);
    Ok(())
}
fn schema_format(schema: &Value) -> Option<String> {
    let dialect = schema.get("$schema")?.as_str()?;
    ["draft-04", "draft-06", "2019-09", "2020-12"]
        .into_iter()
        .find(|version| dialect.contains(version))
        .map(|version| format!("application/schema+json;version={version}"))
}
fn embedded_schema(schema: &Value, location: &[&str]) -> FrameworkResult<Value> {
    let scoped = forklaunch_core::codegen::bundle_schema_resources(schema)
        .map_err(|error| invalid(&format!("socket resource scope: {error}")))?;
    let bundled = forklaunch_core::contract::bundle_literal_schema_references_unscoped(&scoped)
        .map_err(|error| invalid(error.message()))?;
    let mut normalized = bundled.clone();
    let draft4 = schema
        .get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|dialect| dialect.contains("/draft-04/"));
    walk_schema(&mut normalized, &bundled, &mut |object| {
        if (draft4 && object.contains_key("id"))
            || object.get("$id").is_some_and(|id| id.as_str() != Some(""))
            || [
                "$anchor",
                "$dynamicAnchor",
                "$dynamicRef",
                "$recursiveRef",
                "$recursiveAnchor",
            ]
            .iter()
            .any(|key| object.contains_key(*key))
        {
            return Err(invalid(
                "socket schemas with resource IDs or anchors are not supported",
            ));
        }
        if let Some(reference) = object.get("$ref") {
            let reference = reference
                .as_str()
                .ok_or_else(|| invalid("schema reference must be a string"))?;
            let fragment = reference.strip_prefix('#').ok_or_else(|| {
                invalid("only document-local socket schema references are supported")
            })?;
            let pointer = percent_encoding::percent_decode_str(fragment)
                .decode_utf8()
                .map_err(|_| invalid("schema reference is not valid UTF-8"))?;
            if !pointer.is_empty() && !pointer.starts_with('/') {
                return Err(invalid("socket schema anchors are not supported"));
            }
            if bundled.pointer(&pointer).is_none() {
                return Err(invalid("socket schema reference cannot be resolved"));
            }
        }
        Ok(())
    })?;
    serde_json::from_str(&relocate_schema_json_unscoped(
        &normalized.to_string(),
        location,
    ))
    .map_err(|_| invalid("schema relocation failed"))
}
// Visit schema positions only. Enum/const/default/examples may contain literal
// objects with `$ref` properties, which must retain their application meaning.
fn walk_schema(
    value: &mut Value,
    reference_root: &Value,
    visit: &mut impl FnMut(&mut Map<String, Value>) -> FrameworkResult<()>,
) -> FrameworkResult<()> {
    let locations = forklaunch_core::contract::schema_locations(reference_root, false)
        .map_err(|error| invalid(error.message()))?;
    for location in locations {
        let node = value
            .pointer_mut(&location)
            .ok_or_else(|| invalid("schema location disappeared during relocation"))?;
        if let Some(allowed) = node.as_bool() {
            *node = if allowed {
                json!({})
            } else {
                json!({"not":{}})
            };
            continue;
        }
        let object = node
            .as_object_mut()
            .ok_or_else(|| invalid("socket schema must be an object or boolean"))?;
        visit(object)?;
        // The shared traversal identifies locations without imposing a schema
        // vocabulary on literals. Keep this exporter's strict container checks.
        for (keyword, child) in object {
            if forklaunch_core::contract::is_legacy_schema_annotation(reference_root, keyword) {
                continue;
            }
            match keyword.as_str() {
                "properties" | "patternProperties" | "dependentSchemas" | "$defs"
                | "definitions" | "dependencies"
                    if !child.is_object() =>
                {
                    return Err(invalid("schema map must be an object"));
                }
                "allOf" | "anyOf" | "oneOf" | "prefixItems" if !child.is_array() => {
                    return Err(invalid("schema alternatives must be an array"));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_export_rejects_encoded_pointer_separators() {
        for reference in [
            "#%2Fdefinitions/Value",
            "#%2fdefinitions/Value",
            "#/definitions%2FValue",
            "#/definitions%2fValue",
        ] {
            let shape = json!({"$ref": reference, "definitions": {"Value": {"type": "string"}}});
            let contract = SocketContract::new("/encoded")
                .with_client_message("request", SocketSchemaEntry::new(shape));
            let error =
                generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Encoded", "1"), &contract)
                    .unwrap_err();
            assert!(error.message().contains("literal JSON-pointer separators"));
        }
    }
    #[test]
    fn dictionary_targets_keep_contextual_assertions_after_export() {
        let shape = json!({"type":"object","required":["item","const"],"properties":{"item":{"$ref":"#/properties"},"const":{"$ref":"#/definitions/Value"}},"definitions":{"Value":{"type":"string"}}});
        let contract = SocketContract::new("/dictionary")
            .with_client_message("request", SocketSchemaEntry::new(shape.clone()));
        let document: Value = serde_json::from_str(
            &generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Dictionary", "1"), &contract)
                .unwrap(),
        )
        .unwrap();
        let payload = &document["channels"]["request"]["messages"]["request"]["payload"];
        let reference = payload["properties"]["item"]["$ref"].as_str().unwrap();
        let pointer = percent_encoding::percent_decode_str(&reference[1..])
            .decode_utf8()
            .unwrap();
        assert_eq!(
            document.pointer(&pointer).unwrap()["const"],
            shape["properties"]["const"]
        );
        if let Ok(output) = std::env::var("FORKLAUNCH_TEST_RAW_SOCKET_REFERENCES") {
            std::fs::write(
                format!("{output}.dictionary.json"),
                json!({"original":shape,"document":document}).to_string(),
            )
            .unwrap();
        }
    }

    #[test]
    fn references_into_literal_assertions_bundle_without_changing_payload_constraints() {
        let shape = json!({"type":"object","required":["item"],"properties":{"item":{"$ref":"#/definitions/Storage/const"}},"definitions":{"Storage":{"const":{"allOf":[{"$ref":"#/definitions/Value"}]}},"Value":{"type":"string","minLength":2}},"default":{"$ref":"literal default"}});
        let contract = SocketContract::new("/literal")
            .with_client_message("request", SocketSchemaEntry::new(shape.clone()));
        let document: Value = serde_json::from_str(
            &generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Literal", "1"), &contract)
                .unwrap(),
        )
        .unwrap();
        let payload = &document["channels"]["request"]["messages"]["request"]["payload"];
        assert_eq!(
            payload["definitions"]["Storage"],
            shape["definitions"]["Storage"]
        );
        assert_eq!(payload["default"], shape["default"]);
        let reference = payload["properties"]["item"]["$ref"].as_str().unwrap();
        let pointer = percent_encoding::percent_decode_str(&reference[1..])
            .decode_utf8()
            .unwrap();
        let nested = document.pointer(&pointer).unwrap()["allOf"][0]["$ref"]
            .as_str()
            .unwrap();
        let nested_pointer = percent_encoding::percent_decode_str(&nested[1..])
            .decode_utf8()
            .unwrap();
        assert_eq!(
            document.pointer(&nested_pointer),
            Some(&shape["definitions"]["Value"])
        );
        if let Ok(output) = std::env::var("FORKLAUNCH_TEST_RAW_SOCKET_REFERENCES") {
            std::fs::write(format!("{output}.literal.json"), document.to_string()).unwrap();
        }
    }
    #[test]
    fn schemas_referenced_from_ignored_annotations_relocate_their_own_references() {
        for (name, target) in [
            ("Value", "#/definitions/Value"),
            ("Value%25", "#/definitions/Value%2525"),
        ] {
            let shape = json!({"$schema":"http://json-schema.org/draft-06/schema#","type":"object","properties":{"item":{"$ref":"#/if"}},"definitions":{name:{"type":"string","minLength":2}},"if":{"type":"object","required":["value"],"properties":{"value":{"$ref":target}}},"then":{"$ref":"literal annotation"}});
            let contract = SocketContract::new("/legacy").with_client_message(
                "request",
                SocketSchemaEntry::new(shape.clone())
                    .with_channel("room/~ 水")
                    .with_operation("publish/~"),
            );
            let document: Value = serde_json::from_str(
                &generate_raw_socket_asyncapi_json(
                    &AsyncApiInfo::new("Referenced", "1"),
                    &contract,
                )
                .unwrap(),
            )
            .unwrap();
            let schema =
                &document["channels"]["room/~ 水"]["messages"]["publish/~"]["payload"]["schema"];
            let reference = schema["if"]["properties"]["value"]["$ref"]
                .as_str()
                .unwrap();
            let pointer = percent_encoding::percent_decode_str(&reference[1..])
                .decode_utf8()
                .unwrap();
            assert_eq!(
                document.pointer(&pointer),
                Some(&shape["definitions"][name])
            );
            assert_eq!(schema["then"], shape["then"]);
            if name == "Value"
                && let Ok(output) = std::env::var("FORKLAUNCH_TEST_RAW_SOCKET_REFERENCES")
            {
                std::fs::write(format!("{output}.annotation.json"), document.to_string()).unwrap();
            }
        }
    }
    #[test]
    fn ignored_legacy_annotations_are_not_schema_resources_or_references() {
        for dialect in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
        ] {
            let shape = json!({"$schema":dialect,"type":"string","if":{"$ref":"https://example.invalid/literal","$id":"annotation"},"then":false,"dependentSchemas":{"literal":42},"prefixItems":["literal"],"contentSchema":false});
            let contract = SocketContract::new("/legacy")
                .with_client_message("request", SocketSchemaEntry::new(shape.clone()));
            let document: Value = serde_json::from_str(
                &generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Legacy", "1"), &contract)
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                document["channels"]["request"]["messages"]["request"]["payload"]["schema"],
                shape
            );
        }
    }
    #[test]
    fn raw_socket_metadata_conformance() {
        let Ok(input) = std::env::var("FORKLAUNCH_TEST_RAW_SOCKET_INPUT") else {
            return;
        };
        let contracts: Vec<SocketContract> =
            serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        let output: Vec<_> = contracts.iter().map(|contract| match generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Fixture", "1.0.0"), contract) {
            Ok(document) => json!({"ok":true,"document":serde_json::from_str::<Value>(&document).unwrap()}),
            Err(error) => json!({"ok":false,"error":error.to_string()}),
        }).collect();
        std::fs::write(
            std::env::var("FORKLAUNCH_TEST_RAW_SOCKET_OUTPUT").unwrap(),
            serde_json::to_vec(&output).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn raw_socket_metadata_controls_and_options_preserve_typescript_semantics() {
        let contract = SocketContract::new("/chat")
            .with_client_message(
                "request",
                SocketSchemaEntry::new(json!({"type":"string"}))
                    .with_channels(["first", "second"])
                    .with_operations(["publish", "ignored"]),
            )
            .with_server_message(
                "reply",
                SocketSchemaEntry::new(json!(true))
                    .with_channel("first")
                    .with_operation("receive"),
            )
            .with_error("error", SocketSchemaEntry::new(json!({"type":"object"})));
        let options = SocketAsyncApiOptions {
            description: Some("description".into()),
            id: Some("urn:forklaunch:chat".into()),
            default_content_type: Some("application/json".into()),
        };
        let document: Value = serde_json::from_str(
            &generate_raw_socket_asyncapi_json_with_options(
                &AsyncApiInfo::new("Chat", "1.0.0"),
                &contract,
                &options,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            document["channels"]["first"]["messages"]["publish"]["payload"],
            json!({"type":"string"})
        );
        assert_eq!(
            document["channels"]["second"]["messages"]["publish"]["payload"],
            json!({"type":"string"})
        );
        assert_eq!(
            document["channels"]["first"]["messages"]["receive"]["payload"],
            json!({})
        );
        assert_eq!(document["operations"].as_object().unwrap().len(), 3);
        assert_eq!(document["info"]["description"], "description");
        assert_eq!(document["id"], "urn:forklaunch:chat");
        assert_eq!(document["defaultContentType"], "application/json");
    }
    #[test]
    fn declared_dialects_use_multi_format_schemas_and_rebase_inside_the_schema() {
        let shape = json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"object","properties":{"value":{"$ref":"#/definitions/Positive"}},"definitions":{"Positive":{"type":"number","minimum":0,"exclusiveMinimum":true}}});
        let contract = SocketContract::new("/legacy")
            .with_client_message("legacy", SocketSchemaEntry::new(shape));
        let document: Value = serde_json::from_str(
            &generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Legacy", "1"), &contract)
                .unwrap(),
        )
        .unwrap();
        let payload = &document["channels"]["legacy"]["messages"]["legacy"]["payload"];
        assert_eq!(
            payload["schemaFormat"],
            "application/schema+json;version=draft-04"
        );
        let reference = payload["schema"]["properties"]["value"]["$ref"]
            .as_str()
            .unwrap();
        assert_eq!(
            document
                .pointer(reference.strip_prefix('#').unwrap())
                .unwrap()["exclusiveMinimum"],
            true
        );
        let scoped=SocketContract::new("/legacy").with_client_message("legacy",SocketSchemaEntry::new(json!({"$schema":"http://json-schema.org/draft-04/schema#","id":"https://example.test/schema","type":"string"})));
        let document: Value = serde_json::from_str(
            &generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Legacy", "1"), &scoped).unwrap(),
        )
        .unwrap();
        let schema = &document["channels"]["legacy"]["messages"]["legacy"]["payload"]["schema"];
        assert_eq!(schema["type"], "string");
        assert!(schema.get("id").is_none());
    }

    #[test]
    fn raw_socket_schema_pointers_relocate_without_changing_literal_values() {
        let shape = json!({"type":"object","properties":{"next":{"$ref":"#"},"item":{"$ref":"#/$defs/hello%20%E6%B0%B4"}},"$defs":{"hello 水":{"type":"string"}},"const":{"$ref":"literal"},"examples":[{"$ref":"literal"}]});
        let contract = SocketContract::new("/chat").with_client_message(
            "request",
            SocketSchemaEntry::new(shape.clone())
                .with_channel("room/~ 水")
                .with_operation("publish/~"),
        );
        let document: Value = serde_json::from_str(
            &generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Chat", "1"), &contract).unwrap(),
        )
        .unwrap();
        let payload = &document["channels"]["room/~ 水"]["messages"]["publish/~"]["payload"];
        for field in ["next", "item"] {
            let reference = payload["properties"][field]["$ref"].as_str().unwrap();
            let pointer = percent_encoding::percent_decode_str(&reference[1..])
                .decode_utf8()
                .unwrap();
            assert!(document.pointer(&pointer).is_some(), "{reference}");
            assert!(!reference.contains('水'));
        }
        assert_eq!(payload["const"], shape["const"]);
        assert_eq!(payload["examples"], shape["examples"]);
        if let Ok(output) = std::env::var("FORKLAUNCH_TEST_RAW_SOCKET_REFERENCES") {
            std::fs::write(output, document.to_string()).unwrap();
        }
    }
    #[test]
    fn raw_socket_schema_errors_and_metadata_conflicts_fail_explicitly() {
        for shape in [
            json!(42),
            json!({"$ref":"#/missing"}),
            json!({"$ref":"https://example.com/schema"}),
            json!({"$id":"urn:example:resource","$ref":"https://example.com/missing"}),
            json!({"$anchor":"invalid anchor"}),
            json!({"$ref":"#anchor"}),
            json!({"anyOf":{}}),
            json!({"properties":{"invalid":42}}),
        ] {
            let contract = SocketContract::new("/chat")
                .with_client_message("request", SocketSchemaEntry::new(shape));
            assert!(
                generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Chat", "1"), &contract)
                    .is_err()
            );
        }
        let entry = SocketSchemaEntry::new(json!({"type":"string"}))
            .with_channel("shared")
            .with_operation("message");
        let conflict = SocketContract::new("/chat")
            .with_client_message("first", entry.clone())
            .with_server_message(
                "second",
                SocketSchemaEntry::new(json!({"type":"number"}))
                    .with_channel("shared")
                    .with_operation("message"),
            );
        assert!(
            generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Chat", "1"), &conflict).is_err()
        );
        let collision = SocketContract::new("/chat")
            .with_client_message(
                "first",
                entry.clone().with_channel("a-b").with_operation("c"),
            )
            .with_client_message("second", entry.with_channel("a").with_operation("b-c"));
        assert!(
            generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Chat", "1"), &collision).is_err()
        );
        for channel in ["", "#fragment", "path?query"] {
            let contract = SocketContract::new("/").with_client_message(
                "request",
                SocketSchemaEntry::new(json!(true)).with_channels([channel]),
            );
            assert!(
                generate_raw_socket_asyncapi_json(&AsyncApiInfo::new("Chat", "1"), &contract)
                    .is_err()
            );
        }
        let mut shape = json!(true);
        for _ in 0..66 {
            shape = json!({"properties":{"next":shape}});
        }
        assert!(
            generate_raw_socket_asyncapi_json(
                &AsyncApiInfo::new("Chat", "1"),
                &SocketContract::new("/")
                    .with_client_message("deep", SocketSchemaEntry::new(shape))
            )
            .is_err()
        );
    }
}

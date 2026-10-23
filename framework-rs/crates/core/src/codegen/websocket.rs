//! Checked raw socket payload plans shared by native client generators.
use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use forklaunch_core::ws::{SocketContract, SocketSchemaEntry, codec::PayloadCodec};
use serde_json::{Value, json};

use crate::codegen::{SchemaModels, compile_schema};

/// One socket payload direction, including its original runtime constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketPayloadPlan {
    pub schema: Value,
    pub binary: bool,
    pub json_encoding: bool,
    pub models: SchemaModels,
}

/// Directions are from the client's perspective; protocol controls stay shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketSdkPlan {
    pub path: String,
    pub incoming: SocketPayloadPlan,
    pub outgoing: SocketPayloadPlan,
    pub ping: SocketPayloadPlan,
    pub pong: SocketPayloadPlan,
    pub close: SocketPayloadPlan,
}

impl SocketSdkPlan {
    /// Compile standalone schemas into bounded models and checked runtime unions.
    /// Error/context metadata describe framework events, not wire messages.
    ///
    /// # Errors
    /// Rejects invalid paths, schemas, references, unsupported dialects and limits.
    pub fn from_contract(contract: &SocketContract) -> Result<Self> {
        if !contract.path.starts_with('/')
            || contract.path.contains(['?', '#'])
            || contract.path.chars().any(char::is_control)
        {
            bail!(
                "Socket SDK path must be an absolute path without query/fragment/control characters"
            );
        }
        if contract.close_reason.values().any(|entry| entry.binary) {
            bail!("WebSocket close reasons must be UTF-8, not raw binary");
        }
        Ok(Self {
            path: contract.path.clone(),
            incoming: collection("SocketIncoming", &contract.server_messages)?,
            outgoing: collection("SocketOutgoing", &contract.client_messages)?,
            ping: payload("SocketPing", contract.ping.as_ref())?,
            pong: payload("SocketPong", contract.pong.as_ref())?,
            close: collection("SocketClose", &contract.close_reason)?,
        })
    }
}

fn payload(name: &str, entry: Option<&SocketSchemaEntry>) -> Result<SocketPayloadPlan> {
    if let Some(entry) = entry.filter(|entry| entry.binary) {
        if !entry.valid_binary_shape() {
            bail!("Binary socket entries require the canonical binary shape");
        }
        return Ok(SocketPayloadPlan {
            schema: entry.shape.clone(),
            binary: true,
            json_encoding: false,
            models: SchemaModels {
                rust_models: Vec::new(),
                rust_aliases: vec![crate::codegen::RustTypeAliasTemplate {
                    name: name.into(),
                    target_type: "Vec<u8>".into(),
                }],
                rust_type: name.into(),
                typescript_type: "Uint8Array".into(),
                typescript_definitions: BTreeMap::new(),
            },
        });
    }
    let schema = entry.map(|entry| &entry.shape);
    let empty = json!({});
    let schema = schema.unwrap_or(&empty);
    // Validate constraints as well as the type graph before emitting source.
    PayloadCodec::<Value>::json_schema(schema).context("Invalid socket payload schema")?;
    let effective = crate::codegen::schema::standalone_runtime_schema(
        schema,
        &mut crate::codegen::schema::ExpansionBudget::default(),
    )?;
    let models = compile_schema(name, &effective)?;
    // Resolve standalone resource scopes before union relocation so separate
    // messages may safely reuse IDs and dynamic anchor names.
    let schema = crate::codegen::schema::scoped_runtime_schema(schema)?;
    PayloadCodec::<Value>::json_schema(&schema).context("Invalid bundled socket payload schema")?;
    Ok(SocketPayloadPlan {
        schema,
        binary: false,
        json_encoding: entry.is_some_and(|entry| entry.json_encoding),
        models,
    })
}

fn collection(
    name: &str,
    entries: &BTreeMap<String, SocketSchemaEntry>,
) -> Result<SocketPayloadPlan> {
    if entries.keys().any(String::is_empty) {
        bail!("Socket SDK message names must not be empty");
    }
    if entries.len() <= 1 {
        return payload(name, entries.values().next());
    }
    if entries.values().any(|entry| entry.binary) {
        bail!("Binary socket directions require exactly one explicit message entry");
    }
    let json_encoding = entries.values().next().unwrap().json_encoding;
    if entries
        .values()
        .any(|entry| entry.json_encoding != json_encoding)
    {
        bail!("Socket message unions must use one wire encoding");
    }
    if entries.len() > 1024 {
        bail!("Socket message union exceeds 1024 alternatives");
    }
    // Bound the complete union before any schema cloning or compilation.
    let mut remaining = 100_000_usize;
    let mut pending: Vec<_> = entries.values().map(|entry| (&entry.shape, 0)).collect();
    while let Some((value, depth)) = pending.pop() {
        if depth > 60 || remaining == 0 {
            bail!("Socket message union exceeds schema depth/node limits");
        }
        remaining -= 1;
        let children = match value {
            Value::Object(values) => values.len(),
            Value::Array(values) => values.len(),
            _ => 0,
        };
        if children > remaining.saturating_sub(pending.len()) {
            bail!("Socket message union exceeds schema node limits");
        }
        match value {
            Value::Object(values) => {
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, depth + 1))),
            _ => {}
        }
        if pending.len() > remaining {
            bail!("Socket message union exceeds schema node limits");
        }
    }
    let mut bytes = UnionBytes(32 * 1024 * 1024);
    for entry in entries.values() {
        serde_json::to_writer(&mut bytes, &entry.shape)
            .context("Socket message union exceeds 32 MiB")?;
    }
    let mut checked = Vec::with_capacity(entries.len());
    let mut dialects = std::collections::BTreeSet::new();
    for (index, entry) in entries.values().enumerate() {
        let plan = payload(&format!("{name}Variant{index}"), Some(entry))?;
        dialects.insert(
            plan.schema
                .get("$schema")
                .and_then(Value::as_str)
                .unwrap_or("http://json-schema.org/draft-07/schema#")
                .to_owned(),
        );
        checked.push(plan.schema);
    }
    let mixed = dialects.len() > 1;
    let dialect = if mixed {
        "https://json-schema.org/draft/2020-12/schema"
    } else {
        dialects.first().unwrap().as_str()
    };
    let mut schemas = Vec::with_capacity(checked.len());
    for (index, schema) in checked.into_iter().enumerate() {
        let schema = if mixed {
            crate::codegen::schema::runtime_schema_2020(&schema)?
        } else {
            schema
        };
        let relocated = forklaunch_core::contract::relocate_schema_json_unscoped(
            &schema.to_string(),
            &["anyOf", &index.to_string()],
        );
        schemas.push(serde_json::from_str::<Value>(&relocated)?);
    }
    let schema = json!({"$schema": dialect, "anyOf": schemas});
    let mut entry = SocketSchemaEntry::new(schema);
    entry.json_encoding = json_encoding;
    payload(name, Some(&entry))
}

struct UnionBytes(usize);
impl std::io::Write for UnionBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("socket union byte limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_unions_isolate_duplicate_ids_and_recursive_bindings() {
        for (dialect, anchor, reference) in [
            (
                "https://json-schema.org/draft/2020-12/schema",
                "$dynamicAnchor",
                "$dynamicRef",
            ),
            (
                "https://json-schema.org/draft/2019-09/schema",
                "$recursiveAnchor",
                "$recursiveRef",
            ),
            ("http://json-schema.org/draft-07/schema#", "", "$ref"),
        ] {
            let mut contract = SocketContract::new("/scopes");
            for (name, kind) in [("text", "string"), ("count", "integer")] {
                let mut schema = json!({"$schema":dialect, "$id":"https://schemas.test/shared", "type":"object", "additionalProperties":false,
                    "required":["value"], "properties":{"value":{"type":kind}, "next":{reference:if reference == "$dynamicRef" {"#node"} else {"#"}}},
                    "examples":[{"$id":"literal", "$ref":"literal"}]});
                if anchor == "$dynamicAnchor" {
                    schema[anchor] = json!("node");
                }
                if anchor == "$recursiveAnchor" {
                    schema[anchor] = json!(true);
                }
                contract = contract.with_server_message(name, SocketSchemaEntry::json(schema));
            }
            let plan = SocketSdkPlan::from_contract(&contract).unwrap();
            let codec = PayloadCodec::<Value>::json_schema(&plan.incoming.schema).unwrap();
            for value in [
                json!({"value":"a", "next":{"value":"b"}}),
                json!({"value":1, "next":{"value":2}}),
            ] {
                assert!(codec.decode(&serde_json::to_vec(&value).unwrap()).is_ok());
            }
            for value in [
                json!({"value":"a", "next":{"value":2}}),
                json!({"value":1, "next":{"value":"b"}}),
                json!({"next":{}}),
            ] {
                assert!(codec.decode(&serde_json::to_vec(&value).unwrap()).is_err());
            }
            assert!(plan.incoming.schema["anyOf"][0].get("$id").is_none());
            assert_eq!(
                plan.incoming.schema["anyOf"][0]["examples"][0]["$ref"],
                "literal"
            );
        }
    }

    #[test]
    fn json_encoding_is_preserved_and_mixed_encodings_are_rejected() {
        let mut contract = SocketContract::new("/json")
            .with_client_message("string", SocketSchemaEntry::json(json!({"type":"string"})))
            .with_client_message("null", SocketSchemaEntry::json(json!({"type":"null"})));
        assert!(
            SocketSdkPlan::from_contract(&contract)
                .unwrap()
                .outgoing
                .json_encoding
        );
        contract
            .client_messages
            .get_mut("string")
            .unwrap()
            .json_encoding = false;
        assert!(SocketSdkPlan::from_contract(&contract).is_err());
        let mut entry = SocketSchemaEntry::binary();
        entry.json_encoding = true;
        assert!(!entry.valid_binary_shape());
    }

    #[test]
    fn binary_metadata_is_explicit_and_rejects_ambiguous_constraints() {
        let mut contract = SocketContract::new("/bytes")
            .with_client_message("bytes", SocketSchemaEntry::binary())
            .with_server_message("bytes", SocketSchemaEntry::binary());
        contract.ping = Some(SocketSchemaEntry::binary());
        contract.pong = Some(SocketSchemaEntry::binary());
        let encoded = serde_json::to_vec(&contract).unwrap();
        let decoded: SocketContract = serde_json::from_slice(&encoded).unwrap();
        let plan = SocketSdkPlan::from_contract(&decoded).unwrap();
        assert!(
            plan.incoming.binary && plan.outgoing.binary && plan.ping.binary && plan.pong.binary
        );
        assert_eq!(plan.incoming.models.typescript_type, "Uint8Array");
        assert_eq!(plan.outgoing.models.rust_aliases[0].target_type, "Vec<u8>");
        let mut invalid = contract.clone();
        invalid.client_messages.get_mut("bytes").unwrap().shape["minLength"] = json!(3);
        assert!(SocketSdkPlan::from_contract(&invalid).is_err());
        let mut invalid = contract.clone();
        invalid
            .close_reason
            .insert("bytes".into(), SocketSchemaEntry::binary());
        assert!(SocketSdkPlan::from_contract(&invalid).is_err());
        let invalid =
            contract.with_client_message("text", SocketSchemaEntry::new(json!({"type":"string"})));
        assert!(SocketSdkPlan::from_contract(&invalid).is_err());
        let contract = SocketContract::new("/text").with_client_message(
            "text",
            SocketSchemaEntry::new(json!({"type":"string","format":"binary"})),
        );
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        assert!(!plan.outgoing.binary);
        assert_eq!(plan.outgoing.models.typescript_type, "string");
    }

    #[test]
    fn dictionary_aliases_keep_assertions_inside_relocated_message_unions() {
        let shape = json!({"type":"object","required":["item","const"],"properties":{"item":{"$ref":"#/properties"},"const":{"$ref":"#/definitions/Value"}},"definitions":{"Value":{"type":"string"}}});
        let contract = SocketContract::new("/dictionary")
            .with_server_message("dictionary", SocketSchemaEntry::new(shape))
            .with_server_message("number", SocketSchemaEntry::new(json!({"type":"integer"})));
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        let codec = PayloadCodec::<Value>::json_schema(&plan.incoming.schema).unwrap();
        assert!(
            codec
                .decode(br##"{"item":{"$ref":"#/definitions/Value"},"const":"ok"}"##)
                .is_ok()
        );
        assert!(codec.decode(b"42").is_ok());
        for invalid in [
            br##"{"item":{"$ref":"#/definitions/Value"},"const":42}"##.as_slice(),
            br#"{"item":"wrong","const":"ok"}"#,
        ] {
            assert!(codec.decode(invalid).is_err());
        }
    }

    #[test]
    fn client_directions_rebase_standalone_recursive_alternatives_and_keep_literal_data() {
        let tree = json!({"$ref":"#/$defs/Node", "$defs":{"Node":{
            "type":"object", "required":["value"], "properties":{
                "value":{"type":"integer"}, "next":{"$ref":"#/$defs/Node"}},
            "examples":[{"$ref":"literal"}]}}});
        let contract = SocketContract::new("/chat")
            .with_server_message("tree", SocketSchemaEntry::new(tree))
            .with_server_message(
                "text",
                SocketSchemaEntry::new(json!({"type":"string","minLength":1})),
            )
            .with_client_message(
                "count",
                SocketSchemaEntry::new(json!({"type":"integer","minimum":0})),
            );
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        let incoming = PayloadCodec::<Value>::json_schema(&plan.incoming.schema).unwrap();
        assert!(
            incoming
                .decode(br#"{"value":1,"next":{"value":2}}"#)
                .is_ok()
        );
        assert!(
            incoming
                .decode(br#"{"value":1,"next":{"value":"bad"}}"#)
                .is_err()
        );
        assert!(incoming.decode(b"hello").is_ok());
        assert!(incoming.decode(b"123").is_err());
        assert_eq!(
            plan.incoming.schema["anyOf"][1]["$ref"],
            "#/anyOf/1/$defs/Node"
        );
        assert_eq!(
            plan.incoming.schema["anyOf"][1]["$defs"]["Node"]["examples"][0]["$ref"],
            "literal"
        );
        assert_eq!(plan.outgoing.models.rust_type, "SocketOutgoing");
        assert_eq!(plan.outgoing.models.rust_aliases[0].target_type, "i64");
        assert!(!plan.incoming.models.rust_models.is_empty());
    }

    #[test]
    fn draft_four_models_ignore_later_keywords_and_accept_legacy_resource_ids() {
        let schema = json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"object","required":["value"],"properties":{"value":{"type":"string","const":"ignored"}},"const":{"literal":"annotation"}});
        let contract = SocketContract::new("/legacy")
            .with_server_message("legacy", SocketSchemaEntry::new(schema.clone()));
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        assert!(
            plan.incoming
                .models
                .typescript_type
                .contains("value: string")
        );
        assert_eq!(plan.incoming.schema, schema);
        for schema in [
            json!({"$schema":"http://json-schema.org/draft-04/schema#","id":"https://example.test/root","type":"string"}),
            json!({"$schema":"http://json-schema.org/draft-04/schema#","type":"object","properties":{"value":{"id":"nested","type":"string"}}}),
        ] {
            let contract = SocketContract::new("/legacy")
                .with_server_message("legacy", SocketSchemaEntry::new(schema));
            assert!(SocketSdkPlan::from_contract(&contract).is_ok());
        }
    }

    #[test]
    fn older_dialect_annotations_do_not_enter_the_model_schema_graph() {
        for dialect in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
        ] {
            let schema = json!({"$schema":dialect,"type":"string","if":42,"then":false,"else":true,"dependentSchemas":{"literal":42},"prefixItems":["literal"],"contentSchema":false});
            let contract = SocketContract::new("/legacy")
                .with_server_message("reply", SocketSchemaEntry::new(schema.clone()));
            let plan = SocketSdkPlan::from_contract(&contract).unwrap();
            assert_eq!(plan.incoming.schema, schema);
            assert_eq!(plan.incoming.models.typescript_type, "string");
            assert_eq!(
                plan.incoming.models.rust_aliases[0].target_type,
                "::std::string::String"
            );
        }
    }

    #[test]
    fn ignored_legacy_resource_ids_do_not_become_active_schema_resources() {
        for dialect in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
        ] {
            let identity = json!({"id":"https://example.invalid/ignored","$id":"https://example.invalid/ignored"});
            let schema = json!({"$schema":dialect,"type":"string","if":identity,"then":identity});
            let contract = SocketContract::new("/legacy")
                .with_server_message("reply", SocketSchemaEntry::new(schema));
            let plan = SocketSdkPlan::from_contract(&contract).unwrap();
            let codec = PayloadCodec::<Value>::json_schema(&plan.incoming.schema).unwrap();
            assert_eq!(codec.decode(b"value").unwrap(), json!("value"));
        }
    }

    #[test]
    fn malformed_constraints_and_scopes_fail_before_generation() {
        for schema in [
            json!({"type":"string","minLength":-1}),
            json!({"$ref":"#/missing"}),
            json!({"$id":"https://example.test/schema","$ref":"unbundled"}),
        ] {
            let contract = SocketContract::new("/chat")
                .with_client_message("input", SocketSchemaEntry::new(schema));
            assert!(SocketSdkPlan::from_contract(&contract).is_err());
        }
        let contract = SocketContract::new("/chat")
            .with_client_message("draft7", SocketSchemaEntry::new(json!({"type":"string"})))
            .with_client_message("draft2020", SocketSchemaEntry::new(json!({"$schema":"https://json-schema.org/draft/2020-12/schema","type":"integer"})));
        let plan = SocketSdkPlan::from_contract(&contract).unwrap();
        let codec = PayloadCodec::<Value>::json_schema(&plan.outgoing.schema).unwrap();
        assert!(codec.decode(br#""hello""#).is_ok());
        assert!(codec.decode(b"1").is_ok());
        assert!(codec.decode(b"true").is_err());
    }
}

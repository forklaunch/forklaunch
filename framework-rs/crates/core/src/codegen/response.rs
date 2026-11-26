//! Shared response media classification and typed payload compilation.
use anyhow::{Context, Result, bail};
use forklaunch_core::contract::{HttpMethod, SchemaRef};
use serde_json::json;

use crate::codegen::{
    BodyKind, SchemaModels, SdkResponse, compile_schema_ref, render::SdkResponseVariant,
};

/// Compile a response into the same model and decoder used by generated CLI SDKs.
///
/// # Errors
/// Rejects unsupported media, invalid status codes and invalid schema graphs.
#[allow(clippy::too_many_lines)] // Keep body/header schema model compilation together.
pub fn compile_response(
    method: HttpMethod,
    response: &SdkResponse,
    root_name: &str,
) -> Result<(SdkResponseVariant, SchemaModels)> {
    if !(200..=599).contains(&response.status_code) {
        bail!("Invalid SDK response status code");
    }
    let empty = method == HttpMethod::Head
        || matches!(response.status_code, 204 | 205)
        || response.body.is_none();
    let kind = response.body_kind.unwrap_or(BodyKind::Json);
    if response
        .body_json_schema
        .as_deref()
        .is_some_and(|schema| schema.contains("x-forklaunch-date-codec"))
        && !matches!(kind, BodyKind::Json | BodyKind::ServerSentEvent)
    {
        bail!("Date response codecs currently require JSON or SSE payloads");
    }
    let (decoder, default_mime) = match kind {
        _ if response.body.is_none() => ("empty", "application/json"),
        BodyKind::Json => ("json", "application/json"),
        BodyKind::Text => ("text", "text/plain"),
        BodyKind::Binary | BodyKind::File => ("binary", "application/octet-stream"),
        BodyKind::Multipart => ("multipart", "multipart/form-data"),
        BodyKind::ServerSentEvent => ("sse", "text/event-stream"),
        _ => bail!(
            "Unsupported SDK response body kind {}",
            kind.as_artifact_str()
        ),
    };
    let content_type = normalized_media_type(
        response
            .body_content_type
            .as_deref()
            .unwrap_or(default_mime),
    )?;
    if (content_type == "text/event-stream") != (decoder == "sse") {
        bail!("SSE SDK responses require the serverSentEvent body kind and text/event-stream");
    }
    if (decoder == "multipart" && content_type != "multipart/form-data")
        || (content_type.starts_with("multipart/") && decoder != "multipart")
    {
        bail!("Multipart SDK responses require multipart body kind and multipart/form-data");
    }
    let decoder = if empty { "empty" } else { decoder };
    let content_type = if empty { String::new() } else { content_type };
    let schema = match decoder {
        "empty" => SchemaRef::named("Null").with_json_schema(json!({"type":"null"}).to_string()),
        "text" | "binary" | "multipart" => SchemaRef::named("String"),
        _ => SchemaRef {
            name: response.body.clone().unwrap_or_default(),
            json_schema: response.body_json_schema.clone(),
        },
    };
    let payload_schema = schema.clone();
    let schema = if decoder == "sse" {
        forklaunch_core::contract::sse_event_schema(&schema)
    } else {
        schema
    };
    let mut models = compile_schema_ref(root_name, &schema)?;
    if decoder == "sse" {
        models = super::sse_models::reuse(root_name, &payload_schema, models)?;
    }
    if matches!(decoder, "binary" | "multipart") {
        for alias in &mut models.rust_aliases {
            if alias.name == models.rust_type {
                alias.target_type = if decoder == "multipart" {
                    "MultipartResponse"
                } else {
                    "Vec<u8>"
                }
                .into();
            }
        }
    }
    let rust_response_type = match decoder {
        "sse" => format!("Box<SdkEventStream<{}>>", models.rust_type),
        "json" => json_response_type(
            &models.rust_type,
            !models.rust_models.is_empty(),
            models
                .rust_aliases
                .iter()
                .map(|alias| (alias.name.as_str(), alias.target_type.as_str())),
        ),
        _ => models.rust_type.clone(),
    };
    let response_body_type = match decoder {
        "empty" => "undefined".into(),
        "binary" => "Uint8Array".into(),
        "multipart" => "FormData".into(),
        "sse" => format!("AsyncIterable<{}>", models.typescript_type),
        _ => models.typescript_type.clone(),
    };
    let mut headers_type = String::new();
    let mut typescript_headers_type = String::new();
    let mut header_fields = Vec::new();
    let mut rules = serde_json::Map::new();
    if !response.headers.is_empty() {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for header in &response.headers {
            if header
                .json_schema
                .as_deref()
                .is_some_and(|schema| schema.contains("x-forklaunch-date-codec"))
            {
                anyhow::bail!(
                    "Date response headers are not yet supported; date codecs require JSON bodies or SSE payloads"
                );
            }
            let raw = header
                .json_schema
                .clone()
                .unwrap_or_else(|| json!({"type":"string"}).to_string());
            let schema: serde_json::Value = serde_json::from_str(&raw)?;
            rules.insert(
                header.name.clone(),
                json!({"required":header.required,"schema":schema}),
            );
            let relocated = forklaunch_core::contract::relocate_schema_json_unscoped(
                &raw,
                &["properties", &header.name],
            );
            properties.insert(header.name.clone(), serde_json::from_str(&relocated)?);
            if header.required {
                required.push(header.name.clone());
            }
        }
        let headers = crate::codegen::compile_schema(
            &format!("{root_name}Headers"),
            &json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
        )?;
        headers_type.clone_from(&headers.rust_type);
        typescript_headers_type.clone_from(&headers.typescript_type);
        header_fields.clone_from(
            &headers
                .rust_models
                .iter()
                .find(|model| model.name == headers_type)
                .ok_or_else(|| anyhow::anyhow!("Response headers require an object model"))?
                .fields,
        );
        models.rust_models.extend(headers.rust_models);
        models.rust_aliases.extend(headers.rust_aliases);
        models
            .typescript_definitions
            .extend(headers.typescript_definitions);
    }
    let body_schema = (if decoder == "sse" {
        schema.json_schema.clone()
    } else {
        response.body_json_schema.clone()
    })
    .unwrap_or_else(|| {
        match crate::codegen::models::ts_type_from_schema_name(
            response.body.as_deref().unwrap_or("Null"),
        )
        .as_str()
        {
            "string" => json!({"type":"string"}),
            "number" => json!({"type":"number"}),
            "boolean" => json!({"type":"boolean"}),
            _ => json!({}),
        }
        .to_string()
    });
    // Artifact parsing sorts object keys; native schemas must emit identical bytes.
    let body_schema = super::canonical_json(serde_json::from_str(&body_schema)?).to_string();
    Ok((
        SdkResponseVariant {
            shared_response_decoders: false,
            media_discriminator: false,
            headers_type,
            typescript_headers_type,
            header_fields,
            header_rules: super::canonical_json(serde_json::Value::Object(rules.clone()))
                .to_string(),
            body_schema: crate::codegen::escape_typescript_string(&body_schema),
            body_schema_json: body_schema.clone(),
            rust_body_schema: format!("{body_schema:?}"),
            rust_header_rules: format!(
                "{:?}",
                super::canonical_json(serde_json::Value::Object(rules.clone())).to_string()
            ),
            response_type: root_name.into(),
            response_body_type,
            rust_response_type,
            status_code: response.status_code,
            status_variant: format!("Status{}", response.status_code),
            empty_response: empty,
            json_response: decoder == "json",
            text_response: decoder == "text",
            binary_response: decoder == "binary",
            multipart_response: decoder == "multipart",
            streaming_response: decoder == "sse",
            response_kind: decoder.into(),
            content_type,
        },
        models,
    ))
}

/// Choose a status-enum payload type from already compiled Rust model metadata.
/// Collections and boxed schema references already allocate their payloads;
/// inline records and unions are boxed to bound enum size. This does not parse
/// or validate schema annotations.
#[must_use]
pub fn json_response_type<'a>(
    rust_type: &'a str,
    has_models: bool,
    aliases: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> String {
    let aliases: Vec<_> = aliases.into_iter().collect();
    let mut payload = rust_type;
    for _ in 0..aliases.len() {
        let Some((_, target)) = aliases.iter().find(|(name, _)| *name == payload) else {
            break;
        };
        payload = target;
    }
    let allocated = [
        "Box<",
        "::std::boxed::Box<",
        "Vec<",
        "::std::vec::Vec<",
        "BTreeMap<",
        "::std::collections::BTreeMap<",
        "HashMap<",
        "::std::collections::HashMap<",
    ]
    .iter()
    .any(|prefix| payload.starts_with(prefix));
    if has_models && !allocated {
        format!("Box<{rust_type}>")
    } else {
        rust_type.into()
    }
}

pub use crate::codegen::media::normalized_media_type;

/// Normalize a schema type name for Rust declarations.
#[must_use]
pub fn type_identifier(name: &str) -> String {
    crate::codegen::models::rust_type_name(name)
}

/// Normalize a Rust method/field identifier, including reserved keywords.
#[must_use]
pub fn field_identifier(name: &str) -> String {
    crate::codegen::models::rust_field_ident(name)
}

/// Validate an explicitly supplied Rust client identifier.
///
/// # Errors
/// Rejects punctuation, leading digits and reserved Rust keywords.
pub fn validate_identifier(name: &str) -> Result<()> {
    let mut chars = name.chars();
    let first = chars.next().context("Rust client identifier is empty")?;
    if !(first.is_ascii_alphabetic() || first == '_')
        || name == "_"
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        || crate::codegen::models::is_rust_keyword(name)
    {
        bail!("Invalid Rust client identifier {name}");
    }
    Ok(())
}

/// Canonical wire discriminator, including empty responses without Content-Type.
/// # Errors
/// Rejects malformed media declarations.
pub fn response_identity(response: &SdkResponse) -> Result<(u16, String)> {
    let default = match response.body_kind.unwrap_or(BodyKind::Json) {
        BodyKind::Text => "text/plain",
        BodyKind::Binary | BodyKind::File => "application/octet-stream",
        BodyKind::Multipart => "multipart/form-data",
        BodyKind::ServerSentEvent => "text/event-stream",
        _ => "application/json",
    };
    Ok((
        response.status_code,
        if response.body.is_none() {
            String::new()
        } else {
            normalized_media_type(response.body_content_type.as_deref().unwrap_or(default))?
        },
    ))
}

/// Stable readable suffix for a content-type-specific response variant.
#[must_use]
pub fn response_media_name(media: &str) -> String {
    if media.is_empty() {
        return "Empty".into();
    }
    media
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(type_identifier)
        .collect()
}

#[cfg(test)]
mod response_box_tests {
    use super::*;

    fn response(schema: &serde_json::Value) -> (SdkResponseVariant, SchemaModels) {
        compile_response(
            HttpMethod::Get,
            &SdkResponse {
                status_code: 200,
                body: Some("Response".into()),
                body_json_schema: Some(schema.to_string()),
                body_kind: Some(BodyKind::Json),
                body_content_type: None,
                headers: Vec::new(),
            },
            "Response",
        )
        .unwrap()
    }

    #[test]
    fn recursive_and_record_collections_avoid_redundant_response_allocations() {
        let record = json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}},"additionalProperties":false});
        for schema in [
            json!({"type":"array","items":{"$ref":"#"}}),
            json!({"type":"array","items":record}),
            json!({"type":"object","additionalProperties":record}),
        ] {
            let (variant, models) = response(&schema);
            assert!(
                !models.rust_models.is_empty(),
                "fixture must include nested models"
            );
            assert_eq!(variant.rust_response_type, models.rust_type);
        }
    }

    #[test]
    fn inline_records_nullable_records_and_unions_remain_boxed() {
        let record = json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}},"additionalProperties":false});
        for schema in [
            record.clone(),
            json!({"anyOf":[record.clone(),{"type":"null"}]}),
            json!({"oneOf":[record,{"type":"boolean"}]}),
        ] {
            let (variant, models) = response(&schema);
            assert_eq!(
                variant.rust_response_type,
                format!("Box<{}>", models.rust_type)
            );
        }
    }

    #[test]
    fn referenced_recursive_response_with_siblings_keeps_one_box() {
        let schema = json!({
            "$id": "https://schemas.fixture.test/Mutual/index.json",
            "$ref": "nodes/a#MutualA",
            "minProperties": 1,
            "$defs": {
                "A": {
                    "$id": "nodes/a", "$anchor": "MutualA",
                    "type": "object", "additionalProperties": false,
                    "required": ["id", "peer"],
                    "properties": {
                        "id": {"type": "string"},
                        "peer": {"$ref": "b#MutualB"}
                    }
                },
                "B": {
                    "$id": "nodes/b", "$anchor": "MutualB",
                    "type": "object", "additionalProperties": false,
                    "required": ["children"],
                    "properties": {
                        "children": {"type": "array", "items": {"$ref": "a#MutualA"}}
                    }
                }
            }
        });
        let (variant, models) = response(&schema);
        let payload = models
            .rust_aliases
            .iter()
            .find(|alias| alias.name == models.rust_type)
            .expect("reference response retains its named alias");
        assert!(
            payload.target_type.starts_with("::std::boxed::Box<"),
            "{}",
            payload.target_type
        );
        assert!(!models.rust_models.is_empty());
        assert_eq!(variant.rust_response_type, models.rust_type);
        assert_eq!(payload.target_type.matches("Box<").count(), 1);
        assert_eq!(
            variant.body_schema_json,
            super::super::canonical_json(schema).to_string()
        );
    }
}

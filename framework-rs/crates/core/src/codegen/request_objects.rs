//! Whole-object request predicates retain field decoding and original header names.
use super::SdkOperation;
use anyhow::{Context, Result};
use serde_json::{Value, json};
pub(crate) fn metadata(operation: &SdkOperation) -> Result<String> {
    if !operation.version_branches.is_empty() {
        return Ok(String::new());
    }
    let mut groups = serde_json::Map::new();
    for (name, source, fields) in [
        (
            "query",
            &operation.request_query_schema,
            &operation.request_query,
        ),
        (
            "headers",
            &operation.request_headers_schema,
            &operation.request_headers,
        ),
    ] {
        let Some(source) = source else { continue };
        let schema: Value = serde_json::from_str(source)?;
        anyhow::ensure!(
            schema.get("type").and_then(Value::as_str) == Some("object"),
            "Request group schema must be an object"
        );
        // The lightweight runtime receives resolved graphs from Rust emission;
        // admission must validate that same graph before emitting either SDK.
        // Preserve the original metadata for the TypeScript AJV compiler.
        let checked = super::schema::bundle_document_resources(&schema, "")?;
        crate::contract::schema_validation::compile_checked(checked.as_ref().unwrap_or(&schema))
            .map_err(anyhow::Error::msg)?;
        let rules = fields
            .iter()
            .map(|field| {
                let schema: Value = serde_json::from_str(
                    field
                        .json_schema
                        .as_deref()
                        .context("Request object fields require schemas")?,
                )?;
                Ok((
                    field.name.clone(),
                    json!({"required":field.required,"schema":schema}),
                ))
            })
            .collect::<Result<serde_json::Map<_, _>>>()?;
        groups.insert(name.into(), json!({"schema":schema,"fields":rules}));
    }
    Ok(if groups.is_empty() {
        String::new()
    } else {
        super::canonical_json(Value::Object(groups)).to_string()
    })
}

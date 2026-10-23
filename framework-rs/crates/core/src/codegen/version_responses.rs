//! Portable matched-version response metadata; static unions never replace runtime admission.
use super::{BodyKind, SdkField, SdkGenerationPlan, SdkOperation, SdkResponse};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn prepare(plan: &SdkGenerationPlan) -> Result<SdkGenerationPlan> {
    let mut plan = plan.clone();
    for operation in &mut plan.operations {
        if operation.version_branches.is_empty() {
            continue;
        }
        metadata(operation)?;
        let mut grouped = BTreeMap::<(u16, String), Vec<SdkResponse>>::new();
        for branch in &operation.version_branches {
            for response in &branch.responses {
                grouped
                    .entry((response.status_code, media(response)?))
                    .or_default()
                    .push(response.clone());
            }
        }
        operation.responses = grouped
            .into_values()
            .map(|responses| {
                let mut response = responses[0].clone();
                if responses.iter().any(|other| {
                    other.body_kind != response.body_kind
                        || other.body.is_some() != response.body.is_some()
                }) {
                    bail!("Version response codecs differ for the same status/media");
                }
                if response.body.is_some() {
                    response.body_json_schema = Some(
                        union(
                            responses
                                .iter()
                                .map(response_schema)
                                .collect::<Result<Vec<_>>>()?,
                        )?
                        .to_string(),
                    );
                }
                let names: BTreeSet<_> = responses
                    .iter()
                    .flat_map(|response| response.headers.iter().map(|header| header.name.clone()))
                    .collect();
                response.headers = names
                    .into_iter()
                    .map(|name| {
                        let fields: Vec<_> = responses
                            .iter()
                            .filter_map(|response| {
                                response.headers.iter().find(|header| header.name == name)
                            })
                            .collect();
                        let mut field = fields[0].clone();
                        field.required = fields.len() == responses.len()
                            && fields.iter().all(|field| field.required);
                        field.json_schema = Some(
                            union(
                                fields
                                    .iter()
                                    .map(|field| schema(field.json_schema.as_deref()))
                                    .collect::<Result<Vec<_>>>()?,
                            )?
                            .to_string(),
                        );
                        Ok(field)
                    })
                    .collect::<Result<_>>()?;
                Ok(response)
            })
            .collect::<Result<_>>()?;
        operation.response_status_codes = operation
            .responses
            .iter()
            .map(|response| response.status_code)
            .collect();
    }
    Ok(plan)
}
fn schema(source: Option<&str>) -> Result<Value> {
    serde_json::from_str(source.context("Version predicates require explicit schemas")?)
        .map_err(Into::into)
}
fn fields(values: &[SdkField]) -> Result<Value> {
    values
        .iter()
        .map(|field| {
            Ok((
                field.name.clone(),
                json!({"required":field.required,"schema":schema(field.json_schema.as_deref())?}),
            ))
        })
        .collect::<Result<serde_json::Map<_, _>>>()
        .map(Value::Object)
}
fn headers(values: &[SdkField]) -> Result<Value> {
    let mut values = values.to_vec();
    for field in &mut values {
        field.name = field.name.to_ascii_lowercase();
    }
    fields(&values)
}
fn media(response: &SdkResponse) -> Result<String> {
    if response.body.is_none() || matches!(response.status_code, 204 | 205) {
        return Ok(String::new());
    }
    super::media::normalized_media_type(response.body_content_type.as_deref().unwrap_or(
        match response.body_kind {
            Some(BodyKind::Text) => "text/plain",
            Some(BodyKind::Binary | BodyKind::File) => "application/octet-stream",
            Some(BodyKind::ServerSentEvent) => "text/event-stream",
            _ => "application/json",
        },
    ))
}
pub(crate) fn metadata(operation: &SdkOperation) -> Result<String> {
    if operation.version_branches.is_empty() {
        return Ok(String::new());
    }
    if operation.version_branches.len() > 64 {
        bail!("Portable version matching supports at most 64 branches");
    }
    let branches=operation.version_branches.iter().map(|branch|{
        let options: crate::contract::BodyParserOptions = branch.request_parser_options.as_deref()
            .map(serde_json::from_str).transpose()?.unwrap_or_default();
        options.validate_portable().map_err(anyhow::Error::msg)?;
        let parser_options: Value = serde_json::from_str(&options.to_json())?;
        if branch.responses.iter().any(|response|matches!(response.body_kind,Some(BodyKind::Multipart))) {bail!("Versioned multipart responses require a version-aware decoder");}
        if branch.request_headers.iter().any(|field| {let name=field.name.to_ascii_lowercase();!(name.starts_with("x-")||matches!(name.as_str(),"authorization"|"content-type"))}) {bail!("Portable version matching requires explicitly controlled x-*, authorization, or content-type request headers; browser-managed header predicates are unsupported");}
        let kind=match branch.request_body_kind {None=>"empty",Some(BodyKind::Json)=>"json",Some(BodyKind::Text)=>"text",Some(BodyKind::Binary|BodyKind::File)=>"binary",Some(BodyKind::UrlEncoded)=>"form",Some(BodyKind::Multipart)=>"multipart",_=>bail!("Portable version response matching requires finite JSON, text, binary, form, or absent request bodies; multipart version predicates require a supported matcher")};
        let responses=branch.responses.iter().map(|response| Ok((format!("{}:{}",response.status_code,media(response)?),json!({"schema":response_schema(response)?,"headers":headers(&response.headers)?})))).collect::<Result<serde_json::Map<_,_>>>()?;
        let mut metadata = json!({"parserOptions":parser_options,"path":branch.path,"params":fields(&branch.request_params)?,"query":fields(&branch.request_query)?,"headers":headers(&branch.request_headers)?,"bodyKind":kind,"bodyMedia":branch.request_body_content_type.as_deref().unwrap_or(if kind=="json"{"application/json"}else if kind=="binary"{"application/octet-stream"}else if kind=="form"{"application/x-www-form-urlencoded"}else if kind=="multipart"{"multipart/form-data"}else{"text/plain"}).split(';').next().unwrap_or("").trim().to_ascii_lowercase(),"bodyOptional":branch.request_body_optional,"bodySchema":request_body_schema(branch)?,"responses":responses});
        let groups = super::request_objects::metadata(branch)?;
        if !groups.is_empty() {metadata["requestObjects"] = serde_json::from_str(&groups)?;}
        Ok(metadata)
    }).collect::<Result<Vec<_>>>()?;
    let metadata =
        super::canonical_json(json!({"formatVersion":1,"branches":branches})).to_string();
    if metadata.len() > 1024 * 1024 {
        bail!("Portable version metadata exceeds 1 MiB");
    }
    Ok(metadata)
}
fn response_schema(response: &SdkResponse) -> Result<Value> {
    if response.body.is_none() {
        return Ok(Value::Bool(true));
    }
    if response.body_kind == Some(BodyKind::ServerSentEvent) {
        let projected = crate::contract::sse_event_schema(&crate::contract::SchemaRef {
            name: response.body.clone().unwrap_or_default(),
            json_schema: response.body_json_schema.clone(),
        });
        return schema(projected.json_schema.as_deref());
    }
    schema(response.body_json_schema.as_deref())
}
fn request_body_schema(branch: &SdkOperation) -> Result<Value> {
    if branch.request_body.is_none() {
        return Ok(Value::Bool(true));
    }
    let schema = schema(branch.request_body_json_schema.as_deref())?;
    if branch.request_body_kind == Some(BodyKind::Multipart) {
        return crate::contract::schema_validation::multipart_wire_schema(&schema)
            .map_err(anyhow::Error::msg);
    }
    if matches!(
        branch.request_body_kind,
        Some(BodyKind::Binary | BodyKind::File)
    ) {
        if crate::contract::schema_validation::opaque_binary_schema(&schema) {
            return Ok(Value::Bool(true));
        }
        if !schema.is_boolean() && schema.get("type").and_then(Value::as_str) != Some("array") {
            bail!(
                "Versioned binary schemas require an opaque binary string, byte-array schema, or boolean schema"
            );
        }
    }
    Ok(schema)
}
fn union(values: Vec<Value>) -> Result<Value> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    if unique.len() == 1 {
        return Ok(unique.remove(0));
    }
    let mut definitions = serde_json::Map::new();
    let mut alternatives = Vec::new();
    for (index, value) in unique.into_iter().enumerate() {
        let name = format!("version{index}");
        let mut value = super::bundle_schema_resources(&value)?;
        relocate(&mut value, &format!("#/$defs/{name}"))?;
        definitions.insert(name.clone(), value);
        alternatives.push(json!({"$ref":format!("#/$defs/{name}")}));
    }
    Ok(
        json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$defs":definitions,"anyOf":alternatives}),
    )
}
fn relocate(value: &mut Value, prefix: &str) -> Result<()> {
    match value {
        Value::Object(fields) => {
            if fields.contains_key("$dynamicRef") || fields.contains_key("$recursiveRef") {
                bail!("Version response unions require resolved local schema references");
            }
            fields.remove("$id");
            for (key, value) in fields {
                if key == "$ref" {
                    let reference = value.as_str().context("Invalid schema reference")?;
                    if !reference.starts_with('#') {
                        bail!("Unresolved version response schema reference");
                    }
                    *value = Value::String(format!("{prefix}{}", &reference[1..]));
                } else if !matches!(key.as_str(), "const" | "enum" | "default" | "examples") {
                    relocate(value, prefix)?;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                relocate(item, prefix)?;
            }
        }
        _ => {}
    }
    Ok(())
}

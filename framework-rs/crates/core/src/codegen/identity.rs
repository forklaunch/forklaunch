//! Named contract identity, independent from structural schema equality.
use std::collections::BTreeMap;

use anyhow::{Result, bail};
use serde_json::Value;

pub(super) const KEY: &str = super::models::MODEL_IDENTITY_KEY;

pub(super) fn model(schema: &Value) -> Result<Option<(String, String)>> {
    let Some(metadata) = schema.get(KEY) else {
        return Ok(None);
    };
    let id = metadata.get("id").and_then(Value::as_str);
    let name = metadata.get("name").and_then(Value::as_str);
    let projection = metadata.get("projection").and_then(Value::as_str);
    if metadata.get("version").and_then(Value::as_u64) != Some(1)
        || id.is_none_or(str::is_empty)
        || name.is_none_or(str::is_empty)
        || !matches!(projection, Some("value" | "input"))
    {
        bail!("Invalid contract model identity metadata");
    }
    let (Some(id), Some(name), Some(projection)) = (id, name, projection) else {
        bail!("Incomplete contract model identity");
    };
    let mut name = super::models::rust_type_name(name);
    if reserved_model_name(&name) {
        name.push_str("Model");
    }
    let suffix = if projection == "input" { "Input" } else { "" };
    Ok(Some((
        format!("{id}:{projection}"),
        format!("{name}{suffix}"),
    )))
}

// Avoid shadowing compiler/runtime dependencies in either generated language.
fn reserved_model_name(name: &str) -> bool {
    name.starts_with("ForkLaunch")
        || name.starts_with("Sdk")
        || matches!(
            name,
            "Date"
                | "Promise"
                | "Array"
                | "ReadonlyArray"
                | "Record"
                | "Partial"
                | "Omit"
                | "Extract"
                | "Exclude"
                | "ReturnType"
                | "Error"
                | "Request"
                | "Response"
                | "Headers"
                | "RequestInit"
                | "RequestInfo"
                | "BodyInit"
                | "AbortSignal"
                | "Blob"
                | "FormData"
                | "URL"
                | "URLSearchParams"
                | "Uint8Array"
                | "ArrayBuffer"
                | "ReadableStream"
                | "String"
                | "Number"
                | "Boolean"
                | "Object"
                | "Map"
                | "Set"
                | "JSON"
                | "Option"
                | "Result"
                | "Vec"
                | "Box"
                | "BTreeMap"
                | "Arc"
                | "ClientRuntime"
                | "AsyncIterable"
                | "AsyncIterator"
                | "AsyncGenerator"
                | "Iterable"
                | "Iterator"
                | "Readonly"
                | "Required"
                | "NonNullable"
                | "Awaited"
                | "Parameters"
                | "Function"
                | "Ajv"
                | "Ajv2020"
                | "Ajv2019"
                | "AjvDraft4Module"
                | "AnySchema"
                | "ValidateFunction"
                | "FormatsPlugin"
                | "AbortController"
                | "ArrayBufferView"
                | "ArrayLike"
                | "BigInt"
                | "DataView"
                | "RangeError"
                | "RegExp"
                | "ReadableStreamDefaultReader"
                | "Symbol"
                | "TextDecoder"
                | "TypeError"
                | "WeakSet"
                | "WebSocket"
                | "AjvDraft4"
                | "ZodRegexExecution"
                | "MultipartForm"
                | "MultipartPart"
                | "MultipartResponse"
                | "MultipartResponsePart"
                | "ResponseValidator"
                | "Serialize"
                | "Deserialize"
        )
}

/// Validate full schema identity before rendering discards validation assertions.
#[cfg(any(feature = "typescript", feature = "rust-sdk"))]
pub(crate) fn validate(plan: &super::SdkGenerationPlan) -> Result<()> {
    let mut schemas = Vec::new();
    for operation in &plan.operations {
        let mut sources = operation
            .request_body_json_schema
            .iter()
            .collect::<Vec<_>>();
        for field in operation
            .request_params
            .iter()
            .chain(&operation.request_query)
            .chain(&operation.request_headers)
        {
            sources.extend(field.json_schema.iter());
        }
        for response in &operation.responses {
            sources.extend(response.body_json_schema.iter());
            for header in &response.headers {
                sources.extend(header.json_schema.iter());
            }
        }
        for source in sources {
            if source.len() > 32 * 1024 * 1024 {
                bail!("SDK schema graph exceeds 32 MiB");
            }
            let schema: Value = serde_json::from_str(source)?;
            schemas.push(schema);
        }
    }
    validate_schemas(schemas.iter())
}

#[cfg(any(feature = "typescript", feature = "rust-sdk"))]
pub(crate) fn validate_schemas<'a>(schemas: impl IntoIterator<Item = &'a Value>) -> Result<()> {
    let mut identities = BTreeMap::<String, Value>::new();
    let mut names = BTreeMap::<String, String>::new();
    for schema in schemas {
        let dialect = schema.get("$schema").cloned().unwrap_or(Value::Null);
        let mut normalized = super::schema::normalize(schema, "IdentitySchema")?;
        canonicalize(&mut normalized)?;
        for value in graph_nodes(&normalized)? {
            let Some((identity, name)) = model(value)? else {
                continue;
            };
            let mut signature = signature(value)?;
            if let Some(object) = signature.as_object_mut() {
                object.remove(super::schema::DEFINITIONS);
                object.remove("$schema");
            }
            let effective_dialect = value.get("$schema").unwrap_or(&dialect);
            let signature = serde_json::json!({"dialect":effective_dialect,"schema":signature});
            if identities
                .get(&identity)
                .is_some_and(|previous| previous != &signature)
            {
                bail!("Conflicting schemas for contract model identity {identity}");
            }
            if names
                .get(&name)
                .is_some_and(|previous| previous != &identity)
            {
                bail!(
                    "Ambiguous generated model name {name}; distinct identities require distinct names"
                );
            }
            identities.insert(identity.clone(), signature);
            names.insert(name, identity);
        }
    }
    Ok(())
}

#[cfg(any(feature = "typescript", feature = "rust-sdk"))]
fn graph_nodes(value: &Value) -> Result<Vec<&Value>> {
    let mut nodes = Vec::new();
    for root in std::iter::once(value).chain(
        value
            .get(super::schema::DEFINITIONS)
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|definitions| definitions.values()),
    ) {
        for location in crate::contract::schema_locations(root, false)? {
            if let Some(node) = root.pointer(&location) {
                nodes.push(node);
            }
        }
    }
    Ok(nodes)
}

fn signature(value: &Value) -> Result<Value> {
    let mut result = value.clone();
    for location in crate::contract::schema_locations(value, false)? {
        if let Some(object) = result.pointer_mut(&location).and_then(Value::as_object_mut) {
            for annotation in [
                "title",
                "description",
                "examples",
                "$comment",
                "deprecated",
                "x-forklaunch-sse-framing",
            ] {
                object.remove(annotation);
            }
        }
    }
    Ok(result)
}

// Normalization has already interpreted the document dialect. Embedded named
// roots may retain its declaration while referenced nodes inherit it. Runtime
// identity validation above still compares the originating document dialect.
fn rendered_signature(value: &Value) -> Result<Value> {
    let mut value = signature(value)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("$schema");
    }
    Ok(value)
}

/// Rebind graph edges only when their target carries an explicit model identity.
/// Returned names preserve the old operation-specific graph aliases.
pub(super) fn canonicalize(value: &mut Value) -> Result<BTreeMap<String, String>> {
    hoist_inline_models(value)?;
    let mut names = BTreeMap::new();
    if let Some(definitions) = value
        .get(super::schema::DEFINITIONS)
        .and_then(Value::as_object)
    {
        for (old, definition) in definitions {
            if let Some((_, canonical)) = model(definition)? {
                names.insert(old.clone(), canonical);
            }
        }
    }
    if names.is_empty() {
        return Ok(names);
    }
    rewrite_references(value, &names)?;
    if let Some(definitions) = value
        .get_mut(super::schema::DEFINITIONS)
        .and_then(Value::as_object_mut)
    {
        let previous = std::mem::take(definitions);
        for (old, definition) in previous {
            let target = names.get(&old).cloned().unwrap_or(old);
            if let Some(existing) = definitions.get(&target) {
                if rendered_signature(existing)? != rendered_signature(&definition)? {
                    bail!("Conflicting nested model definition {target}");
                }
            } else {
                definitions.insert(target, definition);
            }
        }
    }
    Ok(names)
}

// Preserve the nearest schema resource dialect when moving a nested named model.
fn inherited_dialect(root: &Value, location: &str) -> Option<Value> {
    let mut cursor = location;
    loop {
        if let Some(dialect) = root.pointer(cursor).and_then(|node| node.get("$schema")) {
            return Some(dialect.clone());
        }
        let (parent, _) = cursor.rsplit_once('/')?;
        cursor = parent;
    }
}

fn hoist_inline_models(value: &mut Value) -> Result<()> {
    let mut locations = crate::contract::schema_locations(value, false)?;
    if let Some(definitions) = value
        .get(super::schema::DEFINITIONS)
        .and_then(Value::as_object)
    {
        for (name, definition) in definitions {
            let prefix = format!(
                "/{}/{}",
                super::schema::DEFINITIONS,
                name.replace('~', "~0").replace('/', "~1")
            );
            locations.extend(
                crate::contract::schema_locations(definition, false)?
                    .into_iter()
                    .filter(|location| !location.is_empty())
                    .map(|location| format!("{prefix}{location}")),
            );
        }
    }
    locations.sort_by_key(|location| std::cmp::Reverse(location.len()));
    let mut named = BTreeMap::new();
    for location in locations
        .into_iter()
        .filter(|location| !location.is_empty())
    {
        let dialect = inherited_dialect(value, &location);
        let Some(node) = value.pointer_mut(&location) else {
            continue;
        };
        let Some((_, name)) = model(node)? else {
            continue;
        };
        let mut definition = node.clone();
        if let (Some(dialect), Some(object)) = (dialect, definition.as_object_mut()) {
            object.insert("$schema".into(), dialect);
        }
        if let Some(previous) = named.get(&name) {
            if rendered_signature(previous)? != rendered_signature(&definition)? {
                bail!("Conflicting nested model definition {name}");
            }
        } else {
            named.insert(name.clone(), definition);
        }
        *node = serde_json::json!({super::schema::REFERENCE: name, "x-forklaunch-sdk-inline-model":true});
    }
    if !named.is_empty() {
        let definitions = value
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Named schema must be an object"))?
            .entry(super::schema::DEFINITIONS)
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Invalid model definitions"))?;
        for (name, definition) in named {
            definitions.insert(name, definition);
        }
    }
    Ok(())
}

fn rewrite_references(value: &mut Value, names: &BTreeMap<String, String>) -> Result<()> {
    let mut locations = crate::contract::schema_locations(value, false)?;
    if let Some(definitions) = value
        .get(super::schema::DEFINITIONS)
        .and_then(Value::as_object)
    {
        for (name, definition) in definitions {
            let prefix = format!(
                "/{}/{}",
                super::schema::DEFINITIONS,
                name.replace('~', "~0").replace('/', "~1")
            );
            locations.extend(
                crate::contract::schema_locations(definition, false)?
                    .into_iter()
                    .map(|location| format!("{prefix}{location}")),
            );
        }
    }
    for location in locations {
        if let Some(reference) = value
            .pointer_mut(&location)
            .and_then(|node| node.get_mut(super::schema::REFERENCE))
            && let Some(target) = reference.as_str().and_then(|name| names.get(name))
        {
            *reference = Value::String(target.clone());
        }
    }
    Ok(())
}

pub(super) fn compile(root: &str, schema: &Value) -> Result<super::SchemaModels> {
    let Some((_, name)) = model(schema)? else {
        return super::compile_schema(root, schema);
    };
    let mut shared = super::compile_schema(&name, schema)?;
    let original = super::compile_schema(root, schema)?;
    let original_prefix = super::models::rust_type_name(root);
    let canonical_prefix = super::models::rust_type_name(&name);
    // Preserve all prior operation-specific public names, including nested DTOs.
    for alias in original
        .rust_models
        .iter()
        .map(|model| &model.name)
        .chain(original.rust_aliases.iter().map(|alias| &alias.name))
    {
        if let Some(suffix) = alias.strip_prefix(&original_prefix) {
            let target = if let Some(reference) = suffix.strip_prefix("Schema") {
                format!(
                    "{}Schema{reference}",
                    canonical_prefix.trim_end_matches('_')
                )
            } else {
                format!("{canonical_prefix}{suffix}")
            };
            if alias != &target {
                shared.rust_aliases.push(super::RustTypeAliasTemplate {
                    name: alias.clone(),
                    target_type: target,
                });
            }
        }
    }
    for alias in original.typescript_definitions.keys() {
        if let Some(suffix) = alias.strip_prefix(&original_prefix) {
            let target = if let Some(reference) = suffix.strip_prefix("Schema") {
                format!(
                    "{}Schema{reference}",
                    canonical_prefix.trim_end_matches('_')
                )
            } else {
                format!("{canonical_prefix}{suffix}")
            };
            if alias != &target {
                shared.typescript_definitions.insert(alias.clone(), target);
            }
        }
    }
    shared
        .typescript_definitions
        .insert(name.clone(), shared.typescript_type.clone());
    shared.typescript_type = name;
    shared.rust_type = original.rust_type;
    Ok(shared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema(id: &str, name: &str) -> Value {
        json!({"type":"object","properties":{"name":{"type":"string","minLength":1}},"required":["name"],
            "x-forklaunch-model":{"version":1,"id":id,"name":name,"projection":"value"}})
    }

    #[test]
    fn repeated_identity_preserves_operation_aliases_and_reuses_model() {
        let value = schema("example::Person", "Person");
        let first = compile("CreateResponse", &value).unwrap();
        let second = compile("ReadResponse", &value).unwrap();
        assert_eq!(first.rust_models, second.rust_models);
        assert_eq!(first.rust_models[0].name, "Person");
        assert_eq!(first.rust_aliases[0].name, "CreateResponse");
        assert_eq!(first.rust_aliases[0].target_type, "Person");
        assert_eq!(second.rust_aliases[0].name, "ReadResponse");
        assert_eq!(first.typescript_type, "Person");
    }

    #[test]
    fn structural_equality_does_not_authorize_identity_reuse() {
        let first = compile("FirstResponse", &schema("example::First", "First")).unwrap();
        let second = compile("SecondResponse", &schema("example::Second", "Second")).unwrap();
        assert_ne!(first.rust_models[0].name, second.rust_models[0].name);
        let ordinary =
            json!({"type":"object","title":"Person","properties":{"name":{"type":"string"}}});
        assert_eq!(
            compile("OperationResponse", &ordinary).unwrap().rust_models[0].name,
            "OperationResponse"
        );
    }
}

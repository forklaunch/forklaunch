//! Reuse canonical payload models while retaining the explicitly framed SSE envelope.
use super::{RustTypeAliasTemplate, SchemaModels};
use anyhow::{Context, Result, bail};
use serde_json::Value;

pub(super) fn reuse(
    root: &str,
    payload: &super::SchemaRef,
    mut envelope: SchemaModels,
) -> Result<SchemaModels> {
    let Some(raw) = &payload.json_schema else {
        return Ok(envelope);
    };
    let value: Value = serde_json::from_str(raw)?;
    if value
        .get("x-forklaunch-sse-framing")
        .and_then(Value::as_str)
        != Some("payload")
    {
        return Ok(envelope);
    }
    let Some((_, canonical)) = super::identity::model(&value)? else {
        return Ok(envelope);
    };
    let mut shared = super::compile_schema_ref(&format!("{root}Data"), payload)?;
    let root = super::models::rust_type_name(root);
    let index = envelope
        .rust_models
        .iter()
        .position(|model| model.name == root)
        .context("Missing generated SSE envelope model")?;
    let mut model = envelope.rust_models.remove(index);
    let data = model
        .fields
        .iter_mut()
        .find(|field| field.wire_name_literal == "\"data\"")
        .context("Missing generated SSE data member")?;
    data.field_type.clone_from(&shared.rust_type);
    for previous in envelope
        .rust_models
        .iter()
        .map(|model| &model.name)
        .chain(envelope.rust_aliases.iter().map(|alias| &alias.name))
    {
        if shared
            .rust_models
            .iter()
            .any(|model| &model.name == previous)
        {
            continue;
        }
        let target = target(previous, &root, &canonical)?;
        if !shared.rust_models.iter().any(|model| model.name == target)
            && !shared.rust_aliases.iter().any(|alias| alias.name == target)
        {
            bail!("Cannot preserve SSE model alias {previous} -> {target}");
        }
        if previous != &target
            && !shared
                .rust_aliases
                .iter()
                .any(|alias| &alias.name == previous)
        {
            shared.rust_aliases.push(RustTypeAliasTemplate {
                name: previous.clone(),
                target_type: target,
            });
        }
    }
    for previous in envelope.typescript_definitions.keys() {
        if shared.typescript_definitions.contains_key(previous) {
            continue;
        }
        let target = target(previous, &root, &canonical)?;
        if !shared.typescript_definitions.contains_key(&target) {
            bail!("Cannot preserve SSE TypeScript model alias {previous}");
        }
        if previous != &target {
            shared
                .typescript_definitions
                .insert(previous.clone(), target);
        }
    }
    shared.rust_models.push(model);
    shared.rust_type = root;
    shared.typescript_type = format!("{{ data: {}; id?: string }}", shared.typescript_type);
    Ok(shared)
}

fn target(previous: &str, root: &str, canonical: &str) -> Result<String> {
    if let Some(suffix) = previous.strip_prefix(&format!("{root}Schema")) {
        return Ok(format!("{}Schema{suffix}", canonical.trim_end_matches('_')));
    }
    if let Some(suffix) = previous.strip_prefix(&format!("{root}Data")) {
        return Ok(format!("{canonical}{suffix}"));
    }
    bail!("Unsupported generated SSE model alias {previous}")
}

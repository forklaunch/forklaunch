//! `AsyncAPI` 3 traits use ordered JSON Merge Patch, with the target applied last.
use super::{FrameworkResult, Map, OutputBudget, Value, invalid, object_at, referenced_object};
use std::borrow::Cow;

pub(super) fn apply<'a>(
    document: &Value,
    location: &str,
    target: &'a Map<String, Value>,
    message: bool,
    budget: &mut OutputBudget,
) -> FrameworkResult<Cow<'a, Map<String, Value>>> {
    let Some(traits) = target.get("traits") else {
        return Ok(Cow::Borrowed(target));
    };
    let traits = traits
        .as_array()
        .filter(|traits| traits.len() <= 64)
        .ok_or_else(|| invalid("Expected at most 64 traits per AsyncAPI object"))?;
    let mut merged = Value::Object(Map::new());
    for index in 0..traits.len() {
        let (_, value) = object_at(document, &format!("{location}/traits/{index}"))?;
        // Reference targets also need the trait's field restrictions. In
        // particular, a message trait cannot replace a scoped payload schema.
        for key in value.keys() {
            if key.starts_with("x-")
                || matches!(
                    key.as_str(),
                    "title" | "summary" | "description" | "tags" | "externalDocs" | "bindings"
                )
                || (message
                    && matches!(
                        key.as_str(),
                        "headers" | "correlationId" | "contentType" | "name" | "examples"
                    ))
                || (!message && key == "security")
            {
                continue;
            }
            return Err(invalid(
                "Unsupported field in AsyncAPI trait reference target",
            ));
        }
        let mut patch = value.clone();
        if message {
            resolve_correlation(document, &mut patch, budget)?;
        }
        let patch = Value::Object(patch);
        budget.reserve(&patch)?;
        merge_patch(&mut merged, &patch);
    }
    let mut target = target.clone();
    target.remove("traits");
    if message {
        resolve_correlation(document, &mut target, budget)?;
    }
    merge_patch(&mut merged, &Value::Object(target));
    budget.reserve(&merged)?;
    let Value::Object(merged) = merged else {
        unreachable!()
    };
    Ok(Cow::Owned(merged))
}

// Correlation references designate objects, not merge-patch members. Resolve
// before merging to avoid manufacturing a forbidden `$ref`+`location` sibling.
// This follows the official parser's reference/trait precedence.
fn resolve_correlation(
    document: &Value,
    object: &mut Map<String, Value>,
    budget: &mut OutputBudget,
) -> FrameworkResult<()> {
    if let Some(value) = object.get_mut("correlationId")
        && value.get("$ref").is_some()
    {
        let resolved = Value::Object(referenced_object(document, value)?.clone());
        budget.reserve(&resolved)?;
        *value = resolved;
    }
    Ok(())
}

fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let target = target.as_object_mut().unwrap();
    for (key, value) in patch {
        if value.is_null() {
            target.remove(key);
        } else {
            merge_patch(target.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}

//! Copy referenced schemas out of literal data before rewriting schema edges.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use forklaunch_common::{FrameworkError, FrameworkResult};
use serde_json::{Map, Value};

use super::schema::{
    SchemaGraph, local_pointer, schema_context_from, schema_graph, schema_graph_from,
};

/// Bundle local schema targets stored inside literal assertions or annotations.
/// Original const/enum/default/example values remain byte-for-byte JSON values.
/// Recursive references use reserved, collision-checked definition names.
/// Use only within a checked profile that rejects active resource IDs and
/// dynamic scope; ignored annotation IDs do not establish a reference base.
///
/// # Errors
/// Rejects invalid definition containers or graphs/copies exceeding 64 levels,
/// 100,000 JSON nodes, or 32 MiB. This helper does not resolve resource IDs.
pub fn bundle_literal_schema_references_unscoped(root: &Value) -> FrameworkResult<Value> {
    bundle_with_resource_policy(root, false)
}

/// Extract a standalone schema from a containing document, bundling its local
/// reference graph without changing literal data. `location` is a JSON pointer,
/// not a URI fragment. References are resolved against the containing document.
///
/// # Errors
/// Rejects missing/non-schema targets, external references, resource IDs,
/// anchors/dynamic scope, and shared depth/node/byte limits.
pub fn bundle_document_schema(document: &Value, location: &str) -> FrameworkResult<Value> {
    let mut budget = CopyBudget::default();
    budget.check(document)?;
    let schema = document
        .pointer(location)
        .ok_or_else(|| invalid("document schema location is missing"))?;
    let output = budget.copy(schema)?;
    let graph = match schema_graph_from(document, location, false) {
        Err(error) if error.code() == "schema_container_reference_unsupported" => {
            return bundle_contexts(document, location, output, budget);
        }
        result => result?,
    };
    validate_document_nodes(document, schema, &graph)?;
    let targets = graph.reference_targets.iter().cloned().collect::<Vec<_>>();
    if targets.is_empty() {
        return Ok(output);
    }
    bundle_graph(document, location, output, graph, targets, budget)
}

fn validate_document_nodes(
    document: &Value,
    schema: &Value,
    graph: &SchemaGraph,
) -> FrameworkResult<()> {
    let draft4 = schema
        .get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|dialect| dialect.contains("/draft-04/"));
    for location in &graph.locations {
        let node = document
            .pointer(location)
            .ok_or_else(|| invalid("document schema target is missing"))?;
        if node.is_boolean() {
            continue;
        }
        let object = node
            .as_object()
            .ok_or_else(|| invalid("document schemas must be objects or booleans"))?;
        if (draft4 && object.contains_key("id"))
            || object.get("$id").is_some_and(|id| id.as_str() != Some(""))
            || [
                "$anchor",
                "$dynamicAnchor",
                "$dynamicRef",
                "$recursiveAnchor",
                "$recursiveRef",
            ]
            .iter()
            .any(|key| object.contains_key(*key))
        {
            return Err(invalid(
                "document schema resource IDs and dynamic scope require a resolver",
            ));
        }
        if let Some(reference) = object.get("$ref") {
            let pointer = reference.as_str().and_then(local_pointer).ok_or_else(|| {
                invalid("only UTF-8 document-local schema references are supported")
            })?;
            if document.pointer(&pointer).is_none() {
                return Err(invalid("document schema reference is missing"));
            }
        }
    }
    Ok(())
}

pub(super) fn bundle_with_resource_policy(
    root: &Value,
    preserve_resource_bases: bool,
) -> FrameworkResult<Value> {
    let mut budget = CopyBudget::default();
    let output = budget.copy(root)?;
    let graph = match schema_graph(root, preserve_resource_bases) {
        Err(error)
            if !preserve_resource_bases
                && error.code() == "schema_container_reference_unsupported" =>
        {
            return bundle_contexts(root, "", output, budget);
        }
        result => result?,
    };
    let targets: Vec<_> = graph
        .reference_targets
        .iter()
        .filter(|target| inside_literal(target, &graph.literal_ranges, ""))
        .collect();
    if targets.is_empty() {
        return Ok(output);
    }
    let targets = targets.into_iter().cloned().collect();
    bundle_graph(root, "", output, graph, targets, budget)
}

// A dictionary can also be a referenced schema. Its property named `const`, for
// example, is a schema in the dictionary context but literal data in the schema
// context. Traverse each reference target independently and rewrite only that
// context's structural schema locations. Never rewrite the original JSON data.
fn bundle_contexts(
    root: &Value,
    scope: &str,
    mut output: Value,
    mut budget: CopyBudget,
) -> FrameworkResult<Value> {
    let dialect = root
        .pointer(scope)
        .ok_or_else(|| invalid("schema scope is missing"))?;
    let definitions = output
        .as_object_mut()
        .ok_or_else(|| invalid("schema root must be an object"))?
        .entry("definitions")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| invalid("schema definitions must be an object"))?;
    let mut copies = BTreeMap::from([(scope.to_owned(), String::new())]);
    let mut pending = VecDeque::from([scope.to_owned()]);
    let mut contexts = Vec::new();
    let mut index = 0;
    while let Some(source) = pending.pop_front() {
        let graph = schema_context_from(root, &source, dialect)?;
        validate_document_nodes(root, dialect, &graph)?;
        for target in &graph.reference_targets {
            if copies.contains_key(target) {
                continue;
            }
            let name = loop {
                let name = format!("ForklaunchContext{index}");
                index += 1;
                if !definitions.contains_key(&name) {
                    break name;
                }
            };
            let value = root
                .pointer(target)
                .ok_or_else(|| invalid("schema target missing"))?;
            if !value.is_object() && !value.is_boolean() {
                return Err(invalid("schema targets must be objects or booleans"));
            }
            definitions.insert(name.clone(), budget.copy(value)?);
            copies.insert(target.clone(), format!("/definitions/{name}"));
            pending.push_back(target.clone());
        }
        contexts.push((source, graph.locations));
    }
    for (source, locations) in contexts {
        let destination = &copies[&source];
        for location in locations {
            rewrite_reference(
                &mut output,
                &format!("{destination}{}", &location[source.len()..]),
                &copies,
            )?;
        }
    }
    CopyBudget::default().check(&output)?;
    Ok(output)
}

fn bundle_graph(
    root: &Value,
    scope: &str,
    mut output: Value,
    graph: SchemaGraph,
    targets: Vec<String>,
    mut budget: CopyBudget,
) -> FrameworkResult<Value> {
    let definitions = output
        .as_object_mut()
        .ok_or_else(|| invalid("schema root must be an object"))?
        .entry("definitions")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| invalid("schema definitions must be an object"))?;
    let mut copies = BTreeMap::new();
    let mut index = 0;
    for target in targets {
        let name = loop {
            let name = format!("ForklaunchLiteral{index}");
            index += 1;
            if !definitions.contains_key(&name) {
                break name;
            }
        };
        let schema = root
            .pointer(&target)
            .ok_or_else(|| invalid("literal schema target is missing"))?;
        if !schema.is_object() && !schema.is_boolean() {
            return Err(invalid(
                "literal schema target must be an object or boolean",
            ));
        }
        definitions.insert(name.clone(), budget.copy(schema)?);
        copies.insert(target.clone(), format!("/definitions/{name}"));
    }
    let locations: BTreeSet<_> = graph.locations.into_iter().collect();
    rewrite_subtree(
        &mut output,
        &locations,
        &graph.literal_ranges,
        scope,
        "",
        &copies,
    )?;
    for (source, destination) in &copies {
        rewrite_subtree(
            &mut output,
            &locations,
            &graph.literal_ranges,
            source,
            destination,
            &copies,
        )?;
    }
    CopyBudget::default().check(&output)?;
    Ok(output)
}

fn rewrite_subtree(
    output: &mut Value,
    locations: &BTreeSet<String>,
    literals: &BTreeSet<String>,
    source: &str,
    destination: &str,
    copies: &BTreeMap<String, String>,
) -> FrameworkResult<()> {
    let prefix = format!("{source}/");
    for location in locations.range(source.to_owned()..) {
        if location != source && !location.starts_with(&prefix) {
            break;
        }
        if !inside_literal(location, literals, source) {
            rewrite_reference(
                output,
                &format!("{destination}{}", &location[source.len()..]),
                copies,
            )?;
        }
    }
    Ok(())
}

fn inside_literal(location: &str, literals: &BTreeSet<String>, scope: &str) -> bool {
    (location.len() > scope.len() && literals.contains(location))
        || location
            .rmatch_indices('/')
            .any(|(end, _)| end > scope.len() && literals.contains(&location[..end]))
}

fn rewrite_reference(
    output: &mut Value,
    location: &str,
    copies: &BTreeMap<String, String>,
) -> FrameworkResult<()> {
    let node = output
        .pointer_mut(location)
        .ok_or_else(|| invalid("bundled schema location is missing"))?;
    if let Some(object) = node.as_object_mut()
        && let Some(target) = object
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(local_pointer)
        && let Some(destination) = copies.get(&target)
    {
        object.insert("$ref".into(), Value::String(format!("#{destination}")));
    }
    Ok(())
}

struct CopyBudget {
    nodes: usize,
    bytes: usize,
}
impl Default for CopyBudget {
    fn default() -> Self {
        Self {
            nodes: 100_000,
            bytes: 32 * 1024 * 1024,
        }
    }
}
impl CopyBudget {
    fn copy(&mut self, value: &Value) -> FrameworkResult<Value> {
        self.check(value)?;
        Ok(value.clone())
    }
    fn check(&mut self, value: &Value) -> FrameworkResult<()> {
        let mut pending = vec![(value, 0)];
        while let Some((node, depth)) = pending.pop() {
            if depth > 64 || self.nodes == 0 {
                return Err(invalid("schema bundling exceeds depth/node limits"));
            }
            self.nodes -= 1;
            let children = match node {
                Value::Array(values) => values.len(),
                Value::Object(values) => values.len(),
                _ => 0,
            };
            if children > self.nodes.saturating_sub(pending.len()) {
                return Err(invalid("schema bundling exceeds node limit"));
            }
            match node {
                Value::Array(values) => pending.extend(values.iter().map(|v| (v, depth + 1))),
                Value::Object(values) => pending.extend(values.values().map(|v| (v, depth + 1))),
                _ => {}
            }
        }
        serde_json::to_writer(&mut *self, value)
            .map_err(|_| invalid("schema bundling exceeds 32 MiB"))
    }
}
impl std::io::Write for CopyBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("schema copy byte limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn invalid(message: &str) -> FrameworkError {
    FrameworkError::new("schema_bundling_invalid", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn document_schemas_bundle_transitive_recursive_and_literal_targets() {
        let document = json!({"channels":{"chat":{"payload":{"type":"object","properties":{"item":{"$ref":"#/components/schemas/Node%2525~1~0%20%E6%B0%B4"},"stored":{"$ref":"#/components/schemas/Storage/const"}},"default":{"$ref":"literal data"}}}},"components":{"schemas":{"Node%25/~ 水":{"type":"object","properties":{"next":{"$ref":"#/components/schemas/Node%2525~1~0%20%E6%B0%B4"},"value":{"type":"integer"}}},"Storage":{"const":{"type":"string"}}}}});
        let schema = bundle_document_schema(&document, "/channels/chat/payload").unwrap();
        assert_eq!(
            schema["default"],
            document["channels"]["chat"]["payload"]["default"]
        );
        let pointer =
            local_pointer(schema["properties"]["item"]["$ref"].as_str().unwrap()).unwrap();
        let target = schema.pointer(&pointer).unwrap();
        assert_eq!(target["properties"]["next"]["$ref"], format!("#{pointer}"));
        let literal =
            local_pointer(schema["properties"]["stored"]["$ref"].as_str().unwrap()).unwrap();
        assert_eq!(schema.pointer(&literal), Some(&json!({"type":"string"})));
        assert!(super::super::schema::schema_locations(&schema, false).is_ok());
        for value in [
            json!({"$ref":"https://example.invalid/schema"}),
            json!({"$ref":"#/missing"}),
            json!({"$id":"scope","type":"string"}),
            json!({"$dynamicRef":"#node"}),
            json!(42),
        ] {
            assert!(bundle_document_schema(&json!({"payload":value}), "/payload").is_err());
        }
    }

    #[test]
    fn nested_recursive_literal_targets_are_copied_without_changing_assertions() {
        let schema = json!({"properties":{"item":{"$ref":"#/default"}},"default":{"type":"object","properties":{"next":{"$ref":"#/default"},"value":{"$ref":"#/enum/0"}},"const":{"$ref":"#/default"}},"enum":[{"type":"string","minLength":2}],"definitions":{"ForklaunchLiteral0":{"type":"boolean"}}});
        let output = bundle_literal_schema_references_unscoped(&schema).unwrap();
        assert_eq!(output["default"], schema["default"]);
        assert_eq!(output["enum"], schema["enum"]);
        let target = local_pointer(output["properties"]["item"]["$ref"].as_str().unwrap()).unwrap();
        let copy = output.pointer(&target).unwrap();
        assert_eq!(copy["properties"]["next"]["$ref"], format!("#{target}"));
        assert_eq!(copy["const"], schema["default"]["const"]);
        let value = local_pointer(copy["properties"]["value"]["$ref"].as_str().unwrap()).unwrap();
        assert_eq!(output.pointer(&value), Some(&schema["enum"][0]));
        assert_eq!(
            output["definitions"]["ForklaunchLiteral0"],
            schema["definitions"]["ForklaunchLiteral0"]
        );
        assert!(super::super::schema::schema_locations(&output, false).is_ok());
        assert_eq!(
            bundle_literal_schema_references_unscoped(&output).unwrap(),
            output
        );
    }

    #[test]
    fn dictionary_aliases_preserve_simultaneous_schema_and_literal_roles() {
        let schema = json!({"type":"object","properties":{"item":{"$ref":"#/properties"},"const":{"$ref":"#/definitions/Value"}},"definitions":{"Value":{"type":"string"},"ForklaunchContext0":{"type":"boolean"}}});
        let output = bundle_literal_schema_references_unscoped(&schema).unwrap();
        let target = local_pointer(output["properties"]["item"]["$ref"].as_str().unwrap()).unwrap();
        let alias = output.pointer(&target).unwrap();
        assert_eq!(alias["const"], schema["properties"]["const"]);
        assert_ne!(output["properties"]["const"], schema["properties"]["const"]);
        assert_eq!(
            output["definitions"]["ForklaunchContext0"],
            schema["definitions"]["ForklaunchContext0"]
        );
        assert!(super::super::schema::schema_locations(&output, false).is_ok());
        assert_eq!(
            bundle_literal_schema_references_unscoped(&output).unwrap(),
            output
        );
        let relocated: Value = serde_json::from_str(
            &super::super::schema::relocate_schema_json_unscoped(&schema.to_string(), &["payload"]),
        )
        .unwrap();
        assert_eq!(relocated.pointer(&target).unwrap()["const"], alias["const"]);
        let document = json!({"payload":schema});
        // Document pointers are absolute, so use an equivalent namespaced graph.
        let document: Value = serde_json::from_str(
            &document
                .to_string()
                .replace("#/properties", "#/payload/properties")
                .replace("#/definitions", "#/payload/definitions"),
        )
        .unwrap();
        let extracted = bundle_document_schema(&document, "/payload").unwrap();
        assert!(super::super::schema::schema_locations(&extracted, false).is_ok());
    }

    #[test]
    fn dictionary_contexts_rebase_recursive_edges_without_touching_annotations() {
        let schema = json!({"type":"object","properties":{"item":{"$ref":"#/properties"},"properties":{"next":{"$ref":"#/properties"}}}});
        let output = bundle_literal_schema_references_unscoped(&schema).unwrap();
        let target = local_pointer(output["properties"]["item"]["$ref"].as_str().unwrap()).unwrap();
        let alias = output.pointer(&target).unwrap();
        assert_eq!(alias["properties"]["next"]["$ref"], format!("#{target}"));
        assert_eq!(
            output["properties"]["properties"]["next"],
            schema["properties"]["properties"]["next"]
        );
        assert!(super::super::schema::schema_locations(&output, false).is_ok());
        let resource =
            json!({"properties":{"item":{"$ref":"#/properties"},"$id":{"const":"scope"}}});
        assert!(bundle_literal_schema_references_unscoped(&resource).is_err());
        let missing =
            json!({"properties":{"item":{"$ref":"#/properties"},"other":{"$ref":"#/missing"}}});
        assert!(bundle_literal_schema_references_unscoped(&missing).is_err());
    }

    #[test]
    fn literal_copies_cannot_exceed_shared_budgets_or_mutate_the_input() {
        let schema = json!({"$ref":"#/default","default":{"type":"string"}});
        let original = schema.clone();
        assert!(
            CopyBudget {
                nodes: 2,
                bytes: 1000
            }
            .copy(&schema)
            .is_err()
        );
        assert!(
            CopyBudget {
                nodes: 100,
                bytes: 2
            }
            .copy(&schema)
            .is_err()
        );
        assert!(
            bundle_literal_schema_references_unscoped(&json!({"$ref":"#/const","const":42}))
                .is_err()
        );
        assert_eq!(schema, original);
    }
}

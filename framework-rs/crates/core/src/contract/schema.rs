//! Contract schemas are standalone documents; embedding changes their JSON pointer base.
#[cfg(feature = "typed-schema")]
pub use super::optional::Optional;
#[cfg(feature = "typed-schema")]
pub use super::patch::Patch;
use forklaunch_common::{FrameworkError, FrameworkResult};
use serde_json::Value;
use std::collections::{BTreeSet, VecDeque};

/// Keywords outside an explicitly declared dialect are literal annotations,
/// not locations to validate or relocate embedded schemas.
#[must_use]
pub fn is_legacy_schema_annotation(root: &Value, keyword: &str) -> bool {
    let dialect = root.get("$schema").and_then(Value::as_str).unwrap_or("");
    let draft4 = dialect.contains("draft-04");
    let draft6 = dialect.contains("draft-06");
    let draft7 = dialect.contains("draft-07");
    let legacy = draft4 || draft6 || draft7;
    (draft4 && matches!(keyword, "const" | "contains" | "propertyNames"))
        || ((draft4 || draft6) && matches!(keyword, "if" | "then" | "else"))
        || (legacy
            && matches!(
                keyword,
                "dependentSchemas"
                    | "dependentRequired"
                    | "unevaluatedProperties"
                    | "unevaluatedItems"
                    | "contentSchema"
                    | "minContains"
                    | "maxContains"
            ))
        || ((legacy || dialect.contains("2019-09")) && keyword == "prefixItems")
        || (dialect.contains("2020-12") && keyword == "additionalItems")
}

/// Relocate document-local JSON pointers when embedding a standalone schema.
///
/// Location segments are escaped as JSON pointer tokens. Schema literals and
/// annotations are preserved; malformed JSON is returned unchanged. Resources
/// with their own `$id` retain their reference base.
#[must_use]
pub fn relocate_schema_json(schema: &str, location: &[&str]) -> String {
    relocate_with_resource_policy(schema, location, true)
}

/// Relocate checked schemas whose active resource IDs have already been rejected.
/// Ignored annotation IDs do not establish a reference base in this profile.
#[must_use]
pub fn relocate_schema_json_unscoped(schema: &str, location: &[&str]) -> String {
    relocate_with_resource_policy(schema, location, false)
}

fn relocate_with_resource_policy(
    schema: &str,
    location: &[&str],
    preserve_resource_bases: bool,
) -> String {
    let Ok(parsed) = serde_json::from_str::<Value>(schema) else {
        return schema.into();
    };
    let Ok(mut value) =
        super::schema_bundle::bundle_with_resource_policy(&parsed, preserve_resource_bases)
    else {
        return schema.into();
    };
    let root = value.clone();
    let base = location
        .iter()
        .fold(String::from("#"), |mut base, segment| {
            base.push('/');
            base.push_str(&segment.replace('~', "~0").replace('/', "~1"));
            base
        });
    let Ok(locations) = schema_locations(&root, preserve_resource_bases) else {
        return schema.into();
    };
    for location in locations {
        let Some(object) = value.pointer_mut(&location).and_then(Value::as_object_mut) else {
            continue;
        };
        if preserve_resource_bases && has_resource_id(&root, object) {
            continue;
        }
        if let Some(reference) = object
            .get("$ref")
            .and_then(Value::as_str)
            .map(str::to_owned)
            && let Some(pointer) = local_pointer(&reference)
            && root.pointer(&pointer).is_some()
        {
            const FRAGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
                .remove(b'/')
                .remove(b'~')
                .remove(b'$')
                .remove(b'-')
                .remove(b'.')
                .remove(b'_');
            let rebased = format!("{}{pointer}", &base[1..]);
            object.insert(
                "$ref".into(),
                Value::String(format!(
                    "#{}",
                    percent_encoding::utf8_percent_encode(&rebased, FRAGMENT)
                )),
            );
        }
    }
    value.to_string()
}

fn has_resource_id(root: &Value, schema: &serde_json::Map<String, Value>) -> bool {
    ["$id", "id"].into_iter().any(|key| {
        (key == "$id"
            || root
                .get("$schema")
                .and_then(Value::as_str)
                .is_some_and(|dialect| dialect.contains("/draft-04/")))
            && schema
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|id| !id.is_empty())
    })
}

pub(super) fn local_pointer(reference: &str) -> Option<String> {
    let fragment = reference.strip_prefix('#')?;
    let pointer = percent_encoding::percent_decode_str(fragment)
        .decode_utf8()
        .ok()?;
    (pointer.is_empty() || pointer.starts_with('/')).then(|| pointer.into_owned())
}

fn has_resource_ancestor(root: &Value, location: &str) -> bool {
    location.rmatch_indices('/').any(|(end, _)| {
        root.pointer(&location[..end])
            .and_then(Value::as_object)
            .is_some_and(|schema| has_resource_id(root, schema))
    })
}

/// Locate schema nodes, including targets explicitly referenced from annotation
/// storage. Literal enum/const/default/example data stays outside the traversal.
/// `preserve_resource_bases` skips children of resources with their own IDs.
///
/// # Errors
/// Rejects graphs exceeding 100,000 locations or 64 traversal steps.
/// References into literal assertion/default/example data require bundling and
/// are rejected to avoid changing those literals while relocating schemas.
/// Unscoped checked profiles reject percent-encoded pointer separators
/// to match payload validators; URI encoding within pointer tokens is supported.
pub fn schema_locations(
    root: &Value,
    preserve_resource_bases: bool,
) -> FrameworkResult<Vec<String>> {
    let graph = schema_graph(root, preserve_resource_bases)?;
    reject_literal_targets(graph.reference_targets, &graph.literal_ranges)?;
    Ok(graph.locations)
}

pub(super) struct SchemaGraph {
    pub(super) locations: Vec<String>,
    pub(super) literal_ranges: BTreeSet<String>,
    pub(super) reference_targets: BTreeSet<String>,
}

pub(super) fn schema_graph(
    root: &Value,
    preserve_resource_bases: bool,
) -> FrameworkResult<SchemaGraph> {
    schema_graph_from(root, "", preserve_resource_bases)
}

pub(super) fn schema_graph_from(
    root: &Value,
    start: &str,
    preserve_resource_bases: bool,
) -> FrameworkResult<SchemaGraph> {
    let dialect = schema_node(root, start, 0)?;
    collect_schema_graph(root, start, preserve_resource_bases, true, dialect)
}

/// Traverse one interpretation context without following references into others.
pub(super) fn schema_context_from(
    root: &Value,
    start: &str,
    dialect: &Value,
) -> FrameworkResult<SchemaGraph> {
    collect_schema_graph(root, start, false, false, dialect)
}

fn collect_schema_graph(
    root: &Value,
    start: &str,
    preserve_resource_bases: bool,
    follow_references: bool,
    dialect: &Value,
) -> FrameworkResult<SchemaGraph> {
    let mut queue = VecDeque::from([(start.to_owned(), 0)]);
    let mut seen = BTreeSet::from([start.to_owned()]);
    let mut locations = Vec::new();
    let mut literal_ranges = BTreeSet::new();
    let mut reference_targets = BTreeSet::new();
    let mut containers = BTreeSet::new();
    while let Some((location, depth)) = queue.pop_front() {
        if preserve_resource_bases && has_resource_ancestor(root, &location) {
            continue;
        }
        let node = schema_node(root, &location, depth)?;
        locations.push(location.clone());
        let Some(object) = node.as_object() else {
            continue;
        };
        if preserve_resource_bases && has_resource_id(dialect, object) {
            continue;
        }
        if !preserve_resource_bases {
            validate_reference_profile(object)?;
        }
        if let Some(pointer) = object
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(local_pointer)
            && root.pointer(&pointer).is_some()
        {
            reference_targets.insert(pointer.clone());
            if follow_references {
                enqueue(&mut queue, &mut seen, pointer, depth + 1)?;
            }
        }
        for (key, child) in object {
            if matches!(key.as_str(), "enum" | "const" | "default" | "examples")
                && !is_legacy_schema_annotation(dialect, key)
            {
                literal_ranges.insert(append_pointer(&location, key));
            }
            if is_legacy_schema_annotation(dialect, key) {
                continue;
            }
            let child_location = append_pointer(&location, key);
            match key.as_str() {
                "properties" | "patternProperties" | "dependentSchemas" | "dependencies"
                | "$defs" | "definitions" => {
                    if let Some(schemas) = child.as_object() {
                        containers.insert(child_location.clone());
                        for (name, schema) in schemas {
                            if key == "dependencies" && schema.is_array() {
                                continue;
                            }
                            enqueue(
                                &mut queue,
                                &mut seen,
                                append_pointer(&child_location, name),
                                depth + 1,
                            )?;
                        }
                    }
                }
                "additionalProperties" | "additionalItems" if child.is_boolean() => {}
                "additionalProperties"
                | "additionalItems"
                | "unevaluatedProperties"
                | "unevaluatedItems"
                | "contentSchema"
                | "contains"
                | "not"
                | "if"
                | "then"
                | "else"
                | "propertyNames" => enqueue(&mut queue, &mut seen, child_location, depth + 1)?,
                "items" if !child.is_array() => {
                    enqueue(&mut queue, &mut seen, child_location, depth + 1)?;
                }
                "items" | "oneOf" | "anyOf" | "allOf" | "prefixItems" => {
                    if let Some(schemas) = child.as_array() {
                        for (index, _) in schemas.iter().enumerate() {
                            enqueue(
                                &mut queue,
                                &mut seen,
                                append_pointer(&child_location, &index.to_string()),
                                depth + 1,
                            )?;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    if follow_references {
        reject_container_targets(&reference_targets, &containers)?;
    }
    Ok(SchemaGraph {
        locations,
        literal_ranges,
        reference_targets,
    })
}

fn validate_reference_profile(object: &serde_json::Map<String, Value>) -> FrameworkResult<()> {
    if object
        .get("$ref")
        .and_then(Value::as_str)
        .is_some_and(|reference| {
            reference.starts_with('#') && (reference.contains("%2F") || reference.contains("%2f"))
        })
    {
        return Err(FrameworkError::new(
            "schema_reference_profile_unsupported",
            "checked schemas require literal JSON-pointer separators; escape slash tokens with ~1",
        ));
    }
    Ok(())
}

fn schema_node<'a>(root: &'a Value, location: &str, depth: usize) -> FrameworkResult<&'a Value> {
    if depth > 64 {
        return Err(FrameworkError::new(
            "schema_traversal_limit",
            "schema traversal exceeds 64 steps",
        ));
    }
    root.pointer(location).ok_or_else(|| {
        FrameworkError::new(
            "schema_traversal_invalid",
            "queued schema location does not exist",
        )
    })
}

fn reject_container_targets(
    reference_targets: &BTreeSet<String>,
    containers: &BTreeSet<String>,
) -> FrameworkResult<()> {
    if reference_targets
        .iter()
        .any(|target| containers.contains(target))
    {
        return Err(FrameworkError::new(
            "schema_container_reference_unsupported",
            "schema references to schema dictionaries require context-aware bundling",
        ));
    }
    Ok(())
}

fn reject_literal_targets(
    reference_targets: BTreeSet<String>,
    literal_ranges: &BTreeSet<String>,
) -> FrameworkResult<()> {
    for target in reference_targets {
        if literal_ranges.contains(&target)
            || target
                .rmatch_indices('/')
                .any(|(end, _)| literal_ranges.contains(&target[..end]))
        {
            return Err(FrameworkError::new(
                "schema_literal_reference_unsupported",
                "schema references into literal assertion/default/example data require bundling",
            ));
        }
    }
    Ok(())
}

fn append_pointer(base: &str, segment: &str) -> String {
    format!("{base}/{}", segment.replace('~', "~0").replace('/', "~1"))
}

fn enqueue(
    queue: &mut VecDeque<(String, usize)>,
    seen: &mut BTreeSet<String>,
    location: String,
    depth: usize,
) -> FrameworkResult<()> {
    if seen.insert(location.clone()) {
        if seen.len() > 100_000 {
            return Err(FrameworkError::new(
                "schema_traversal_limit",
                "schema traversal exceeds 100000 locations",
            ));
        }
        queue.push_back((location, depth));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn checked_profiles_reject_encoded_separators_only_at_schema_positions() {
        for reference in [
            "#%2Fdefinitions/Value",
            "#%2fdefinitions/Value",
            "#/definitions%2FValue",
            "#/definitions%2fValue",
        ] {
            let schema = json!({"$ref": reference, "definitions": {"Value": {"type": "string"}}});
            assert_eq!(
                schema_locations(&schema, false).unwrap_err().code(),
                "schema_reference_profile_unsupported"
            );
            assert!(schema_locations(&schema, true).is_ok());
            let literal = json!({"type": "object", "const": {"$ref": reference}, "default": {"$ref": reference}});
            assert!(schema_locations(&literal, false).is_ok());
            let reachable = json!({"$ref": "#/default", "default": schema});
            assert!(schema_locations(&reachable, false).is_err());
        }
    }

    #[test]
    fn reachable_annotation_schemas_and_percent_encoded_targets_are_located_once() {
        let schema = json!({"$schema":"http://json-schema.org/draft-06/schema#","type":"object","properties":{"item":{"$ref":"#/if"}},"if":{"type":"object","properties":{"next":{"$ref":"#/if"},"value":{"$ref":"#/definitions/Value%2525"}}},"definitions":{"Value%25":{"type":"string"}},"then":{"$ref":"literal data"}});
        let locations = schema_locations(&schema, false).unwrap();
        assert!(locations.contains(&"/if/properties/value".into()));
        assert!(locations.contains(&"/definitions/Value%25".into()));
        assert!(!locations.contains(&"/then".into()));
        assert_eq!(
            locations
                .iter()
                .filter(|location| location.as_str() == "/if")
                .count(),
            1
        );
        let relocated: Value =
            serde_json::from_str(&relocate_schema_json(&schema.to_string(), &["payload"])).unwrap();
        assert_eq!(
            relocated["if"]["properties"]["value"]["$ref"],
            "#/payload/definitions/Value%2525"
        );
        assert_eq!(
            relocated["if"]["properties"]["next"]["$ref"],
            "#/payload/if"
        );
        assert_eq!(relocated["then"], schema["then"]);
    }

    #[test]
    fn referenced_draft4_const_annotation_is_schema_storage_not_a_literal_assertion() {
        let schema = json!({"$schema":"http://json-schema.org/draft-04/schema#","$ref":"#/const","const":{"$ref":"#/definitions/Value"},"definitions":{"Value":{"type":"string"}}});
        let relocated: Value = serde_json::from_str(&relocate_schema_json_unscoped(
            &schema.to_string(),
            &["payload"],
        ))
        .unwrap();
        assert_eq!(relocated["$ref"], "#/payload/const");
        assert_eq!(relocated["const"]["$ref"], "#/payload/definitions/Value");
    }

    #[test]
    fn relocated_fragments_escape_uri_bytes_after_pointer_tokens() {
        let schema = json!({"$ref":"#%2Fdefinitions%2FValue%2525","definitions":{"Value%25":{"type":"string"}},"const":{"$ref":"literal %25 水"}});
        let relocated: Value = serde_json::from_str(&relocate_schema_json(
            &schema.to_string(),
            &["components", "schema/~ 水%"],
        ))
        .unwrap();
        let reference = relocated["$ref"].as_str().unwrap();
        assert_eq!(
            reference,
            "#/components/schema~1~0%20%E6%B0%B4%25/definitions/Value%2525"
        );
        let pointer = percent_encoding::percent_decode_str(&reference[1..])
            .decode_utf8()
            .unwrap();
        let document = json!({"components":{"schema/~ 水%":relocated}});
        assert_eq!(
            document.pointer(&pointer),
            Some(&schema["definitions"]["Value%25"])
        );
        assert_eq!(
            document["components"]["schema/~ 水%"]["const"],
            schema["const"]
        );
    }

    #[test]
    fn reachable_schema_traversal_enforces_depth_and_location_budgets() {
        let mut deep = json!({});
        for _ in 0..65 {
            deep = json!({"not":deep});
        }
        assert!(schema_locations(&deep, false).is_err());
        assert!(schema_locations(&json!({"allOf":vec![json!({});100_000]}), false).is_err());
    }

    #[test]
    fn references_into_scoped_resources_keep_the_resources_reference_base() {
        let schema = json!({"properties":{"resource":{"$ref":"#/$defs/Resource/properties/self"}},"$defs":{"Resource":{"$id":"https://example.invalid/resource","properties":{"self":{"$ref":"#"}}}}});
        let relocated: Value =
            serde_json::from_str(&relocate_schema_json(&schema.to_string(), &["payload"])).unwrap();
        assert_eq!(
            relocated["properties"]["resource"]["$ref"],
            "#/payload/$defs/Resource/properties/self"
        );
        assert_eq!(relocated["$defs"]["Resource"], schema["$defs"]["Resource"]);
    }

    #[test]
    fn literal_assertion_targets_are_bundled_instead_of_mutating_wire_data() {
        let schema = json!({"properties":{"item":{"$ref":"#/const"}},"const":{"$ref":"#/definitions/Value"},"definitions":{"Value":{"type":"string"}}});
        assert!(
            schema_locations(&schema, false)
                .unwrap_err()
                .message()
                .contains("literal")
        );
        let relocated: Value =
            serde_json::from_str(&relocate_schema_json(&schema.to_string(), &["payload"])).unwrap();
        assert_eq!(relocated["const"], schema["const"]);
        let reference =
            local_pointer(relocated["properties"]["item"]["$ref"].as_str().unwrap()).unwrap();
        let document = serde_json::json!({"payload":relocated});
        assert_eq!(
            document.pointer(&reference).unwrap()["$ref"],
            "#/payload/definitions/Value"
        );
    }

    #[test]
    fn explicit_older_dialects_preserve_ignored_keyword_literals() {
        for dialect in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
        ] {
            let input = json!({"$schema":dialect,"type":"object","properties":{"value":{"$ref":"#/definitions/Value"}},"definitions":{"Value":{"type":"string"}},"if":{"$ref":"#"},"then":false,"dependentSchemas":{"literal":{"$ref":"#/definitions/Value"}},"prefixItems":[{"$ref":"#"}],"contentSchema":{"$ref":"#"}});
            let output: Value =
                serde_json::from_str(&relocate_schema_json(&input.to_string(), &["payload"]))
                    .unwrap();
            assert_eq!(
                output["properties"]["value"]["$ref"],
                "#/payload/definitions/Value"
            );
            for keyword in [
                "if",
                "then",
                "dependentSchemas",
                "prefixItems",
                "contentSchema",
            ] {
                assert_eq!(output[keyword], input[keyword]);
            }
        }
    }

    #[test]
    fn later_dialect_annotations_keep_literal_reference_locations() {
        for (dialect, keyword) in [
            (
                "http://json-schema.org/draft-07/schema#",
                "dependentSchemas",
            ),
            ("http://json-schema.org/draft-07/schema#", "contentSchema"),
            (
                "https://json-schema.org/draft/2019-09/schema",
                "prefixItems",
            ),
            (
                "https://json-schema.org/draft/2020-12/schema",
                "additionalItems",
            ),
        ] {
            let mut input = json!({"$schema":dialect,"properties":{"value":{"$ref":"#"}}});
            input[keyword] = json!({"$ref":"#/literal"});
            let output: Value =
                serde_json::from_str(&relocate_schema_json(&input.to_string(), &["payload"]))
                    .unwrap();
            assert_eq!(output[keyword], input[keyword]);
            assert_eq!(output["properties"]["value"]["$ref"], "#/payload");
        }
    }

    #[test]
    fn tuple_and_conditional_schema_refs_move_but_embedded_resources_keep_their_base() {
        let input = json!({"type":"array", "items":[{"$ref":"#"}],
            "contains":{"if":{"$ref":"#"},"then":{"$ref":"#"}},
            "$defs":{"Resource":{"$id":"https://example.invalid/nested","type":"object","properties":{"self":{"$ref":"#"}}}},
            "unevaluatedItems":{"$ref":"#"}});
        let value: Value = serde_json::from_str(&relocate_schema_json(
            &input.to_string(),
            &["properties", "body"],
        ))
        .unwrap();
        assert_eq!(value["items"][0]["$ref"], "#/properties/body");
        assert_eq!(value["contains"]["if"]["$ref"], "#/properties/body");
        assert_eq!(value["unevaluatedItems"]["$ref"], "#/properties/body");
        assert_eq!(value["$defs"]["Resource"], input["$defs"]["Resource"]);
        let resource = input["$defs"]["Resource"].to_string();
        assert_eq!(
            serde_json::from_str::<Value>(&relocate_schema_json(
                &resource,
                &["properties", "body"]
            ))
            .unwrap(),
            input["$defs"]["Resource"]
        );
    }

    #[test]
    fn standalone_recursive_refs_are_rebased_without_rewriting_literals_or_names() {
        let input = json!({"$ref":"#/$defs/A", "$defs":{"A":{"type":"object","properties":{
            "$ref":{"type":"string"}, "self":{"$ref":"#"}, "child":{"$ref":"#/$defs/A"}
        },"default":{"$ref":"#"},"examples":[{"$ref":"#/$defs/A"}]}}});
        let value: Value = serde_json::from_str(&relocate_schema_json(
            &input.to_string(),
            &["components", "schemas", "Tree/Node~"],
        ))
        .unwrap();
        let base = "#/components/schemas/Tree~1Node~0";
        assert_eq!(value["$ref"], format!("{base}/$defs/A"));
        assert_eq!(value["$defs"]["A"]["properties"]["self"]["$ref"], base);
        assert_eq!(value["$defs"]["A"]["properties"]["$ref"]["type"], "string");
        assert_eq!(
            value["$defs"]["A"]["default"],
            input["$defs"]["A"]["default"]
        );
        assert_eq!(
            value["$defs"]["A"]["examples"],
            input["$defs"]["A"]["examples"]
        );
    }
}

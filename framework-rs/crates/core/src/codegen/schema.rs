//! Bounded, document-local schema graphs. References remain edges, including cycles.
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

#[path = "schema_resources.rs"]
mod resources;

pub(crate) struct ExpansionBudget {
    pub(crate) nodes: usize,
    pub(crate) bytes: usize,
}
impl Default for ExpansionBudget {
    fn default() -> Self {
        Self {
            nodes: 100_000,
            bytes: 32 * 1024 * 1024,
        }
    }
}

pub(crate) fn clone_literal(value: &Value, budget: &mut ExpansionBudget) -> Result<Value> {
    // Literal annotations are data, but must obey the same resource bounds
    // before recursive serialization/cloning runs on a caller-supplied Value.
    let mut pending = vec![(value, 0)];
    while let Some((value, depth)) = pending.pop() {
        if depth > 64 || budget.nodes == 0 {
            bail!("SDK schema literals exceed expansion limits");
        }
        budget.nodes -= 1;
        let children = match value {
            Value::Array(values) => values.len(),
            Value::Object(values) => values.len(),
            _ => 0,
        };
        if children > budget.nodes.saturating_sub(pending.len()) {
            bail!("SDK schema literals exceed expansion limits");
        }
        match value {
            Value::Array(values) => pending.extend(values.iter().map(|value| (value, depth + 1))),
            Value::Object(values) => {
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            _ => {}
        }
    }
    let mut counter = LiteralByteBudget {
        remaining: budget.bytes,
    };
    serde_json::to_writer(&mut counter, value).context("SDK schema graph exceeds 32 MiB")?;
    budget.bytes = counter.remaining;
    Ok(value.clone())
}

struct LiteralByteBudget {
    remaining: usize,
}
impl std::io::Write for LiteralByteBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.remaining = self
            .remaining
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("SDK schema byte budget exceeded"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// URI fragments encode JSON pointer text; decode once before pointer token
// unescaping. Canonical decoded paths also identify equivalent graph edges.
pub(crate) fn document_pointer(reference: &str) -> Option<String> {
    let fragment = reference.strip_prefix('#')?;
    let pointer = percent_encoding::percent_decode_str(fragment)
        .decode_utf8()
        .ok()?;
    (pointer.is_empty() || pointer.starts_with('/')).then(|| pointer.into_owned())
}

pub(crate) const REFERENCE: &str = "x-forklaunch-sdk-reference";
pub(crate) const DEFINITIONS: &str = "x-forklaunch-sdk-definitions";

/// Reuses the bounded resource index while importing schemas from one document.
/// The document borrow keeps pointer-based lexical scopes valid for the cache.
#[allow(dead_code)] // Used by the CLI OpenAPI importer through the shared source module.
pub(crate) struct DocumentSchemas<'a> {
    document: &'a Value,
    resources: Option<std::sync::Arc<resources::Resources>>,
}
#[allow(dead_code)] // Used by the CLI OpenAPI importer through the shared source module.
impl<'a> DocumentSchemas<'a> {
    pub(crate) fn new(document: &'a Value) -> Self {
        Self {
            document,
            resources: None,
        }
    }
    pub(crate) fn bundle(&mut self, schema: &Value, budget: &mut ExpansionBudget) -> Result<Value> {
        if self.resources.is_none() {
            self.resources = Some(std::sync::Arc::new(resources::Resources::index(
                self.document,
                budget,
            )?));
        }
        bundle_runtime_profile(
            schema,
            self.document,
            budget,
            true,
            false,
            false,
            self.resources.clone(),
        )
    }
}

pub(crate) fn bundle(
    schema: &Value,
    document: &Value,
    budget: &mut ExpansionBudget,
) -> Result<Value> {
    bundle_with_resource_policy(schema, document, budget, true)
}

/// Eliminate resource scopes before placing a standalone schema into a union.
/// Resource-free schemas retain their original annotations and pointer layout.
#[allow(dead_code)] // Shared CLI module only uses model generation.
pub(crate) fn scoped_runtime_schema(schema: &Value) -> Result<Value> {
    let mut budget = ExpansionBudget::default();
    let effective = standalone_runtime_schema(schema, &mut budget)?;
    let resources = resources::Resources::index(&effective, &mut budget)?;
    if resources.needs_runtime_bundle() {
        bundle_with_profile(&effective, &effective, &mut budget, true, true)
    } else {
        clone_literal(schema, &mut budget)
    }
}

/// Convert an already checked standalone message before joining mixed drafts.
#[allow(dead_code)] // Shared CLI source only generates HTTP models.
pub(crate) fn runtime_schema_2020(schema: &Value) -> Result<Value> {
    let mut budget = ExpansionBudget::default();
    let effective = standalone_runtime_schema(schema, &mut budget)?;
    bundle_runtime_profile(&effective, &effective, &mut budget, true, true, true, None)
}

/// Standalone socket codecs default to Draft 7; HTTP model schemas retain
/// their document's dialect policy. Never mutate caller-owned annotations.
#[allow(dead_code)] // Shared CLI module only uses HTTP model generation.
pub(crate) fn standalone_runtime_schema(
    schema: &Value,
    budget: &mut ExpansionBudget,
) -> Result<Value> {
    let mut effective = clone_literal(schema, budget)?;
    if let Some(object) = effective.as_object_mut() {
        object
            .entry("$schema")
            .or_insert_with(|| json!("http://json-schema.org/draft-07/schema#"));
    }
    Ok(effective)
}

#[allow(dead_code)] // The shared CLI copy does not import AsyncAPI documents.
pub(crate) fn bundle_document_resources(document: &Value, location: &str) -> Result<Option<Value>> {
    let mut budget = ExpansionBudget::default();
    let resources = resources::Resources::index(document, &mut budget)?;
    if !resources.needs_runtime_bundle() {
        return Ok(None);
    }
    let schema = document
        .pointer(location)
        .context("Document schema location is missing")?;
    resources.check_target(schema)?;
    bundle_with_profile(schema, document, &mut budget, true, true).map(Some)
}

pub(crate) fn bundle_with_resource_policy(
    schema: &Value,
    document: &Value,
    budget: &mut ExpansionBudget,
    resource_ids: bool,
) -> Result<Value> {
    bundle_with_profile(schema, document, budget, resource_ids, false)
}

fn bundle_with_profile(
    schema: &Value,
    document: &Value,
    budget: &mut ExpansionBudget,
    resource_ids: bool,
    runtime: bool,
) -> Result<Value> {
    bundle_runtime_profile(schema, document, budget, resource_ids, runtime, false, None)
}

fn bundle_runtime_profile(
    schema: &Value,
    document: &Value,
    budget: &mut ExpansionBudget,
    resource_ids: bool,
    runtime: bool,
    translate: bool,
    resources: Option<std::sync::Arc<resources::Resources>>,
) -> Result<Value> {
    let mut graph = Graph {
        document,
        budget,
        resource_ids,
        runtime,
        runtime_dialects: BTreeSet::new(),
        translate,
        names: BTreeMap::new(),
        definitions: Map::new(),
        resources,
    };
    let mut root = graph.visit(schema, 0, &resources::Bindings::new())?;
    if runtime && !translate && graph.runtime_dialects.len() > 1 {
        return bundle_runtime_profile(
            schema,
            document,
            graph.budget,
            resource_ids,
            true,
            true,
            graph.resources.clone(),
        );
    }
    // TypeScript cannot represent a cycle consisting only of aliases/unions.
    // Object properties and array items provide the necessary structural guard.

    let mut done = BTreeSet::new();
    for name in graph.definitions.keys() {
        check_aliases(name, &graph.definitions, &mut Vec::new(), &mut done, 0)?;
    }

    let original = graph.definitions.clone();
    graph
        .definitions
        .retain(|_, schema| schema.get("$ref").is_none());
    for definition in graph.definitions.values_mut() {
        collapse(definition, &original)?;
    }
    collapse(&mut root, &original)?;

    let edges = graph
        .definitions
        .iter()
        .map(|(name, schema)| {
            let mut targets = BTreeSet::new();
            dependencies(schema, &mut targets);
            (name.clone(), targets)
        })
        .collect::<BTreeMap<_, _>>();

    let mut inline = BTreeMap::new();
    for name in edges.keys() {
        safe_inline(name, &edges, &mut inline, 0)?;
    }

    let definitions = graph.definitions.clone();
    graph.definitions.retain(|name, _| !inline[name]);
    for definition in graph.definitions.values_mut() {
        expand(definition, &definitions, &inline, graph.budget, 0)?;
    }
    expand(&mut root, &definitions, &inline, graph.budget, 0)?;
    if let Some(reference) = root.get("$ref").and_then(Value::as_str) {
        let mut expanded = graph.definitions[reference.strip_prefix("#/$defs/").unwrap()].clone();
        for (key, value) in root.as_object().unwrap() {
            if key != "$ref" {
                expanded[key] = value.clone();
            }
        }
        root = expanded;
    }
    if !graph.definitions.is_empty() {
        root.as_object_mut()
            .context("Referenced schema must be an object")?
            .insert("$defs".into(), Value::Object(graph.definitions));
    }
    if translate {
        if root.is_boolean() {
            root = json!({"allOf": [root]});
        }
        root["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    }
    Ok(root)
}

type AnchorVisit<'a> = (&'a Value, String, bool, usize);
fn queue_anchor<'a>(
    pending: &mut Vec<AnchorVisit<'a>>,
    budget: &mut ExpansionBudget,
    entry: AnchorVisit<'a>,
) -> Result<()> {
    if pending.len() >= budget.nodes {
        bail!("SDK schema anchor index exceeds expansion limits");
    }
    budget.bytes = budget
        .bytes
        .checked_sub(entry.1.len())
        .context("SDK schema anchor index exceeds 32 MiB")?;
    pending.push(entry);
    Ok(())
}

struct Graph<'a> {
    document: &'a Value,
    budget: &'a mut ExpansionBudget,
    names: BTreeMap<(String, resources::Bindings), String>,
    definitions: Map<String, Value>,
    resources: Option<std::sync::Arc<resources::Resources>>,
    resource_ids: bool,
    runtime: bool,
    runtime_dialects: BTreeSet<String>,
    translate: bool,
}

impl Graph<'_> {
    #[allow(clippy::too_many_lines)]
    fn visit(
        &mut self,
        schema: &Value,
        depth: usize,
        inherited: &resources::Bindings,
    ) -> Result<Value> {
        if depth > 64 || self.budget.nodes == 0 {
            bail!("SDK schema graph exceeds expansion limits");
        }
        self.budget.nodes -= 1;
        let Some(object) = schema.as_object() else {
            if !schema.is_boolean() {
                bail!("JSON schemas must be objects or booleans");
            }
            return clone_literal(schema, self.budget);
        };
        if self.resources.is_none() {
            self.resources = Some(std::sync::Arc::new(resources::Resources::index(
                self.document,
                self.budget,
            )?));
        }
        let resources = self.resources.as_ref().unwrap();
        let dialect = resources.dialect(schema).to_owned();
        if self.runtime && (!dialect.is_empty() || !object.contains_key("$ref")) {
            let dialect = resources::dialect_key(&dialect);
            if !self.runtime_dialects.contains(dialect) {
                self.budget.bytes = self
                    .budget
                    .bytes
                    .checked_sub(dialect.len())
                    .context("SDK schema graph exceeds 32 MiB")?;
                self.runtime_dialects.insert(dialect.to_owned());
            }
        }
        if self.translate
            && !matches!(
                resources::dialect_key(&dialect),
                "draft-04" | "draft-06" | "draft-07" | "2019-09" | "2020-12"
            )
        {
            bail!("Mixed runtime schemas require a supported JSON Schema dialect");
        }
        let draft4 = dialect.contains("/draft-04/");
        let resource_key = if draft4 { "id" } else { "$id" };
        if object.contains_key(resource_key) {
            if !self.resource_ids {
                bail!("Resource IDs are unsupported in this SDK profile");
            }
            resources.check_target(schema)?;
        }
        if object.contains_key("$recursiveRef")
            && (!self.resource_ids || !dialect.contains("2019-09"))
        {
            bail!("Recursive references require the 2019-09 HTTP SDK profile");
        }
        if object.contains_key("$dynamicRef")
            && (!self.resource_ids || !dialect.contains("2020-12"))
        {
            bail!("Dynamic references require the 2020-12 HTTP SDK profile");
        }
        let bindings = resources.bindings(schema, inherited, self.budget)?;
        if object.contains_key(REFERENCE) || object.contains_key(DEFINITIONS) {
            bail!("Reserved SDK schema annotation");
        }
        let reference_keys = ["$ref", "$dynamicRef", "$recursiveRef"];
        let mut sibling_references = Vec::new();
        for reference_key in reference_keys {
            let Some(reference) = object.get(reference_key) else {
                continue;
            };
            let reference = reference.as_str().context("Schema $ref must be a string")?;
            self.budget.bytes = self
                .budget
                .bytes
                .checked_sub(reference.len() + object.keys().map(String::len).sum::<usize>())
                .context("SDK schema graph exceeds 32 MiB")?;
            let resources = self.resources.as_ref().unwrap();
            let pointer = if reference_key == "$recursiveRef" {
                resources.recursive_pointer(schema, reference, &bindings)?
            } else if reference_key == "$dynamicRef" {
                resources.dynamic_pointer(schema, reference, &bindings)?
            } else {
                resources.pointer(schema, reference)?
            };
            let target = self
                .document
                .pointer(&pointer)
                .with_context(|| format!("Missing schema reference {reference}"))?;
            self.resources.as_ref().unwrap().check_target(target)?;
            let target_bindings =
                self.resources
                    .as_ref()
                    .unwrap()
                    .bindings(target, &bindings, self.budget)?;
            let identity = (pointer, target_bindings);
            let name = if let Some(name) = self.names.get(&identity) {
                name.clone()
            } else {
                let name = format!("Ref{}", self.names.len() + 1);
                // Reserve before visiting to terminate self and mutual recursion.
                let target_bindings =
                    self.resources
                        .as_ref()
                        .unwrap()
                        .bindings(target, &identity.1, self.budget)?;
                self.names.insert(identity, name.clone());
                let resolved = self.visit(target, depth + 1, &target_bindings)?;
                self.definitions.insert(name.clone(), resolved);
                name
            };
            let mut resolved = json!({"$ref": format!("#/$defs/{name}")});
            let has_constraints = object.keys().any(|key| {
                key != reference_key
                    && key != resource_key
                    && !matches!(
                        key.as_str(),
                        "description"
                            | "summary"
                            | "title"
                            | "$schema"
                            | "$defs"
                            | "definitions"
                            | "$anchor"
                            | "$dynamicAnchor"
                            | "$recursiveAnchor"
                            | "$id"
                    )
                    && !key.starts_with("x-")
                    && !resources::ignored_keyword(&dialect, key)
            });
            if has_constraints {
                // Keep sibling keywords at the containing schema location.
                // In particular, unevaluatedProperties/Items must see the
                // annotations produced by the reference's successful target.
                sibling_references.push(resolved);
            } else {
                for (key, value) in object {
                    if !(self.translate && key == "$schema")
                        && key != reference_key
                        && key != resource_key
                        && !matches!(
                            key.as_str(),
                            "$defs"
                                | "definitions"
                                | "$anchor"
                                | "$dynamicAnchor"
                                | "$recursiveAnchor"
                                | "$id"
                        )
                        && !resources::ignored_keyword(&dialect, key)
                    {
                        resolved[key] = clone_literal(value, self.budget)?;
                    }
                }
                return Ok(resolved);
            }
        }
        let mut result = Map::new();
        for (key, value) in object {
            if key == resource_key
                || reference_keys.contains(&key.as_str())
                || (self.translate && key == "$schema")
            {
                continue;
            }
            // Older dialects treat later validation keywords as annotations.
            // Keep literal data intact; only schema positions pass through visit.
            if resources::ignored_keyword(&dialect, key) {
                continue;
            }
            self.budget.bytes = self
                .budget
                .bytes
                .checked_sub(key.len())
                .context("SDK schema graph exceeds 32 MiB")?;
            let value = match key.as_str() {
                // Only reachable definitions participate in the graph.
                "$defs" | "definitions" | "$anchor" | "$dynamicAnchor" | "$recursiveAnchor"
                | "$id" => continue,
                "properties" | "patternProperties" | "dependentSchemas" | "dependencies" => {
                    let mut properties = Map::new();
                    for (name, schema) in value
                        .as_object()
                        .context("Named schemas must be an object")?
                    {
                        self.budget.bytes = self
                            .budget
                            .bytes
                            .checked_sub(name.len())
                            .context("SDK schema graph exceeds 32 MiB")?;
                        let schema = if key == "dependencies" && schema.is_array() {
                            if !schema.as_array().unwrap().iter().all(Value::is_string) {
                                bail!("Property dependencies must be strings");
                            }
                            clone_literal(schema, self.budget)?
                        } else {
                            self.visit(schema, depth + 1, &bindings)?
                        };
                        properties.insert(name.clone(), schema);
                    }
                    Value::Object(properties)
                }
                "items" if value.is_array() => Value::Array(
                    value
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|schema| self.visit(schema, depth + 1, &bindings))
                        .collect::<Result<_>>()?,
                ),
                "items"
                | "additionalProperties"
                | "additionalItems"
                | "unevaluatedProperties"
                | "unevaluatedItems"
                | "contentSchema"
                | "contains"
                | "not"
                | "if"
                | "then"
                | "else"
                | "propertyNames" => self.visit(value, depth + 1, &bindings)?,
                "oneOf" | "anyOf" | "allOf" | "prefixItems" => Value::Array(
                    value
                        .as_array()
                        .context("Schema alternatives must be an array")?
                        .iter()
                        .map(|value| self.visit(value, depth + 1, &bindings))
                        .collect::<Result<_>>()?,
                ),
                _ => clone_literal(value, self.budget)?,
            };
            result.insert(key.clone(), value);
        }
        if !sibling_references.is_empty() {
            result
                .entry("allOf")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .context("Schema allOf must be an array")?
                .extend(sibling_references);
        }
        if self.translate {
            translate_runtime_keywords(&mut result, &dialect)?;
        }
        Ok(Value::Object(result))
    }
}

/// References are resolved against the original resource graph before keyword
/// conversion, so moving tuple schemas cannot change pointer identities.
fn translate_runtime_keywords(object: &mut Map<String, Value>, dialect: &str) -> Result<()> {
    let draft = resources::dialect_key(dialect);
    if draft == "draft-04" {
        for (exclusive, inclusive) in [
            ("exclusiveMinimum", "minimum"),
            ("exclusiveMaximum", "maximum"),
        ] {
            if let Some(flag) = object.remove(exclusive) {
                let flag = flag
                    .as_bool()
                    .context("Draft 4 exclusive bounds must be booleans")?;
                if flag && let Some(bound) = object.remove(inclusive) {
                    object.insert(exclusive.into(), bound);
                }
            }
        }
    }
    if draft != "2020-12" {
        let tail = object.remove("additionalItems");
        if object.get("items").is_some_and(Value::is_array) {
            let tuple = object.remove("items").unwrap();
            object.insert("prefixItems".into(), tuple);
            if let Some(tail) = tail {
                object.insert("items".into(), tail);
            }
        }
    }
    Ok(())
}

/// Assign operation-independent, service-scoped names after importing the schema.
pub(crate) fn normalize(schema: &Value, prefix: &str) -> Result<Value> {
    let mut bundled = bundle(schema, schema, &mut ExpansionBudget::default())?;
    let definitions = bundled
        .as_object_mut()
        .and_then(|object| object.remove("$defs"));

    lower(&mut bundled, prefix);
    if let Some(Value::Object(definitions)) = definitions {
        let mut named = Map::new();
        for (name, mut schema) in definitions {
            lower(&mut schema, prefix);
            named.insert(format!("{prefix}{name}"), schema);
        }
        bundled[DEFINITIONS] = Value::Object(named);
    }
    Ok(bundled)
}

fn visit_alias_edges(
    value: &Value,
    definitions: &Map<String, Value>,
    active: &mut Vec<String>,
    done: &mut BTreeSet<String>,
    depth: usize,
) -> Result<()> {
    if depth > 64 {
        bail!("SDK schema graph exceeds expansion limits");
    }
    if let Some(reference) = value.get("$ref").and_then(Value::as_str) {
        check_aliases(
            reference.strip_prefix("#/$defs/").unwrap(),
            definitions,
            active,
            done,
            depth + 1,
        )?;
    }
    for key in ["oneOf", "anyOf", "allOf"] {
        if let Some(variants) = value.get(key).and_then(Value::as_array) {
            for variant in variants {
                visit_alias_edges(variant, definitions, active, done, depth + 1)?;
            }
        }
    }
    for key in ["not", "if", "then", "else"] {
        if let Some(schema) = value.get(key) {
            visit_alias_edges(schema, definitions, active, done, depth + 1)?;
        }
    }
    for key in ["dependencies", "dependentSchemas"] {
        if let Some(schemas) = value.get(key).and_then(Value::as_object) {
            for schema in schemas.values().filter(|schema| !schema.is_array()) {
                visit_alias_edges(schema, definitions, active, done, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn check_aliases(
    name: &str,
    definitions: &Map<String, Value>,
    active: &mut Vec<String>,
    done: &mut BTreeSet<String>,
    depth: usize,
) -> Result<()> {
    if depth > 64 {
        bail!("SDK schema graph exceeds expansion limits");
    }
    if done.contains(name) {
        return Ok(());
    }
    if active.iter().any(|item| item == name) {
        bail!("Recursive schema aliases require an object or array guard");
    }
    active.push(name.into());

    visit_alias_edges(&definitions[name], definitions, active, done, depth + 1)?;
    active.pop();
    done.insert(name.into());
    Ok(())
}

fn canonical<'a>(mut name: &'a str, definitions: &'a Map<String, Value>) -> Result<&'a str> {
    let mut depth = 0;
    while let Some(reference) = definitions[name].get("$ref").and_then(Value::as_str) {
        depth += 1;
        if depth > 64 {
            bail!("SDK schema graph exceeds expansion limits");
        }
        name = reference.strip_prefix("#/$defs/").unwrap();
    }
    Ok(name)
}

fn collapse(value: &mut Value, definitions: &Map<String, Value>) -> Result<()> {
    let Some(object) = value.as_object_mut() else {
        return Ok(());
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        let name = canonical(reference.strip_prefix("#/$defs/").unwrap(), definitions)?;
        object.insert("$ref".into(), json!(format!("#/$defs/{name}")));
    }
    for (key, value) in object {
        match key.as_str() {
            "properties" | "patternProperties" | "dependentSchemas" | "dependencies" => {
                if let Some(properties) = value.as_object_mut() {
                    for schema in properties.values_mut() {
                        collapse(schema, definitions)?;
                    }
                }
            }
            "items" if value.is_array() => {
                for schema in value.as_array_mut().unwrap() {
                    collapse(schema, definitions)?;
                }
            }
            "items"
            | "additionalProperties"
            | "additionalItems"
            | "unevaluatedProperties"
            | "unevaluatedItems"
            | "contentSchema"
            | "contains"
            | "not"
            | "if"
            | "then"
            | "else"
            | "propertyNames" => collapse(value, definitions)?,
            "oneOf" | "anyOf" | "allOf" | "prefixItems" => {
                if let Some(variants) = value.as_array_mut() {
                    for schema in variants {
                        collapse(schema, definitions)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn dependencies(value: &Value, names: &mut BTreeSet<String>) {
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        names.insert(reference.strip_prefix("#/$defs/").unwrap().into());
    }
    for (key, value) in object {
        match key.as_str() {
            "properties" | "patternProperties" | "dependentSchemas" | "dependencies" => {
                if let Some(properties) = value.as_object() {
                    for schema in properties.values() {
                        dependencies(schema, names);
                    }
                }
            }
            "items" if value.is_array() => {
                for schema in value.as_array().unwrap() {
                    dependencies(schema, names);
                }
            }
            "items"
            | "additionalProperties"
            | "additionalItems"
            | "unevaluatedProperties"
            | "unevaluatedItems"
            | "contentSchema"
            | "contains"
            | "not"
            | "if"
            | "then"
            | "else"
            | "propertyNames" => dependencies(value, names),
            "oneOf" | "anyOf" | "allOf" | "prefixItems" => {
                if let Some(variants) = value.as_array() {
                    for schema in variants {
                        dependencies(schema, names);
                    }
                }
            }
            _ => {}
        }
    }
}

fn safe_inline(
    name: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    memo: &mut BTreeMap<String, bool>,
    depth: usize,
) -> Result<bool> {
    if depth > 64 {
        bail!("SDK schema graph exceeds expansion limits");
    }
    if let Some(result) = memo.get(name) {
        return Ok(*result);
    }
    memo.insert(name.into(), false);
    let mut result = true;
    for child in &edges[name] {
        if !safe_inline(child, edges, memo, depth + 1)? {
            result = false;
            break;
        }
    }
    memo.insert(name.into(), result);
    Ok(result)
}

fn expand(
    value: &mut Value,
    definitions: &Map<String, Value>,
    inline: &BTreeMap<String, bool>,
    budget: &mut ExpansionBudget,
    depth: usize,
) -> Result<()> {
    if depth > 64 || budget.nodes == 0 {
        bail!("SDK schema graph exceeds expansion limits");
    }
    budget.nodes -= 1;
    if let Some(reference) = value.get("$ref").and_then(Value::as_str) {
        let name = reference.strip_prefix("#/$defs/").unwrap();
        if inline[name] {
            let mut replacement = clone_literal(&definitions[name], budget)?;
            for (key, annotation) in value.as_object().unwrap() {
                if key != "$ref" {
                    replacement
                        .as_object_mut()
                        .context("Schema reference annotations require an object target")?
                        .insert(key.clone(), clone_literal(annotation, budget)?);
                }
            }
            *value = replacement;
            return expand(value, definitions, inline, budget, depth + 1);
        }
    }
    if let Some(object) = value.as_object_mut() {
        for (key, value) in object {
            match key.as_str() {
                "properties" | "patternProperties" | "dependentSchemas" | "dependencies" => {
                    if let Some(properties) = value.as_object_mut() {
                        for schema in properties.values_mut() {
                            expand(schema, definitions, inline, budget, depth + 1)?;
                        }
                    }
                }
                "items" if value.is_array() => {
                    for schema in value.as_array_mut().unwrap() {
                        expand(schema, definitions, inline, budget, depth + 1)?;
                    }
                }
                "items"
                | "additionalProperties"
                | "additionalItems"
                | "unevaluatedProperties"
                | "unevaluatedItems"
                | "contentSchema"
                | "contains"
                | "not"
                | "if"
                | "then"
                | "else"
                | "propertyNames" => expand(value, definitions, inline, budget, depth + 1)?,
                "oneOf" | "anyOf" | "allOf" | "prefixItems" => {
                    if let Some(variants) = value.as_array_mut() {
                        for schema in variants {
                            expand(schema, definitions, inline, budget, depth + 1)?;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn lower(value: &mut Value, prefix: &str) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.remove("$ref") {
                let name = reference
                    .as_str()
                    .unwrap()
                    .strip_prefix("#/$defs/")
                    .unwrap();
                object.insert(REFERENCE.into(), json!(format!("{prefix}{name}")));
            }
            for (key, value) in object {
                match key.as_str() {
                    "properties" | "patternProperties" | "dependentSchemas" | "dependencies" => {
                        if let Some(properties) = value.as_object_mut() {
                            for schema in properties.values_mut() {
                                lower(schema, prefix);
                            }
                        }
                    }
                    "items"
                    | "additionalProperties"
                    | "additionalItems"
                    | "unevaluatedProperties"
                    | "unevaluatedItems"
                    | "contentSchema"
                    | "contains"
                    | "not"
                    | "if"
                    | "then"
                    | "else"
                    | "propertyNames"
                    | "oneOf"
                    | "anyOf"
                    | "allOf"
                    | "prefixItems" => lower(value, prefix),
                    _ => {}
                }
            }
        }
        Value::Array(values) => values.iter_mut().for_each(|value| lower(value, prefix)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_runtime_translation_uses_reachable_resources_only() {
        let unused = json!({"$schema":"http://json-schema.org/draft-07/schema#",
        "$id":"https://schemas.test/root", "type":"string", "definitions":{
            "Unused":{"$schema":"https://schemas.test/custom", "$id":"unused", "type":"number"}
        }});
        assert_eq!(
            scoped_runtime_schema(&unused).unwrap()["$schema"],
            unused["$schema"]
        );
        let mut reachable = unused;
        reachable["$ref"] = json!("unused");
        assert!(
            scoped_runtime_schema(&reachable)
                .unwrap_err()
                .to_string()
                .contains("supported JSON Schema dialect")
        );
        let no_ids = json!({"$schema":"https://json-schema.org/draft/2020-12/schema", "properties":{
            "legacy":{"$schema":"http://json-schema.org/draft-04/schema#", "minimum":1, "exclusiveMinimum":true}
        }});
        let original = no_ids.clone();
        let compiled = scoped_runtime_schema(&no_ids).unwrap();
        assert_eq!(compiled["properties"]["legacy"]["exclusiveMinimum"], 1);
        assert!(compiled["properties"]["legacy"].get("$schema").is_none());
        assert_eq!(no_ids, original);
    }

    #[test]
    fn inactive_dialect_keywords_do_not_index_resources_or_validate_annotations() {
        for (dialect, keywords) in [
            (
                "http://json-schema.org/draft-07/schema#",
                vec![
                    "dependentSchemas",
                    "prefixItems",
                    "contentSchema",
                    "unevaluatedProperties",
                ],
            ),
            (
                "https://json-schema.org/draft/2019-09/schema",
                vec!["prefixItems"],
            ),
            (
                "https://json-schema.org/draft/2020-12/schema",
                vec!["additionalItems"],
            ),
        ] {
            for keyword in keywords {
                let mut value = json!({"$schema":dialect,"type":"string"});
                value[keyword] = json!({"$id":42,"$ref":"https://unbundled.invalid/annotation"});
                let original = value.clone();
                let bundled = bundle(&value, &value, &mut ExpansionBudget::default()).unwrap();
                assert!(bundled.get(keyword).is_none());
                assert_eq!(scoped_runtime_schema(&value).unwrap(), original);
                assert_eq!(value, original);
            }
        }
        let implicit = json!({"type":"string", "prefixItems":[{"$id":42}]});
        assert_eq!(scoped_runtime_schema(&implicit).unwrap(), implicit);
    }

    #[test]
    fn same_instance_applicator_cycles_fail_without_rejecting_property_recursion() {
        for keyword in ["not", "if", "then", "else"] {
            let mut value = json!({});
            value[keyword] = json!({"$ref":"#"});
            assert!(normalize(&value, "Cycle").is_err(), "{keyword}");
        }
        for keyword in ["dependencies", "dependentSchemas"] {
            let mut value = json!({});
            value[keyword] = json!({"trigger":{"$ref":"#"}});
            assert!(normalize(&value, "Cycle").is_err(), "{keyword}");
        }
        assert!(
            normalize(
                &json!({"type":"object","properties":{"child":{"$ref":"#"}}}),
                "Tree"
            )
            .is_ok()
        );
    }

    #[test]
    fn checked_references_require_literal_pointer_separators() {
        for reference in [
            "#%2Fdefinitions/Value",
            "#%2fdefinitions/Value",
            "#/definitions%2FValue",
            "#/definitions%2fValue",
        ] {
            let schema = json!({"$ref": reference, "definitions": {"Value": {"type": "string"}}});
            assert!(normalize(&schema, "Encoded").is_err());
        }
        let schema = json!({"$ref": "#/definitions/Value%252F", "definitions": {"Value%2F": {"type": "string"}}});
        assert!(normalize(&schema, "LiteralPercent").is_ok());
        assert_eq!(document_pointer("#"), Some(String::new()));
    }

    #[test]
    fn literal_data_cannot_bypass_graph_resource_limits() {
        let mut deep = json!(null);
        for _ in 0..66 {
            deep = json!([deep]);
        }
        assert!(normalize(&json!({"type":"string","default":deep}), "Literal").is_err());
        let mut budget = ExpansionBudget {
            nodes: 2,
            bytes: 100,
        };
        assert!(clone_literal(&json!([0, 1]), &mut budget).is_err());
        let mut budget = ExpansionBudget { nodes: 2, bytes: 3 };
        assert!(clone_literal(&json!("\""), &mut budget).is_err());
        let mut budget = ExpansionBudget { nodes: 2, bytes: 4 };
        assert_eq!(
            clone_literal(&json!("\""), &mut budget).unwrap(),
            json!("\"")
        );
        assert_eq!(budget.bytes, 0);
    }

    #[test]
    fn self_and_mutual_recursion_terminate_with_named_edges() {
        let schema = json!({"$ref":"#/$defs/Node", "$defs":{
            "Node":{"type":"object","properties":{"child":{"$ref":"#/$defs/Node"},"peer":{"$ref":"#/$defs/Peer"}}},
            "Peer":{"type":"array","items":{"$ref":"#/$defs/Node"}}
        }});
        let value = normalize(&schema, "Tree").unwrap();
        assert_eq!(value["type"], "object");
        assert_eq!(
            value[DEFINITIONS]["TreeRef1"]["properties"]["child"][REFERENCE],
            "TreeRef1"
        );
        assert_eq!(
            value[DEFINITIONS]["TreeRef2"]["items"][REFERENCE],
            "TreeRef1"
        );
        assert_eq!(value[DEFINITIONS].as_object().unwrap().len(), 2);
    }

    #[test]
    fn root_references_property_names_and_literals_are_distinct() {
        let schema = json!({"type":"object","properties":{
            "$ref":{"type":"string"}, "nested":{"$ref":"#"}
        },"const":{"$ref":"https://example.invalid/literal"}});
        let value = normalize(&schema, "Root").unwrap();
        assert_eq!(value["properties"]["$ref"]["type"], "string");
        assert_eq!(value["properties"]["nested"][REFERENCE], "RootRef1");
        assert_eq!(value["const"], schema["const"]);
    }

    #[test]
    fn encoded_schema_fragments_decode_once_and_equivalent_recursive_edges_share_models() {
        let schema = json!({"type":"object","properties":{
            "encoded":{"$ref":"#/$defs/Node%2525~1~0%20%E6%B0%B4"},
            "equivalent":{"$ref":"#/%24defs/%4Eode%2525~1~0%20%E6%B0%B4"}
        },"$defs":{"Node%25/~ 水":{"type":"object","properties":{"value":{"type":"string"},"next":{"$ref":"#/$defs/Node%2525~1~0%20%E6%B0%B4"}}}}});
        let value = normalize(&schema, "Encoded").unwrap();
        assert_eq!(value["properties"]["encoded"][REFERENCE], "EncodedRef1");
        assert_eq!(value["properties"]["equivalent"][REFERENCE], "EncodedRef1");
        assert_eq!(value[DEFINITIONS].as_object().unwrap().len(), 1);
        assert_eq!(
            value[DEFINITIONS]["EncodedRef1"]["properties"]["next"][REFERENCE],
            "EncodedRef1"
        );
        for reference in ["#/%FF", "#missing", "https://example.invalid"] {
            assert!(normalize(&json!({"$ref":reference}), "Invalid").is_err());
        }
    }

    #[test]
    fn static_anchors_share_recursive_edges_and_ignore_literal_annotations() {
        let schema = json!({"type":"object", "properties":{
            "anchored":{"$ref":"#N%6Fde"}, "pointer":{"$ref":"#/$defs/Node"}
        }, "$defs":{"Node":{"$anchor":"Node", "type":"object", "properties":{
            "next":{"$ref":"#Node"}, "value":{"type":"string"}
        }}}, "examples":[{"$anchor":"Node"}], "default":{"$anchor":"Node"}});
        let value = normalize(&schema, "Anchored").unwrap();
        assert_eq!(value["properties"]["anchored"][REFERENCE], "AnchoredRef1");
        assert_eq!(value["properties"]["pointer"][REFERENCE], "AnchoredRef1");
        assert_eq!(value[DEFINITIONS].as_object().unwrap().len(), 1);
        assert_eq!(value["examples"], schema["examples"]);
        assert_eq!(value["default"], schema["default"]);
        let document = json!({"openapi":"3.1.0", "components":{"schemas":{"Node":schema["$defs"]["Node"]}}, "paths":{}});
        let bundled = bundle(
            &json!({"$ref":"#Node"}),
            &document,
            &mut ExpansionBudget::default(),
        )
        .unwrap();
        assert_eq!(bundled["properties"]["value"]["type"], "string");
    }

    #[test]
    fn invalid_duplicate_and_foreign_resource_anchors_fail_closed() {
        for schema in [
            json!({"$ref":"#Node", "$defs":{"A":{"$anchor":"Node"}, "B":{"$anchor":"Node"}}}),
            json!({"$ref":"#1invalid", "$defs":{"A":{"$anchor":"1invalid"}}}),
            json!({"$ref":"#Node", "$defs":{"A":{"$id":"https://other.test", "$anchor":"Node"}}}),
            json!({"$ref":"#Node", "examples":[{"$anchor":"Node", "type":"string"}]}),
            json!({"$schema":"http://json-schema.org/draft-07/schema#", "$ref":"#Node", "$defs":{"A":{"$anchor":"Node"}}}),
        ] {
            assert!(normalize(&schema, "Invalid").is_err(), "{schema}");
        }
        let schema = json!({"$ref":"#Node", "$defs":{"Node":{"$anchor":"Node", "type":"string"}}});
        assert!(
            bundle(
                &schema,
                &schema,
                &mut ExpansionBudget {
                    nodes: 2,
                    bytes: 1024
                }
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_references_reserved_markers_and_unguarded_aliases_fail() {
        for schema in [
            json!({"$ref":"#/missing"}),
            json!({"$ref":"https://example.invalid"}),
            json!({"$ref":"#anchor"}),
            json!({"$ref":"#"}),
            json!({"anyOf":[{"$ref":"#"},{"type":"string"}]}),
            json!({REFERENCE:"Forged"}),
            json!({"$dynamicRef":"#node"}),
            json!({"$ref":"#/$defs/A","title":"annotation","$defs":{"A":true}}),
            json!({"$ref":"#/$defs/A","$defs":{"A":42}}),
        ] {
            assert!(normalize(&schema, "Invalid").is_err(), "{schema}");
        }
    }

    #[test]
    fn cached_wide_graphs_cannot_bypass_recursive_processing_limits() {
        for aliases in [true, false] {
            let mut definitions = Map::new();
            let mut properties = Map::new();
            for index in 0..300 {
                let name = format!("Node{index:04}");
                let schema = if index == 0 {
                    json!({"type":"string"})
                } else {
                    let reference = json!({"$ref":format!("#/$defs/Node{:04}", index - 1)});
                    if aliases {
                        reference
                    } else {
                        json!({"type":"object","properties":{"next":reference}})
                    }
                };
                definitions.insert(name.clone(), schema);
                properties.insert(name.clone(), json!({"$ref":format!("#/$defs/{name}")}));
            }
            let schema = json!({"type":"object","properties":properties,"$defs":definitions});
            assert!(normalize(&schema, "Wide").is_err());
        }
    }

    #[test]
    fn shared_targets_do_not_expand_exponentially_and_limits_still_apply() {
        let schema = json!({"type":"object","properties":{"a":{"$ref":"#/$defs/A"},"b":{"$ref":"#/$defs/A"}},"$defs":{"A":{"type":"string"}}});
        let value = normalize(&schema, "Shared").unwrap();
        assert!(value.get(DEFINITIONS).is_none());
        assert_eq!(value["properties"]["a"]["type"], "string");
        assert!(
            bundle(
                &schema,
                &schema,
                &mut ExpansionBudget {
                    nodes: 2,
                    bytes: 1024
                }
            )
            .is_err()
        );
        assert!(
            bundle(
                &json!({"const":"x".repeat(200)}),
                &schema,
                &mut ExpansionBudget {
                    nodes: 100,
                    bytes: 100
                }
            )
            .is_err()
        );
    }
}

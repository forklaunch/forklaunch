//! Offline resource identity and lexical-scope index for SDK schema imports.
use super::{ExpansionBudget, queue_anchor};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

const ROOT: &str = "https://forklaunch.invalid/__document__";

pub(super) type Bindings = BTreeMap<String, String>;

pub(super) struct Resources {
    scopes: BTreeMap<usize, String>,
    dialects: BTreeMap<usize, String>,
    default_dialect: String,
    default_base: String,
    has_ids: bool,
    scope_keywords: bool,
    roots: BTreeMap<String, String>,
    anchors: BTreeMap<String, String>,
    dynamic: BTreeMap<String, Bindings>,
    recursive: BTreeSet<String>,
}
impl Resources {
    #[allow(clippy::too_many_lines)]
    pub(super) fn index(document: &Value, budget: &mut ExpansionBudget) -> Result<Self> {
        let dialect = document.get("$schema").and_then(Value::as_str).unwrap_or(
            if document.get("asyncapi").is_some() {
                "http://json-schema.org/draft-07/schema#"
            } else {
                ""
            },
        );
        budget.bytes = budget
            .bytes
            .checked_sub(dialect.len())
            .and_then(|remaining| remaining.checked_sub(dialect.len()))
            .context("SDK schema resource index exceeds 32 MiB")?;
        let mut result = Self {
            scopes: BTreeMap::new(),
            dialects: BTreeMap::new(),
            default_dialect: dialect.into(),
            default_base: ROOT.into(),
            has_ids: false,
            scope_keywords: false,
            roots: BTreeMap::from([(ROOT.into(), String::new())]),
            anchors: BTreeMap::new(),
            dynamic: BTreeMap::new(),
            recursive: BTreeSet::new(),
        };
        let mut pending = vec![(
            document,
            String::new(),
            document.get("openapi").is_none() && document.get("asyncapi").is_none(),
            0,
        )];
        let mut parents = BTreeMap::from([(String::new(), (ROOT.to_owned(), dialect.to_owned()))]);
        while let Some((value, pointer, schema, depth)) = pending.pop() {
            if depth > 64 || budget.nodes == 0 {
                bail!("SDK schema resource index exceeds expansion limits");
            }
            budget.nodes -= 1;
            let (mut base, mut dialect) = parents
                .remove(&pointer)
                .context("Missing schema resource scope")?;
            if schema && value.is_boolean() {
                result
                    .scopes
                    .insert(std::ptr::from_ref(value) as usize, base.clone());
            }
            if let Some(object) = value.as_object() {
                if schema {
                    result.scope_keywords |= [
                        "$anchor",
                        "$dynamicAnchor",
                        "$dynamicRef",
                        "$recursiveAnchor",
                        "$recursiveRef",
                    ]
                    .iter()
                    .any(|keyword| object.contains_key(*keyword));
                    if let Some(declared) = object.get("$schema") {
                        let declared = declared
                            .as_str()
                            .context("Schema dialect must be a string")?;
                        budget.bytes = budget
                            .bytes
                            .checked_sub(declared.len())
                            .context("SDK schema resource index exceeds 32 MiB")?;
                        result.scope_keywords |= dialect_key(declared) != dialect_key(&dialect);
                        declared.clone_into(&mut dialect);
                    }
                    let draft4 = dialect.contains("draft-04");
                    let id_key = if draft4 { "id" } else { "$id" };
                    if let Some(id) = object.get(id_key) {
                        result.has_ids = true;
                        let id = id.as_str().context("Schema resource ID must be a string")?;
                        budget.bytes = budget
                            .bytes
                            .checked_sub(id.len())
                            .context("SDK schema resource index exceeds 32 MiB")?;
                        let uri = resolve(&base, id)?;
                        if let Some(anchor) = uri.fragment().filter(|fragment| !fragment.is_empty())
                        {
                            let legacy = dialect.is_empty()
                                || ["draft-04", "draft-06", "draft-07"]
                                    .iter()
                                    .any(|draft| dialect.contains(draft));
                            if !legacy {
                                bail!("Nonempty schema ID fragments require a legacy dialect");
                            }
                            let anchor = normalize_uri_encoding(anchor);
                            if !id.starts_with('#') {
                                let mut scope = uri.clone();
                                scope.set_fragment(None);
                                base = scope.into();
                            }
                            budget.bytes = budget
                                .bytes
                                .checked_sub(base.len() + anchor.len() + pointer.len())
                                .context("SDK schema resource index exceeds 32 MiB")?;
                            if result
                                .anchors
                                .insert(format!("{base}#{anchor}"), pointer.clone())
                                .is_some()
                            {
                                bail!("Duplicate schema anchor");
                            }
                        } else {
                            let mut uri = uri;
                            uri.set_fragment(None);
                            base = uri.into();
                            if let Some(previous) =
                                result.roots.insert(base.clone(), pointer.clone())
                                && previous != pointer
                            {
                                bail!("Duplicate schema resource ID");
                            }
                        }
                    }
                    if pointer.is_empty() {
                        result.default_base.clone_from(&base);
                        result.default_dialect.clone_from(&dialect);
                    }
                    result
                        .dialects
                        .insert(std::ptr::from_ref(value) as usize, dialect.clone());
                    result
                        .scopes
                        .insert(std::ptr::from_ref(value) as usize, base.clone());
                    if let Some(anchor) = object.get("$recursiveAnchor") {
                        if !dialect.contains("2019-09") || !anchor.is_boolean() {
                            bail!("$recursiveAnchor requires a boolean and the 2019-09 dialect");
                        }
                        if anchor == true {
                            if result.roots.get(&base) != Some(&pointer) {
                                bail!("SDK recursive anchors must identify resource roots");
                            }
                            budget.bytes = budget
                                .bytes
                                .checked_sub(base.len())
                                .context("SDK schema resource index exceeds 32 MiB")?;
                            result.recursive.insert(base.clone());
                        }
                    }
                    for keyword in ["$anchor", "$dynamicAnchor"] {
                        if let Some(anchor) = object.get(keyword) {
                            if keyword == "$dynamicAnchor" && !dialect.contains("2020-12") {
                                bail!("$dynamicAnchor requires the 2020-12 dialect");
                            }
                            if ["draft-04", "draft-06", "draft-07"]
                                .iter()
                                .any(|draft| dialect.contains(draft))
                            {
                                bail!("$anchor is unavailable in the selected schema dialect");
                            }
                            let anchor =
                                anchor.as_str().context("Schema anchor must be a string")?;
                            validate_anchor(anchor, dialect.contains("2019-09"))?;
                            budget.bytes = budget
                                .bytes
                                .checked_sub(base.len() + anchor.len() + pointer.len())
                                .context("SDK schema resource index exceeds 32 MiB")?;
                            let key = format!("{base}#{anchor}");
                            if result.anchors.insert(key, pointer.clone()).is_some() {
                                bail!("Duplicate schema anchor");
                            }
                            if keyword == "$dynamicAnchor" {
                                budget.bytes = budget
                                    .bytes
                                    .checked_sub(base.len() + anchor.len() + pointer.len())
                                    .context("SDK schema resource index exceeds 32 MiB")?;
                                result
                                    .dynamic
                                    .entry(base.clone())
                                    .or_default()
                                    .insert(anchor.into(), pointer.clone());
                            }
                        }
                    }
                }
                for (key, child) in object {
                    let path = format!("{pointer}/{}", escape(key));
                    let mut enqueue = |value, path: String, schema| -> Result<()> {
                        budget.bytes = budget
                            .bytes
                            .checked_sub(base.len() + path.len() + dialect.len())
                            .context("SDK schema resource index exceeds 32 MiB")?;
                        parents.insert(path.clone(), (base.clone(), dialect.clone()));
                        queue_anchor(&mut pending, budget, (value, path, schema, depth + 1))
                    };
                    if schema {
                        if ignored_keyword(&dialect, key) {
                            continue;
                        }
                        match key.as_str() {
                            "$defs" | "definitions" | "properties" | "patternProperties"
                            | "dependentSchemas" | "dependencies" => {
                                if let Some(map) = child.as_object() {
                                    for (name, value) in map {
                                        if key == "dependencies" && value.is_array() {
                                            continue;
                                        }
                                        enqueue(value, format!("{path}/{}", escape(name)), true)?;
                                    }
                                }
                            }
                            "allOf" | "anyOf" | "oneOf" | "prefixItems" => {
                                if let Some(values) = child.as_array() {
                                    for (index, value) in values.iter().enumerate() {
                                        enqueue(value, format!("{path}/{index}"), true)?;
                                    }
                                }
                            }
                            "items" if child.is_array() => {
                                for (index, value) in child.as_array().unwrap().iter().enumerate() {
                                    enqueue(value, format!("{path}/{index}"), true)?;
                                }
                            }
                            "items"
                            | "additionalProperties"
                            | "additionalItems"
                            | "contains"
                            | "not"
                            | "if"
                            | "then"
                            | "else"
                            | "propertyNames"
                            | "unevaluatedProperties"
                            | "unevaluatedItems"
                            | "contentSchema" => enqueue(child, path, true)?,
                            _ => {}
                        }
                    } else if pointer == "/components/schemas"
                        || key == "schema"
                        || (key == "payload"
                            && document.get("asyncapi").is_some()
                            && child.get("schemaFormat").is_none())
                    {
                        enqueue(child, path, true)?;
                    } else if !key.starts_with("x-")
                        && !matches!(
                            key.as_str(),
                            "example" | "examples" | "default" | "enum" | "const"
                        )
                    {
                        enqueue(child, path, false)?;
                    }
                }
            } else if !schema && let Some(values) = value.as_array() {
                for (index, value) in values.iter().enumerate() {
                    let path = format!("{pointer}/{index}");
                    budget.bytes = budget
                        .bytes
                        .checked_sub(base.len() + path.len() + dialect.len())
                        .context("SDK schema resource index exceeds 32 MiB")?;
                    parents.insert(path.clone(), (base.clone(), dialect.clone()));
                    queue_anchor(&mut pending, budget, (value, path, false, depth + 1))?;
                }
            }
        }
        Ok(result)
    }

    pub(super) fn dialect<'a>(&'a self, schema: &'a Value) -> &'a str {
        self.dialects
            .get(&(std::ptr::from_ref(schema) as usize))
            .map_or_else(
                || {
                    schema
                        .get("$schema")
                        .and_then(Value::as_str)
                        .unwrap_or(&self.default_dialect)
                },
                String::as_str,
            )
    }

    pub(super) fn check_target(&self, target: &Value) -> Result<()> {
        let id = if self.dialect(target).contains("draft-04") {
            "id"
        } else {
            "$id"
        };
        if (self.has_ids || target.get(id).is_some())
            && !self
                .scopes
                .contains_key(&(std::ptr::from_ref(target) as usize))
        {
            bail!(
                "Resource-aware references to literal data or schema containers require explicit schema definitions"
            );
        }
        Ok(())
    }

    pub(super) fn needs_runtime_bundle(&self) -> bool {
        self.has_ids || self.scope_keywords
    }

    pub(super) fn bindings(
        &self,
        schema: &Value,
        inherited: &Bindings,
        budget: &mut ExpansionBudget,
    ) -> Result<Bindings> {
        for (name, pointer) in inherited {
            budget.bytes = budget
                .bytes
                .checked_sub(name.len() + pointer.len())
                .context("SDK dynamic schema context exceeds 32 MiB")?;
        }
        let mut bindings = inherited.clone();
        if let Some(anchors) = self.dynamic.get(self.base(schema)) {
            for (name, pointer) in anchors {
                if !bindings.contains_key(name) {
                    budget.bytes = budget
                        .bytes
                        .checked_sub(name.len() + pointer.len())
                        .context("SDK dynamic schema context exceeds 32 MiB")?;
                    bindings.insert(name.clone(), pointer.clone());
                }
            }
        }
        // The empty name cannot collide with a legal dynamic anchor. It tracks
        // the outermost resource in the uninterrupted recursive-anchor chain.
        if self.recursive.contains(self.base(schema)) {
            if !bindings.contains_key("") {
                let pointer = &self.roots[self.base(schema)];
                budget.bytes = budget
                    .bytes
                    .checked_sub(pointer.len())
                    .context("SDK recursive schema context exceeds 32 MiB")?;
                bindings.insert(String::new(), pointer.clone());
            }
        } else {
            bindings.remove("");
        }
        Ok(bindings)
    }

    fn base(&self, schema: &Value) -> &str {
        self.scopes
            .get(&(std::ptr::from_ref(schema) as usize))
            .map_or(self.default_base.as_str(), String::as_str)
    }

    pub(super) fn recursive_pointer(
        &self,
        schema: &Value,
        reference: &str,
        bindings: &Bindings,
    ) -> Result<String> {
        if reference != "#" {
            bail!("2019-09 recursive references must use #");
        }
        if self.recursive.contains(self.base(schema)) {
            return bindings
                .get("")
                .cloned()
                .context("Missing recursive schema binding");
        }
        self.pointer(schema, reference)
    }

    pub(super) fn dynamic_pointer(
        &self,
        schema: &Value,
        reference: &str,
        bindings: &Bindings,
    ) -> Result<String> {
        let pointer = self.pointer(schema, reference)?;
        let mut uri = resolve(self.base(schema), reference)?;
        let fragment = percent_encoding::percent_decode_str(uri.fragment().unwrap_or(""))
            .decode_utf8()
            .context("Schema reference fragment must be UTF-8")?
            .into_owned();
        uri.set_fragment(None);
        if self
            .dynamic
            .get(uri.as_str())
            .is_some_and(|anchors| anchors.contains_key(&fragment))
            && let Some(target) = bindings.get(&fragment)
        {
            return Ok(target.clone());
        }
        Ok(pointer)
    }

    pub(super) fn pointer(&self, schema: &Value, reference: &str) -> Result<String> {
        let base = self
            .scopes
            .get(&(std::ptr::from_ref(schema) as usize))
            .map_or(self.default_base.as_str(), String::as_str);
        let mut uri = resolve(base, reference)?;
        let fragment = uri.fragment().unwrap_or("").to_owned();
        uri.set_fragment(None);
        if let Some(pointer) = self
            .anchors
            .get(&format!("{uri}#{}", normalize_uri_encoding(&fragment)))
        {
            return Ok(pointer.clone());
        }
        if fragment.contains("%2F") || fragment.contains("%2f") {
            bail!(
                "Checked schemas require literal JSON-pointer separators; escape slash tokens with ~1"
            );
        }
        if let Some(pointer) = super::document_pointer(&format!("#{fragment}")) {
            let root = self.roots.get(uri.as_str()).context("Schema resource is not bundled in the document; SDK generation does not fetch remote schemas")?;
            return Ok(format!("{root}{pointer}"));
        }
        let fragment = percent_encoding::percent_decode_str(&fragment)
            .decode_utf8()
            .context("Schema reference fragment must be UTF-8")?;
        if fragment.is_empty() || fragment.starts_with('/') {
            let root = self.roots.get(uri.as_str()).context("Schema resource is not bundled in the document; SDK generation does not fetch remote schemas")?;
            Ok(format!("{root}{fragment}"))
        } else {
            self.anchors
                .get(&format!("{uri}#{fragment}"))
                .cloned()
                .context("Missing schema resource anchor")
        }
    }
}
pub(super) fn ignored_keyword(dialect: &str, keyword: &str) -> bool {
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
        || ((dialect.is_empty() || legacy || dialect.contains("2019-09"))
            && keyword == "prefixItems")
        || (dialect.contains("2020-12") && keyword == "additionalItems")
}

pub(super) fn dialect_key(value: &str) -> &str {
    if value.is_empty() {
        return "draft-07";
    }
    ["draft-04", "draft-06", "draft-07", "2019-09", "2020-12"]
        .into_iter()
        .find(|draft| value.contains(draft))
        .unwrap_or(value)
}

fn validate_anchor(anchor: &str, legacy: bool) -> Result<()> {
    let mut bytes = anchor.bytes();
    if !bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || (!legacy && b == b'_'))
        || !bytes.all(|b| b.is_ascii_alphanumeric() || b"-._".contains(&b) || (legacy && b == b':'))
    {
        bail!("Invalid schema anchor name");
    }
    Ok(())
}

// URI normalization decodes unreserved characters but keeps encoded delimiters
// distinct: a legacy ID "#%2Falias" is not the pointer-shaped ID "#/alias".
fn normalize_uri_encoding(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (
                char::from(bytes[index + 1]).to_digit(16),
                char::from(bytes[index + 2]).to_digit(16),
            )
        {
            let decoded =
                u8::try_from(high * 16 + low).expect("two hexadecimal digits fit in a byte");
            if decoded.is_ascii_alphanumeric() || b"-._~".contains(&decoded) {
                output.push(char::from(decoded));
            } else {
                use std::fmt::Write;
                write!(output, "%{decoded:02X}").unwrap();
            }
            index += 3;
        } else {
            let character = value[index..].chars().next().unwrap();
            output.push(character);
            index += character.len_utf8();
        }
    }
    output
}

fn escape(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}
fn resolve(base: &str, reference: &str) -> Result<Url> {
    if reference
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
        || reference.contains('\\')
    {
        bail!("Invalid schema resource URI");
    }
    let resolved = Url::parse(base)?
        .join(reference)
        .context("Invalid schema resource URI")?;
    Url::parse(&normalize_uri_encoding(resolved.as_str())).context("Invalid schema resource URI")
}

#[cfg(test)]
mod tests {
    use super::super::{DEFINITIONS, ExpansionBudget, REFERENCE, bundle, normalize};
    use serde_json::json;

    #[test]
    fn asyncapi_payload_and_component_resource_scopes_are_indexed() {
        let document = json!({"asyncapi":"3.0.0", "components":{"schemas":{"Node":{
            "$schema":"https://json-schema.org/draft/2020-12/schema", "$id":"urn:fixture:node", "$dynamicAnchor":"node",
            "type":"object", "properties":{"next":{"$dynamicRef":"#node"}}
        }}}, "channels":{"tree":{"messages":{"node":{"payload":{"schemaFormat":"application/schema+json;version=2020-12",
            "schema":{"$schema":"https://json-schema.org/draft/2020-12/schema", "$ref":"urn:fixture:node"}}}}}}});
        let location = "/channels/tree/messages/node/payload/schema";
        let compiled = super::super::bundle_document_resources(&document, location)
            .unwrap()
            .unwrap();
        assert_eq!(compiled["type"], "object");
        assert!(compiled["properties"]["next"]["$ref"].is_string());
        let mut invalid = document.clone();
        invalid.pointer_mut(location).unwrap()["$schema"] =
            json!("http://json-schema.org/draft-07/schema#");
        assert_eq!(
            super::super::bundle_document_resources(&invalid, location)
                .unwrap()
                .unwrap()["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
    }

    #[test]
    fn static_anchor_names_follow_the_declared_dialect() {
        for (draft, anchor, valid) in [
            ("2019-09", "Node:v1", true),
            ("2019-09", "_Node", false),
            ("2020-12", "_Node", true),
            ("2020-12", "Node:v1", false),
        ] {
            let schema = json!({"$schema":format!("https://json-schema.org/draft/{draft}/schema"),
                "$ref":format!("#{anchor}"), "$defs":{"Value":{"$anchor":anchor,"type":"string"}}});
            assert_eq!(normalize(&schema, "Anchor").is_ok(), valid);
        }
    }

    #[test]
    fn legacy_fragment_ids_preserve_resource_roots_and_scoped_aliases() {
        for (dialect, id) in [
            ("http://json-schema.org/draft-04/schema#", "id"),
            ("http://json-schema.org/draft-06/schema#", "$id"),
            ("http://json-schema.org/draft-07/schema#", "$id"),
        ] {
            let schema = json!({"$schema":dialect, id:"https://schemas.test/root", "type":"object", "properties":{
                "first":{"$ref":"#Node:v1"}, "second":{"$ref":"other#Node:v1"}}, "definitions":{
                "Root":{"type":"object", id:"#Node:v1", "properties":{"label":{"type":"string"},"next":{"$ref":"#Node:v1"}}},
                "Other":{id:"other", "type":"object", "properties":{"value":{"$ref":"#Node:v1"}}, "definitions":{"Value":{id:"#Node:v1","type":"integer"}}}
            }});
            let compiled = bundle(&schema, &schema, &mut ExpansionBudget::default()).unwrap();
            assert_eq!(compiled["properties"]["second"]["type"], "integer");
            assert!(compiled["properties"]["first"]["$ref"].is_string());
        }
    }

    #[test]
    fn legacy_fragment_normalization_rejects_duplicate_aliases() {
        for (first, second) in [("#%41", "#A"), ("#%2falias", "#%2Falias")] {
            let value = json!({"$schema":"http://json-schema.org/draft-07/schema#","definitions":{"First":{"$id":first},"Second":{"$id":second}}});
            assert!(normalize(&value, "Duplicate").is_err());
        }
    }

    #[test]
    fn uri_fragment_ids_do_not_define_bare_resources() {
        for reference in ["other", "other#/properties/value", "missing#Word"] {
            let schema = json!({"$schema":"http://json-schema.org/draft-07/schema#", "$id":"https://aliases.test/root", "$ref":reference, "definitions":{"Word":{"$id":"other#Word","type":"string"}}});
            assert!(normalize(&schema, "Missing").is_err());
        }
        let schema = json!({"$schema":"http://json-schema.org/draft-07/schema#", "$id":"https://aliases.test/root", "definitions":{"A":{"$id":"other#Word"},"B":{"$id":"other#Word"}}});
        assert!(normalize(&schema, "Duplicate").is_err());
        for dialect in [
            "https://json-schema.org/draft/2019-09/schema",
            "https://json-schema.org/draft/2020-12/schema",
        ] {
            assert!(
                normalize(
                    &json!({"$schema":dialect,"$id":"https://aliases.test/other#Word"}),
                    "Modern"
                )
                .is_err()
            );
        }
    }

    #[test]
    fn recursive_resource_roots_and_profile_boundaries() {
        let schema = json!({"$schema":"https://json-schema.org/draft/2019-09/schema", "$recursiveAnchor":true,
            "type":"object", "properties":{"next":{"$recursiveRef":"#"}}});
        assert!(bundle(&schema, &schema, &mut ExpansionBudget::default()).is_ok());
        for invalid in [
            json!({"$recursiveAnchor":true, "type":"object"}),
            json!({"$schema":"https://json-schema.org/draft/2019-09/schema", "$recursiveAnchor":"yes"}),
            json!({"$schema":"https://json-schema.org/draft/2019-09/schema", "$recursiveRef":"#/other"}),
            json!({"$schema":"https://json-schema.org/draft/2019-09/schema", "properties":{"child":{"$recursiveAnchor":true}}}),
            json!({"$schema":"https://json-schema.org/draft/2020-12/schema", "$recursiveRef":"#"}),
        ] {
            assert!(normalize(&invalid, "Rejected").is_err());
        }
        assert!(
            super::super::bundle_with_resource_policy(
                &schema,
                &schema,
                &mut ExpansionBudget::default(),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn dynamic_recursion_requires_an_explicit_supported_dialect() {
        let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema", "$dynamicAnchor":"node",
            "type":"object", "properties":{"next":{"$dynamicRef":"#node"}}});
        let compiled = bundle(&schema, &schema, &mut ExpansionBudget::default()).unwrap();
        assert!(compiled["properties"]["next"]["$ref"].is_string());
        for dialect in [
            None,
            Some("http://json-schema.org/draft-07/schema#"),
            Some("https://json-schema.org/draft/2019-09/schema"),
        ] {
            let mut invalid = schema.clone();
            if let Some(dialect) = dialect {
                invalid["$schema"] = json!(dialect);
            } else {
                invalid.as_object_mut().unwrap().remove("$schema");
            }
            assert!(normalize(&invalid, "Rejected").is_err());
        }
        assert!(
            super::super::bundle_with_resource_policy(
                &schema,
                &schema,
                &mut ExpansionBudget::default(),
                false
            )
            .is_err()
        );
    }

    #[test]
    fn resource_roots_and_relative_ids_preserve_recursive_identity() {
        let schema = json!({"$id":"https://schemas.test/root.json","type":"object","properties":{
            "child":{"$ref":"nodes/node.json"},"alias":{"$ref":"https://schemas.test/nodes/node.json"}
        },"$defs":{"Node":{"$id":"nodes/node.json","type":"object","properties":{
            "value":{"type":"string"},"next":{"$ref":"#"},"root":{"$ref":"../root.json"}
        }}}});
        let result = normalize(&schema, "Resource").unwrap();
        assert_eq!(
            result["properties"]["child"][REFERENCE],
            result["properties"]["alias"][REFERENCE]
        );
        let name = result["properties"]["child"][REFERENCE].as_str().unwrap();
        assert_eq!(
            result[DEFINITIONS][name]["properties"]["next"][REFERENCE],
            name
        );
        assert_eq!(
            result[DEFINITIONS][name]["properties"]["value"]["type"],
            "string"
        );
    }

    #[test]
    fn anchors_are_scoped_to_their_resource_and_literals_are_preserved() {
        let schema = json!({"$id":"https://schemas.test/root","type":"object","properties":{
            "a":{"$ref":"a#Value"},"b":{"$ref":"b#Value"}
        },"$defs":{
            "A":{"$id":"a","$anchor":"Value","type":"string"},
            "B":{"$id":"b","$anchor":"Value","type":"integer"}
        },"default":{"$id":"a","$anchor":"Value","$ref":"not-a-schema"}});
        let result = normalize(&schema, "Scopes").unwrap();
        assert_eq!(result["properties"]["a"]["type"], "string");
        assert_eq!(result["properties"]["b"]["type"], "integer");
        assert_eq!(result["default"], schema["default"]);
    }

    #[test]
    fn openapi_component_resource_pointers_resolve_in_the_component() {
        let document = json!({"openapi":"3.1.0","components":{"schemas":{
            "Node":{"$id":"https://schemas.test/node","type":"object","properties":{
                "value":{"$ref":"#/$defs/Value"},"next":{"$ref":"https://schemas.test/node"}
            },"$defs":{"Value":{"type":"string"}}}
        }}});
        let result = bundle(
            &document["components"]["schemas"]["Node"],
            &document,
            &mut ExpansionBudget::default(),
        )
        .unwrap();
        assert_eq!(result["properties"]["value"]["type"], "string");
        assert!(result["properties"]["next"]["$ref"].is_string());
    }

    #[test]
    fn malformed_duplicate_missing_and_dynamic_resources_fail_closed() {
        for schema in [
            json!({"$id":12,"type":"string"}),
            json!({"$id":"https://schemas.test/has space","type":"string"}),
            json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"https://schemas.test/root#anchor","type":"string"}),
            json!({"$id":"https://schemas.test/root","$ref":"https://remote.test/missing"}),
            json!({"$id":"https://schemas.test/root","$ref":"same","$defs":{"A":{"$id":"same","type":"string"},"B":{"$id":"same","type":"number"}}}),
            json!({"$id":"https://schemas.test/root","$dynamicRef":"#Node","$dynamicAnchor":"Node"}),
            json!({"$id":"https://schemas.test/root","$ref":"#Missing"}),
        ] {
            assert!(normalize(&schema, "Rejected").is_err(), "{schema}");
        }
    }

    #[test]
    fn non_hierarchical_ids_and_literal_target_boundaries() {
        let schema =
            json!({"$id":"urn:fixture:node","type":"object","properties":{"next":{"$ref":"#"}}});
        let result = normalize(&schema, "Urn").unwrap();
        assert_eq!(result["properties"]["next"][REFERENCE], "UrnRef1");
        let schema = json!({"$id":"https://schemas.test/root","$ref":"#/default","default":{"$id":"nested","type":"string"}});
        assert!(normalize(&schema, "Literal").is_err());
        let external = json!({"$id":"https://schemas.test/external","type":"string"});
        assert!(bundle(&external, &json!({}), &mut ExpansionBudget::default()).is_err());
    }

    #[test]
    fn draft_four_ids_resolve_without_activating_newer_annotations() {
        let schema = json!({"$schema":"http://json-schema.org/draft-04/schema#","id":"https://schemas.test/legacy/root","$ref":"child","definitions":{
            "Child":{"id":"child","type":"object","properties":{"value":{"type":"string","const":"ignored"},"next":{"$ref":"#"}},"if":{"$id":"not-active","type":"bad"}}
        }});
        let result = normalize(&schema, "Legacy").unwrap();
        assert_eq!(result["properties"]["value"]["type"], "string");
        assert!(result["properties"]["value"].get("const").is_none());
        assert!(result.get("if").is_none());
        assert!(result["properties"]["next"][REFERENCE].is_string());
    }

    #[test]
    fn embedded_resource_dialects_control_keyword_meaning() {
        let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"https://schemas.test/current","type":"object","properties":{
            "old":{"$ref":"legacy"},"new":{"type":"string","const":"only"}
        },"$defs":{"Legacy":{"$schema":"http://json-schema.org/draft-04/schema#","id":"legacy","type":"string","const":"ignored","if":{"$id":false}}}});
        let result = normalize(&schema, "Mixed").unwrap();
        assert!(result["properties"]["old"].get("const").is_none());
        assert_eq!(result["properties"]["new"]["const"], "only");
        let document = json!({"openapi":"3.1.0","components":{"schemas":{"Legacy":{
            "$schema":"http://json-schema.org/draft-04/schema#","id":"https://schemas.test/legacy","type":"string","const":"ignored"
        }}}});
        let result = bundle(
            &document["components"]["schemas"]["Legacy"],
            &document,
            &mut ExpansionBudget::default(),
        )
        .unwrap();
        assert_eq!(result["type"], "string");
        assert!(result.get("const").is_none());
    }

    #[test]
    fn resource_indexes_share_node_and_byte_limits() {
        let schema = json!({"$id":"https://schemas.test/root","$ref":"child","$defs":{"Child":{"$id":"child","type":"string"}}});
        for mut budget in [
            ExpansionBudget {
                nodes: 2,
                bytes: 1024,
            },
            ExpansionBudget {
                nodes: 1000,
                bytes: 10,
            },
        ] {
            assert!(bundle(&schema, &schema, &mut budget).is_err());
        }
    }
}

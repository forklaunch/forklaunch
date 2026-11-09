//! Preserve explicit component identity while keeping anonymous schemas anonymous.
use anyhow::{Context, Result};
use convert_case::{Case, Casing};
use serde_json::{Value, json};

/// Mark concrete named component definitions before graph bundling loses their paths.
/// Anonymous schemas and reference aliases do not create new model identities.
pub(super) fn document(
    source: &Value,
    budget: &mut super::schema::ExpansionBudget,
) -> Result<Value> {
    let mut document = super::schema::clone_literal(source, budget)?;
    if let Some(components) = document
        .pointer_mut("/components/schemas")
        .and_then(Value::as_object_mut)
    {
        for (name, schema) in components {
            let Some(object) = schema.as_object_mut() else {
                continue;
            };
            if object.contains_key("$ref")
                || object.get("type").and_then(Value::as_str) != Some("object")
            {
                continue;
            }
            let token = name.replace('~', "~0").replace('/', "~1");
            object
                .entry("x-forklaunch-model".to_owned())
                .or_insert_with(|| {
                    json!({
                        "version":1,"id":format!("openapi-component:/components/schemas/{token}"),
                        "name":name,"projection":"value"
                    })
                });
        }
    }
    Ok(document)
}

pub(super) fn component(source: &Value, mut bundled: Value) -> Value {
    let Some(object) = source.as_object() else {
        return bundled;
    };
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "$ref" | "title" | "description" | "summary"))
    {
        return bundled;
    }
    let Some(reference) = object.get("$ref").and_then(Value::as_str) else {
        return bundled;
    };
    let Some(pointer) = crate::codegen::schema::document_pointer(reference) else {
        return bundled;
    };
    let Some(name) = pointer.strip_prefix("/components/schemas/") else {
        return bundled;
    };
    // A component name is one JSON pointer token, not a nested component property.
    if name.contains('/') || name.is_empty() {
        return bundled;
    }
    let name = name.replace("~1", "/").replace("~0", "~");
    if let Some(schema) = bundled.as_object_mut() {
        schema.entry("x-forklaunch-model".to_owned()).or_insert_with(|| json!({
            "version":1,"id":format!("openapi-component:{pointer}"),"name":name,"projection":"value"
        }));
    }
    bundled
}

pub(super) fn version(contract: &mut Value, version: &str) -> Result<()> {
    let routes = contract["routes"]
        .as_array_mut()
        .context("Imported model routes missing")?;
    for route in routes {
        // Path parameters must stay identical across versions. Their own schema
        // comparisons already reject incompatible version declarations.
        for location in [
            "/request/body",
            "/request/query",
            "/request/headers",
            "/responses",
        ] {
            if let Some(value) = route.pointer_mut(location) {
                visit_contract(value, version)?;
            }
        }
    }
    Ok(())
}
fn visit_contract(value: &mut Value, version: &str) -> Result<()> {
    if let Some(schema) = value.get_mut("jsonSchema") {
        for location in crate::contract::schema_locations(schema, false)? {
            if let Some(metadata) = schema
                .pointer_mut(&location)
                .and_then(|node| node.get_mut("x-forklaunch-model"))
                && let Some(id) = metadata.get("id").and_then(Value::as_str)
                && id.starts_with("openapi-component:")
            {
                let id = format!("{id}@version:{version}");
                let name = format!(
                    "{}V{}",
                    metadata["name"].as_str().unwrap_or_default(),
                    version.to_case(Case::Pascal)
                );
                metadata["id"] = json!(id);
                metadata["name"] = json!(name);
            }
        }
        return Ok(());
    }
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                visit_contract(child, version)?;
            }
        }
        Value::Array(array) => {
            for child in array {
                visit_contract(child, version)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> Value {
        json!({"openapi":"3.1.0","paths":{
            "/people":{"get":{"operationId":"listPeople","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Person"}}}}}}},
            "/person":{"get":{"operationId":"readPerson","responses":{"200":{"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Person","description":"An independently documented use"}}}}}}}
        },"components":{"schemas":{"Person":{"type":"object","additionalProperties":false,"required":["name"],"properties":{"name":{"type":"string"}}}}}})
    }
    #[test]
    fn imported_components_share_authoritative_identity_not_titles() {
        let output =
            crate::build::openapi_artifact_sources(&document().to_string(), "people").unwrap();
        if let Ok(directory) = std::env::var("FORKLAUNCH_MODEL_IMPORT_DIR") {
            std::fs::write(format!("{directory}/imported.rs"), &output.sources.rust).unwrap();
            std::fs::write(
                format!("{directory}/imported.ts"),
                &output.sources.typescript,
            )
            .unwrap();
        }
        assert_eq!(
            output.sources.rust.matches("pub struct Person {").count(),
            1
        );
        assert!(
            output
                .sources
                .rust
                .contains("pub type ListPeopleResponse = Person;")
        );
        assert!(
            output
                .sources
                .rust
                .contains("pub type ReadPersonResponse = Person;")
        );
        assert_eq!(
            output
                .sources
                .typescript
                .matches("export type Person =")
                .count(),
            1
        );
    }
    #[test]
    fn nested_imported_components_reuse_root_and_recursive_models() {
        let mut document = document();
        document["components"]["schemas"]["Envelope"] = json!({
            "type":"object","additionalProperties":false,"required":["person","people"],
            "properties":{
                "person":{"$ref":"#/components/schemas/Person"},
                "people":{"type":"array","items":{"$ref":"#/components/schemas/Person"}}
            }
        });
        document["components"]["schemas"]["Person"]["properties"]["child"] =
            json!({"$ref":"#/components/schemas/Person"});
        document["paths"]["/people"]["get"]["responses"]["200"]["content"]["application/json"]["schema"] =
            json!({"$ref":"#/components/schemas/Envelope"});
        let output =
            crate::build::openapi_artifact_sources(&document.to_string(), "people").unwrap();
        assert_eq!(
            output.sources.rust.matches("pub struct Person {").count(),
            1
        );
        assert_eq!(
            output
                .sources
                .typescript
                .matches("export type Person =")
                .count(),
            1
        );
        assert!(
            output
                .sources
                .rust
                .contains("pub person: ::std::boxed::Box<Person>,")
        );
        assert!(
            output
                .sources
                .rust
                .contains("pub people: ::std::vec::Vec<Person>,")
        );
        assert!(
            output
                .sources
                .rust
                .contains("pub type ReadPersonResponse = Person;")
        );
        if let Ok(directory) = std::env::var("FORKLAUNCH_NESTED_IMPORT_DIR") {
            std::fs::write(format!("{directory}/imported.rs"), &output.sources.rust).unwrap();
            std::fs::write(
                format!("{directory}/imported.ts"),
                &output.sources.typescript,
            )
            .unwrap();
        }
    }

    #[test]
    fn versioned_component_names_do_not_conflate_changed_schemas() {
        let first = document();
        let mut second = document();
        second["components"]["schemas"]["Person"]["properties"]["name"]["minLength"] = json!(3);
        let versions = json!({"":{"openapi":"3.1.0","paths":{}},"1":first,"2":second});
        let imported = super::super::contract_from_openapi(&versions).unwrap();
        let routes = imported["routes"].as_array().unwrap();
        for route in routes {
            let branches = route["versions"].as_array().unwrap();
            assert_eq!(
                branches[0]["responses"][0]["body"]["schema"]["jsonSchema"]["x-forklaunch-model"]["name"],
                "PersonV1"
            );
            assert_eq!(
                branches[1]["responses"][0]["body"]["schema"]["jsonSchema"]["x-forklaunch-model"]["name"],
                "PersonV2"
            );
        }
    }
    #[test]
    fn native_identity_and_assertion_overrides_are_not_relabelled() {
        let native = json!({"type":"object","x-forklaunch-model":{"version":1,"id":"native::Person","name":"Person","projection":"value"}});
        assert_eq!(
            component(
                &json!({"$ref":"#/components/schemas/Person"}),
                native.clone()
            ),
            native
        );
        let schema = json!({"type":"string","minLength":3});
        assert_eq!(
            component(
                &json!({"$ref":"#/components/schemas/Name","minLength":3}),
                schema.clone()
            ),
            schema
        );
        assert_eq!(
            component(&json!({"title":"Person"}), json!({"type":"object"})),
            json!({"type":"object"})
        );
    }
}

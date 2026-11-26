//! Shared version projections for CLI and native HTTP SDKs.
use super::schema::{ExpansionBudget, clone_literal};
use anyhow::{Context, Result, bail};
use convert_case::{Case, Casing};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

pub(crate) fn operation_name(operation: &str, version: &str) -> String {
    format!("{operation} version {version}").to_case(Case::Camel)
}

#[allow(dead_code)] // Native plans project typed contracts directly.
pub(crate) fn expand_contract(contract: &mut Value) -> Result<()> {
    let Some(routes) = contract.get_mut("routes").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    let mut expanded = Vec::new();
    let mut budget = ExpansionBudget::default();
    for route in routes.iter() {
        let Some(versions) = route.get("versions") else {
            expanded.push(clone_literal(route, &mut budget)?);
            continue;
        };
        let versions = versions
            .as_array()
            .context("Contract versions must be an array")?;
        if versions.is_empty() {
            expanded.push(clone_literal(route, &mut budget)?);
            continue;
        }
        if versions.len() > 1024 {
            bail!("SDK route exceeds 1024 versions");
        }
        let base = route
            .get("operationName")
            .and_then(Value::as_str)
            .context("Versioned operation name is required")?;
        let mut base_route = Map::new();
        for (key, value) in route
            .as_object()
            .context("Contract route must be an object")?
        {
            if !matches!(key.as_str(), "versions" | "request" | "responses") {
                base_route.insert(key.clone(), clone_literal(value, &mut budget)?);
            }
        }
        let base_route = Value::Object(base_route);
        let mut names = BTreeSet::new();
        let first_projection = expanded.len();
        for version in versions {
            let label = version
                .get("version")
                .and_then(Value::as_str)
                .context("API version name is required")?;
            let name = operation_name(base, label);
            if !names.insert(name.clone()) {
                bail!("API versions produce duplicate SDK operation names: {name}");
            }
            budget.bytes = budget
                .bytes
                .checked_sub(name.len() + label.len())
                .context("SDK version expansion exceeds 32 MiB")?;
            let mut projected = clone_literal(&base_route, &mut budget)?;
            let mut request = clone_literal(
                version
                    .get("request")
                    .filter(|request| request.is_object())
                    .context("Versioned request must be an object")?,
                &mut budget,
            )?;
            request["params"] = clone_literal(
                route.pointer("/request/params").unwrap_or(&json!([])),
                &mut budget,
            )?;
            projected["request"] = request;
            projected["responses"] = clone_literal(
                version
                    .get("responses")
                    .filter(|responses| responses.is_array())
                    .context("Versioned responses must be an array")?,
                &mut budget,
            )?;
            projected["operationName"] = json!(name);
            projected["x-forklaunch-api-version"] = json!(label);
            expanded.push(projected);
        }
        let branches = Value::Array(expanded[first_projection..].to_vec());
        for projected in &mut expanded[first_projection..] {
            projected["x-forklaunch-version-branches"] = clone_literal(&branches, &mut budget)?;
        }
    }
    *routes = expanded;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_common_params_cannot_bypass_expansion_budget() {
        let versions = (0..40)
            .map(|index| json!({"version":format!("v{index}"),"request":{},"responses":[]}))
            .collect::<Vec<_>>();
        let mut contract = json!({"routes":[{"operationName":"write","request":{"params":[{"name":"id","description":"x".repeat(1024*1024)}]},"versions":versions}]});
        assert!(
            expand_contract(&mut contract)
                .unwrap_err()
                .to_string()
                .contains("32 MiB")
        );
    }

    #[test]
    fn projects_versions_with_common_params_and_rejects_ambiguous_aliases() {
        let mut contract = json!({"routes":[{"operationName":"write","request":{"params":[{"name":"id"}]},"versions":[
            {"version":"v1","request":{"body":{"schema":{"jsonSchema":{"type":"string"}}}},"responses":[{"statusCode":201}]},
            {"version":"v2","request":{"body":{"schema":{"jsonSchema":{"type":"integer"}}}},"responses":[{"statusCode":202}]}
        ]}]});
        let original = contract.clone();
        expand_contract(&mut contract).unwrap();
        assert_eq!(contract["routes"].as_array().unwrap().len(), 2);
        assert_eq!(contract["routes"][0]["operationName"], "writeVersionV1");
        assert_eq!(contract["routes"][1]["operationName"], "writeVersionV2");
        assert_eq!(
            contract["routes"][0]["request"]["params"],
            original["routes"][0]["request"]["params"]
        );
        assert_eq!(contract["routes"][1]["responses"][0]["statusCode"], 202);
        let mut ambiguous = original;
        ambiguous["routes"][0]["versions"][1]["version"] = json!("v1");
        assert!(expand_contract(&mut ambiguous).is_err());
    }
}

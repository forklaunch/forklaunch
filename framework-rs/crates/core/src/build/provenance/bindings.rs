//! Declared managed operation sets, independently checked against exported assets.
use crate::contract::assembly::BindingDeclaration;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BindingManifest {
    pub format_version: String,
    pub declarations: Vec<BindingDeclaration>,
}
impl BindingManifest {
    /// Check complete one-to-one HTTP/socket declaration coverage. This is not
    /// evidence that a runtime router mounts those declarations.
    /// # Errors
    /// Rejects missing, duplicate, conflicting or unsupported declarations.
    pub fn verify_contract(&self, artifact: &Value) -> Result<()> {
        ensure!(
            self.format_version == "forklaunch.bindings.v1",
            "Unsupported binding evidence version"
        );
        let routes = artifact
            .get("routes")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow::anyhow!("Contract routes missing"))?;
        let sockets = artifact.get("sockets").and_then(Value::as_object);
        let mut remaining: Vec<(&str, &Value)> =
            routes.iter().map(|route| ("http", route)).collect();
        if let Some(sockets) = sockets {
            remaining.extend(sockets.values().map(|socket| ("websocket", socket)));
        }
        for declaration in &self.declarations {
            ensure!(
                !declaration.source.is_empty()
                    && !declaration.group.is_empty()
                    && !declaration.member.is_empty(),
                "Incomplete binding identity"
            );
            let matches: Vec<_> = remaining
                .iter()
                .enumerate()
                .filter(|(_, (kind, contract))| {
                    *kind == declaration.transport && **contract == declaration.contract
                })
                .map(|(index, _)| index)
                .collect();
            ensure!(
                matches.len() == 1,
                "Missing, duplicate or mismatched compiled binding {}::{}",
                declaration.group,
                declaration.member
            );
            remaining.remove(matches[0]);
        }
        ensure!(
            remaining.is_empty(),
            "Exported operation has no compiled static binding declaration"
        );
        Ok(())
    }
}

#[cfg(all(test, feature = "typed-schema"))]
mod tests {
    use super::*;
    use crate::contract::{
        ContractRegistry, HttpMethod,
        endpoint::{Endpoint, NoBody, TextBody},
    };
    #[crate::contract::api]
    mod operations {
        use super::*;
        pub fn health() -> Endpoint<NoBody, TextBody> {
            Endpoint::new("health", "health", HttpMethod::Get, "/health", 200).unwrap()
        }
    }
    #[test]
    fn compiler_declarations_cover_exact_export_without_claiming_mounts() {
        let registry: ContractRegistry = operations::registry();
        let artifact: Value = serde_json::from_str(&registry.into_artifact().to_json()).unwrap();
        let mut manifest = BindingManifest {
            format_version: "forklaunch.bindings.v1".into(),
            declarations: operations::binding_manifest().unwrap(),
        };
        manifest.verify_contract(&artifact).unwrap();
        assert_eq!(manifest.declarations[0].member, "health");
        let original = manifest.declarations[0].clone();
        manifest.declarations.push(original.clone());
        assert!(manifest.verify_contract(&artifact).is_err());
        manifest.declarations.clear();
        assert!(manifest.verify_contract(&artifact).is_err());
        manifest.declarations.push(original);
        manifest.declarations[0].contract["path"] = Value::String("/other".into());
        assert!(manifest.verify_contract(&artifact).is_err());
    }
}

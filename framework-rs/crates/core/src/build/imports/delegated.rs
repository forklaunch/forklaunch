//! Preserve explicit provider ownership without inventing endpoint contracts.
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Surface {
    id: String,
    provider: String,
    coverage: String,
    client: Client,
    runtime: Runtime,
    callbacks: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Client {
    package: String,
    export: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Runtime {
    base_path: String,
    #[serde(rename = "openApi")]
    _open_api: bool,
}

pub(super) fn validate(value: &Value) -> Result<()> {
    let surfaces: Vec<Surface> = serde_json::from_value(value.clone())?;
    let mut ids = std::collections::BTreeSet::new();
    for surface in surfaces {
        ensure!(
            !surface.id.is_empty()
                && ids.insert(surface.id)
                && !surface.provider.is_empty()
                && surface.coverage == "provider-owned"
                && !surface.client.package.is_empty()
                && !surface.client.export.is_empty()
                && surface.runtime.base_path == "runtime-resolved"
                && surface.callbacks.iter().all(|path| path.starts_with('/')),
            "Invalid delegated surface ownership metadata"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn ownership_metadata_survives_import_without_endpoint_claims() {
        let surface = json!({"id":"auth","provider":"better-auth","coverage":"provider-owned",
            "client":{"package":"better-auth/client","export":"createAuthClient"},
            "runtime":{"basePath":"runtime-resolved","openApi":true},"callbacks":["/callback"]});
        let spec =
            json!({"openapi":"3.1.0","paths":{},"x-forklaunch-delegated-surfaces":[surface]});
        let contract = super::super::contract_from_openapi(&spec).unwrap();
        assert!(contract["routes"].as_array().unwrap().is_empty());
        assert_eq!(
            contract["delegatedSurfaces"],
            spec["x-forklaunch-delegated-surfaces"]
        );
        let mut bad = spec.clone();
        bad["x-forklaunch-delegated-surfaces"][0]["secret"] = json!("forbidden");
        assert!(super::super::contract_from_openapi(&bad).is_err());
        let mut version = spec.clone();
        version["x-forklaunch-delegated-surfaces"][0]["provider"] = json!("other");
        assert!(super::super::contract_from_openapi(&json!({"":spec,"v2":version})).is_err());
    }
}

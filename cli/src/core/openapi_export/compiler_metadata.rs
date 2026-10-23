//! Retain compiler-owned layout only when it describes the exported registry.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{fs, path::Path};

const LAYOUT_KEYS: [&str; 4] = [
    "rustClientName",
    "rustOutputLayout",
    "sdkLayout",
    "typescriptRuntimeLayout",
];

pub(super) fn restore(
    root: &Path,
    service_directory: &Path,
    service: &str,
    destination: &Path,
    exported: &mut Value,
) -> Result<()> {
    let root = root.canonicalize()?;
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?;
    let modules = manifest
        .get("modules_path")
        .and_then(toml::Value::as_str)
        .unwrap_or("src");
    let candidates = [
        service_directory.join("sdk/typescript/contract.json"),
        root.join(modules)
            .join("client-sdk/typescript")
            .join(service)
            .join("contract.json"),
        destination.to_owned(),
    ];
    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        let candidate = candidate.canonicalize()?;
        ensure!(
            candidate.starts_with(&root),
            "Compiler contract must stay inside the application"
        );
        ensure!(
            fs::metadata(&candidate)?.len() <= 16 * 1024 * 1024,
            "Compiler contract exceeds 16 MiB"
        );
        let compiled: Value = serde_json::from_slice(&fs::read(candidate)?)?;
        if matching_layout(&compiled, exported)? {
            for key in LAYOUT_KEYS {
                if let Some(value) = compiled.get(key) {
                    exported[key] = value.clone();
                }
            }
            break;
        }
    }
    Ok(())
}

fn matching_layout(compiled: &Value, exported: &Value) -> Result<bool> {
    if compiled.get("rustOutputLayout").and_then(Value::as_str) != Some("service-module")
        || compiled
            .get("rustClientName")
            .and_then(Value::as_str)
            .is_none()
        || compiled.get("sdkCompiler").and_then(Value::as_str) != Some("forklaunch.sdk.http.v1")
    {
        return Ok(false);
    }
    let mut registry = compiled.clone();
    let object = registry
        .as_object_mut()
        .context("Compiler contract must be an object")?;
    for key in LAYOUT_KEYS {
        object.remove(key);
    }
    let mut expected = exported.clone();
    let object = expected
        .as_object_mut()
        .context("Exported contract must be an object")?;
    for key in LAYOUT_KEYS {
        object.remove(key);
    }
    Ok(registry == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn compiler_export_preserves_actual_local_and_legacy_layout_for_custom_modules() {
        for (modules, local) in [("src", false), ("domains", false), ("src", true)] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
            fs::write(
                root.path().join(".forklaunch/manifest.toml"),
                format!("modules_path='{modules}'"),
            )
            .unwrap();
            let service = root.path().join(modules).join("notes");
            let compiler = if local {
                service.join("sdk/typescript/contract.json")
            } else {
                root.path()
                    .join(modules)
                    .join("client-sdk/typescript/notes/contract.json")
            };
            fs::create_dir_all(compiler.parent().unwrap()).unwrap();
            let mut exported = json!({"sdkCompiler":"forklaunch.sdk.http.v1","routes":[],"service":{"name":"notes"}});
            let mut compiled = exported.clone();
            compiled["rustOutputLayout"] = "service-module".into();
            compiled["rustClientName"] = if local { "Client" } else { "NotesClient" }.into();
            compiled["typescriptRuntimeLayout"] =
                if local { "local" } else { "shared-parent" }.into();
            if local {
                compiled["sdkLayout"] = "project-local-v1".into();
            }
            fs::write(&compiler, serde_json::to_vec(&compiled).unwrap()).unwrap();
            restore(
                root.path(),
                &service,
                "notes",
                &root.path().join("exports/contract.json"),
                &mut exported,
            )
            .unwrap();
            assert_eq!(exported, compiled);
        }
    }

    #[test]
    fn compiler_export_does_not_borrow_stale_or_incomplete_metadata() {
        let raw =
            json!({"sdkCompiler":"forklaunch.sdk.http.v1","routes":[],"service":{"name":"notes"}});
        let mut compiled = raw.clone();
        compiled["rustOutputLayout"] = "service-module".into();
        assert!(!matching_layout(&compiled, &raw).unwrap());
        compiled["rustClientName"] = "NotesClient".into();
        assert!(matching_layout(&compiled, &raw).unwrap());
        compiled["routes"] = json!([{"path":"/stale"}]);
        assert!(!matching_layout(&compiled, &raw).unwrap());
        compiled["routes"] = json!([]);
        compiled["service"]["name"] = "other".into();
        assert!(!matching_layout(&compiled, &raw).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn compiler_export_rejects_metadata_outside_application() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "modules_path='src'",
        )
        .unwrap();
        fs::create_dir_all(root.path().join("src/notes/sdk")).unwrap();
        fs::write(external.path().join("contract.json"), "{}").unwrap();
        std::os::unix::fs::symlink(
            external.path(),
            root.path().join("src/notes/sdk/typescript"),
        )
        .unwrap();
        assert!(
            restore(
                root.path(),
                &root.path().join("src/notes"),
                "notes",
                &root.path().join("contract.json"),
                &mut json!({})
            )
            .is_err()
        );
    }
}

use std::{fs::read_to_string, path::Path};

use anyhow::{Context, Result};
use serde_json::{Value, json, to_string_pretty};

use super::rendered_template::RenderedTemplate;
use crate::core::manifest::application::ApplicationManifestData;

pub(crate) fn generate_project_tsconfig(
    path_dir: &Path,
    extra_types: Option<&[&str]>,
) -> Result<Option<RenderedTemplate>> {
    let path = path_dir.join("tsconfig.json");
    if path.exists() {
        return Ok(None);
    }

    let mut compiler_options = json!({
        "outDir": "dist"
    });

    if let Some(types) = extra_types {
        let mut all_types = vec!["node", "vitest/globals"];
        all_types.extend_from_slice(types);
        compiler_options["types"] = json!(all_types);
    }

    Ok(Some(RenderedTemplate {
        path,
        content: to_string_pretty(&json!({
            "extends": "../tsconfig.base.json",
            "compilerOptions": compiler_options,
            "exclude": [
                "node_modules",
                "dist",
                "eslint.config.mjs"
            ]
        }))?,
        context: None,
    }))
}

pub(crate) fn generate_root_tsconfig(
    path_dir: &Path,
    manifest_data: &ApplicationManifestData,
) -> Result<Option<RenderedTemplate>> {
    let path = path_dir.join("tsconfig.json");
    if path.exists() {
        return Ok(None);
    }

    let references = manifest_data
        .projects
        .iter()
        .map(|project| json!({ "path": project.name }))
        .collect::<Vec<Value>>();

    Ok(Some(RenderedTemplate {
        path,
        content: to_string_pretty(&json!({
          "files": [],
          "references": references,
          "compilerOptions": {
            "declaration": true,
            "declarationMap": true
          }
        }))?,
        context: None,
    }))
}

/// Generates the modules tsconfig.json with references to all sub-projects
pub(crate) fn generate_modules_tsconfig(
    modules_path: &Path,
    manifest_data: &ApplicationManifestData,
) -> Result<RenderedTemplate> {
    let path = modules_path.join("tsconfig.json");

    let references = manifest_data
        .projects
        .iter()
        .filter(|project| project.name != "client-sdk")
        .map(|project| json!({ "path": project.name }))
        .collect::<Vec<Value>>();

    Ok(RenderedTemplate {
        path,
        content: to_string_pretty(&json!({
            "files": [],
            "declaration": true,
            "declarationMap": true,
            "references": references
        }))?,
        context: None,
    })
}

/// Adds a project reference to the modules tsconfig.json
pub(crate) fn add_project_to_modules_tsconfig(
    modules_path: &Path,
    project_name: &str,
) -> Result<RenderedTemplate> {
    let path = modules_path.join("tsconfig.json");

    let mut tsconfig: serde_json::Map<String, Value> = if path.exists() {
        let content = read_to_string(&path).with_context(|| "Failed to read tsconfig.json")?;
        serde_json::from_str(&content).with_context(|| "Failed to parse tsconfig.json")?
    } else {
        serde_json::Map::from_iter([
            ("files".to_string(), json!([])),
            ("declaration".to_string(), json!(true)),
            ("declarationMap".to_string(), json!(true)),
            ("references".to_string(), json!([])),
        ])
    };

    let references = tsconfig
        .entry("references")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .context("references must be an array")?;

    let reference_exists = references
        .iter()
        .any(|r| r.get("path").and_then(|p| p.as_str()) == Some(project_name));

    if !reference_exists {
        references.push(json!({ "path": project_name }));
    }

    Ok(RenderedTemplate {
        path,
        content: to_string_pretty(&tsconfig)?,
        context: None,
    })
}

pub(crate) fn remove_project_from_modules_tsconfig(
    modules_path: &Path,
    project_name: &str,
) -> Result<RenderedTemplate> {
    let path = modules_path.join("tsconfig.json");

    if !path.exists() {
        anyhow::bail!("tsconfig.json does not exist at {:?}", path);
    }

    let content = read_to_string(&path).with_context(|| "Failed to read tsconfig.json")?;
    let mut tsconfig: serde_json::Map<String, Value> =
        serde_json::from_str(&content).with_context(|| "Failed to parse tsconfig.json")?;

    let references = tsconfig
        .get_mut("references")
        .and_then(|r| r.as_array_mut())
        .context("references must be an array")?;

    references.retain(|r| r.get("path").and_then(|p| p.as_str()) != Some(project_name));

    Ok(RenderedTemplate {
        path,
        content: to_string_pretty(&tsconfig)?,
        context: None,
    })
}

pub(crate) fn update_project_in_modules_tsconfig(
    modules_path: &Path,
    old_name: &str,
    new_name: &str,
) -> Result<RenderedTemplate> {
    let path = modules_path.join("tsconfig.json");

    if !path.exists() {
        anyhow::bail!("tsconfig.json does not exist at {:?}", path);
    }

    let content = read_to_string(&path).with_context(|| "Failed to read tsconfig.json")?;
    let mut tsconfig: serde_json::Map<String, Value> =
        serde_json::from_str(&content).with_context(|| "Failed to parse tsconfig.json")?;

    let references = tsconfig
        .get_mut("references")
        .and_then(|r| r.as_array_mut())
        .context("references must be an array")?;

    for reference in references.iter_mut() {
        if let Some(path_value) = reference.get("path") {
            if path_value.as_str() == Some(old_name) {
                *reference = json!({ "path": new_name });
                break;
            }
        }
    }

    Ok(RenderedTemplate {
        path,
        content: to_string_pretty(&tsconfig)?,
        context: None,
    })
}

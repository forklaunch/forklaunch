use std::{fs::read_to_string, path::Path};

use anyhow::{Context, Result};
use serde_json::{Value, json, to_string_pretty};

use super::rendered_template::RenderedTemplate;
use crate::constants::TestFramework;
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

fn test_framework_type_entry(test_framework: &TestFramework) -> &'static str {
    match test_framework {
        TestFramework::Vitest => "vitest/globals",
        TestFramework::Jest => "jest",
    }
}

const ALL_TEST_FRAMEWORK_TYPE_ENTRIES: &[&str] = &["vitest/globals", "jest"];

/// Updates the test framework type entry in a tsconfig's compilerOptions.types array.
/// Strips every known test framework type entry, then appends new_type if set.
/// If the types array doesn't exist, does nothing.
fn swap_test_type_in_tsconfig(
    tsconfig: &mut serde_json::Map<String, Value>,
    new_type: Option<&str>,
) {
    if let Some(compiler_options) = tsconfig
        .get_mut("compilerOptions")
        .and_then(|co| co.as_object_mut())
    {
        if let Some(types) = compiler_options
            .get_mut("types")
            .and_then(|t| t.as_array_mut())
        {
            types.retain(|t| {
                t.as_str()
                    .map(|s| !ALL_TEST_FRAMEWORK_TYPE_ENTRIES.contains(&s))
                    .unwrap_or(true)
            });
            if let Some(new_type) = new_type {
                if !types.iter().any(|t| t.as_str() == Some(new_type)) {
                    types.push(json!(new_type));
                }
            }
        }
    }
}

/// Updates tsconfig.base.json and per-project tsconfig.json files when the test framework changes.
/// Removes every known test framework type entry, then adds the new one if specified.
/// Pass None for new_test_framework to strip test types without replacement (e.g. switching to Bun).
pub(crate) fn update_tsconfig_test_framework_types(
    base_path: &Path,
    new_test_framework: Option<&TestFramework>,
    project_names: &[&str],
) -> Result<Vec<RenderedTemplate>> {
    let new_type = new_test_framework.map(test_framework_type_entry);

    let mut templates = vec![];

    // Update tsconfig.base.json
    let base_tsconfig_path = base_path.join("tsconfig.base.json");
    if base_tsconfig_path.exists() {
        let content = read_to_string(&base_tsconfig_path)
            .with_context(|| "Failed to read tsconfig.base.json")?;
        let mut tsconfig: serde_json::Map<String, Value> =
            serde_json::from_str(&content).with_context(|| "Failed to parse tsconfig.base.json")?;

        swap_test_type_in_tsconfig(&mut tsconfig, new_type);

        templates.push(RenderedTemplate {
            path: base_tsconfig_path,
            content: to_string_pretty(&tsconfig)?,
            context: None,
        });
    }

    // Update per-project tsconfig.json files
    for project_name in project_names {
        let project_tsconfig_path = base_path.join(project_name).join("tsconfig.json");
        if project_tsconfig_path.exists() {
            let content = read_to_string(&project_tsconfig_path)
                .with_context(|| format!("Failed to read tsconfig.json for {}", project_name))?;
            let mut tsconfig: serde_json::Map<String, Value> = serde_json::from_str(&content)
                .with_context(|| format!("Failed to parse tsconfig.json for {}", project_name))?;

            // Only update if this tsconfig has its own types array (overrides base)
            if tsconfig
                .get("compilerOptions")
                .and_then(|co| co.get("types"))
                .is_some()
            {
                swap_test_type_in_tsconfig(&mut tsconfig, new_type);

                templates.push(RenderedTemplate {
                    path: project_tsconfig_path,
                    content: to_string_pretty(&tsconfig)?,
                    context: None,
                });
            }
        }
    }

    Ok(templates)
}

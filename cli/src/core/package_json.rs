use std::{fs::read_to_string, path::Path};

use anyhow::{Context, Result, bail};
use application_package_json::ApplicationPackageJson;
use ramhorns::{Content, Template};
use serde::Serialize;
use serde_json::{from_str, to_string_pretty};

use super::{
    manifest::{InitializableManifestConfig, ManifestData},
    pnpm_workspace::PnpmWorkspace,
    rendered_template::{RenderedTemplate, RenderedTemplatesCache},
};
use crate::{
    constants::{
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PACKAGE_JSON, ERROR_FAILED_TO_PARSE_PACKAGE_JSON,
        ERROR_FAILED_TO_READ_PACKAGE_JSON, ERROR_FAILED_TO_UPDATE_APPLICATION_PACKAGE_JSON,
        ERROR_UNSUPPORTED_DATABASE, Runtime,
    },
    core::manifest::{ManifestConfig, ProjectManifestConfig},
};
use package_json_constants::{compose_up_command, split_compose_up_command};

pub(crate) mod application_package_json;
pub(crate) mod package_json_constants;
pub(crate) mod project_package_json;

/// `rendered` (a package.json the CLI reserialized from its structs) laid
/// out like `original`, the file on disk: unchanged content keeps the original
/// bytes, and otherwise every object keeps its original key order, with new
/// keys after the existing ones, and the file keeps its trailing newline.
/// The structs serialize in their own field order, so without this a change
/// that touches one field reorders keys it never touched, and a change and
/// its inverse do not round-trip.
pub(crate) fn preserve_json_layout(original: &str, rendered: &str) -> String {
    preserve_json_layout_renamed(original, rendered, &|key| key.to_string())
}

/// [`preserve_json_layout`] for a change that renames keys (an application
/// rename moves `@old/core` to `@new/core`): `rename` maps an original key to
/// the key it became, which then takes the original's place.
pub(crate) fn preserve_json_layout_renamed(
    original: &str,
    rendered: &str,
    rename: &dyn Fn(&str) -> String,
) -> String {
    use serde_json::{Map, Value};
    fn ordered(original: &Value, rendered: &Value, rename: &dyn Fn(&str) -> String) -> Value {
        match (original, rendered) {
            (Value::Object(before), Value::Object(after)) => {
                // Keys kept from the original, in its order.
                let mut keys: std::vec::Vec<(String, Value)> = std::vec::Vec::new();
                for (key, value) in before {
                    let key = if after.contains_key(key) { key.clone() } else { rename(key) };
                    if let Some(new) = after.get(&key) {
                        keys.push((key, ordered(value, new, rename)));
                    }
                }
                // New keys go after the key they follow in `rendered` (a
                // replaced dependency takes its predecessor's place), or
                // first when they lead it.
                let mut previous: Option<&String> = None;
                for (key, value) in after {
                    if !keys.iter().any(|(existing, _)| existing == key) {
                        let at = previous
                            .and_then(|p| keys.iter().position(|(existing, _)| existing == p))
                            .map_or(0, |i| i + 1);
                        keys.insert(at, (key.clone(), value.clone()));
                    }
                    previous = Some(key);
                }
                Value::Object(keys.into_iter().collect::<Map<String, Value>>())
            }
            (Value::Array(before), Value::Array(after)) => Value::Array(
                after
                    .iter()
                    .enumerate()
                    .map(|(i, value)| {
                        before.get(i).map_or_else(|| value.clone(), |old| ordered(old, value, rename))
                    })
                    .collect(),
            ),
            (_, rendered) => rendered.clone(),
        }
    }
    let (Ok(before), Ok(after)) = (
        serde_json::from_str::<Value>(original),
        serde_json::from_str::<Value>(rendered),
    ) else {
        return rendered.to_string();
    };
    if before == after {
        return original.to_string();
    }
    let Ok(mut out) = to_string_pretty(&ordered(&before, &after, rename)) else {
        return rendered.to_string();
    };
    if original.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub(crate) fn add_project_definition_to_package_json<
    T: Content + ManifestConfig + ProjectManifestConfig + InitializableManifestConfig + Serialize,
>(
    base_path: &Path,
    manifest_data: &T,
) -> Result<String> {
    let mut full_package_json: ApplicationPackageJson = from_str(
        &read_to_string(base_path.join("package.json"))
            .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?,
    )
    .with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?;

    if let Some(workspaces) = full_package_json.workspaces.as_mut() {
        if !workspaces.contains(&manifest_data.name()) {
            workspaces.push(manifest_data.name().clone());
        }
    }

    Ok(Template::new(
        to_string_pretty(&full_package_json)
            .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PACKAGE_JSON)?
            .as_str(),
    )?
    .render(&manifest_data))
}

pub(crate) fn update_application_package_json(
    manifest_data: &ManifestData,
    base_path: &Path,
    existing_package_json: Option<String>,
) -> Result<Option<RenderedTemplate>> {
    let mut full_package_json: ApplicationPackageJson = from_str(
        &existing_package_json.unwrap_or(
            read_to_string(base_path.join("package.json"))
                .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?,
        ),
    )
    .with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?;

    if let ManifestData::Worker(worker_data) = manifest_data {
        if !worker_data.is_database_enabled {
            return Ok(Some(RenderedTemplate {
                path: base_path.join("package.json"),
                content: to_string_pretty(&full_package_json)?,
                context: Some(ERROR_FAILED_TO_UPDATE_APPLICATION_PACKAGE_JSON.to_string()),
            }));
        }
    }

    let scripts = full_package_json.scripts.as_mut().unwrap();

    let database = match manifest_data {
        ManifestData::Service(service_data) => service_data.database.to_string(),
        ManifestData::Worker(worker_data) => worker_data.database.clone().unwrap(),
        _ => bail!(ERROR_UNSUPPORTED_DATABASE),
    };

    let is_mongo = match manifest_data {
        ManifestData::Service(service_data) => service_data.is_mongo,
        ManifestData::Worker(worker_data) => worker_data.is_mongo,
        _ => false,
    };

    let update_docker_cmd = |cmd: &str| {
        let (mut services, rest_cmd) = split_compose_up_command(cmd);
        if !services.contains(&database) {
            services.push(database.clone());
            if is_mongo {
                services.push("mongo-init".to_string());
            }
        }
        let new_docker_cmd = compose_up_command(&services);
        if rest_cmd.is_empty() {
            new_docker_cmd
        } else {
            format!("{} && {}", new_docker_cmd, rest_cmd)
        }
    };

    if let Some(init_script) = &scripts.migrate_init {
        scripts.migrate_init = Some(update_docker_cmd(&init_script));
    }

    if let Some(create_script) = &scripts.migrate_create {
        scripts.migrate_create = Some(update_docker_cmd(&create_script));
    }

    if let Some(up_script) = &scripts.migrate_up {
        scripts.migrate_up = Some(update_docker_cmd(&up_script));
    }

    if let Some(down_script) = &scripts.migrate_down {
        scripts.migrate_down = Some(update_docker_cmd(&down_script));
    }

    if let Some(seed_script) = &scripts.seed {
        scripts.seed = Some(update_docker_cmd(&seed_script));
    }

    Ok(Some(
        RenderedTemplate {
            path: base_path.join("package.json"),
            content: to_string_pretty(&full_package_json)?,
            context: Some(ERROR_FAILED_TO_UPDATE_APPLICATION_PACKAGE_JSON.to_string()),
        }
        .preserving_json_layout(),
    ))
}

pub(crate) fn replace_project_in_workspace_definition(
    application_base_path: &Path,
    application_package_json: &mut ApplicationPackageJson,
    runtime: &Runtime,
    existing_project_name: &str,
    new_project_name: &str,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    match runtime {
        Runtime::Bun => {
            if let Some(workspaces) = application_package_json.workspaces.as_mut() {
                workspaces.remove(
                    workspaces
                        .iter()
                        .position(|name| name == existing_project_name)
                        .unwrap(),
                );
                workspaces.push(new_project_name.to_string());
            }
        }
        Runtime::Node => {
            let workspace_definition = rendered_templates_cache
                .get(Path::new(application_base_path).join("pnpm-workspace.yaml"))?
                .unwrap()
                .content;

            let mut workspace_definition: PnpmWorkspace =
                serde_yml::from_str(&workspace_definition)
                    .with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?;

            workspace_definition.packages.remove(
                workspace_definition
                    .packages
                    .iter()
                    .position(|name| name == existing_project_name)
                    .unwrap(),
            );

            workspace_definition
                .packages
                .push(new_project_name.to_string());

            rendered_templates_cache.insert(
                application_base_path
                    .join("pnpm-workspace.yaml")
                    .to_string_lossy(),
                RenderedTemplate {
                    path: application_base_path.join("pnpm-workspace.yaml"),
                    content: serde_yml::to_string(&workspace_definition)?,
                    context: None,
                },
            );
        }
    }

    Ok(())
}

pub(crate) fn remove_project_definition_to_package_json(
    base_path: &Path,
    project_name: &str,
) -> Result<String> {
    let mut full_package_json: ApplicationPackageJson = from_str(
        &read_to_string(base_path.join("package.json"))
            .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?,
    )
    .with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?;

    if let Some(workspaces) = full_package_json.workspaces.as_mut() {
        if let Some(position) = workspaces.iter().position(|name| name == project_name) {
            workspaces.remove(position);
        }
    }

    Ok(to_string_pretty(&full_package_json)
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PACKAGE_JSON)?)
}

#[cfg(test)]
mod layout_tests {
    use super::preserve_json_layout;

    #[test]
    fn unchanged_content_keeps_its_bytes_and_changes_keep_key_order() {
        let original = "{\n  \"name\": \"a\",\n  \"dependencies\": {\"@app/core\": \"workspace:*\", \"zod\": \"1\"}\n}\n";
        let same = "{\"dependencies\":{\"zod\":\"1\",\"@app/core\":\"workspace:*\"},\"name\":\"a\"}";
        assert_eq!(preserve_json_layout(original, same), original);
        let changed = "{\"dependencies\":{\"zod\":\"2\",\"@app/core\":\"workspace:*\",\"uuid\":\"1\"},\"name\":\"a\"}";
        assert_eq!(
            preserve_json_layout(original, changed),
            "{\n  \"name\": \"a\",\n  \"dependencies\": {\n    \"@app/core\": \"workspace:*\",\n    \"uuid\": \"1\",\n    \"zod\": \"2\"\n  }\n}\n"
        );
    }
}

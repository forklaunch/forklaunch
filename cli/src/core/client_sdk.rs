use std::{collections::HashMap, fs::read_to_string, path::Path};

use anyhow::{Context, Result, bail};
use convert_case::{Case, Casing};
use oxc_allocator::Allocator;
use oxc_ast::ast::SourceType;
use oxc_codegen::{Codegen, CodegenOptions};
use serde_json::from_str;

use crate::{
    constants::{
        ERROR_FAILED_TO_PARSE_PACKAGE_JSON, ERROR_FAILED_TO_READ_PACKAGE_JSON,
        error_failed_to_read_file,
    },
    core::package_json::package_json_constants::CORE_VERSION,
    core::{
        ast::{
            injections::inject_into_client_sdk::{ClientSdkSpecialCase, inject_into_client_sdk},
            parse_ast_program::parse_ast_program,
            transformations::transform_client_sdk::{
                transform_client_sdk_add_sdk_with_special_case, transform_client_sdk_change_sdk,
                transform_client_sdk_remove_sdk,
            },
        },
        package_json::project_package_json::ProjectPackageJson,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache},
    },
};

pub(crate) fn get_client_sdk_additional_deps(
    app_name: &String,
    is_billing_enabled: bool,
    is_iam_enabled: bool,
) -> HashMap<String, String> {
    let mut additional_deps = HashMap::new();

    if is_billing_enabled {
        additional_deps.insert(format!("@{app_name}/billing"), "workspace:*".to_string());
    }
    if is_iam_enabled {
        additional_deps.insert(format!("@{app_name}/iam"), "workspace:*".to_string());
    }
    additional_deps
}

pub(crate) fn add_project_to_client_sdk(
    rendered_templates_cache: &mut RenderedTemplatesCache,
    base_path: &Path,
    app_name: &str,
    name: &str,
    special_case: Option<ClientSdkSpecialCase>,
) -> Result<()> {
    let kebab_case_app_name = &app_name.to_case(Case::Kebab);
    let kebab_case_name = &name.to_case(Case::Kebab);

    let sdk_ts_path = base_path.join("client-sdk").join("clientSdk.ts");
    let sdk_package_json_path = base_path.join("client-sdk").join("package.json");

    let new_ts_content = transform_client_sdk_add_sdk_with_special_case(
        rendered_templates_cache,
        base_path,
        app_name,
        name,
        special_case,
    )?;

    rendered_templates_cache.insert(
        sdk_ts_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: sdk_ts_path,
            content: new_ts_content,
            context: Some("Failed to write client SDK".to_string()),
        },
    );

    let sdk_pkg_template = rendered_templates_cache.get(&sdk_package_json_path)?;
    let mut client_sdk_project_json: ProjectPackageJson = if let Some(template) = sdk_pkg_template {
        from_str(&template.content).context(ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?
    } else {
        from_str(
            &read_to_string(&sdk_package_json_path)
                .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?
    };

    client_sdk_project_json
        .dev_dependencies
        .as_mut()
        .unwrap()
        .additional_deps
        .insert(
            format!("@{}/{}", &kebab_case_app_name, &kebab_case_name),
            "workspace:*".to_string(),
        );

    // Ensure @forklaunch/core is a dependency (needed by compliance.ts)
    if let Some(ref mut deps) = client_sdk_project_json.dependencies {
        if deps.forklaunch_core.is_none() {
            deps.forklaunch_core = Some(CORE_VERSION.to_string());
        }
    }

    rendered_templates_cache.insert(
        sdk_package_json_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: sdk_package_json_path,
            content: serde_json::to_string_pretty(&client_sdk_project_json)?,
            context: Some("Failed to write SDK package.json".to_string()),
        },
    );

    Ok(())
}

pub(crate) fn remove_project_from_client_sdk(
    rendered_templates_cache: &mut RenderedTemplatesCache,
    base_path: &Path,
    app_name: &str,
    name: &str,
) -> Result<()> {
    let kebab_case_app_name = &app_name.to_case(Case::Kebab);
    let kebab_case_name = &name.to_case(Case::Kebab);

    let sdk_ts_path = base_path.join("client-sdk").join("clientSdk.ts");
    let sdk_package_json_path = base_path.join("client-sdk").join("package.json");

    let new_ts_content =
        transform_client_sdk_remove_sdk(rendered_templates_cache, base_path, app_name, name)?;

    rendered_templates_cache.insert(
        sdk_ts_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: sdk_ts_path,
            content: new_ts_content,
            context: Some("Failed to write client SDK".to_string()),
        },
    );

    let sdk_pkg_template = rendered_templates_cache.get(&sdk_package_json_path)?;
    let mut client_sdk_project_json: ProjectPackageJson = if let Some(template) = sdk_pkg_template {
        from_str(&template.content).context(ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?
    } else {
        from_str(
            &read_to_string(&sdk_package_json_path)
                .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?
    };

    if let Some(ref mut dev_deps) = client_sdk_project_json.dev_dependencies {
        dev_deps
            .additional_deps
            .remove(&format!("@{}/{}", &kebab_case_app_name, &kebab_case_name));
    }

    if let Some(ref mut deps) = client_sdk_project_json.dependencies {
        deps.additional_deps
            .remove(&format!("@{}/{}", &kebab_case_app_name, &kebab_case_name));
    }

    rendered_templates_cache.insert(
        sdk_package_json_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: sdk_package_json_path,
            content: serde_json::to_string_pretty(&client_sdk_project_json)?,
            context: Some("Failed to write SDK package.json".to_string()),
        },
    );

    Ok(())
}

pub(crate) fn change_project_in_client_sdk(
    rendered_templates: &mut RenderedTemplatesCache,
    base_path: &Path,
    app_name: &str,
    existing_name: &str,
    name: &str,
) -> Result<()> {
    let kebab_case_app_name = &app_name.to_case(Case::Kebab);
    let kebab_case_existing_name = &existing_name.to_case(Case::Kebab);
    let kebab_case_name = &name.to_case(Case::Kebab);

    rendered_templates.insert(
        base_path
            .join("client-sdk")
            .join("clientSdk.ts")
            .to_string_lossy(),
        RenderedTemplate {
            path: base_path.join("client-sdk").join("clientSdk.ts"),
            content: transform_client_sdk_change_sdk(
                rendered_templates,
                base_path,
                app_name,
                existing_name,
                name,
            )?,
            context: None,
        },
    );

    let sdk_package_json_path = base_path.join("client-sdk").join("package.json");
    let sdk_pkg_template = rendered_templates.get(&sdk_package_json_path)?;
    let mut client_sdk_project_json: ProjectPackageJson = if let Some(template) = sdk_pkg_template {
        from_str(&template.content).context(ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?
    } else {
        bail!(error_failed_to_read_file(&sdk_package_json_path));
    };

    let additional_deps = &mut client_sdk_project_json
        .dev_dependencies
        .as_mut()
        .unwrap()
        .additional_deps;

    additional_deps.remove(&format!(
        "@{}/{}",
        &kebab_case_app_name, &kebab_case_existing_name
    ));
    additional_deps.insert(
        format!("@{}/{}", &kebab_case_app_name, &kebab_case_name),
        "workspace:*".to_string(),
    );

    rendered_templates.insert(
        sdk_package_json_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: sdk_package_json_path,
            content: serde_json::to_string_pretty(&client_sdk_project_json)?,
            context: None,
        },
    );

    Ok(())
}

pub(crate) fn add_project_vec_to_client_sdk<'a>(
    app_name: &str,
    projects_to_add: &Vec<String>,
    ast_program_text: &String,
    project_json: &mut ProjectPackageJson,
) -> Result<(String, ProjectPackageJson)> {
    let allocator = Allocator::default();
    let mut ast_program_ast = parse_ast_program(&allocator, ast_program_text, SourceType::ts());
    let kebab_case_app_name = &app_name.to_case(Case::Kebab);
    for project in projects_to_add {
        let kebab_case_project = &project.to_case(Case::Kebab);
        inject_into_client_sdk(
            &allocator,
            &mut ast_program_ast,
            app_name,
            kebab_case_project,
            "",
            None,
        )?;

        let kebab_case_project = &project.to_case(Case::Kebab);
        project_json
            .dev_dependencies
            .as_mut()
            .unwrap()
            .additional_deps
            .insert(
                format!("@{}/{}", &kebab_case_app_name, &kebab_case_project),
                "workspace:*".to_string(),
            );
    }

    // TODO: validate client SDK changes
    Ok((
        Codegen::new()
            .with_options(CodegenOptions::default())
            .build(&ast_program_ast)
            .code,
        project_json.clone(),
    ))
}

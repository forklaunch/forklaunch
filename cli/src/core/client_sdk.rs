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
        manifest::{ProjectEntry, ProjectType},
        package_json::project_package_json::ProjectPackageJson,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache},
    },
};

/// Regenerates `client-sdk/compliance.ts` based on the current project list.
///
/// The generated file imports per-service SDK factories from `./clientSdk` and
/// hardcodes parallel `Promise.all` calls to each service's
/// `compliance.eraseUserData` / `exportUserData` method. Result types are
/// inferred from the SDK factories so callers narrow on the real discriminated
/// `code` union without casts.
///
/// A project participates if:
/// - it is a Service or Worker (libraries are skipped)
/// - it has a database resource (compliance erase/export touches persisted data)
/// - the app has an `iam` project (the generated controller resolves
///   `JWKS_PUBLIC_KEY_URL`, which only exists when iam is configured)
///
/// Special-case: when the iam project's variant is `iam-better-auth`, the
/// `clientIamSdkClient` factory returns `{ core, betterAuth }`, so compliance
/// is accessed via `config.iam.core.compliance.*`.
pub(crate) fn regenerate_client_sdk_compliance(
    rendered_templates_cache: &mut RenderedTemplatesCache,
    base_path: &Path,
    projects: &[ProjectEntry],
) -> Result<()> {
    let path = base_path.join("client-sdk").join("compliance.ts");

    let stub = || RenderedTemplate {
        path: path.clone(),
        content: "// This file is regenerated automatically when services are added, removed,\n// or renamed. Do not edit by hand — your changes will be overwritten.\n//\n// When at least one db-backed service exists alongside an iam project, this\n// file will export `createComplianceClient` with hardcoded calls to each\n// service's `compliance.eraseUserData` / `exportUserData` SDK method.\n\nexport {};\n".to_string(),
        context: Some("Failed to write client-sdk compliance.ts".to_string()),
    };

    // Compliance controllers depend on JWKS_PUBLIC_KEY_URL, which only exists
    // when an iam project is configured. Without iam, no service exposes a
    // working `compliance.eraseUserData`/`exportUserData` SDK method.
    if !projects.iter().any(|p| p.name == "iam") {
        rendered_templates_cache
            .insert(path.to_string_lossy().to_string(), stub());
        return Ok(());
    }

    let mut compliant: Vec<&ProjectEntry> = projects
        .iter()
        .filter(|p| {
            matches!(p.r#type, ProjectType::Service | ProjectType::Worker)
                && p.resources
                    .as_ref()
                    .and_then(|r| r.database.as_ref())
                    .is_some()
        })
        .collect();
    compliant.sort_by(|a, b| a.name.cmp(&b.name));

    if compliant.is_empty() {
        rendered_templates_cache
            .insert(path.to_string_lossy().to_string(), stub());
        return Ok(());
    }

    let mut imports: Vec<String> = Vec::new();
    let mut type_aliases: Vec<String> = Vec::new();
    let mut config_fields: Vec<String> = Vec::new();
    let mut destructure_names: Vec<String> = Vec::new();
    let mut erase_calls: Vec<String> = Vec::new();
    let mut export_calls: Vec<String> = Vec::new();

    for project in &compliant {
        let camel = project.name.to_case(Case::Camel);
        let pascal = project.name.to_case(Case::Pascal);

        let factory = format!("{}SdkClient", camel);
        let type_alias = format!("{}Client", pascal);

        // `clientIamSdkClient` is emitted as a wrapped `{ core, betterAuth }`
        // factory when the iam project uses the better-auth variant.
        let is_better_auth = project.name == "iam"
            && project.variant.as_deref() == Some("iam-better-auth");
        let access = if is_better_auth {
            format!("config.{}.core.compliance", camel)
        } else {
            format!("config.{}.compliance", camel)
        };

        imports.push(factory.clone());
        type_aliases.push(format!(
            "type {} = Awaited<ReturnType<typeof {}>>;",
            type_alias, factory
        ));
        config_fields.push(format!("  {}: {};", camel, type_alias));
        destructure_names.push(camel.clone());
        erase_calls.push(format!(
            "        {}.eraseUserData({{ params: {{ userId }}, headers }})",
            access
        ));
        export_calls.push(format!(
            "        {}.exportUserData({{ params: {{ userId }}, headers }})",
            access
        ));
    }

    let imports_line = format!("import {{ {} }} from './clientSdk';", imports.join(", "));
    let type_aliases_block = type_aliases.join("\n");
    let config_fields_block = config_fields.join("\n");
    let destructure = destructure_names.join(", ");
    let return_object = destructure_names.join(", ");
    let erase_block = erase_calls.join(",\n");
    let export_block = export_calls.join(",\n");

    let content = format!(
"{imports_line}

{type_aliases_block}

/**
 * Compliance fan-out client. Calls erase/export on every registered service
 * in parallel. Per-service responses are the exact discriminated unions
 * produced by each SDK (e.g. `{{ code: 200; response: {{...}} }} | {{ code: 404; response: string }}`),
 * so callers narrow on `code` without any casts.
 *
 * This file is regenerated when services are added, removed, or renamed.
 *
 * Requires a JWT token from a user with SYSTEM role.
 */
export function createComplianceClient(config: {{
  token: string;
{config_fields_block}
}}) {{
  const headers = {{ authorization: `Bearer ${{config.token}}` }} as const;

  return {{
    async erase(userId: string) {{
      const [{destructure}] = await Promise.all([
{erase_block}
      ]);
      return {{ {return_object} }};
    }},

    async export(userId: string) {{
      const [{destructure}] = await Promise.all([
{export_block}
      ]);
      return {{ {return_object} }};
    }}
  }};
}}
"
    );

    rendered_templates_cache.insert(
        path.to_string_lossy().to_string(),
        RenderedTemplate {
            path,
            content,
            context: Some("Failed to write client-sdk compliance.ts".to_string()),
        },
    );

    Ok(())
}

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

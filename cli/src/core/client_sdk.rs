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
        package_json::{
            package_json_constants::{BETTER_AUTH_VERSION, CORE_VERSION, VALIDATOR_VERSION},
            project_package_json::ProjectPackageJson,
        },
        rendered_template::{RenderedTemplate, RenderedTemplatesCache},
    },
};

/// Regenerates `client-sdk/compliance.ts` based on the current project list.
///
/// The generated file imports TypeScript SDK factories from `./clientSdk`, accepts
/// typed native adapters, and makes parallel `Promise.all` calls (a direct await
/// when only one service participates) to each service's
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

    let stub = || {
        RenderedTemplate {
        path: path.clone(),
        content: "// This file is regenerated automatically when services are added, removed,\n// or renamed. Do not edit by hand — your changes will be overwritten.\n//\n// When at least one db-backed service exists alongside an iam project, this\n// file will export `createComplianceClient` with hardcoded calls to each\n// service's `compliance.eraseUserData` / `exportUserData` SDK method.\n\nexport {};\n".to_string(),
        context: Some("Failed to write client-sdk compliance.ts".to_string()),
    }
    };

    // Compliance controllers depend on JWKS_PUBLIC_KEY_URL, which only exists
    // when an iam project is configured. Without iam, no service exposes a
    // working `compliance.eraseUserData`/`exportUserData` SDK method.
    if !projects.iter().any(|project| {
        project.r#type == ProjectType::Service
            && (project.name == "iam"
                || matches!(
                    project.variant.as_deref(),
                    Some("iam-base" | "iam-better-auth")
                ))
    }) {
        rendered_templates_cache.insert(path.to_string_lossy().to_string(), stub());
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
        rendered_templates_cache.insert(path.to_string_lossy().to_string(), stub());
        return Ok(());
    }

    let mut imports: Vec<String> = Vec::new();
    let mut type_aliases: Vec<String> = Vec::new();
    let mut config_fields: Vec<String> = Vec::new();
    let mut destructure_names: Vec<String> = Vec::new();
    let mut erase_calls: Vec<String> = Vec::new();
    let mut export_calls: Vec<String> = Vec::new();
    let mut response_parameters: Vec<String> = Vec::new();
    let mut client_checks: Vec<String> = Vec::new();

    for project in &compliant {
        let camel = project.name.to_case(Case::Camel);
        let pascal = project.name.to_case(Case::Pascal);

        let factory = format!("{}SdkClient", camel);
        let type_alias = format!("{}Client", pascal);

        let native = project.metadata.as_ref().and_then(|metadata| metadata.language.as_deref()) == Some("rust");
        // Only the TypeScript Better Auth factory wraps its API in `core`.
        let is_better_auth = !native && project.name == "iam" && project.variant.as_deref() == Some("iam-better-auth");
        let access = if is_better_auth {
            format!("config.{}.core.compliance", camel)
        } else {
            format!("config.{}.compliance", camel)
        };
        let erase_response = format!("{pascal}EraseResponse");
        let export_response = format!("{pascal}ExportResponse");
        response_parameters.push(format!("  {erase_response} = unknown"));
        response_parameters.push(format!("  {export_response} = unknown"));
        let client_type = if native {
            format!("ComplianceClient<{erase_response}, {export_response}>")
        } else {
            imports.push(factory.clone());
            type_aliases.push(format!("type {type_alias} = Awaited<ReturnType<typeof {factory}>>;"));
            if is_better_auth {
                format!("{type_alias} & {{ core: RequireCompliance<{type_alias}['core'], {erase_response}, {export_response}> }}")
            } else {
                format!("RequireCompliance<{type_alias}, {erase_response}, {export_response}>")
            }
        };
        config_fields.push(format!("  {camel}: {client_type};"));
        let checked_access = if is_better_auth {
            format!("config.{camel}?.core?.compliance")
        } else { format!("config.{camel}?.compliance") };
        client_checks.push(format!("    {{ name: {}, client: {checked_access} }}", serde_json::to_string(&project.name)?));
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

    let imports_line = if imports.is_empty() {
        String::new()
    } else {
        format!("import {{ {} }} from './clientSdk';", imports.join(", "))
    };
    let type_aliases_block = type_aliases.join("\n");
    let config_fields_block = config_fields.join("\n");
    let destructure = destructure_names.join(", ");
    let return_object = destructure_names.join(", ");
    // A lone service awaits its call directly: `Promise.all` over a single
    // promise adds nothing and fails lint (unicorn/no-single-promise-in-promise-methods).
    let fan_out = |calls: &[String]| match calls {
        [call] => format!("      const {destructure} = await {};", call.trim_start()),
        _ => format!(
            "      const [{destructure}] = await Promise.all([\n{}\n      ]);",
            calls.join(",\n")
        ),
    };
    let erase_block = fan_out(&erase_calls);
    let export_block = fan_out(&export_calls);
    let response_parameters = response_parameters.join(",\n");
    let client_checks = client_checks.join(",\n");

    let content = format!(
"{imports_line}

{type_aliases_block}

type ComplianceRequest = {{ params: {{ userId: string }}; headers: {{ authorization: string }} }};
type ComplianceClient<Erase, Export> = {{
  compliance: {{
    eraseUserData: (request: ComplianceRequest) => Promise<Erase>;
    exportUserData: (request: ComplianceRequest) => Promise<Export>;
  }};
}};
type RequireCompliance<Client, Erase, Export> = Client extends {{
  compliance: {{ eraseUserData: (...args: never[]) => unknown; exportUserData: (...args: never[]) => unknown }}
}} ? Client : Client & ComplianceClient<Erase, Export>;

/**
 * Compliance fan-out client. Calls erase/export on every registered service
 * in parallel. Per-service responses are the exact discriminated unions
 * produced by each SDK (e.g. `{{ code: 200; response: {{...}} }} | {{ code: 404; response: string }}`),
 * so callers narrow on `code` without any casts.
 *
 * Native clients are supplied as {{ compliance: sharedClient.service.compliance }}.
 * Services without declared compliance routes require an explicit implementation;
 * no registered database service is silently omitted. All clients are checked
 * before any request is dispatched. Adapter response types remain inferred.
 * This file is regenerated when services are added, removed, or renamed.
 *
 * Requires a JWT token from a user with SYSTEM role.
 */
export function createComplianceClient<
{response_parameters}
>(config: {{
  token: string;
{config_fields_block}
}}) {{
  const clients = [
{client_checks}
  ];
  for (const {{ name, client }} of clients) {{
    if (!client || typeof client.eraseUserData !== 'function' || typeof client.exportUserData !== 'function') {{
      throw new Error(`Missing compliance handlers for ${{name}}; provide both eraseUserData and exportUserData before creating the client`);
    }}
  }}
  const headers = {{ authorization: `Bearer ${{config.token}}` }} as const;

  return {{
    async erase(userId: string) {{
{erase_block}
      return {{ {return_object} }};
    }},

    async export(userId: string) {{
{export_block}
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
    is_messaging_enabled: bool,
    is_cac_enabled: bool,
    is_ecommerce_enabled: bool,
) -> HashMap<String, String> {
    let mut additional_deps = HashMap::new();

    if is_billing_enabled {
        additional_deps.insert(format!("@{app_name}/billing"), "workspace:*".to_string());
    }
    if is_iam_enabled {
        additional_deps.insert(format!("@{app_name}/iam"), "workspace:*".to_string());
    }
    if is_messaging_enabled {
        additional_deps.insert(format!("@{app_name}/messaging"), "workspace:*".to_string());
    }
    if is_cac_enabled {
        additional_deps.insert(format!("@{app_name}/cac"), "workspace:*".to_string());
    }
    if is_ecommerce_enabled {
        additional_deps.insert(format!("@{app_name}/ecommerce"), "workspace:*".to_string());
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

    let better_auth = matches!(
        special_case.as_ref(),
        Some(ClientSdkSpecialCase::BetterAuth)
    );
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
        .get_or_insert_with(Default::default)
        .additional_deps
        .insert(
            format!("@{}/{}", &kebab_case_app_name, &kebab_case_name),
            "workspace:*".to_string(),
        );

    let dependencies = client_sdk_project_json
        .dependencies
        .get_or_insert_with(Default::default);
    dependencies
        .forklaunch_core
        .get_or_insert_with(|| CORE_VERSION.to_string());
    // Inferred SDK client types (the Better Auth factory) name the validator's
    // schema types in declaration output.
    dependencies
        .forklaunch_validator
        .get_or_insert_with(|| VALIDATOR_VERSION.to_string());
    if better_auth {
        dependencies
            .better_auth
            .get_or_insert_with(|| BETTER_AUTH_VERSION.to_string());
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
            .shift_remove(&format!("@{}/{}", &kebab_case_app_name, &kebab_case_name));
    }

    if let Some(ref mut deps) = client_sdk_project_json.dependencies {
        deps.additional_deps
            .shift_remove(&format!("@{}/{}", &kebab_case_app_name, &kebab_case_name));
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

    additional_deps.shift_remove(&format!(
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

#[cfg(test)]
mod compliance_tests {
    use super::*;

    fn project(name: &str, language: &str, variant: Option<&str>) -> ProjectEntry {
        serde_json::from_value(serde_json::json!({
            "type":"Service", "name":name, "description":"Fixture",
            "variant":variant, "resources":{"database":"postgresql"},
            "metadata":{"language":language}
        }))
        .unwrap()
    }

    #[test]
    fn client_sdk_depends_on_every_module_package_it_exports() {
        // compliance.ts imports `<module>SdkClient` from clientSdk.ts for every
        // db-backed module, so the client SDK must depend on ecommerce too.
        let deps = get_client_sdk_additional_deps(&"shop".to_string(), true, true, false, false, true);
        assert_eq!(deps.get("@shop/ecommerce").map(String::as_str), Some("workspace:*"));
        let deps = get_client_sdk_additional_deps(&"shop".to_string(), true, true, false, false, false);
        assert!(!deps.contains_key("@shop/ecommerce"));
    }

    fn generated(projects: &[ProjectEntry]) -> String {
        let root = tempfile::tempdir().unwrap();
        let mut cache = RenderedTemplatesCache::new();
        regenerate_client_sdk_compliance(&mut cache, root.path(), projects).unwrap();
        cache
            .get(root.path().join("client-sdk/compliance.ts"))
            .unwrap()
            .unwrap()
            .content
    }

    #[test]
    fn better_auth_sdk_declares_its_runtime_dependency_and_preserves_overrides() {
        for version in [None, Some("custom-version")] {
            let root = tempfile::tempdir().unwrap();
            let mut cache = RenderedTemplatesCache::new();
            let sdk = root.path().join("client-sdk/clientSdk.ts");
            cache.insert(
                &sdk.display(),
                RenderedTemplate {
                    path: sdk.clone(),
                    content: "import { universalSdk } from '@forklaunch/universal-sdk';".into(),
                    context: None,
                },
            );
            let package = root.path().join("client-sdk/package.json");
            let content = match version {
                Some(version) => {
                    serde_json::json!({"name":"@fixture/client-sdk", "dependencies":{"better-auth":version}})
                }
                None => serde_json::json!({"name":"@fixture/client-sdk"}),
            };
            cache.insert(
                &package.display(),
                RenderedTemplate {
                    path: package.clone(),
                    content: content.to_string(),
                    context: None,
                },
            );
            add_project_to_client_sdk(
                &mut cache,
                root.path(),
                "fixture",
                "iam",
                Some(ClientSdkSpecialCase::BetterAuth),
            )
            .unwrap();
            let package: serde_json::Value =
                serde_json::from_str(&cache.get(package).unwrap().unwrap().content).unwrap();
            assert_eq!(
                package["dependencies"]["better-auth"],
                version.unwrap_or(BETTER_AUTH_VERSION)
            );
            assert_eq!(package["devDependencies"]["@fixture/iam"], "workspace:*");
            assert_eq!(package["dependencies"]["@forklaunch/core"], CORE_VERSION);
        }
    }

    #[test]
    fn native_clients_use_typed_adapters_without_typescript_factory_imports() {
        let source = generated(&[
            project("iam", "typescript", Some("iam-base")),
            project("records", "rust", None),
        ]);
        assert!(source.contains("iamSdkClient } from './clientSdk'"));
        assert!(!source.contains("recordsSdkClient"));
        assert!(
            source
                .contains("records: ComplianceClient<RecordsEraseResponse, RecordsExportResponse>")
        );
        assert!(
            source.contains("RequireCompliance<IamClient, IamEraseResponse, IamExportResponse>")
        );
        assert!(source.contains("config.records.compliance.eraseUserData"));
        assert!(source.contains("Missing compliance handlers for"));
        assert!(
            source.find("for (const { name, client }").unwrap()
                < source.find("async erase(").unwrap()
        );
    }

    #[test]
    fn only_typescript_better_auth_clients_use_the_core_wrapper() {
        let typescript = generated(&[project("iam", "typescript", Some("iam-better-auth"))]);
        assert!(typescript.contains("RequireCompliance<IamClient['core']"));
        assert!(typescript.contains("config.iam.core.compliance.eraseUserData"));
        let native = generated(&[project("identity", "rust", Some("iam-better-auth"))]);
        assert!(!native.contains("SdkClient"));
        assert!(!native.contains(".core."));
        assert!(native.contains("config.identity.compliance.eraseUserData"));
    }

    #[test]
    fn applications_without_iam_keep_the_inactive_compliance_module() {
        let source = generated(&[project("records", "rust", None)]);
        assert!(source.ends_with("export {};\n"));
    }
}

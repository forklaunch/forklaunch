#[cfg(test)]
use std::fs::write;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{create_dir_all, read_to_string},
    io::Write,
    path::Path,
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command, parser::ValueSource};
use convert_case::{Case, Casing};
use ramhorns::Content;
use serde_json::{Value as JsonValue, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};
use toml::Value;

use super::{
    media::{normalized_media_type, request_media_type},
    models::*,
    render::*,
};
use crate::{
    CliCommand,
    core::{
        command::command, rendered_template::TEMPLATES_DIR, template::render_embedded_template,
    },
    init::rust_project::{self, add_rust_workspace_member},
};

#[derive(Debug)]
pub(crate) struct GenerateCommand;

impl GenerateCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for GenerateCommand {
    fn command(&self) -> Command {
        command("generate", "Generate SDKs from ForkLaunch artifacts")
            .arg(
                Arg::new("base_path")
                    .long("path")
                    .short('p')
                    .help("Path to application root"),
            )
            .arg(
                Arg::new("target")
                    .long("target")
                    .short('t')
                    .default_value("all")
                    .value_parser(["ts", "typescript", "rust", "all"])
                    .help("SDK target to generate"),
            )
            .arg(
                Arg::new("target_alias")
                    .index(1)
                    .value_parser(["ts", "typescript", "rust", "all"])
                    .help("SDK target shorthand: ts, rust, or all"),
            )
            .arg(
                Arg::new("service")
                    .long("service")
                    .short('s')
                    .help("Generate SDKs for one service"),
            )
            .arg(
                Arg::new("from")
                    .long("from")
                    .default_value("openapi")
                    .value_parser(["openapi", "contract", "socket-contract", "asyncapi", "all"])
                    .help("Artifact source (all combines HTTP contracts and socket contracts)"),
            )
            .arg(
                Arg::new("check")
                    .long("check")
                    .action(ArgAction::SetTrue)
                    .help(
                        "Check whether generated SDK outputs are up to date without writing files",
                    ),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let (app_root, _) = crate::core::validate::require_manifest(matches)?;

        let target = resolve_sdk_generate_target(matches)?;
        let source = matches.get_one::<String>("from").unwrap().as_str();
        let selected_service = matches.get_one::<String>("service").map(String::as_str);
        let check = matches.get_flag("check");

        generate_or_check_sdk_outputs(
            &app_root,
            &target,
            source,
            selected_service,
            check,
            &mut stdout,
        )
    }
}

fn resolve_sdk_generate_target(matches: &ArgMatches) -> Result<String> {
    let flag_target = matches
        .get_one::<String>("target")
        .map(String::as_str)
        .unwrap_or("all");
    let resolved_flag_target = normalize_sdk_generate_target(flag_target)?;
    let explicit_flag_target = matches.value_source("target") == Some(ValueSource::CommandLine);

    let Some(target_alias) = matches
        .get_one::<String>("target_alias")
        .map(String::as_str)
    else {
        return Ok(resolved_flag_target);
    };

    let resolved_alias_target = normalize_sdk_generate_target(target_alias)?;
    if explicit_flag_target && resolved_alias_target != resolved_flag_target {
        bail!(
            "SDK target alias `{}` conflicts with --target `{}`",
            target_alias,
            flag_target
        );
    }

    Ok(resolved_alias_target)
}

fn normalize_sdk_generate_target(target: &str) -> Result<String> {
    match target {
        "ts" | "typescript" => Ok("typescript".to_string()),
        "rust" => Ok("rust".to_string()),
        "all" => Ok("all".to_string()),
        _ => bail!("Unsupported SDK target `{target}`"),
    }
}

pub(crate) fn generate_or_check_sdk_outputs(
    app_root: &Path,
    target: &str,
    source: &str,
    selected_service: Option<&str>,
    check: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    generate_or_check(app_root, target, source, selected_service, check, false, stdout)
}

/// Watch passes generate what exists: a TypeScript service whose OpenAPI export
/// has not been written yet (development exports it from its container) joins
/// once the export appears.
pub(crate) fn generate_sdk_outputs_while_watching(
    app_root: &Path,
    target: &str,
    source: &str,
    selected_service: Option<&str>,
    stdout: &mut StandardStream,
) -> Result<()> {
    generate_or_check(app_root, target, source, selected_service, false, true, stdout)
}

fn generate_or_check(
    app_root: &Path,
    target: &str,
    source: &str,
    selected_service: Option<&str>,
    check: bool,
    pending_exports: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    if matches!(source, "all" | "all-openapi") {
        // One watcher serializes both writers because they share workspace metadata.
        // Try both surfaces so a missing HTTP artifact does not block socket updates.
        let http = generate_or_check(
            app_root,
            target,
            if source == "all-openapi" {
                "openapi"
            } else {
                "contract"
            },
            selected_service,
            check,
            pending_exports,
            stdout,
        );
        let sockets = super::websocket::generate_from(
            app_root,
            target,
            "socket-contract",
            selected_service,
            check,
            stdout,
            true,
        );
        return match (http, sockets) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(http), Err(sockets)) => bail!("HTTP SDK: {http:#}; WebSocket SDK: {sockets:#}"),
            (Err(error), _) | (_, Err(error)) => Err(error),
        };
    }
    if matches!(source, "socket-contract" | "asyncapi") {
        return super::websocket::generate_from(
            app_root,
            target,
            source,
            selected_service,
            check,
            stdout,
            false,
        );
    }
    let manifest = read_manifest(app_root)?;
    // A watch pass that skipped a service still waiting for its export writes
    // only what it found and keeps the skipped service's existing outputs.
    let mut skipped_exports = false;
    let services = match source {
        "contract" | "openapi" if pending_exports => {
            let (services, skipped) =
                exported_http_sdk_services(app_root, &manifest, selected_service, source, stdout)?;
            skipped_exports = skipped;
            services
        }
        "contract" => http_sdk_services_from_contracts(app_root, &manifest, selected_service)?,
        "openapi" => {
            http_sdk_services_from_artifacts(app_root, &manifest, selected_service, "openapi")?
        }
        // Internal lifecycle scaffolding can run before artifacts exist. This
        // source is deliberately unavailable through the public command.
        "manifest" => rust_sdk_services(&manifest, selected_service),
        _ => bail!("Unsupported SDK artifact source {source}"),
    };

    if services.is_empty() {
        if pending_exports {
            return Ok(());
        }
        bail!("No HTTP services or workers found for SDK generation");
    }

    // Validate every requested output before mutating workspace files.
    if matches!(target, "typescript" | "all") {
        typescript_sdk(&services)?;
    }
    if matches!(target, "rust" | "all") {
        validate_rust_output_layout(app_root, &services)?;
        for service in &services {
            rust_sdk(service)?;
        }
    }

    if check {
        if selected_service.is_some() {
            check_sdk_outputs_scoped(app_root, target, &services, true)?;
        } else {
            check_sdk_outputs(app_root, target, &services)?;
        }
        log_ok!(stdout, "SDK outputs are up to date");
        return Ok(());
    }

    if matches!(target, "typescript" | "all") {
        generate_typescript_sdk_scoped(
            app_root,
            &services,
            selected_service.is_some() || skipped_exports,
        )?;
        log_ok!(stdout, "Generated TypeScript SDK");
    }

    if matches!(target, "rust" | "all") {
        generate_rust_sdks(app_root, &services)?;
        log_ok!(stdout, "Generated Rust SDK sources");
    }

    Ok(())
}

pub(crate) fn regenerate_sdk_outputs_if_present(
    app_root: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    let sdk_root = app_root.join(super::layout::typescript(app_root)?);
    let typescript_present = sdk_root.join("package.json").exists()
        || super::layout::source(app_root, &sdk_root)?
            .join("index.ts")
            .exists();
    let rust_root = app_root.join(super::layout::rust(app_root)?);
    if !typescript_present && !rust_root.is_dir() {
        return Ok(());
    }
    let manifest = read_manifest(app_root)?;
    let services = rust_sdk_services_for_lifecycle(app_root, &manifest)?;
    if typescript_present {
        generate_typescript_sdk(app_root, &services)?;
        log_ok!(stdout, "Regenerated TypeScript SDK");
    }
    let existing_rust_services = services
        .into_iter()
        .filter(|service| {
            rust_root
                .join(format!("{}-sdk/Cargo.toml", service.name))
                .is_file()
        })
        .collect::<Vec<_>>();
    if !existing_rust_services.is_empty() {
        generate_rust_sdks(app_root, &existing_rust_services)?;
        log_ok!(stdout, "Regenerated Rust SDK crate(s)");
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct SdkService {
    modern: Option<super::modern::CompiledSdk>,
    name: String,
    ts_models: Vec<RustTypeAliasTemplate>,
    routers: Vec<SdkRouter>,
}

#[derive(Debug, Clone)]
struct SdkRouter {
    name: String,
    operations: Vec<SdkOperation>,
}

#[derive(Debug, Clone)]
struct SdkOperation {
    name: String,
    method: String,
    path: String,
    request_type: String,
    responses_type: String,
    request_params_type: String,
    request_query_type: String,
    request_body_type: String,
    rust_request_body_type: String,
    request_body_optional: bool,
    request_headers_type: String,
    request_kind: String,
    request_content_type: String,
    responses: Vec<SdkResponseVariant>,
    rust_models: Vec<RustModelTemplate>,
    rust_aliases: Vec<RustTypeAliasTemplate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseDecoder {
    Empty,
    Json,
    Text,
    Binary,
    Multipart,
    Sse,
}
impl ResponseDecoder {
    fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Json => "json",
            Self::Text => "text",
            Self::Binary => "binary",
            Self::Multipart => "multipart",
            Self::Sse => "sse",
        }
    }
}
#[derive(Debug, Clone)]
struct SdkResponseContract {
    status: u16,
    schema: Option<JsonValue>,
    decoder: ResponseDecoder,
    content_type: String,
}
impl SdkResponseContract {
    fn empty(status: u16) -> Self {
        Self {
            status,
            schema: None,
            decoder: ResponseDecoder::Empty,
            content_type: "application/json".into(),
        }
    }
}

fn read_manifest(app_root: &Path) -> Result<Value> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let content = read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    toml::from_str::<Value>(&content)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))
}

fn rust_sdk_services(manifest: &Value, selected_service: Option<&str>) -> Vec<SdkService> {
    manifest
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let project_type = project.get("type").and_then(Value::as_str)?;
            if !matches!(project_type, "Service" | "Worker")
                || !crate::core::openapi_export::raw_contract_export_enabled(project)
            {
                return None;
            }
            let name = project.get("name").and_then(Value::as_str)?;
            if let Some(selected_service) = selected_service
                && selected_service != name
            {
                return None;
            }
            let metadata = project.get("metadata").and_then(Value::as_table)?;
            if metadata.get("language").and_then(Value::as_str) != Some("rust") {
                return None;
            }
            let router_names = project
                .get("routers")
                .and_then(Value::as_array)
                .map(|routers| {
                    routers
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let durable_worker = project_type == "Worker"
                && matches!(
                    project.get("variant").and_then(Value::as_str),
                    Some("worker-postgres" | "worker-redis" | "worker-kafka" | "worker-bullmq")
                );
            let router_names = if durable_worker {
                Vec::new()
            } else if router_names.is_empty() {
                vec!["health".to_string()]
            } else {
                router_names
            };
            let routers = router_names
                .iter()
                .map(|router| sdk_router(name, router, project_type == "Service"))
                .collect();
            Some(SdkService {
                modern: None,
                name: name.to_string(),
                ts_models: Vec::new(),
                routers,
            })
        })
        .collect()
}

fn sdk_router(service_name: &str, router_name: &str, service: bool) -> SdkRouter {
    let operation_name = if service {
        if router_name == "health" {
            "getHealth".into()
        } else {
            format!("list{}", router_name.to_case(Case::Pascal))
        }
    } else if router_name == "health" {
        "get".to_string()
    } else {
        "list".to_string()
    };
    let path = format!("/{router_name}");
    let response_schema = default_response_json_schema(router_name);

    SdkRouter {
        name: router_name.to_string(),
        operations: vec![build_sdk_operation(
            service_name,
            router_name,
            &operation_name,
            "GET",
            &path,
            RequestTypeShapes::default(),
            &SdkResponseContract {
                status: 200,
                schema: Some(response_schema),
                decoder: ResponseDecoder::Json,
                content_type: "application/json".into(),
            },
        )],
    }
}

// Lifecycle operations can invalidate one project's artifact. Preserve every
// other project's actual contract, and never conceal malformed artifacts.
fn rust_sdk_services_for_lifecycle(app_root: &Path, manifest: &Value) -> Result<Vec<SdkService>> {
    let mut services = rust_sdk_services(manifest, None)
        .into_iter()
        .map(|fallback| {
            match http_sdk_services_from_contracts(app_root, manifest, Some(&fallback.name)) {
                Ok(mut services) => services.pop().context("Missing Rust SDK service"),
                Err(error)
                    if error.chain().any(|cause| {
                        cause
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    }) =>
                {
                    Ok(fallback)
                }
                Err(error) => Err(error),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    let tracked = super::http_sources::read(app_root)?;
    for project in typescript_sdk_projects(manifest, None) {
        let name = project
            .get("name")
            .and_then(Value::as_str)
            .context("SDK service name is required")?;
        match http_sdk_services_from_artifacts(app_root, manifest, Some(name), "openapi") {
            Ok(imported) => services.extend(imported),
            Err(error)
                if !tracked.contains(name)
                    && error.chain().any(|cause| {
                        cause
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    }) => {}
            Err(error) => {
                return Err(error.context(format!(
                    "Restore the TypeScript OpenAPI export for {name} before regenerating its SDK"
                )));
            }
        }
    }
    Ok(services)
}

/// Every service's artifact, skipping TypeScript services whose export has not
/// been written yet. The flag reports whether any service was skipped.
fn exported_http_sdk_services(
    app_root: &Path,
    manifest: &Value,
    selected_service: Option<&str>,
    source: &str,
    stdout: &mut StandardStream,
) -> Result<(Vec<SdkService>, bool)> {
    let mut services = Vec::new();
    let mut skipped = false;
    for project in super::http_sources::projects(manifest, selected_service) {
        let name = project
            .get("name")
            .and_then(Value::as_str)
            .context("SDK service name is required")?;
        match http_sdk_services_from_artifacts(app_root, manifest, Some(name), source) {
            Ok(found) => services.extend(found),
            Err(error)
                if super::http_sources::is_typescript(project)
                    && error.chain().any(|cause| {
                        cause
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    }) =>
            {
                skipped = true;
                log_info!(stdout, "Waiting for {name} to export its OpenAPI document");
            }
            Err(error) => return Err(error),
        }
    }
    Ok((services, skipped))
}

fn http_sdk_services_from_contracts(
    app_root: &Path,
    manifest: &Value,
    selected_service: Option<&str>,
) -> Result<Vec<SdkService>> {
    http_sdk_services_from_artifacts(app_root, manifest, selected_service, "contract")
}

fn typescript_sdk_projects<'a>(
    manifest: &'a Value,
    selected_service: Option<&str>,
) -> Vec<&'a Value> {
    super::http_sources::projects(manifest, selected_service)
        .into_iter()
        .filter(|project| super::http_sources::is_typescript(project))
        .collect()
}

fn http_sdk_services_from_artifacts(
    app_root: &Path,
    manifest: &Value,
    selected_service: Option<&str>,
    source: &str,
) -> Result<Vec<SdkService>> {
    let mut services = Vec::new();
    for project in super::http_sources::projects(manifest, selected_service) {
        let service_name = project
            .get("name")
            .and_then(Value::as_str)
            .context("SDK service name is required")?;
        let (source, artifact_path) = super::http_sources::artifact(app_root, project, source)?;
        let artifact_path = artifact_path
            .canonicalize()
            .with_context(|| format!("Failed to locate {}", artifact_path.display()))?;
        if !artifact_path.starts_with(app_root.canonicalize()?) {
            bail!("SDK artifact path must stay inside the application");
        }
        if std::fs::metadata(&artifact_path)?.len() > 16 * 1024 * 1024 {
            bail!("SDK artifact exceeds 16 MiB");
        }
        let content = read_to_string(&artifact_path)
            .with_context(|| format!("Failed to read {}", artifact_path.display()))?;
        let artifact = serde_json::from_str::<JsonValue>(&content)
            .with_context(|| format!("Failed to parse {}", artifact_path.display()))?;
        let local = super::layout::local_sdk(app_root, service_name, "typescript")?.is_some();
        let mut contract = if source == "openapi" && local {
            // Share the build compiler's normalization of schema names, empty
            // responses and versioned operations instead of adapting them here.
            let imported = forklaunch_core::build::openapi_artifact_sources(&content, service_name)
                .with_context(|| format!("Failed to import {}", artifact_path.display()))?;
            serde_json::from_str(&imported.contract)?
        } else if source == "openapi" {
            super::openapi::contract_from_openapi(&artifact)
                .with_context(|| format!("Failed to import {}", artifact_path.display()))?
        } else {
            artifact
        };

        if local {
            contract["sdkLayout"] = "project-local-v1".into();
            contract["typescriptRuntimeLayout"] = "local".into();
            if contract.get("rustClientName").is_none() {
                contract["rustClientName"] = "Client".into();
                if !super::layout::typescript_authored(app_root, service_name)? {
                    contract["rustOutputLayout"] = "service-module".into();
                }
            }
        }

        if super::modern::requested(&contract)? {
            services.push(SdkService {
                name: service_name.into(),
                routers: vec![],
                ts_models: vec![],
                modern: Some(super::modern::compile(service_name, &contract)?),
            });
        } else {
            services.push(sdk_service_from_contract(service_name, &contract)?);
        }
    }

    Ok(services)
}

fn sdk_service_from_contract(service_name: &str, contract: &JsonValue) -> Result<SdkService> {
    let mut contract = contract.clone();
    super::versions::expand_contract(&mut contract)?;
    // Internal model references are type-generation metadata, not executable
    // JSON Schema. Keep the actual validation graph before lowering types.
    let runtime_contract = contract.clone();
    let mut ts_models = Vec::new();
    let mut schema_index = 0;
    fn prepare(
        schema: &mut JsonValue,
        service_name: &str,
        index: &mut usize,
        ts_models: &mut Vec<RustTypeAliasTemplate>,
    ) -> Result<()> {
        *index += 1;
        *schema = super::schema::normalize(
            schema,
            &format!(
                "{}Schema{}",
                rust_type_name(service_name).trim_end_matches('_'),
                index
            ),
        )?;
        if let Some(definitions) = schema
            .get(super::schema::DEFINITIONS)
            .and_then(JsonValue::as_object)
        {
            for (name, definition) in definitions {
                ts_models.push(RustTypeAliasTemplate {
                    name: name.clone(),
                    target_type: ts_type_from_schema(definition),
                });
            }
        }
        Ok(())
    }
    if let Some(routes) = contract.get_mut("routes").and_then(JsonValue::as_array_mut) {
        for route in routes {
            if let Some(request) = route.get_mut("request") {
                for field in ["params", "query", "headers"] {
                    if let Some(fields) = request.get_mut(field).and_then(JsonValue::as_array_mut) {
                        for field in fields {
                            if let Some(schema) = field.pointer_mut("/schema/jsonSchema") {
                                prepare(schema, service_name, &mut schema_index, &mut ts_models)?;
                            }
                        }
                    }
                }
                if let Some(schema) = request.pointer_mut("/body/schema/jsonSchema") {
                    prepare(schema, service_name, &mut schema_index, &mut ts_models)?;
                }
            }
            if let Some(responses) = route.get_mut("responses").and_then(JsonValue::as_array_mut) {
                for response in responses {
                    if let Some(schema) = response.pointer_mut("/body/schema/jsonSchema") {
                        prepare(schema, service_name, &mut schema_index, &mut ts_models)?;
                    }
                }
            }
        }
    }
    let mut routers = BTreeMap::<String, Vec<SdkOperation>>::new();
    for (route_index, route) in contract
        .get("routes")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let method = route
            .get("method")
            .and_then(JsonValue::as_str)
            .unwrap_or("GET")
            .to_ascii_uppercase();
        let path = route
            .get("path")
            .or_else(|| route.get("openapiPath"))
            .and_then(JsonValue::as_str)
            .unwrap_or("/health")
            .to_string();
        let router_name = router_name_from_path(&path);
        let operation_name = route
            .get("operationName")
            .and_then(JsonValue::as_str)
            .filter(|name| !name.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| sdk_operation_name(&method, &path));
        let request_shapes = request_type_shapes_from_contract(route)?;
        let responses = response_contracts(route)?;
        let first_response = responses
            .first()
            .cloned()
            .unwrap_or_else(|| SdkResponseContract::empty(200));
        let mut operation = build_sdk_operation(
            service_name,
            &router_name,
            &operation_name,
            &method,
            &path,
            request_shapes.clone(),
            &first_response,
        );
        if responses.is_empty() {
            operation.responses.clear();
            operation.rust_models.clear();
            operation.rust_aliases.clear();
        }
        for response in responses.iter().skip(1) {
            let mut alternative = build_sdk_operation(
                service_name,
                &router_name,
                &format!("{operation_name}Status{}", response.status),
                &method,
                &path,
                request_shapes.clone(),
                response,
            );
            operation.responses.append(&mut alternative.responses);
            operation.rust_models.append(&mut alternative.rust_models);
            operation.rust_aliases.append(&mut alternative.rust_aliases);
        }
        let runtime_responses = response_contracts(&runtime_contract["routes"][route_index])?;
        for response in &mut operation.responses {
            if !(response.json_response || response.text_response || response.streaming_response) {
                continue;
            }
            let source = runtime_responses
                .iter()
                .find(|original| original.status == response.status_code)
                .and_then(|original| original.schema.as_ref());
            if let Some(source) = source {
                let schema =
                    forklaunch_core::codegen::bundle_document_schema_resources(source, "")?
                        .to_string();
                response.rust_body_schema = format!("{schema:?}");
                response.body_schema = escape_ts_string(&schema);
                response.body_schema_json = schema;
            }
        }
        routers.entry(router_name).or_default().push(operation);
    }

    Ok(SdkService {
        modern: None,
        name: service_name.to_string(),
        ts_models,
        routers: routers
            .into_iter()
            .map(|(name, operations)| SdkRouter { name, operations })
            .collect(),
    })
}

#[derive(Debug, Clone)]
struct RequestTypeShapes {
    kind: String,
    content_type: String,
    params: String,
    query: String,
    body: String,
    body_schema: Option<JsonValue>,
    body_optional: bool,
    headers: String,
}

impl Default for RequestTypeShapes {
    fn default() -> Self {
        Self {
            kind: "json".into(),
            content_type: "application/json".into(),
            params: "never".to_string(),
            query: "never".to_string(),
            body: "never".to_string(),
            body_schema: None,
            body_optional: false,
            headers: "ForkLaunchHeaders".to_string(),
        }
    }
}

fn build_sdk_operation(
    service_name: &str,
    router_name: &str,
    operation_name: &str,
    method: &str,
    path: &str,
    request_shapes: RequestTypeShapes,
    response: &SdkResponseContract,
) -> SdkOperation {
    let type_prefix = format!(
        "{}{}{}",
        service_name.to_case(Case::Pascal),
        router_name.to_case(Case::Pascal),
        operation_name.to_case(Case::Pascal)
    );
    let status_code = response.status;
    let empty_response = method.eq_ignore_ascii_case("HEAD")
        || matches!(status_code, 204 | 205)
        || response.decoder == ResponseDecoder::Empty;
    let decoder = if empty_response {
        ResponseDecoder::Empty
    } else {
        response.decoder
    };
    let fallback_schema = match decoder {
        ResponseDecoder::Empty => json!({"type":"null"}),
        ResponseDecoder::Text | ResponseDecoder::Binary | ResponseDecoder::Multipart => {
            json!({"type":"string"})
        }
        ResponseDecoder::Json | ResponseDecoder::Sse => json!({}),
    };
    let response_schema = if matches!(
        decoder,
        ResponseDecoder::Empty
            | ResponseDecoder::Text
            | ResponseDecoder::Binary
            | ResponseDecoder::Multipart
    ) {
        &fallback_schema
    } else {
        response.schema.as_ref().unwrap_or(&fallback_schema)
    };
    let response_type = format!("{type_prefix}Response");
    let response_type_pascal = rust_type_name(&response_type);
    let (mut rust_models, mut rust_aliases, mut rust_response_type) =
        rust_types_from_schema(&response_type_pascal, response_schema);
    if matches!(
        decoder,
        ResponseDecoder::Binary | ResponseDecoder::Multipart
    ) {
        for alias in &mut rust_aliases {
            if alias.name == rust_response_type {
                alias.target_type = if decoder == ResponseDecoder::Multipart {
                    "MultipartResponse"
                } else {
                    "Vec<u8>"
                }
                .into();
            }
        }
    }

    if decoder == ResponseDecoder::Sse {
        rust_response_type = format!("Box<SdkEventStream<{rust_response_type}>>");
    } else if decoder == ResponseDecoder::Json {
        // Retained legacy artifacts use the same response allocation policy as
        // native SDKs, including unboxed collections of nested records.
        rust_response_type = forklaunch_core::codegen::response::json_response_type(
            &rust_response_type,
            !rust_models.is_empty(),
            rust_aliases
                .iter()
                .map(|alias| (alias.name.as_str(), alias.target_type.as_str())),
        );
    }

    let rust_request_body_type = if request_shapes.kind == "json" {
        request_shapes
            .body_schema
            .as_ref()
            .map(|schema| {
                let (body_models, body_aliases, body_type) =
                    rust_types_from_schema(&format!("{type_prefix}RequestBody"), schema);
                rust_models.extend(body_models);
                rust_aliases.extend(body_aliases);
                body_type
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    SdkOperation {
        name: operation_name.to_string(),
        method: method.to_ascii_uppercase(),
        path: path.to_string(),
        request_type: format!("{type_prefix}Request"),
        responses_type: format!("{type_prefix}Responses"),
        request_params_type: request_shapes.params,
        request_query_type: request_shapes.query,
        request_body_type: request_shapes.body,
        rust_request_body_type,
        request_body_optional: request_shapes.body_optional,
        request_headers_type: request_shapes.headers,
        request_kind: request_shapes.kind,
        request_content_type: request_shapes.content_type,
        responses: vec![SdkResponseVariant {
            shared_response_decoders: false,
            media_discriminator: false,
            headers_type: String::new(),
            typescript_headers_type: String::new(),
            header_fields: Vec::new(),
            header_rules: "{}".into(),
            body_schema: escape_ts_string(&response_schema.to_string()),
            body_schema_json: response_schema.to_string(),
            rust_body_schema: format!("{:?}", response_schema.to_string()),
            rust_header_rules: "\"{}\"".into(),
            response_type,
            response_body_type: match decoder {
                ResponseDecoder::Empty => "undefined".into(),
                ResponseDecoder::Text => "string".into(),
                ResponseDecoder::Binary => "Uint8Array".into(),
                ResponseDecoder::Multipart => "FormData".into(),
                ResponseDecoder::Json => ts_type_from_schema(response_schema),
                ResponseDecoder::Sse => {
                    format!("AsyncIterable<{}>", ts_type_from_schema(response_schema))
                }
            },
            status_code,
            status_variant: format!("Status{status_code}"),
            empty_response,
            json_response: decoder == ResponseDecoder::Json,
            text_response: decoder == ResponseDecoder::Text,
            binary_response: decoder == ResponseDecoder::Binary,
            multipart_response: decoder == ResponseDecoder::Multipart,
            streaming_response: decoder == ResponseDecoder::Sse,
            response_kind: decoder.as_str().into(),
            content_type: if empty_response {
                String::new()
            } else {
                response.content_type.clone()
            },
            rust_response_type,
        }],
        rust_models,
        rust_aliases,
    }
}

fn default_response_json_schema(router_name: &str) -> JsonValue {
    if router_name == "health" {
        json!({
            "type": "object",
            "required": ["service", "status", "recordCount", "routes"],
            "properties": {
                "service": { "type": "string" },
                "status": { "type": "string", "const": "ok" },
                "recordCount": { "type": "integer", "minimum": 0 },
                "routes": { "type": "array", "items": { "type": "string" } }
            }
        })
    } else {
        json!({
            "type": "object",
            "required": ["route", "summary", "records"],
            "properties": {
                "route": { "type": "string", "const": router_name },
                "summary": {
                    "type": "object",
                    "required": ["service", "total"],
                    "properties": {
                        "service": { "type": "string" },
                        "total": { "type": "integer", "minimum": 0 }
                    }
                },
                "records": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["id", "name"],
                        "properties": {
                            "id": { "type": "string" },
                            "name": { "type": "string" }
                        }
                    }
                }
            }
        })
    }
}

fn request_type_shapes_from_contract(route: &JsonValue) -> Result<RequestTypeShapes> {
    let Some(request) = route.get("request") else {
        return Ok(RequestTypeShapes::default());
    };

    let params = fields_ts_type(request.get("params"), false);
    let query = fields_ts_type(request.get("query"), false);
    let headers = fields_ts_type(request.get("headers"), true);
    let contract = request.get("body").filter(|body| !body.is_null());
    let kind = match contract
        .and_then(|body| body.get("kind"))
        .and_then(JsonValue::as_str)
    {
        None | Some("json") => "json",
        Some("text") => "text",
        Some("urlEncoded") => "urlEncoded",
        Some("multipart") => "multipart",
        Some("binary" | "file") => "binary",
        Some(kind) => bail!("Unsupported SDK request body kind {kind}"),
    };
    let content_type = request_media_type(
        contract
            .and_then(|body| body.get("contentType"))
            .and_then(JsonValue::as_str)
            .unwrap_or(match kind {
                "text" => "text/plain",
                "binary" => "application/octet-stream",
                "urlEncoded" => "application/x-www-form-urlencoded",
                "multipart" => "multipart/form-data",
                _ => "application/json",
            }),
    )?;
    let normalized_content_type = normalized_media_type(&content_type)?;
    if normalized_content_type == "text/event-stream" {
        bail!("SSE request bodies are unsupported");
    }
    if normalized_content_type.starts_with("multipart/") != (kind == "multipart")
        || kind == "multipart" && normalized_content_type != "multipart/form-data"
    {
        bail!("Multipart SDK requests require multipart/form-data and the multipart body kind");
    }
    let body = if contract.is_none() {
        "never".into()
    } else {
        match kind {
            "text" => "string".into(),
            "binary" => "Uint8Array | ArrayBuffer | Blob".into(),
            "multipart" => format!(
                "FormData | {}",
                contract
                    .and_then(|body| body.get("schema"))
                    .and_then(|schema| schema.get("jsonSchema"))
                    .map(ts_type_from_schema)
                    .unwrap_or_else(|| "Record<string, unknown>".into())
            ),
            "urlEncoded" => format!(
                "URLSearchParams | {}",
                contract
                    .and_then(|body| body.get("schema"))
                    .and_then(|schema| schema.get("jsonSchema"))
                    .map(ts_type_from_schema)
                    .unwrap_or_else(|| { "Record<string, unknown>".into() })
            ),
            _ => contract
                .and_then(|body| body.get("schema"))
                .and_then(|schema| schema.get("jsonSchema"))
                .map(ts_type_from_schema)
                .unwrap_or_else(|| "unknown".into()),
        }
    };
    let body_schema = contract.map(|body| {
        body.pointer("/schema/jsonSchema")
            .cloned()
            .unwrap_or_else(|| {
                match body
                    .pointer("/schema/name")
                    .and_then(JsonValue::as_str)
                    .map(ts_type_from_schema_name)
                    .as_deref()
                {
                    Some("string") => json!({"type":"string"}),
                    Some("number") => json!({"type":"number"}),
                    Some("boolean") => json!({"type":"boolean"}),
                    _ => json!({}),
                }
            })
    });
    Ok(RequestTypeShapes {
        body_schema,
        body_optional: route
            .pointer("/request/bodyOptional")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
        params,
        query,
        body,
        headers,
        kind: kind.into(),
        content_type,
    })
}

fn fields_ts_type(fields: Option<&JsonValue>, allow_arbitrary: bool) -> String {
    let fields = fields
        .and_then(JsonValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    if fields.is_empty() {
        return if allow_arbitrary {
            "ForkLaunchHeaders".to_string()
        } else {
            "never".to_string()
        };
    }

    let mut parts = Vec::new();
    for field in fields {
        let Some(name) = field.get("name").and_then(JsonValue::as_str) else {
            continue;
        };
        let optional = !field
            .get("required")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        let ty = field
            .get("schema")
            .map(ts_type_from_field_schema)
            .unwrap_or_else(|| "unknown".to_string());
        parts.push(format!(
            "{}{}: {}",
            ts_property_key(name),
            if optional { "?" } else { "" },
            ty
        ));
    }

    if parts.is_empty() {
        if allow_arbitrary {
            "ForkLaunchHeaders".to_string()
        } else {
            "never".to_string()
        }
    } else if allow_arbitrary {
        format!("{{ {} }} & ForkLaunchHeaders", parts.join("; "))
    } else {
        format!("{{ {} }}", parts.join("; "))
    }
}

fn response_contracts(route: &JsonValue) -> Result<Vec<SdkResponseContract>> {
    let mut contracts = BTreeMap::new();
    for response in route
        .get("responses")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
    {
        let status = response
            .get("statusCode")
            .and_then(JsonValue::as_u64)
            .filter(|status| (100..=599).contains(status))
            .context("Invalid SDK response status code")? as u16;
        if status < 200 {
            continue;
        }
        let body = response.get("body").filter(|body| !body.is_null());
        let decoder = match body
            .and_then(|body| body.get("kind"))
            .and_then(JsonValue::as_str)
        {
            _ if body.is_none() => ResponseDecoder::Empty,
            None | Some("json") => ResponseDecoder::Json,
            Some("text") => ResponseDecoder::Text,
            Some("binary" | "file") => ResponseDecoder::Binary,
            Some("serverSentEvent") => ResponseDecoder::Sse,
            Some("multipart") => ResponseDecoder::Multipart,
            Some(kind) => bail!("Unsupported SDK response body kind {kind}"),
        };
        let schema = body
            .and_then(|body| body.get("schema"))
            .and_then(|schema| schema.get("jsonSchema"))
            .cloned();
        let content_type = normalized_media_type(
            body.and_then(|body| body.get("contentType"))
                .and_then(JsonValue::as_str)
                .unwrap_or(match decoder {
                    ResponseDecoder::Empty | ResponseDecoder::Json => "application/json",
                    ResponseDecoder::Text => "text/plain",
                    ResponseDecoder::Binary => "application/octet-stream",
                    ResponseDecoder::Multipart => "multipart/form-data",
                    ResponseDecoder::Sse => "text/event-stream",
                }),
        )?;
        if (content_type == "text/event-stream") != (decoder == ResponseDecoder::Sse) {
            bail!("SSE SDK responses require the serverSentEvent body kind and text/event-stream");
        }
        if (decoder == ResponseDecoder::Multipart && content_type != "multipart/form-data")
            || (content_type.starts_with("multipart/") && decoder != ResponseDecoder::Multipart)
        {
            bail!("Multipart SDK responses require multipart body kind and multipart/form-data");
        }
        if contracts
            .insert(
                status,
                SdkResponseContract {
                    status,
                    schema,
                    decoder,
                    content_type,
                },
            )
            .is_some()
        {
            bail!("Duplicate SDK response status {status}");
        }
    }
    Ok(contracts.into_values().collect())
}

fn router_name_from_path(path: &str) -> String {
    path.trim_matches('/')
        .split('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("health")
        .replace('-', "_")
}

fn sdk_operation_name(method: &str, path: &str) -> String {
    if path.trim_matches('/') == "health" {
        return "get".to_string();
    }

    match method {
        "GET" => "list",
        "POST" => "create",
        "PUT" | "PATCH" => "update",
        "DELETE" => "delete",
        _ => "call",
    }
    .to_string()
}

fn ts_type_from_field_schema(schema: &JsonValue) -> String {
    schema
        .get("jsonSchema")
        .map(ts_type_from_schema)
        .or_else(|| {
            schema
                .get("name")
                .and_then(JsonValue::as_str)
                .map(ts_type_from_schema_name)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Prepare source slots and existing consumer manifests before the first build.
/// No contract artifact is invented: the installed compiler populates real APIs.
pub(crate) fn prepare_contract_consumers(root: &Path, name: &str) -> Result<()> {
    prepare_contract_consumers_with_transports(root, name, false, false)
}

pub(crate) fn prepare_contract_consumers_with_transports(
    root: &Path,
    name: &str,
    request_multipart: bool,
    response_multipart: bool,
) -> Result<()> {
    let empty = json!({"formatVersion":"forklaunch.contract.v1","sdkCompiler":"forklaunch.sdk.http.v1","routes":[]});
    let mut compiled = super::modern::compile(name, &empty)?;
    compiled.request_multipart = request_multipart;
    compiled.response_multipart = response_multipart;
    let service = SdkService {
        name: name.into(),
        modern: Some(compiled),
        ts_models: vec![],
        routers: vec![],
    };
    if super::layout::typescript_authored(root, name)? {
        return generate_rust_sdks(root, std::slice::from_ref(&service));
    }
    let source = super::layout::source(root, &root.join(super::layout::typescript(root)?))?;
    let mut services = Vec::new();
    if source.is_dir() {
        for entry in std::fs::read_dir(&source)? {
            let directory = entry?.path();
            if read_to_string(directory.join(".forklaunch-generated"))
                .ok()
                .as_deref()
                != Some("forklaunch.sdk.http.v1\n")
            {
                continue;
            }
            let existing = directory
                .file_name()
                .and_then(|name| name.to_str())
                .context("SDK source name must be UTF-8")?;
            if existing == name {
                return Ok(());
            }
            services.push(SdkService {
                name: existing.into(),
                modern: Some(super::modern::CompiledSdk {
                    typescript: read_to_string(directory.join("index.ts"))?,
                    rust: String::new(),
                    rust_module: false,
                    typescript_runtime_shared: true,
                    typescript_files: Default::default(),
                    rust_files: Default::default(),
                    request_multipart: false,
                    response_multipart: false,
                }),
                ts_models: vec![],
                routers: vec![],
            });
        }
    }
    generate_rust_sdks(root, std::slice::from_ref(&service))?;
    services.push(service);
    services.sort_by(|left, right| left.name.cmp(&right.name));
    generate_typescript_sdk(root, &services)
}

pub(crate) fn refresh_contract_consumers(root: &Path) -> Result<()> {
    let source = super::layout::source(root, &root.join(super::layout::typescript(root)?))?;
    if !source.is_dir() {
        return Ok(());
    }
    let mut services = Vec::new();
    for entry in std::fs::read_dir(&source)? {
        let directory = entry?.path();
        if read_to_string(directory.join(".forklaunch-generated"))
            .ok()
            .as_deref()
            != Some("forklaunch.sdk.http.v1\n")
        {
            continue;
        }
        let name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .context("Invalid SDK source name")?;
        services.push(SdkService {
            name: name.into(),
            modern: Some(super::modern::CompiledSdk {
                typescript: read_to_string(directory.join("index.ts"))?,
                rust: String::new(),
                rust_module: false,
                typescript_runtime_shared: true,
                typescript_files: Default::default(),
                rust_files: Default::default(),
                request_multipart: false,
                response_multipart: false,
            }),
            ts_models: vec![],
            routers: vec![],
        });
    }
    services.sort_by(|left, right| left.name.cmp(&right.name));
    generate_typescript_sdk(root, &services)
}

fn typescript_service_outputs(
    source: &Path,
    service: &SdkService,
) -> Vec<(std::path::PathBuf, String)> {
    let modern = service.modern.as_ref().expect("modern SDK service");
    if modern.typescript_files.is_empty() {
        return vec![(
            source.join(&service.name).join("index.ts"),
            modern.typescript.clone(),
        )];
    }
    modern
        .typescript_files
        .iter()
        .map(|(name, contents)| {
            let directory = if name == "runtime.ts" && modern.typescript_runtime_shared {
                source.to_path_buf()
            } else {
                source.join(&service.name)
            };
            (directory.join(name), contents.clone())
        })
        .collect()
}

fn rust_service_outputs(
    source: &Path,
    service: &SdkService,
    flat: bool,
) -> Result<Vec<(std::path::PathBuf, String)>> {
    let modern = service.modern.as_ref();
    let facade = if flat {
        source.join(format!("{}.rs", service.name))
    } else {
        source.join("lib.rs")
    };
    let Some(modern) = modern.filter(|sdk| !sdk.rust_files.is_empty()) else {
        return Ok(vec![(facade, rust_sdk(service)?)]);
    };
    if !flat {
        return Ok(modern
            .rust_files
            .iter()
            .map(|(name, content)| (source.join(name), content.clone()))
            .collect());
    }
    let mut outputs = vec![(
        facade.clone(),
        forklaunch_core::build::rust_http_facade(&facade, &modern.rust_files)?,
    )];
    outputs.extend(
        modern
            .rust_files
            .iter()
            .filter(|(name, _)| name.as_str() != "lib.rs")
            .map(|(name, content)| (source.join(&service.name).join(name), content.clone())),
    );
    Ok(outputs)
}

fn write_sdk_outputs(outputs: Vec<(std::path::PathBuf, String)>) -> Result<()> {
    for (path, contents) in outputs {
        super::write_if_changed(path, contents)?;
    }
    Ok(())
}

fn generate_typescript_sdk(app_root: &Path, services: &[SdkService]) -> Result<()> {
    generate_typescript_sdk_scoped(app_root, services, false)
}

fn package_service_names(
    source: &Path,
    services: &[SdkService],
    partial: bool,
) -> Result<Vec<String>> {
    let mut names = services
        .iter()
        .map(|service| service.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if partial && source.is_dir() {
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let marker = entry.path().join(".forklaunch-generated");
            let owned = match read_to_string(&marker) {
                Ok(value) => value == "forklaunch.sdk.http.v1\n",
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.into()),
            };
            if owned {
                names.insert(
                    entry
                        .file_name()
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("Invalid SDK service name"))?,
                );
            }
        }
    }
    Ok(names.into_iter().collect())
}

/// The generated application SDK package re-exports each service-local SDK's
/// client type when every service owns its SDK. Applications that maintain their
/// own TypeScript client SDK are left alone.
/// A partial pass keeps every service-local SDK already on disk in the aggregate.
fn local_typescript_aggregate(
    app_root: &Path,
    services: &[SdkService],
    partial: bool,
) -> Result<()> {
    let sdk_root = app_root.join(super::layout::typescript(app_root)?);
    let manifest_path = sdk_root.join("package.json");
    let Ok(source) = read_to_string(&manifest_path) else { return Ok(()) };
    let mut package: serde_json::Value = serde_json::from_str(&source)?;
    if package.get("name").and_then(serde_json::Value::as_str) != Some("@forklaunch-generated/sdk") {
        return Ok(());
    }
    let mut names = services
        .iter()
        .map(|service| service.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if partial {
        let manifest = read_manifest(app_root)?;
        for project in super::http_sources::projects(&manifest, None) {
            let Some(name) = project.get("name").and_then(Value::as_str) else { continue };
            let Some(local) = super::layout::local_sdk(app_root, name, "typescript")? else { continue };
            if local.join("package.json").is_file() {
                names.insert(name.to_owned());
            }
        }
    }
    let mut packages = std::collections::BTreeMap::new();
    for service in &names {
        let Some(local) = super::layout::local_sdk(app_root, service, "typescript")? else { continue };
        let local: serde_json::Value = serde_json::from_str(&read_to_string(local.join("package.json"))?)?;
        let name = local.get("name").and_then(serde_json::Value::as_str).context("Service SDK package name")?;
        packages.insert(service.clone(), name.to_owned());
    }
    let mut index = String::new();
    let mut fields = String::new();
    let mut factories = String::new();
    for (service, package_name) in &packages {
        let client = format!("{}SdkClient", service.to_case(Case::Pascal));
        let camel = service.to_case(Case::Camel);
        index.push_str(&format!("export type {{ {client} }} from '{package_name}';\n"));
        fields.push_str(&format!("  {camel}: import('{package_name}').{client};\n"));
        factories.push_str(&format!(
            "/** Connect to `{service}`. Internal routes need `hmac: {{ secretKey }}` with its HMAC key. */\nexport const {camel}SdkClient = (options: ServiceClientOptions) =>\n  connectService<ForkLaunchSdk['{camel}']>(options);\n"
        ));
    }
    index.push_str(&if fields.is_empty() {
        "\nexport type ForkLaunchSdk = Record<string, never>;\n".to_owned()
    } else {
        format!("\nexport type ForkLaunchSdk = {{\n{fields}}};\n")
    });
    if !factories.is_empty() {
        index.insert_str(0, "import { universalSdk, type UniversalSdkOptions } from '@forklaunch/universal-sdk';\n\n");
        index.push_str(&format!(r#"
/** One service's base URL, its registry (defaults to the native `/openapi.json`),
 * and transport options. `hmac: {{ secretKey }}` signs every request for internal routes. */
export type ServiceClientOptions = Omit<UniversalSdkOptions, 'host' | 'registryOptions'> & {{
  host: string;
  registryOptions?: UniversalSdkOptions['registryOptions'];
}};

function connectService<T>({{ host, registryOptions, ...options }}: ServiceClientOptions): Promise<T> {{
  const base = host.replace(/\/+$/, '');
  return universalSdk<T>({{ ...options, host: base, registryOptions: registryOptions ?? {{ url: `${{base}}/openapi.json` }} }});
}}

{factories}
const factories = {{
{}
}} as {{ [K in keyof ForkLaunchSdk]: (options: ServiceClientOptions) => Promise<ForkLaunchSdk[K]> }};

/** Connect several services at once: `createForkLaunchSdk({{ billing: {{ host, hmac }} }})`. */
export async function createForkLaunchSdk<const S extends keyof ForkLaunchSdk>(
  services: {{ [K in S]: ServiceClientOptions }}
): Promise<Pick<ForkLaunchSdk, S>> {{
  const entries = await Promise.all(
    (Object.keys(services) as S[]).map(async (name) => [name, await factories[name](services[name])] as const)
  );
  return Object.fromEntries(entries) as Pick<ForkLaunchSdk, S>;
}}
"#, packages.keys().map(|service| {{ let camel = service.to_case(Case::Camel); format!("  {camel}: {camel}SdkClient,") }}).collect::<Vec<_>>().join("\n")));
    }
    let source_dir = super::layout::source(app_root, &sdk_root)?;
    super::write_if_changed(source_dir.join("index.ts"), index)?;
    let dependencies = package
        .as_object_mut()
        .context("SDK package manifest")?
        .entry("dependencies")
        .or_insert_with(|| json!({}));
    for package_name in packages.values() {
        dependencies[package_name] = json!("workspace:*");
    }
    if !packages.is_empty() && dependencies.get("@forklaunch/universal-sdk").is_none() {
        // The runtime factories need the shared SDK runtime.
        dependencies["@forklaunch/universal-sdk"] =
            json!(crate::core::package_json::package_json_constants::UNIVERSAL_SDK_VERSION);
    }
    let updated = serde_json::to_string_pretty(&package)? + "\n";
    if updated.trim_end() != source.trim_end() {
        super::write_if_changed(&manifest_path, updated)?;
    }
    Ok(())
}

fn generate_typescript_sdk_scoped(
    app_root: &Path,
    services: &[SdkService],
    partial: bool,
) -> Result<()> {
    let mut shared = Vec::new();
    for service in services {
        if let Some(local) = super::layout::local_sdk(app_root, &service.name, "typescript")? {
            if super::layout::typescript_authored(app_root, &service.name)? {
                continue;
            }
            let modern = service
                .modern
                .as_ref()
                .context("Service-local SDK requires a current compiled contract")?;
            for (name, contents) in &modern.typescript_files {
                super::write_if_changed(local.join(name), contents)?;
            }
        } else {
            shared.push(service.clone());
        }
    }
    if shared.len() != services.len() {
        // Local type exports still participate in the application's package
        // workspace. Repair that membership before returning from the local path.
        super::workspace::include_typescript_sdk(app_root)?;
        if shared.is_empty() {
            return local_typescript_aggregate(app_root, services, partial);
        }
        return generate_typescript_sdk_scoped(app_root, &shared, partial);
    }
    let sdk_root = app_root.join(super::layout::typescript(app_root)?);
    let source = super::layout::source(app_root, &sdk_root)?;
    create_dir_all(&source)?;
    let compiler_config = sdk_root.join("tsconfig.json");
    if !compiler_config.exists() {
        let config = if super::layout::native(app_root)? {
            rust_project::TYPESCRIPT_SDK_TSCONFIG.replace("src/**/*.ts", "**/*.ts")
        } else {
            rust_project::TYPESCRIPT_SDK_TSCONFIG.to_owned()
        };
        super::write_if_changed(&compiler_config, config)?;
    }
    if super::layout::native(app_root)? && services.iter().all(|service| service.modern.is_some()) {
        for service in services {
            let directory = source.join(&service.name);
            create_dir_all(&directory)?;
            write_sdk_outputs(typescript_service_outputs(&source, service))?;
            super::write_if_changed(
                directory.join(".forklaunch-generated"),
                "forklaunch.sdk.http.v1\n",
            )?;
        }
        let names = package_service_names(&source, services, partial)?;
        super::service_package::update(&source, &names, false)?;
        return super::workspace::include_typescript_sdk(app_root);
    }

    super::write_if_changed(
        sdk_root.join("package.json"),
        serde_json::to_string_pretty(&json!({
            "name": "@forklaunch-generated/sdk",
            "version": "0.0.0",
            "private": true,
            "type": "module",
            "exports": {
                ".": if super::layout::native(app_root)? { "./index.ts" } else { "./src/index.ts" }
            },
            "scripts": {
                "build": "tsc --noEmit"
            },
            "dependencies": {"ajv":"^8.20.0","ajv-draft-04":"^1.0.0","ajv-formats":"^3.0.1","qs":"^6.16.0","@types/qs":"^6.15.1"},
            "devDependencies": {
                "typescript": "^5.0.0"
            }
        }))?,
    )?;
    for service in services {
        if service.modern.is_some() {
            let directory = source.join(&service.name);
            create_dir_all(&directory)?;
            write_sdk_outputs(typescript_service_outputs(&source, service))?;
            super::write_if_changed(
                directory.join(".forklaunch-generated"),
                "forklaunch.sdk.http.v1\n",
            )?;
        }
    }
    super::write_if_changed(source.join("index.ts"), typescript_sdk(services)?)?;
    super::http_sources::record(&source, services.iter().map(|service| service.name.as_str()))?;
    // Bun requires literal members to exist. Publish the actual SDK package
    // before recording its workspace membership.
    super::workspace::include_typescript_sdk(app_root)
}

// Only remove owned per-service output. Adjacent user-maintained SDK files remain intact.
pub(crate) fn cleanup_service_generated_files(root: &Path, name: &str) -> Result<()> {
    let compiled = root
        .join(super::sdk_base(root)?)
        .join("typescript")
        .join(name);
    let owns_compiled = compiled.join("contract.json").is_file();
    let explicit = root.join(super::layout::typescript(root)?);
    let explicit = super::layout::source(root, &explicit)?.join(name);
    if read_to_string(explicit.join(".forklaunch-generated"))
        .ok()
        .as_deref()
        == Some("forklaunch.sdk.http.v1\n")
    {
        for file in [
            explicit.join("index.ts"),
            explicit.join("models.ts"),
            explicit.join("client.ts"),
            explicit.join("runtime.ts"),
            explicit.join(".forklaunch-generated"),
        ] {
            if file.is_file() {
                std::fs::remove_file(file)?;
            }
        }
        if std::fs::read_dir(&explicit)?.next().is_none() {
            std::fs::remove_dir(&explicit)?;
        }
    }
    if owns_compiled {
        if compiled.exists() {
            std::fs::remove_dir_all(&compiled)?;
        }
        let module = root
            .join(super::sdk_base(root)?)
            .join("rust")
            .join(format!("{name}.rs"));
        if module.is_file() {
            std::fs::remove_file(module)?;
        }
        let children = root.join(super::sdk_base(root)?).join("rust").join(name);
        for child in ["models.rs", "client.rs", "schemas.rs"] {
            let file = children.join(child);
            if file.is_file() {
                std::fs::remove_file(file)?;
            }
        }
        if children.is_dir() && std::fs::read_dir(&children)?.next().is_none() {
            std::fs::remove_dir(children)?;
        }
    }
    let rust_relative = super::layout::rust(root)?.join(format!("{name}-sdk"));
    let rust_directory = root.join(&rust_relative);
    if read_to_string(rust_directory.join(".forklaunch-generated"))
        .ok()
        .as_deref()
        == Some("forklaunch.sdk.http.v1\n")
    {
        let rust_source = super::layout::source(root, &rust_directory)?;
        for file in [
            rust_source.join("lib.rs"),
            rust_source.join("models.rs"),
            rust_source.join("client.rs"),
            rust_source.join("schemas.rs"),
            rust_directory.join("Cargo.toml"),
            rust_directory.join(".forklaunch-generated"),
        ] {
            if file.is_file() {
                std::fs::remove_file(file)?;
            }
        }
        // Leave unrelated application-owned files in place.
        if rust_source.is_dir() && std::fs::read_dir(&rust_source)?.next().is_none() {
            std::fs::remove_dir(&rust_source)?;
        }
        if rust_directory.is_dir() && std::fs::read_dir(&rust_directory)?.next().is_none() {
            std::fs::remove_dir(&rust_directory)?;
        }
        let workspace_path = root.join("Cargo.toml");
        let mut workspace: toml::Value = toml::from_str(&read_to_string(&workspace_path)?)?;
        if let Some(members) = workspace
            .get_mut("workspace")
            .and_then(|workspace| workspace.get_mut("members"))
            .and_then(toml::Value::as_array_mut)
        {
            members.retain(|member| member.as_str() != rust_relative.to_str());
            super::write_if_changed(workspace_path, toml::to_string_pretty(&workspace)?)?;
        }
    }
    for directory in [compiled.parent(), explicit.parent()].into_iter().flatten() {
        if directory.join(".forklaunch-sdk-package.json").is_file() {
            super::service_package::refresh(directory)?;
        }
    }
    Ok(())
}

fn check_sdk_outputs(app_root: &Path, target: &str, services: &[SdkService]) -> Result<()> {
    check_sdk_outputs_scoped(app_root, target, services, false)
}

fn check_sdk_outputs_scoped(
    app_root: &Path,
    target: &str,
    services: &[SdkService],
    partial: bool,
) -> Result<()> {
    let mut stale_outputs = Vec::new();
    let mut shared = Vec::new();
    for service in services {
        if let Some(local) = super::layout::local_sdk(app_root, &service.name, "typescript")? {
            let modern = service
                .modern
                .as_ref()
                .context("Service-local SDK requires a current compiled contract")?;
            if matches!(target, "typescript" | "all")
                && !super::layout::typescript_authored(app_root, &service.name)?
            {
                for (name, content) in &modern.typescript_files {
                    compare_generated_file(&local.join(name), content, &mut stale_outputs)?;
                }
            }
            if matches!(target, "rust" | "all") {
                for (path, content) in local_rust_outputs(app_root, service)? {
                    compare_generated_file(&path, &content, &mut stale_outputs)?;
                }
            }
        } else {
            shared.push(service.clone());
        }
    }
    if shared.len() != services.len() {
        if matches!(target, "typescript" | "all") {
            for (path, contents) in super::workspace::plan_typescript_sdk(
                app_root,
                super::workspace::SdkWorkspace::Http,
            )? {
                compare_generated_file(&app_root.join(path), &contents, &mut stale_outputs)?;
            }
        }
        anyhow::ensure!(
            stale_outputs.is_empty(),
            "SDK outputs are stale or missing: {}",
            stale_outputs.join(", ")
        );
        if shared.is_empty() {
            return Ok(());
        }
        return check_sdk_outputs_scoped(app_root, target, &shared, partial);
    }

    if matches!(target, "typescript" | "all")
        && super::layout::native(app_root)?
        && services.iter().all(|service| service.modern.is_some())
    {
        let source = super::layout::source(
            app_root,
            &app_root.join(super::layout::typescript(app_root)?),
        )?;
        let names = package_service_names(&source, services, partial)?;
        super::service_package::update(&source, &names, true)?;
    } else if matches!(target, "typescript" | "all") {
        compare_generated_file(
            &super::layout::source(
                app_root,
                &app_root.join(super::layout::typescript(app_root)?),
            )?
            .join("index.ts"),
            &typescript_sdk(services)?,
            &mut stale_outputs,
        )?;
    }

    if matches!(target, "typescript" | "all") {
        let source = super::layout::source(
            app_root,
            &app_root.join(super::layout::typescript(app_root)?),
        )?;
        for service in services {
            if service.modern.is_some() {
                for (path, content) in typescript_service_outputs(&source, service) {
                    compare_generated_file(&path, &content, &mut stale_outputs)?;
                }
            }
        }
    }

    if matches!(target, "rust" | "all") {
        validate_rust_output_layout(app_root, services)?;
        for service in services {
            if super::layout::native(app_root)?
                && service.modern.as_ref().is_some_and(|sdk| sdk.rust_module)
            {
                for (path, content) in rust_service_outputs(
                    &app_root.join(super::layout::rust(app_root)?),
                    service,
                    true,
                )? {
                    compare_generated_file(&path, &content, &mut stale_outputs)?;
                }
                continue;
            }
            let package_name = format!("{}-sdk", service.name);
            let crate_dir = app_root
                .join(super::layout::rust(app_root)?)
                .join(&package_name);
            compare_generated_file(
                &crate_dir.join("Cargo.toml"),
                &rust_sdk_cargo_for_service(app_root, &package_name, service)?,
                &mut stale_outputs,
            )?;
            for (path, content) in rust_service_outputs(
                &super::layout::source(app_root, &crate_dir)?,
                service,
                false,
            )? {
                compare_generated_file(&path, &content, &mut stale_outputs)?;
            }
        }
    }

    if stale_outputs.is_empty() {
        return Ok(());
    }

    bail!(
        "SDK outputs are stale or missing:\n  - {}\nRun `forklaunch sdk generate --target {target}` to regenerate them.",
        stale_outputs.join("\n  - ")
    )
}

fn compare_generated_file(
    path: &Path,
    expected: &str,
    stale_outputs: &mut Vec<String>,
) -> Result<()> {
    match read_to_string(path) {
        Ok(actual)
            if actual == expected
                || (path
                    .extension()
                    .is_some_and(|extension| extension == "json")
                    && serde_json::from_str::<serde_json::Value>(&actual)
                        .ok()
                        .zip(serde_json::from_str::<serde_json::Value>(expected).ok())
                        .is_some_and(|(actual, expected)| actual == expected))
                // The socket workspace planner preserves authored comments and
                // quoting. HTTP freshness must compare the same membership and
                // policies, without requiring that planner to rewrite the file.
                || (path.file_name().is_some_and(|name| name == "pnpm-workspace.yaml")
                    && serde_yml::from_str::<serde_yml::Value>(&actual)
                        .ok()
                        .zip(serde_yml::from_str::<serde_yml::Value>(expected).ok())
                        .is_some_and(|(actual, expected)| actual == expected)) =>
        {
            Ok(())
        }
        Ok(_) => {
            stale_outputs.push(path.display().to_string());
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            stale_outputs.push(path.display().to_string());
            Ok(())
        }
        Err(error) => Err(error).with_context(|| format!("Failed to read {}", path.display())),
    }
}

fn typescript_sdk(services: &[SdkService]) -> Result<String> {
    if services.iter().any(|service| service.modern.is_some()) {
        let legacy = services
            .iter()
            .filter(|service| service.modern.is_none())
            .cloned()
            .collect::<Vec<_>>();
        let mut source = typescript_sdk(&legacy)?.replace(
            "export function createForkLaunchSdk(",
            "function createLegacyForkLaunchSdk(",
        );
        let mut factories = Vec::new();
        for (index, service) in services
            .iter()
            .filter(|service| service.modern.is_some())
            .enumerate()
        {
            let name = service.name.to_case(Case::Pascal);
            source.push_str(&format!("\nimport {{createForkLaunchSdk as createModernSdk{index}}} from './{}/index.js';\nexport * as {name}Sdk from './{}/index.js';\n",service.name,service.name));
            factories.push(format!("...createModernSdk{index}(config)"));
        }
        source.push_str(&format!("\nexport function createForkLaunchSdk(config: ForkLaunchSdkConfig = {{}}) {{ return {{...createLegacyForkLaunchSdk(config), {}}}; }}\n",factories.join(", ")));
        return Ok(source);
    }

    let mut type_names = BTreeSet::new();
    for model in services.iter().flat_map(|service| &service.ts_models) {
        if !type_names.insert(model.name.clone()) {
            bail!("Duplicate generated TypeScript SDK type {}", model.name);
        }
    }
    let mut operation_types = Vec::new();
    let mut service_blocks = Vec::new();
    let mut aliases = Vec::new();

    for (service_index, service) in services.iter().enumerate() {
        let service_key = service.name.to_case(Case::Camel);
        let alias = format!("{}Service", service_key);
        let mut routers = Vec::new();
        for (router_index, router) in service.routers.iter().enumerate() {
            let router_key = router.name.to_case(Case::Camel);
            let mut operations = Vec::new();
            for (operation_index, operation) in router.operations.iter().enumerate() {
                for name in [&operation.request_type, &operation.responses_type]
                    .into_iter()
                    .chain(
                        operation
                            .responses
                            .iter()
                            .map(|response| &response.response_type),
                    )
                {
                    if !type_names.insert(name.clone()) {
                        bail!("Duplicate generated TypeScript SDK type {name}");
                    }
                }
                operation_types.push(TypeScriptOperationTypeTemplate {
                    request_type: operation.request_type.clone(),
                    responses_type: operation.responses_type.clone(),
                    request_params_type: operation.request_params_type.clone(),
                    request_query_type: operation.request_query_type.clone(),
                    request_body_type: operation.request_body_type.clone(),
                    request_body_optional: operation.request_body_optional,
                    request_headers_type: operation.request_headers_type.clone(),
                    responses: operation.responses.clone(),
                });
                operations.push(TypeScriptOperationTemplate {
                    request_objects: String::new(),
                    version_metadata: String::new(),
                    request_schema: "true".into(),
                    operation_key: operation.name.to_case(Case::Camel),
                    request_type: operation.request_type.clone(),
                    responses_type: operation.responses_type.clone(),
                    optional_input: operation.request_params_type == "never"
                        && operation.request_query_type == "never"
                        && (operation.request_body_type == "never"
                            || operation.request_body_optional),
                    service_name: escape_ts_string(&service.name),
                    router_name: escape_ts_string(&router.name),
                    operation_name: escape_ts_string(&operation.name),
                    method: operation.method.to_string(),
                    path: escape_ts_string(&operation.path),
                    request_kind: operation.request_kind.clone(),
                    request_content_type: escape_ts_string(&operation.request_content_type),
                    responses: operation.responses.clone(),
                    last: operation_index + 1 == router.operations.len(),
                });
            }
            routers.push(TypeScriptRouterTemplate {
                router_key,
                operations,
                last: router_index + 1 == service.routers.len(),
            });
        }
        service_blocks.push(TypeScriptServiceTemplate {
            service_key: service_key.clone(),
            routers,
            last: service_index + 1 == services.len(),
        });
        aliases.push(TypeScriptAliasTemplate { alias, service_key });
    }

    render_sdk_template(
        "sdk/typescript/index.ts",
        &TypeScriptSdkTemplate {
            schema_models: services
                .iter()
                .flat_map(|service| service.ts_models.clone())
                .collect(),
            sse_source: TEMPLATES_DIR
                .get_file("sdk/typescript/sse.ts")
                .context("Missing SSE SDK helper")?
                .contents_utf8()
                .context("Invalid SSE SDK helper")?
                .to_owned(),
            transport_source: format!(
                "{}\n{}",
                super::schema_runtime::typescript_schema_runtime_source(),
                TEMPLATES_DIR
                    .get_file("sdk/typescript/transport.ts")
                    .context("Missing SDK transport helper")?
                    .contents_utf8()
                    .context("Invalid SDK transport helper")?
            ),
            form_stringify_source: TEMPLATES_DIR
                .get_file("sdk/typescript/safeStringify.ts")
                .context("Missing SDK form stringifier")?
                .contents_utf8()
                .context("Invalid SDK form stringifier")?
                .to_owned(),
            operation_types,
            services: service_blocks,
            aliases,
        },
    )
}

fn validate_rust_output_layout(root: &Path, services: &[SdkService]) -> Result<()> {
    if super::layout::native(root)? {
        for service in services {
            if service.modern.as_ref().is_some_and(|sdk| !sdk.rust_module)
                && root
                    .join(super::layout::rust(root)?)
                    .join(format!("{}.rs", service.name))
                    .exists()
            {
                bail!(
                    "Existing Rust service module {} has no trustworthy compiler layout/client metadata. Rebuild that service to refresh its contract before SDK generation/check; the existing module was preserved.",
                    service.name
                );
            }
        }
    }
    Ok(())
}

fn local_rust_outputs(
    root: &Path,
    service: &SdkService,
) -> Result<Vec<(std::path::PathBuf, String)>> {
    let local =
        super::layout::local_sdk(root, &service.name, "rust")?.context("Missing local Rust SDK")?;
    let modern = service
        .modern
        .as_ref()
        .context("Service-local SDK requires compiled contract")?;
    if super::layout::typescript_authored(root, &service.name)? {
        return rust_service_outputs(&local, service, false);
    }
    let facade = local.join("generated.rs");
    let mut files = vec![(
        facade.clone(),
        forklaunch_core::build::rust_http_facade(&facade, &modern.rust_files)?,
    )];
    files.extend(
        modern
            .rust_files
            .iter()
            .filter(|(name, _)| name.as_str() != "lib.rs")
            .map(|(name, content)| (local.join("generated").join(name), content.clone())),
    );
    Ok(files)
}

fn ensure_rust_sdk_workspace(app_root: &Path) -> Result<()> {
    // TypeScript-only applications need a Cargo workspace for their generated
    // Rust clients, without changing the application's language or Docker setup.
    let workspace =
        crate::core::application_output::output_path(app_root, Path::new("Cargo.toml"))?;
    if !workspace.exists() {
        let manifest = read_manifest(app_root)?;
        let license = manifest
            .get("license")
            .and_then(Value::as_str)
            .unwrap_or("MIT");
        let authors = manifest
            .get("author")
            .and_then(Value::as_str)
            .filter(|author| !author.is_empty())
            .into_iter()
            .collect::<Vec<_>>();
        let cargo = json!({"workspace": {
            "resolver": "3", "members": [],
            "package": {"version": "0.1.0", "edition": "2024", "license": license, "authors": authors}
        }});
        super::write_if_changed(workspace, toml::to_string_pretty(&cargo)?)?;
    }
    crate::init::rust_project::ensure_embedded_framework(app_root, false)?;
    Ok(())
}

fn generate_rust_sdks(app_root: &Path, services: &[SdkService]) -> Result<()> {
    if services.is_empty() {
        return Ok(());
    }
    let mut shared = Vec::new();
    for service in services {
        if let Some(local) = super::layout::local_sdk(app_root, &service.name, "rust")? {
            if super::layout::typescript_authored(app_root, &service.name)? {
                ensure_rust_sdk_workspace(app_root)?;
                super::write_if_changed(
                    local.join("Cargo.toml"),
                    rust_sdk_cargo_for_service(
                        app_root,
                        &format!("{}-sdk", service.name),
                        service,
                    )?,
                )?;
                add_rust_workspace_member(
                    app_root,
                    &local
                        .strip_prefix(app_root.canonicalize()?)?
                        .to_string_lossy(),
                )?;
                super::write_if_changed(
                    local.join(".forklaunch-generated"),
                    "forklaunch.sdk.http.v1\n",
                )?;
            }
            write_sdk_outputs(local_rust_outputs(app_root, service)?)?;
        } else {
            shared.push(service.clone());
        }
    }
    if shared.len() != services.len() {
        // The generated `clients` aggregate re-exports every service-local SDK.
        crate::init::rust_project::reconcile_rust_client_aggregate(app_root)?;
        return generate_rust_sdks(app_root, &shared);
    }

    validate_rust_output_layout(app_root, services)?;
    if super::layout::native(app_root)?
        && services
            .iter()
            .all(|service| service.modern.as_ref().is_some_and(|sdk| sdk.rust_module))
    {
        for service in services {
            write_sdk_outputs(rust_service_outputs(
                &app_root.join(super::layout::rust(app_root)?),
                service,
                true,
            )?)?;
        }
        return Ok(());
    }
    ensure_rust_sdk_workspace(app_root)?;
    for service in services {
        if super::layout::native(app_root)?
            && service.modern.as_ref().is_some_and(|sdk| sdk.rust_module)
        {
            write_sdk_outputs(rust_service_outputs(
                &app_root.join(super::layout::rust(app_root)?),
                service,
                true,
            )?)?;
            continue;
        }
        let package_name = format!("{}-sdk", service.name);
        let crate_dir = app_root
            .join(super::layout::rust(app_root)?)
            .join(&package_name);
        let source = super::layout::source(app_root, &crate_dir)?;
        create_dir_all(&source)?;
        super::write_if_changed(
            crate_dir.join("Cargo.toml"),
            rust_sdk_cargo_for_service(app_root, &package_name, service)?,
        )?;
        write_sdk_outputs(rust_service_outputs(&source, service, false)?)?;
        super::write_if_changed(
            crate_dir.join(".forklaunch-generated"),
            "forklaunch.sdk.http.v1\n",
        )?;
        add_rust_workspace_member(
            app_root,
            &super::layout::rust(app_root)?
                .join(&package_name)
                .to_string_lossy(),
        )?;
    }
    crate::init::rust_project::ensure_embedded_framework(app_root, false)?;
    Ok(())
}

fn service_has_multipart(service: &SdkService) -> bool {
    if let Some(modern) = &service.modern {
        return modern.request_multipart;
    }
    service.routers.iter().any(|router| {
        router
            .operations
            .iter()
            .any(|operation| operation.request_kind == "multipart")
    })
}

fn service_has_multipart_response(service: &SdkService) -> bool {
    if let Some(modern) = &service.modern {
        return modern.response_multipart;
    }
    service.routers.iter().any(|router| {
        router.operations.iter().any(|op| {
            op.responses
                .iter()
                .any(|response| response.multipart_response)
        })
    })
}

fn rust_sdk_cargo_for_service(root: &Path, package: &str, service: &SdkService) -> Result<String> {
    // Modern clients delegate response decoding to the shared runtime and support
    // every request transport without changing their manifest on contract edits.
    let (request, response) = if service.modern.is_some() {
        (true, false)
    } else {
        (
            service_has_multipart(service),
            service_has_multipart_response(service),
        )
    };
    rust_sdk_cargo_for_layout(root, package, &service.name, request, response)
}

fn rust_sdk_cargo_for_layout(
    root: &Path,
    package: &str,
    service: &str,
    multipart: bool,
    multipart_response: bool,
) -> Result<String> {
    let mut cargo = rust_sdk_cargo_toml(package, service, multipart, multipart_response);
    if super::layout::native(root)? || super::layout::local_sdk(root, service, "rust")?.is_some() {
        // Native acceptance workspaces need not declare package authors.
        cargo = cargo.replace("authors.workspace = true\n", "");
        cargo.push_str("\n[lib]\npath = \"lib.rs\"\n");
    }
    let directory = super::layout::local_sdk(root, service, "rust")?
        .unwrap_or(root.join(super::layout::rust(root)?).join(package));
    let framework = root.join(".forklaunch/framework-rs");
    // `diff_paths` gives no answer for one relative and one absolute path.
    let (framework, directory) = (std::path::absolute(&framework)?, std::path::absolute(&directory)?);
    let relative = pathdiff::diff_paths(framework.join("crates/core"), &directory)
        .context("SDK framework dependency path")?;
    let mut document = cargo.parse::<toml_edit::DocumentMut>()?;
    document["dependencies"]["forklaunch-core"]["path"] =
        toml_edit::value(relative.to_string_lossy().as_ref());
    document["dependencies"]["forklaunch-core"]["features"] =
        toml_edit::value(toml_edit::Array::from_iter(["schema-validation"]));
    let runtime = pathdiff::diff_paths(framework.join("crates/universal-sdk"), &directory)
        .context("SDK runtime dependency path")?;
    document["dependencies"]["forklaunch-universal-sdk"]["path"] =
        toml_edit::value(runtime.to_string_lossy().as_ref());
    crate::init::rust_project::dependencies::render_for_path(
        &directory.join("Cargo.toml"),
        &document.to_string(),
    )
}

fn rust_sdk_cargo_toml(
    package_name: &str,
    service_name: &str,
    multipart: bool,
    multipart_response: bool,
) -> String {
    let response_dependencies = if multipart_response {
        "multer = {}\nfutures-util = {}\n"
    } else {
        ""
    };
    let multipart_feature = if multipart { ", \"multipart\"" } else { "" };
    format!(
        "[package]\nname = \"{}\"\nversion.workspace = true\nedition.workspace = true\nlicense.workspace = true\nauthors.workspace = true\ndescription = \"Generated Rust SDK for {}\"\n\n[dependencies]\nreqwest = {{ features = [\"json\", \"query\"{}] }}\nserde = {{ features = [\"derive\"] }}\nserde_json = {{}}\n{}",
        package_name, service_name, multipart_feature, response_dependencies
    )
}

fn rust_sdk(service: &SdkService) -> Result<String> {
    if let Some(modern) = &service.modern {
        return Ok(modern.rust.clone());
    }
    let has_multipart = service_has_multipart(service);
    let has_sse = service.routers.iter().any(|router| {
        router.operations.iter().any(|operation| {
            operation
                .responses
                .iter()
                .any(|response| response.streaming_response)
        })
    });
    let client_name = format!("{}Client", service.name.to_case(Case::Pascal));
    let mut operations = Vec::new();
    let mut models = BTreeMap::<String, RustModelTemplate>::new();
    let mut aliases = BTreeMap::<String, RustTypeAliasTemplate>::new();
    let mut operation_type_names = BTreeSet::new();
    let mut method_names = BTreeSet::new();
    for router in &service.routers {
        for operation in &router.operations {
            for name in [&operation.request_type, &operation.responses_type] {
                let name = name.to_case(Case::Pascal);
                if !operation_type_names.insert(name.clone()) {
                    bail!("Duplicate generated Rust SDK type {name}");
                }
            }
            for model in &operation.rust_models {
                if models
                    .get(&model.name)
                    .is_some_and(|existing| existing != model)
                {
                    bail!("Conflicting generated Rust SDK model {}", model.name);
                }
                models
                    .entry(model.name.clone())
                    .or_insert_with(|| model.clone());
            }
            for alias in &operation.rust_aliases {
                if aliases
                    .get(&alias.name)
                    .is_some_and(|existing| existing != alias)
                {
                    bail!("Conflicting generated Rust SDK alias {}", alias.name);
                }
                aliases
                    .entry(alias.name.clone())
                    .or_insert_with(|| alias.clone());
            }
            let method_name = format!(
                "{}_{}_request",
                router.name.to_case(Case::Snake),
                operation.name.to_case(Case::Snake)
            );
            let execute_method_name = format!(
                "{}_{}",
                router.name.to_case(Case::Snake),
                operation.name.to_case(Case::Snake)
            );
            if !method_names.insert(method_name.clone())
                || !method_names.insert(execute_method_name.clone())
            {
                bail!("Duplicate generated Rust SDK method {execute_method_name}");
            }
            if !operation.rust_request_body_type.is_empty() {
                for name in [
                    format!("{execute_method_name}_raw"),
                    format!("{execute_method_name}_raw_request"),
                ] {
                    if !method_names.insert(name.clone()) {
                        bail!("Duplicate generated Rust SDK method {name}");
                    }
                }
            }
            operations.push(RustOperationTemplate {
                has_body: operation.request_kind != "empty",
                no_input: false,
                shared_response_decoders: false,
                request_objects: String::new(),
                version_metadata: String::new(),
                params_type: String::new(),
                query_type: String::new(),
                headers_type: String::new(),
                typed_fields: false,
                json_request_type: operation.rust_request_body_type.clone(),
                form_request: false,
                request_type: operation.request_type.to_case(Case::Pascal),
                responses_type: operation.responses_type.to_case(Case::Pascal),
                responses: operation.responses.clone(),
                execute_method_name,
                method_name,
                http_method: operation.method.to_string(),
                path: escape_rust_string(&operation.path),
                binary_request: operation.request_kind == "binary",
                multipart_request: operation.request_kind == "multipart",
                streaming_response: operation
                    .responses
                    .iter()
                    .any(|response| response.streaming_response),
                multipart_field: if has_multipart {
                    format!(
                        "multipart: {},",
                        if operation.request_kind == "multipart" {
                            "input.body"
                        } else {
                            "None"
                        }
                    )
                } else {
                    String::new()
                },
                request_content_type: escape_rust_string(&operation.request_content_type),
            });
        }
    }
    for name in models.keys().chain(aliases.keys()) {
        if operation_type_names.contains(name)
            || models.contains_key(name) && aliases.contains_key(name)
        {
            bail!("Conflicting generated Rust SDK type {name}");
        }
    }
    render_sdk_template(
        "sdk/rust/lib.rs",
        &RustSdkTemplate {
            model_source: render_rust_models(
                &models.values().cloned().collect::<Vec<_>>(),
                &aliases.values().cloned().collect::<Vec<_>>(),
            )?,
            has_operations: !operations.is_empty(),
            has_multipart,
            has_multipart_response: service_has_multipart_response(service),
            has_sse,
            has_serde: !models.is_empty() || !operations.is_empty(),
            client_name,
            operations,
        },
    )
}

fn escape_rust_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn render_sdk_template<T: Content>(template_path: &str, data: &T) -> Result<String> {
    render_embedded_template(template_path, data)
}

#[cfg(test)]
mod compiler_config_tests {
    use super::*;
    #[test]
    fn lifecycle_cleanup_removes_owned_modules_and_preserves_unowned_files() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            "language = 'rust'\nmodules_path = 'src'\n",
        )
        .unwrap();
        let base = root.path().join("src/client-sdk");
        create_dir_all(base.join("rust")).unwrap();
        for service in ["built", "cli-only", "user-owned"] {
            create_dir_all(base.join("typescript").join(service)).unwrap();
            write(
                base.join("typescript").join(service).join("index.ts"),
                "source",
            )
            .unwrap();
            write(base.join("rust").join(format!("{service}.rs")), "source").unwrap();
        }
        write(base.join("typescript/built/contract.json"), "{}").unwrap();
        write(
            base.join("typescript/cli-only/.forklaunch-generated"),
            "forklaunch.sdk.http.v1\n",
        )
        .unwrap();
        write(
            base.join("typescript/built/.forklaunch-generated"),
            "forklaunch.sdk.http.v1\n",
        )
        .unwrap();
        super::super::service_package::refresh(&base.join("typescript")).unwrap();
        assert!(
            read_to_string(base.join("typescript/index.ts"))
                .unwrap()
                .contains("BuiltSdk")
        );
        for service in ["built", "cli-only", "user-owned"] {
            cleanup_service_generated_files(root.path(), service).unwrap();
        }
        assert!(!base.join("typescript/built").exists());
        assert!(!base.join("rust/built.rs").exists());
        assert!(!base.join("typescript/cli-only").exists());
        assert!(base.join("rust/cli-only.rs").is_file());
        assert!(base.join("typescript/user-owned/index.ts").is_file());
        assert!(base.join("rust/user-owned.rs").is_file());
        let index = read_to_string(base.join("typescript/index.ts")).unwrap();
        assert!(!index.contains("BuiltSdk") && !index.contains("CliOnlySdk"));
        let package: serde_json::Value =
            serde_json::from_str(&read_to_string(base.join("typescript/package.json")).unwrap())
                .unwrap();
        assert!(package["exports"].get("./built").is_none());
        assert!(package["exports"].get("./cli-only").is_none());
    }

    #[test]
    fn native_outputs_are_flat_and_preserve_application_owned_sdk_projects() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            "language = 'mixed'\nmodules_path = 'src'\nruntime = 'tokio'\n",
        )
        .unwrap();
        write(root.path().join("Cargo.toml"), "[workspace]\nmembers = []\n[workspace.package]\nversion = '0.1.0'\nedition = '2024'\nlicense = 'MIT'\n").unwrap();
        let service = SdkService {
            modern: None,
            name: "catalog".into(),
            ts_models: vec![],
            routers: vec![],
        };
        generate_typescript_sdk(root.path(), &[]).unwrap();
        assert!(
            root.path()
                .join("src/client-sdk/typescript/index.ts")
                .is_file()
        );
        assert!(!root.path().join("src/client-sdk/typescript/src").exists());
        let tsconfig: JsonValue = serde_json::from_str(
            &read_to_string(root.path().join("src/client-sdk/typescript/tsconfig.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(tsconfig["include"], json!(["**/*.ts"]));
        create_dir_all(root.path().join("src/client-sdk/rust")).unwrap();
        write(
            root.path().join("src/client-sdk/rust/Cargo.toml"),
            "application-owned Rust SDK",
        )
        .unwrap();
        write(
            root.path().join("src/client-sdk/rust/lib.rs"),
            "application-owned facade",
        )
        .unwrap();
        write(
            root.path().join("src/client-sdk/typescript/index.ts"),
            "owned compile-time TS SDK",
        )
        .unwrap();
        generate_typescript_sdk(root.path(), &[]).unwrap();
        generate_rust_sdks(root.path(), std::slice::from_ref(&service)).unwrap();
        assert_eq!(
            read_to_string(root.path().join("src/client-sdk/typescript/index.ts")).unwrap(),
            "owned compile-time TS SDK"
        );
        assert_eq!(
            read_to_string(root.path().join("src/client-sdk/rust/Cargo.toml")).unwrap(),
            "application-owned Rust SDK"
        );
        assert_eq!(
            read_to_string(root.path().join("src/client-sdk/rust/lib.rs")).unwrap(),
            "application-owned facade"
        );
        assert!(
            !root
                .path()
                .join("src/client-sdk/typescript/generated")
                .exists()
        );
        assert!(
            root.path()
                .join("src/client-sdk/rust/catalog-sdk/lib.rs")
                .is_file()
        );
        assert!(!root.path().join("sdks").exists());
        let cargo: toml::Value = toml::from_str(
            &read_to_string(
                root.path()
                    .join("src/client-sdk/rust/catalog-sdk/Cargo.toml"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            cargo["dependencies"]["forklaunch-core"]["features"][0].as_str(),
            Some("schema-validation")
        );
        // Framework crates resolve from the registry; nothing is vendored.
        assert!(cargo["dependencies"]["forklaunch-core"].get("path").is_none());
        assert!(cargo["dependencies"]["forklaunch-core"]["version"].as_str().is_some_and(|version| version.starts_with('=')));
        assert!(!root.path().join("vendor").exists() && !root.path().join(".forklaunch/framework-rs").exists());
        check_sdk_outputs(root.path(), "rust", &[service]).unwrap();
        check_sdk_outputs(root.path(), "typescript", &[]).unwrap();
    }

    #[test]
    fn repeated_generation_preserves_output_and_workspace_timestamps() {
        let root = tempfile::tempdir().unwrap();
        generate_typescript_sdk(root.path(), &[]).unwrap();
        let paths = [
            "package.json",
            "pnpm-workspace.yaml",
            "sdks/typescript/package.json",
            "sdks/typescript/tsconfig.json",
            "sdks/typescript/src/index.ts",
        ];
        let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(123456789);
        for path in paths {
            std::fs::File::options()
                .write(true)
                .open(root.path().join(path))
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(timestamp))
                .unwrap();
        }
        generate_typescript_sdk(root.path(), &[]).unwrap();
        for path in paths {
            assert_eq!(
                std::fs::metadata(root.path().join(path))
                    .unwrap()
                    .modified()
                    .unwrap(),
                timestamp,
                "rewrote {path}"
            );
        }
    }

    #[test]
    fn sdk_generation_adds_missing_compiler_configuration_and_preserves_user_settings() {
        let root = tempfile::tempdir().unwrap();
        generate_typescript_sdk(root.path(), &[]).unwrap();
        let path = root.path().join("sdks/typescript/tsconfig.json");
        assert!(path.is_file());
        let custom = r#"{"compilerOptions":{"strict":true,"noUncheckedIndexedAccess":true},"include":["src/**/*.ts","custom/**/*.ts"]}"#;
        write(&path, custom).unwrap();
        generate_typescript_sdk(root.path(), &[]).unwrap();
        assert_eq!(read_to_string(path).unwrap(), custom);
    }
}

#[cfg(test)]
mod wire_name_tests {
    use super::*;

    #[test]
    fn rust_models_preserve_exact_json_property_names() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "recordCount": {"type": "integer"},
                "type": {"type": "string"},
                "user-name": {"type": "string"},
                "quote\"\\\n\u{0000}": {"type": "string"}
            }
        });
        let mut models = BTreeMap::new();
        rust_model_from_schema("Response", &schema, &mut models);
        let fields = &models["Response"].fields;
        for (wire, ident) in [
            ("recordCount", "record_count"),
            ("type", "type_"),
            ("user-name", "user_name"),
        ] {
            let field = fields
                .iter()
                .find(|field| field.field_name == ident)
                .unwrap();
            assert_eq!(field.wire_name_literal, format!("{wire:?}"));
        }
        let source = render_sdk_template(
            "sdk/rust/lib.rs",
            &RustSdkTemplate {
                model_source: render_rust_models(
                    &models.values().cloned().collect::<Vec<_>>(),
                    &[],
                )
                .unwrap(),
                has_operations: false,
                has_multipart: false,
                has_multipart_response: false,
                has_sse: false,
                has_serde: !models.is_empty(),
                client_name: "Client".into(),
                operations: vec![],
            },
        )
        .unwrap();
        assert!(source.contains("#[serde(rename = \"recordCount\")]"));
        assert!(source.contains(r#"#[serde(rename = "quote\"\\\n\0")]"#));
    }
}

#[cfg(test)]
mod empty_contract_tests {
    use super::*;
    #[test]
    fn durable_worker_lifecycle_fallback_never_advertises_unmounted_routes() {
        let root = tempfile::tempdir().unwrap();
        for variant in [
            "worker-postgres",
            "worker-redis",
            "worker-kafka",
            "worker-bullmq",
        ] {
            let manifest: Value = toml::from_str(&format!(
                r#"
[[projects]]
name = "jobs"
type = "Worker"
variant = "{variant}"
routers = ["health", "process"]
[projects.metadata]
language = "rust"
"#
            ))
            .unwrap();
            let services = rust_sdk_services_for_lifecycle(root.path(), &manifest).unwrap();
            assert_eq!(services.len(), 1);
            assert!(services[0].routers.is_empty());
            assert!(!typescript_sdk(&services).unwrap().contains("/health"));
        }
    }
    #[test]
    fn empty_application_contract_does_not_invent_health_routes() {
        let service = sdk_service_from_contract("jobs", &json!({"routes": []})).unwrap();
        assert!(service.routers.is_empty());
        let source = rust_sdk(&service).unwrap();
        assert!(!source.contains("pub async fn"));
        assert!(!source.contains("use serde::{"));
        assert!(!source.contains("fn path_with_params"));
        assert!(source.contains("runtime: ClientRuntime"));
        let source = typescript_sdk(&[service]).unwrap();
        assert!(!source.contains("/health"));
    }
}

#[cfg(test)]
mod empty_response_tests {
    use super::*;
    #[test]
    fn absent_bodies_and_json_null_have_distinct_types() {
        for (method, status, body, empty) in [
            ("GET", 200, JsonValue::Null, true),
            ("DELETE", 204, JsonValue::Null, true),
            (
                "POST",
                205,
                json!({"schema":{"jsonSchema":{"type":"string"}}}),
                true,
            ),
            (
                "HEAD",
                200,
                json!({"schema":{"jsonSchema":{"type":"string"}}}),
                true,
            ),
            (
                "GET",
                200,
                json!({"schema":{"jsonSchema":{"type":"null"}}}),
                false,
            ),
        ] {
            let service = sdk_service_from_contract(
                "example",
                &json!({"routes":[{
                    "method":method, "path":"/item", "operationName":"call", "responses":[{"statusCode":status, "body":body}]
                }]}),
            )
            .unwrap();
            let operation = &service.routers[0].operations[0];
            let response = &operation.responses[0];
            assert_eq!(response.empty_response, empty);
            assert_eq!(
                response.content_type,
                if empty { "" } else { "application/json" }
            );
            assert_eq!(
                response.response_body_type,
                if empty { "undefined" } else { "null" }
            );
            assert!(operation.rust_aliases.iter().any(|alias| alias.name
                == response.rust_response_type
                && alias.target_type == "()"));
            let source = typescript_sdk(&[service]).unwrap();
            if empty {
                assert!(source.contains(&format!("'{status}:': 'empty'")));
                assert!(!source.contains(&format!("'{status}:application/json': 'empty'")));
            } else {
                assert!(source.contains("'200:application/json': 'json'"));
            }
        }
    }
}

#[cfg(test)]
mod multiple_response_tests {
    use super::*;
    #[test]
    fn generates_distinct_success_models_and_all_status_branches() {
        let contract = json!({"routes":[{"method":"GET","path":"/item","operationName":"getItem","responses":[
            {"statusCode":201,"body":{"schema":{"jsonSchema":{"type":"object","properties":{"createdAt":{"type":"string"}}}}}},
            {"statusCode":200,"body":{"schema":{"jsonSchema":{"type":"object","properties":{"displayName":{"type":"string"}}}}}},
            {"statusCode":204}
        ]}]});
        let service = sdk_service_from_contract("example", &contract).unwrap();
        let operation = &service.routers[0].operations[0];
        assert_eq!(
            operation
                .responses
                .iter()
                .map(|response| response.status_code)
                .collect::<Vec<_>>(),
            vec![200, 201, 204]
        );
        assert_ne!(
            operation.responses[0].rust_response_type,
            operation.responses[1].rust_response_type
        );
        let source = rust_sdk(&service).unwrap();
        assert!(source.contains("Status201(Box<ExampleItemGetItemStatus201Response>)"));
        assert!(source.contains("pub created_at: ::std::option::Option<::std::string::String>"));
        let source = typescript_sdk(&[service]).unwrap();
        assert!(source.contains("status: 201; contentType: 'application/json'; response: ExampleItemGetItemStatus201Response"));
        assert!(
            source.contains("status: 204;")
                && source.contains("response: ExampleItemGetItemStatus204Response")
        );
    }
    #[test]
    fn colliding_response_model_names_fail_before_generation() {
        let contract = json!({"routes":[
            {"path":"/item","operationName":"getItem","responses":[
                {"statusCode":200},
                {"statusCode":201,"body":{"schema":{"jsonSchema":{"type":"object","properties":{"createdAt":{"type":"string"}}}}}}
            ]},
            {"path":"/item/new","operationName":"getItemStatus201","responses":[
                {"statusCode":200,"body":{"schema":{"jsonSchema":{"type":"object","properties":{"different":{"type":"string"}}}}}}
            ]}
        ]});
        let service = sdk_service_from_contract("example", &contract).unwrap();
        assert!(rust_sdk(&service).is_err());
        assert!(typescript_sdk(&[service]).is_err());
    }
    #[test]
    fn duplicate_statuses_fail_and_error_only_contracts_do_not_invent_success() {
        let duplicate = json!({"routes":[{"path":"/item","responses":[{"statusCode":200},{"statusCode":200}]}]});
        assert!(sdk_service_from_contract("example", &duplicate).is_err());
        let service = sdk_service_from_contract(
            "example",
            &json!({"routes":[{"path":"/item","responses":[{"statusCode":403}]}]}),
        )
        .unwrap();
        assert_eq!(
            service.routers[0].operations[0].responses[0].status_code,
            403
        );
        assert!(!rust_sdk(&service).unwrap().contains("Status200"));
    }
}

#[cfg(test)]
mod response_media_tests {
    use super::*;
    #[test]
    fn legacy_runtime_schemas_preserve_recursive_constraints_and_literal_values() {
        let literal = json!({"$ref":"#literal","x-forklaunch-sdk-reference":"literal"});
        let schema = json!({"$ref":"#Node","$defs":{"node":{
            "$anchor":"Node","type":"object","required":["name","children"],
            "additionalProperties":false,"properties":{
                "name":{"type":"string","minLength":2},
                "children":{"type":"array","items":{"$ref":"#Node"}}
            }
        }},"examples":[literal]});
        let service = sdk_service_from_contract("trees", &json!({"routes":[{
            "path":"/trees","operationName":"read","responses":[
                {"statusCode":200,"body":{"kind":"json","schema":{"jsonSchema":schema}}},
                {"statusCode":409,"body":{"kind":"json","schema":{"jsonSchema":{"type":"string","minLength":4}}}}
            ]
        }]})).unwrap();
        let operation = &service.routers[0].operations[0];
        assert!(!operation.rust_models.is_empty());
        for response in &operation.responses {
            let encoded: String = serde_json::from_str(&response.rust_body_schema).unwrap();
            let runtime: JsonValue = serde_json::from_str(&encoded).unwrap();
            assert_eq!(encoded, response.body_schema_json);
            let validate =
                forklaunch_core::contract::schema_validation::compile_checked(&runtime).unwrap();
            if response.status_code == 200 {
                assert_eq!(runtime["examples"], json!([literal]));
                assert!(
                    validate(&json!({"name":"root","children":[{"name":"child","children":[]}]}))
                        .is_ok()
                );
                // This is a valid generated String type, but violates the nested schema.
                assert!(
                    validate(&json!({"name":"root","children":[{"name":"x","children":[]}]}))
                        .is_err()
                );
                assert!(validate(&json!({"name":"root","children":[],"extra":true})).is_err());
            } else {
                assert!(validate(&json!("valid")).is_ok());
                assert!(validate(&json!("x")).is_err());
            }
        }
        assert!(rust_sdk(&service).is_ok());
        assert!(typescript_sdk(&[service]).is_ok());
    }

    #[test]
    fn legacy_json_collection_responses_share_the_native_allocation_policy() {
        let record = json!({"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false});
        for (schema, boxed) in [
            (json!({"type":"array","items":record}), false),
            (
                json!({"type":"object","additionalProperties":record}),
                false,
            ),
            (record.clone(), true),
            (json!({"anyOf":[record,{"type":"null"}]}), true),
        ] {
            let service = sdk_service_from_contract(
                "trees",
                &json!({"routes":[{
                    "path":"/trees", "operationName":"read", "method":"GET", "responses":[{
                        "statusCode":200,"body":{"kind":"json","schema":{"jsonSchema":schema}}
                    }]
                }]}),
            )
            .unwrap();
            let operation = &service.routers[0].operations[0];
            assert!(!operation.rust_models.is_empty());
            assert_eq!(
                operation.responses[0]
                    .rust_response_type
                    .starts_with("Box<"),
                boxed
            );
            assert!(
                rust_sdk(&service)
                    .unwrap()
                    .contains("pub enum TreesTreesReadResponses")
            );
        }
        let default = sdk_router("trees", "health", true);
        assert_eq!(default.operations[0].responses[0].status_code, 200);
        assert!(
            default.operations[0].responses[0]
                .rust_response_type
                .starts_with("Box<")
        );
    }

    #[test]
    fn declared_errors_and_downloads_have_contract_specific_decoders() {
        let service = sdk_service_from_contract("example", &json!({"routes":[{"path":"/item","operationName":"getItem","responses":[
            {"statusCode":200,"body":{"kind":"binary","contentType":"application/octet-stream"}},
            {"statusCode":400,"body":{"kind":"json","contentType":"application/problem+json","schema":{"jsonSchema":{"type":"object","required":["errorCode"],"properties":{"errorCode":{"type":"string"}}}}}},
            {"statusCode":409,"body":{"kind":"text","contentType":"text/plain; charset=utf-8"}}
        ]}]})).unwrap();
        let operation = &service.routers[0].operations[0];
        assert!(operation.responses[0].binary_response);
        assert_eq!(operation.responses[0].response_body_type, "Uint8Array");
        assert!(operation.responses[1].json_response);
        assert_eq!(
            operation.responses[1].content_type,
            "application/problem+json"
        );
        assert!(operation.responses[2].text_response);
        let source = rust_sdk(&service).unwrap();
        assert!(source.contains("Vec<u8>"));
        assert!(source.contains("pub error_code: ::std::string::String"));
        assert!(source.contains("response.bytes().await?"));
        let source = typescript_sdk(&[service]).unwrap();
        assert!(source.contains("status: 400; contentType: 'application/problem+json'"));
        assert!(source.contains("Omit<ForkLaunchErrorResponses"));
    }
    #[test]
    fn sse_responses_generate_owned_typed_streams_and_configurable_limits() {
        let service = sdk_service_from_contract("events", &json!({"routes":[{"path":"/events","operationName":"listen","method":"GET","responses":[{"statusCode":200,"body":{"kind":"serverSentEvent","schema":{"jsonSchema":{"type":"object","required":["data"],"properties":{"data":{"type":"string"},"id":{"type":"string"}}}}}}]}]})).unwrap();
        assert!(service.routers[0].operations[0].responses[0].streaming_response);
        let source = rust_sdk(&service).unwrap();
        assert!(source.contains("pub use runtime::{SdkEventStream, sse};"));
        assert!(source.contains("Box<SdkEventStream<"));
        assert!(source.contains("pub data: ::std::string::String"));
        assert!(
            source
                .contains("SdkEventStream::from_response(response,self.runtime.sse_event_limit(),")
        );
        assert!(source.contains("pub fn with_sse_event_limit"));
        assert!(
            source.contains("self.runtime = self.runtime.with_sse_event_limit(max_event_bytes)?;")
        );
        let source = typescript_sdk(&[service]).unwrap();
        assert!(source.contains("AsyncIterable<"));
        assert!(source.contains("'200:text/event-stream': 'sse'"));
        assert!(source.contains("iterator.return = async"));
    }

    #[test]
    fn inconsistent_streaming_and_multipart_response_media_fail_explicitly() {
        for body in [
            json!({"kind":"text","contentType":"text/event-stream"}),
            json!({"kind":"multipart","contentType":"multipart/mixed"}),
            json!({"kind":"text","contentType":"multipart/form-data"}),
        ] {
            assert!(
                response_contracts(&json!({"responses":[{"statusCode":200,"body":body}]})).is_err()
            );
        }
    }
    #[test]
    fn unspecified_json_schema_retains_json_value_type() {
        let mut models = BTreeMap::new();
        assert_eq!(
            build_rust_type("Unknown", &json!({}), &mut models),
            "::serde_json::Value"
        );
    }
}

#[cfg(test)]
mod identifier_collision_tests {
    use super::*;
    #[test]
    fn fields_and_nested_models_keep_distinct_wire_values_after_normalization() {
        let schema = json!({"type":"object","properties":{
            "fooBar":{"type":"object","properties":{"first":{"type":"string"}}},
            "foo_bar":{"type":"object","properties":{"second":{"type":"integer"}}},
            "foo_bar2":{"type":"object","properties":{"third":{"type":"boolean"}}},
            "foo_bar_2":{"type":"object","properties":{"fourth":{"type":"string"}}},
            "type":{"type":"string"},"type_":{"type":"integer"},"gen":{"type":"string"},"_":{"type":"string"},"quote\"\\":{"type":"string"}
        }});
        let mut models = BTreeMap::new();
        let root = rust_model_from_schema("Response", &schema, &mut models);
        let fields = models[&root]
            .fields
            .iter()
            .filter(|field| !field.flatten)
            .collect::<Vec<_>>();
        assert_eq!(fields.len(), 9);
        assert_eq!(
            fields
                .iter()
                .map(|f| &f.field_name)
                .collect::<BTreeSet<_>>()
                .len(),
            9
        );
        for field in &fields {
            assert!(
                field
                    .field_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            );
            assert!(!is_rust_keyword(&field.field_name));
        }
        let nested = fields
            .iter()
            .filter(|field| field.field_type.contains("ResponseFoo"))
            .collect::<Vec<_>>();
        assert_eq!(nested.len(), 4);
        assert_eq!(
            nested
                .iter()
                .map(|f| &f.field_type)
                .collect::<BTreeSet<_>>()
                .len(),
            4
        );
    }
}

#[cfg(test)]
mod nullable_dictionary_tests {
    use super::*;
    #[test]
    fn nullable_dialects_and_dictionary_values_preserve_wire_types() {
        for schema in [
            json!({"type":["string","null"]}),
            json!({"type":"string","nullable":true}),
            json!({"anyOf":[{"type":"string"},{"type":"null"}]}),
            json!({"oneOf":[{"type":"null"},{"type":"string"}]}),
        ] {
            assert_eq!(
                build_rust_type("Value", &schema, &mut BTreeMap::new()),
                "::std::option::Option<::std::string::String>"
            );
            assert!(ts_type_from_schema(&schema).contains("null"));
        }
        let dictionary =
            json!({"type":"object","additionalProperties":{"type":["integer","null"]}});
        assert_eq!(
            build_rust_type("Map", &dictionary, &mut BTreeMap::new()),
            "::std::collections::BTreeMap<::std::string::String, ::std::option::Option<i64>>"
        );
        assert_eq!(
            ts_type_from_schema(&dictionary),
            "Record<string, number | null>"
        );
        assert_eq!(
            build_rust_type("List", &json!({"type":"array"}), &mut BTreeMap::new()),
            "::std::vec::Vec<::serde_json::Value>"
        );
        assert_eq!(
            build_rust_type(
                "Union",
                &json!({"type":["string","integer"]}),
                &mut BTreeMap::new()
            ),
            "Union"
        );
        assert_eq!(
            build_rust_type("Constant", &json!({"const":1.5}), &mut BTreeMap::new()),
            "f64"
        );
    }
    #[test]
    fn additional_values_use_a_distinct_flattened_field() {
        let schema = json!({"type":"object","properties":{"additional_properties":{"type":"string"}},"additionalProperties":{"type":"integer"}});
        let mut models = BTreeMap::new();
        rust_model_from_schema("Example", &schema, &mut models);
        let field = models["Example"]
            .fields
            .iter()
            .find(|field| field.flatten)
            .unwrap();
        assert_eq!(field.field_name, "additional_properties_2");
        assert_eq!(
            field.field_type,
            "::std::collections::BTreeMap<::std::string::String, i64>"
        );
        let closed = json!({"type":"object","properties":{"id":{"type":"string"}},"additionalProperties":false});
        rust_model_from_schema("Closed", &closed, &mut models);
        assert!(!models["Closed"].fields.iter().any(|field| field.flatten));
    }
}

#[cfg(test)]
mod lifecycle_contract_tests {
    use super::*;
    fn manifest() -> Value {
        toml::from_str(
            r#"
[[projects]]
name = "existing"
type = "Service"
routers = ["health", "custom"]
[projects.metadata]
language = "rust"
[[projects]]
name = "changed"
type = "Service"
routers = ["health", "internal-users", "audit_log"]
[projects.metadata]
language = "rust"
"#,
        )
        .unwrap()
    }
    #[test]
    fn missing_contract_falls_back_only_for_its_own_service() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(".forklaunch/contracts/existing");
        std::fs::create_dir_all(&path).unwrap();
        write(path.join("contract.json"), json!({"routes":[{"path":"/custom","operationName":"fetchActual","responses":[{"statusCode":202,"body":{"schema":{"jsonSchema":{"type":"object","properties":{"realField":{"type":"boolean"}}}}}}]}]}).to_string()).unwrap();
        let services = rust_sdk_services_for_lifecycle(root.path(), &manifest()).unwrap();
        let source = typescript_sdk(&services).unwrap();
        assert!(source.contains("fetchActual:"));
        assert!(source.contains("realField?: boolean"));
        assert!(source.contains("listInternalUsers:"));
        assert!(source.contains("getHealth:"));
        assert!(source.contains("path: '/audit_log'"));
        assert!(!source.contains("listCustom:"));
    }
    #[test]
    fn rust_only_lifecycle_refresh_updates_only_existing_sdk_crates() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            toml::to_string(&manifest()).unwrap(),
        )
        .unwrap();
        write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = []\n",
        )
        .unwrap();
        let sdk = root.path().join("sdks/rust/changed-sdk");
        std::fs::create_dir_all(sdk.join("src")).unwrap();
        write(sdk.join("Cargo.toml"), "old SDK").unwrap();
        write(sdk.join("src/lib.rs"), "stale method").unwrap();
        regenerate_sdk_outputs_if_present(
            root.path(),
            &mut StandardStream::stdout(ColorChoice::Never),
        )
        .unwrap();
        let source = read_to_string(sdk.join("src/lib.rs")).unwrap();
        assert!(source.contains("internal_users_list_internal_users"));
        assert!(!source.contains("stale method"));
        assert!(!root.path().join("sdks/typescript").exists());
        assert!(!root.path().join("sdks/rust/existing-sdk").exists());
        let paths = [
            root.path().join("Cargo.toml"),
            sdk.join("Cargo.toml"),
            sdk.join("src/lib.rs"),
        ];
        let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(123456789);
        for path in &paths {
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(timestamp))
                .unwrap();
        }
        regenerate_sdk_outputs_if_present(
            root.path(),
            &mut StandardStream::stdout(ColorChoice::Never),
        )
        .unwrap();
        for path in &paths {
            assert_eq!(
                std::fs::metadata(path).unwrap().modified().unwrap(),
                timestamp,
                "rewrote {}",
                path.display()
            );
        }
    }
    #[test]
    fn malformed_existing_artifact_is_not_silently_replaced() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(".forklaunch/contracts/existing");
        std::fs::create_dir_all(&path).unwrap();
        write(path.join("contract.json"), "invalid JSON").unwrap();
        assert!(rust_sdk_services_for_lifecycle(root.path(), &manifest()).is_err());
    }
}

#[cfg(test)]
mod request_media_tests {
    use super::*;
    #[test]
    fn text_binary_and_unspecified_json_requests_have_correct_types() {
        for (kind, media, expected) in [
            ("text", "text/plain; charset=utf-8", "string"),
            (
                "binary",
                "application/octet-stream",
                "Uint8Array | ArrayBuffer | Blob",
            ),
            ("file", "image/png", "Uint8Array | ArrayBuffer | Blob"),
            ("json", "application/problem+json", "unknown"),
        ] {
            let shapes = request_type_shapes_from_contract(
                &json!({"request":{"body":{"kind":kind,"contentType":media}}}),
            )
            .unwrap();
            assert_eq!(shapes.body, expected);
            assert_eq!(shapes.content_type, media);
        }
        let service = sdk_service_from_contract("example", &json!({"routes":[{"path":"/bytes","operationName":"upload","method":"POST","request":{"body":{"kind":"binary"}},"responses":[{"statusCode":204}]}]})).unwrap();
        let source = rust_sdk(&service).unwrap();
        assert!(source.contains("pub body: Option<Vec<u8>>"));
        assert!(source.contains("content_type: \"application/octet-stream\""));
        assert!(
            typescript_sdk(&[service])
                .unwrap()
                .contains("requestKind: 'binary'")
        );
    }
    #[test]
    fn optional_body_artifact_preserves_supplied_type_and_optional_input() {
        for optional in [false, true] {
            let artifact = json!({"routes":[{"path":"/optional","operationName":"optional","method":"POST","request":{"bodyOptional":optional,"body":{"kind":"json","schema":{"jsonSchema":{"type":"string"}}}},"responses":[{"statusCode":204}]}]});
            let shapes = request_type_shapes_from_contract(&artifact["routes"][0]).unwrap();
            assert_eq!(shapes.body_optional, optional);
            assert_eq!(shapes.body, "string");
            let service = sdk_service_from_contract("example", &artifact).unwrap();
            let rust = rust_sdk(&service).unwrap();
            assert!(rust.contains("pub body: Option<T>"));
            assert!(rust.contains("RequestBody = ::std::string::String;"));
            assert!(rust.contains("_raw_request(&self, input:"));
            let source = typescript_sdk(&[service]).unwrap();
            assert_eq!(source.contains("ForkLaunchHeaders, true>"), optional);
            assert_eq!(source.contains("Request = {})"), optional);
        }
    }

    #[test]
    fn rust_json_request_body_models_are_typed_and_responses_unchanged() {
        let artifact = json!({"routes":[{"path":"/profiles","operationName":"create","method":"POST","request":{"body":{"kind":"json","schema":{"jsonSchema":{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string"}},"required":["name"]}}}},"responses":[{"statusCode":200,"body":{"kind":"json","schema":{"jsonSchema":{"type":"string"}}}}]}]});
        let service = sdk_service_from_contract("profiles", &artifact).unwrap();
        let source = rust_sdk(&service).unwrap();
        assert!(source.contains("RequestBody {"));
        assert!(source.contains("pub name: ::std::string::String"));
        assert!(source.contains("pub body: Option<T>"));
        assert!(source.contains("Response = ::std::string::String;"));
        assert!(source.contains("serde_json::to_string(&body)"));
        assert!(source.contains("_raw(&self, input:"));
    }

    #[test]
    fn multipart_requests_use_native_forms_and_encoder_owned_boundaries() {
        let service = sdk_service_from_contract("uploads", &json!({"routes":[{"path":"/upload","operationName":"upload","method":"POST","request":{"body":{"kind":"multipart"}},"responses":[{"statusCode":204}]}]})).unwrap();
        assert!(service_has_multipart(&service));
        let source = rust_sdk(&service).unwrap();
        assert!(source.contains("pub body: Option<MultipartForm>"));
        assert!(source.contains("pub use runtime::{MultipartForm, MultipartPart};"));
        assert!(source.contains("type ClientRuntime = runtime::UniversalSdk<MultipartForm>;"));
        assert!(source.contains("multipart: input.body,"));
        assert!(source.contains("let response = self.runtime.execute(request).await?;"));
        let cargo: toml::Value =
            toml::from_str(&rust_sdk_cargo_toml("uploads-sdk", "uploads", true, false)).unwrap();
        assert!(
            cargo["dependencies"]["reqwest"]["features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|feature| feature.as_str() == Some("multipart"))
        );
        let source = typescript_sdk(&[service]).unwrap();
        assert!(
            source
                .contains("ForkLaunchSdkRequest<never, never, FormData | Record<string, unknown>,")
        );
        assert!(source.contains("requestKind: 'multipart'"));
        assert!(source.contains("requestHeaders.delete('content-type')"));
    }

    #[test]
    fn unsupported_request_transports_fail_before_rendering() {
        for body in [
            json!({"kind":"multipart","contentType":"multipart/mixed"}),
            json!({"kind":"text","contentType":"text/event-stream"}),
            json!({"kind":"binary","contentType":"multipart/form-data"}),
            json!({"kind":"json","contentType":"application/json/injected"}),
        ] {
            assert!(request_type_shapes_from_contract(&json!({"request":{"body":body}})).is_err());
        }
    }
}

#[cfg(test)]
mod union_model_tests {
    use super::*;
    #[test]
    fn schema_unions_generate_untagged_models_with_distinct_branches() {
        let mut models = BTreeMap::new();
        let schema = json!({"oneOf":[{"type":"object","required":["name"],"properties":{"name":{"type":"string"}},"additionalProperties":false},{"type":"object","required":["count"],"properties":{"count":{"type":"integer"}},"additionalProperties":false},{"type":"boolean"}]});
        assert_eq!(build_rust_type("Choice", &schema, &mut models), "Choice");
        let union = &models["Choice"];
        assert!(union.union);
        assert_eq!(
            union
                .variants
                .iter()
                .map(|v| v.value_type.as_str())
                .collect::<Vec<_>>(),
            ["ChoiceVariant1", "ChoiceVariant2", "bool"]
        );
        assert!(models["ChoiceVariant1"].deny_unknown_fields);
        let source = render_sdk_template(
            "sdk/rust/lib.rs",
            &RustSdkTemplate {
                model_source: render_rust_models(
                    &models.values().cloned().collect::<Vec<_>>(),
                    &[],
                )
                .unwrap(),
                has_operations: false,
                has_multipart: false,
                has_multipart_response: false,
                has_sse: false,
                has_serde: !models.is_empty(),
                client_name: "Client".into(),
                operations: vec![],
            },
        )
        .unwrap();
        assert!(source.contains("#[serde(untagged)]"));
        assert!(source.contains("impl ::std::default::Default for Choice"));
        assert!(source.contains("#[serde(deny_unknown_fields)]"));
    }
}

#[cfg(test)]
mod recursive_model_tests {
    use super::*;

    #[test]
    fn required_recursive_defaults_are_omitted_but_guarded_defaults_remain() {
        let schema = super::super::schema::normalize(
            &json!({"type":"object","required":["next"],"properties":{"next":{"$ref":"#"}}}),
            "Cycle",
        )
        .unwrap();
        let (models, _, _) = rust_types_from_schema("Root", &schema);
        assert!(models.iter().all(|model| !model.has_default));
        let schema = super::super::schema::normalize(
            &json!({"type":"object","properties":{"next":{"$ref":"#"}}}),
            "Optional",
        )
        .unwrap();
        let (models, _, _) = rust_types_from_schema("Root", &schema);
        assert!(models.iter().all(|model| model.has_default));
    }

    #[test]
    fn acyclic_refs_preserve_existing_inline_types_and_recursive_maps_have_structural_guards() {
        let schema = super::super::schema::normalize(&json!({"type":"object","required":["label"],"properties":{"label":{"$ref":"#/$defs/Text"}},"$defs":{"Text":{"type":"string"}}}), "Plain").unwrap();
        let (models, _, _) = rust_types_from_schema("Root", &schema);
        assert_eq!(models.len(), 1);
        assert_eq!(
            models[0]
                .fields
                .iter()
                .find(|field| field.field_name == "label")
                .unwrap()
                .field_type,
            "::std::string::String"
        );
        let schema = super::super::schema::normalize(
            &json!({"type":"object","additionalProperties":{"$ref":"#"}}),
            "Map",
        )
        .unwrap();
        assert_eq!(ts_type_from_schema(&schema), "{ [key: string]: MapRef1 }");
    }
}

#[cfg(test)]
mod mixed_tests;

#[cfg(test)]
mod reserved_service_name_tests {
    use super::*;

    #[test]
    fn reserved_rust_name_preserves_typescript_reference_labels() {
        let service = sdk_service_from_contract("Self", &json!({"routes":[{
            "method":"GET", "path":"/item", "operationName":"getItem",
            "responses":[{"statusCode":200,"body":{"schema":{"jsonSchema":{
                "$defs":{"Node":{"type":"object","properties":{"next":{"$ref":"#/$defs/Node"}}}},
                "$ref":"#/$defs/Node"
            }}}}]
        }]})).unwrap();
        assert!(
            service
                .ts_models
                .iter()
                .any(|model| model.name == "SelfSchema1Ref1")
        );
        assert!(
            service
                .ts_models
                .iter()
                .all(|model| !model.name.starts_with("Self_"))
        );
    }
}

#[cfg(test)]
mod contract_preparation_tests {
    use super::*;
    #[test]
    fn contract_consumer_preparation_creates_sources_without_artifact() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            "runtime='node'\nmodules_path='modules'\napp_name='demo'\nlicense='MIT'\n",
        )
        .unwrap();
        prepare_contract_consumers(root.path(), "notes").unwrap();
        assert!(
            !root
                .path()
                .join(".forklaunch/contracts/notes/contract.json")
                .exists()
        );
        assert!(root.path().join("sdks/rust/notes-sdk/Cargo.toml").is_file());
        // The published framework crates are selected; nothing is vendored.
        assert!(!root.path().join(".forklaunch/framework-rs").exists());
        assert!(crate::core::forklaunch_state::has_framework(root.path()));
        let first = read_to_string(root.path().join("sdks/typescript/src/notes/index.ts")).unwrap();
        prepare_contract_consumers(root.path(), "notes").unwrap();
        assert_eq!(
            first,
            read_to_string(root.path().join("sdks/typescript/src/notes/index.ts")).unwrap()
        );
        prepare_contract_consumers(root.path(), "jobs").unwrap();
        let index = read_to_string(root.path().join("sdks/typescript/src/index.ts")).unwrap();
        assert!(index.contains("notes") && index.contains("jobs"));
    }
}

#[cfg(test)]
mod json_freshness_tests {
    use super::compare_generated_file;
    #[test]
    fn generated_json_ignores_object_order_but_detects_semantic_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("package.json");
        std::fs::write(
            &path,
            r#"{"name":"sdk","exports":{".":"./index.ts"},"private":true}"#,
        )
        .unwrap();
        let mut stale = Vec::new();
        compare_generated_file(
            &path,
            r#"{"private":true,"exports":{".":"./index.ts"},"name":"sdk"}"#,
            &mut stale,
        )
        .unwrap();
        assert!(stale.is_empty());
        compare_generated_file(
            &path,
            r#"{"private":false,"exports":{".":"./index.ts"},"name":"sdk"}"#,
            &mut stale,
        )
        .unwrap();
        assert_eq!(stale.len(), 1);
        std::fs::write(&path, "invalid JSON").unwrap();
        stale.clear();
        compare_generated_file(&path, "{}", &mut stale).unwrap();
        assert_eq!(stale.len(), 1);
    }

    #[test]
    fn workspace_freshness_preserves_yaml_formatting_but_detects_semantic_drift() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("pnpm-workspace.yaml");
        let authored = "# Keep exclusions after inclusions.\npackages:\n  - 'tools/*'\n  - 'sdks/typescript-websocket/*'\n  - '!tools/disabled'\nminimumReleaseAge: 1440\nallowBuilds:\n  esbuild: true\n";
        std::fs::write(&path, authored).unwrap();
        let expected = "packages:\n- tools/*\n- sdks/typescript-websocket/*\n- '!tools/disabled'\nminimumReleaseAge: 1440\nallowBuilds: {esbuild: true}\n";
        let mut stale = Vec::new();
        compare_generated_file(&path, expected, &mut stale).unwrap();
        assert!(stale.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), authored);
        for changed in [
            expected.replace("- sdks/typescript-websocket/*\n", ""),
            expected.replace("1440", "0"),
            expected.replace("esbuild: true", "esbuild: false"),
            expected.replace(
                "- tools/*\n- sdks/typescript-websocket/*\n- '!tools/disabled'",
                "- '!tools/disabled'\n- tools/*\n- sdks/typescript-websocket/*",
            ),
            "packages: [".to_owned(),
        ] {
            stale.clear();
            compare_generated_file(&path, &changed, &mut stale).unwrap();
            assert_eq!(stale, vec![path.display().to_string()]);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), authored);
        }
        // Other generated YAML assets still require the exact compiler output.
        let asset = directory.path().join("schema.yaml");
        std::fs::write(&asset, authored).unwrap();
        stale.clear();
        compare_generated_file(&asset, expected, &mut stale).unwrap();
        assert_eq!(stale, vec![asset.display().to_string()]);
    }
}

#[cfg(test)]
mod native_compile_layout_tests {
    use super::*;
    #[test]
    fn local_typescript_generation_repairs_workspace_and_check_detects_drift_without_writes() {
        for (runtime, modules) in [("node", "modules"), ("bun", "src")] {
            let root = tempfile::tempdir().unwrap();
            create_dir_all(root.path().join(".forklaunch")).unwrap();
            create_dir_all(root.path().join(modules)).unwrap();
            let manifest = format!(
                "modules_path='{modules}'\nruntime='{runtime}'\n[[projects]]\nname='notes'\ntype='Service'\n[projects.metadata]\nlanguage='rust'\ncratePath='{modules}/notes'\nsdkLayout='project-local-v1'\n"
            );
            write(root.path().join(".forklaunch/manifest.toml"), manifest).unwrap();
            write(
                root.path().join(modules).join("package.json"),
                serde_json::to_string(&json!({
                    "devDependencies":{"typescript":"^5.9.3","tsx":"^4.0.0"},
                    "workspaces":["*"],"scripts":{"build":"fixture build"}
                }))
                .unwrap(),
            )
            .unwrap();
            write(
                root.path().join(modules).join("pnpm-workspace.yaml"),
                "packages: ['*']\n",
            )
            .unwrap();
            let package = root.path().join("package.json");
            let original = r#"{"private":true,"scripts":{"owned":"echo preserved","build":"application-build"}}"#;
            write(&package, original).unwrap();
            let workspace = root.path().join("pnpm-workspace.yaml");
            write(
                &workspace,
                "packages: ['tools/*']\nminimumReleaseAge: 1440\n",
            )
            .unwrap();
            let artifact = json!({"formatVersion":"forklaunch.contract.v1", "sdkCompiler":"forklaunch.sdk.http.v1",
                "sdkLayout":"project-local-v1", "typescriptRuntimeLayout":"local", "routes":[]});
            let services = vec![SdkService {
                name: "notes".into(),
                modern: Some(super::super::modern::compile("notes", &artifact).unwrap()),
                ts_models: vec![],
                routers: vec![],
            }];
            generate_typescript_sdk(root.path(), &services).unwrap();
            let repaired: JsonValue =
                serde_json::from_str(&read_to_string(&package).unwrap()).unwrap();
            assert_eq!(repaired["scripts"]["owned"], "echo preserved");
            assert_eq!(repaired["scripts"]["build"], "application-build");
            assert_eq!(repaired["devDependencies"]["typescript"], "^5.9.3");
            assert_eq!(repaired["devDependencies"]["tsx"], "^4.0.0");
            let expected = format!("{modules}/*/sdk/typescript");
            assert!(read_to_string(&workspace).unwrap().contains(&expected));
            assert!(read_to_string(&workspace).unwrap().contains("tools/*"));
            assert!(read_to_string(&workspace).unwrap().contains("1440"));
            if runtime == "bun" {
                assert!(
                    repaired["workspaces"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(expected))
                );
            }
            check_sdk_outputs(root.path(), "typescript", &services).unwrap();
            if runtime == "node" {
                // Combined generation appends the socket workspace using a
                // comment-preserving edit; HTTP checks must accept that form.
                let authored = read_to_string(&workspace).unwrap().replacen(
                    "packages:\n",
                    "# Shared HTTP and socket SDK packages.\npackages:\n- 'sdks/typescript-websocket/*'\n",
                    1,
                );
                write(&workspace, &authored).unwrap();
                check_sdk_outputs(root.path(), "typescript", &services).unwrap();
                assert_eq!(read_to_string(&workspace).unwrap(), authored);
            }
            let sdk = root
                .path()
                .join(modules)
                .join("notes/sdk/typescript/sdk.ts");
            let source = read_to_string(&sdk).unwrap();
            write(&package, original).unwrap();
            let drift = "packages: ['tools/*']\nminimumReleaseAge: 1440\n";
            write(&workspace, drift).unwrap();
            let error = check_sdk_outputs(root.path(), "typescript", &services)
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("package.json") && error.contains("pnpm-workspace.yaml"),
                "{error}"
            );
            assert_eq!(read_to_string(&package).unwrap(), original);
            assert_eq!(read_to_string(&workspace).unwrap(), drift);
            assert_eq!(read_to_string(&sdk).unwrap(), source);
            generate_typescript_sdk(root.path(), &services).unwrap();
            check_sdk_outputs(root.path(), "typescript", &services).unwrap();
            assert_eq!(read_to_string(&sdk).unwrap(), source);
        }
    }

    #[test]
    fn project_local_openapi_import_uses_current_compiler() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        create_dir_all(root.path().join("src/notes/sdk/typescript")).unwrap();
        let manifest: Value = toml::from_str("modules_path='src'\n[[projects]]\nname='notes'\ntype='Service'\n[projects.metadata]\nlanguage='rust'\ncratePath='src/notes'\nsdkLayout='project-local-v1'\nopenapiPath='src/notes/sdk/typescript/openapi.json'\n").unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let openapi = json!({"openapi":"3.1.0","info":{"title":"Notes","version":"1"},"paths":{
            "/notes/{id}": {"get": {"operationId":"getNote", "parameters":[{"in":"path","name":"id","required":true,"schema":{"type":"string"}}],
                "responses":{"200":{"description":"Note","content":{"application/json":{"schema":{"type":"object","properties":{"title":{"type":"string"}},"required":["title"],"additionalProperties":false}}}},"204":{"description":"Empty"}}
            }}
        }});
        write(
            root.path().join("src/notes/sdk/typescript/openapi.json"),
            serde_json::to_string(&openapi).unwrap(),
        )
        .unwrap();
        let services =
            http_sdk_services_from_artifacts(root.path(), &manifest, Some("notes"), "openapi")
                .unwrap();
        assert_eq!(services.len(), 1);
        let compiled = services[0]
            .modern
            .as_ref()
            .expect("Project-local imports need the current SDK compiler");
        assert!(compiled.rust_module);
        assert!(compiled.typescript_files["sdk.ts"].contains("getNote"));
        assert!(compiled.rust.contains("get_note"));
    }

    #[test]
    fn filtered_service_preserves_siblings_but_full_check_detects_stale_orphans() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            "modules_path='src'\nruntime='node'\n",
        )
        .unwrap();
        let artifact = json!({"formatVersion":"forklaunch.contract.v1","sdkCompiler":"forklaunch.sdk.http.v1","routes":[]});
        let services = ["notes", "insights", "iam"].map(|name| SdkService {
            name: name.into(),
            modern: Some(super::super::modern::compile(name, &artifact).unwrap()),
            ts_models: vec![],
            routers: vec![],
        });
        generate_typescript_sdk(root.path(), &services).unwrap();
        let source = root.path().join("src/client-sdk/typescript");
        let package = read_to_string(source.join("package.json")).unwrap();
        let barrel = read_to_string(source.join("index.ts")).unwrap();
        let selected = std::slice::from_ref(&services[1]);
        check_sdk_outputs_scoped(root.path(), "typescript", selected, true).unwrap();
        generate_typescript_sdk_scoped(root.path(), selected, true).unwrap();
        assert_eq!(
            package,
            read_to_string(source.join("package.json")).unwrap()
        );
        assert_eq!(barrel, read_to_string(source.join("index.ts")).unwrap());
        // A partial check validates only selected sources, but a complete check
        // must still detect stale sibling sources and obsolete root exports.
        write(source.join("notes/client.ts"), "// stale sibling").unwrap();
        check_sdk_outputs_scoped(root.path(), "typescript", selected, true).unwrap();
        assert!(check_sdk_outputs(root.path(), "typescript", &services).is_err());
        generate_typescript_sdk(root.path(), &services).unwrap();
        assert!(check_sdk_outputs(root.path(), "typescript", selected).is_err());
        write(source.join("insights/client.ts"), "// stale selected").unwrap();
        assert!(check_sdk_outputs_scoped(root.path(), "typescript", selected, true).is_err());
        generate_typescript_sdk_scoped(root.path(), selected, true).unwrap();
        check_sdk_outputs(root.path(), "typescript", &services).unwrap();
    }

    #[test]
    fn native_modern_rust_matches_compile_output_without_duplicate_crate() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            "language='rust'\nmodules_path='src'\nruntime='tokio'\n",
        )
        .unwrap();
        let workspace = "[workspace]\nmembers=[]\n[workspace.package]\nversion='0.1.0'\nedition='2024'\nlicense='MIT'\n";
        write(root.path().join("Cargo.toml"), workspace).unwrap();
        let artifact = json!({"formatVersion":"forklaunch.contract.v1","sdkCompiler":"forklaunch.sdk.http.v1","rustClientName":"NotesClient","rustOutputLayout":"service-module","routes":[]});
        let compiled = super::super::modern::compile("notes", &artifact).unwrap();
        assert!(compiled.rust.contains("pub struct NotesClient"));
        let service = SdkService {
            name: "notes".into(),
            modern: Some(compiled.clone()),
            ts_models: vec![],
            routers: vec![],
        };
        let file = root.path().join("src/client-sdk/rust/notes.rs");
        create_dir_all(file.parent().unwrap()).unwrap();
        let facade = forklaunch_core::build::rust_http_facade(&file, &compiled.rust_files).unwrap();
        write_sdk_outputs(rust_service_outputs(file.parent().unwrap(), &service, true).unwrap())
            .unwrap();
        check_sdk_outputs(root.path(), "rust", std::slice::from_ref(&service)).unwrap();
        generate_rust_sdks(root.path(), std::slice::from_ref(&service)).unwrap();
        assert_eq!(read_to_string(&file).unwrap(), facade);
        assert!(!root.path().join("src/client-sdk/rust/notes-sdk").exists());
        assert_eq!(
            read_to_string(root.path().join("Cargo.toml")).unwrap(),
            workspace
        );
        let models = file.parent().unwrap().join("notes/models.rs");
        assert_eq!(
            read_to_string(&models).unwrap(),
            compiled.rust_files["models.rs"]
        );
        write(&models, "// stale").unwrap();
        assert!(check_sdk_outputs(root.path(), "rust", std::slice::from_ref(&service)).is_err());
        generate_rust_sdks(root.path(), std::slice::from_ref(&service)).unwrap();
        check_sdk_outputs(root.path(), "rust", std::slice::from_ref(&service)).unwrap();
        let mut old_artifact = artifact;
        old_artifact
            .as_object_mut()
            .unwrap()
            .remove("rustOutputLayout");
        old_artifact
            .as_object_mut()
            .unwrap()
            .remove("rustClientName");
        let old_service = SdkService {
            name: "notes".into(),
            modern: Some(super::super::modern::compile("notes", &old_artifact).unwrap()),
            ts_models: vec![],
            routers: vec![],
        };
        assert!(
            generate_rust_sdks(root.path(), std::slice::from_ref(&old_service))
                .unwrap_err()
                .to_string()
                .contains("Rebuild")
        );
        assert_eq!(read_to_string(&file).unwrap(), facade);
    }
}

//! Native, service-owned layout used by new Rust projects. Registered legacy
//! projects keep their existing source paths and runtime adapters.
use super::*;

/// Formats only the source about to be generated; rustfmt never opens child
/// modules or traverses dependencies. Missing toolchains do not block init.
pub(super) fn format_source(path: &Path, source: &str) -> String {
    if path.extension().and_then(|extension| extension.to_str()) != Some("rs")
        || std::env::var_os("FORKLAUNCH_SKIP_FORMAT").is_some()
    {
        return source.to_owned();
    }
    match rustfmt_source(path, source) {
        Ok(formatted) => formatted,
        Err(_) => {
            static WARNED: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!(
                    "rustfmt could not format generated Rust source; leaving it unformatted. Install the Rust rustfmt component and run the generated project's `make fmt` task."
                );
            }
            source.to_owned()
        }
    }
}

pub(super) fn conversion_source(path: &Path, source: &str) -> String {
    if path.extension().and_then(|value| value.to_str()) == Some("rs") {
        rustfmt_source(path, source)
            .unwrap_or_else(|_| source.to_owned())
            .trim()
            .to_owned()
    } else {
        source.trim().to_owned()
    }
}

fn rustfmt_source(path: &Path, source: &str) -> Result<String> {
    use std::process::{Command, Stdio};
    let config = path
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .find_map(|directory| {
            ["rustfmt.toml", ".rustfmt.toml"]
                .into_iter()
                .map(|name| directory.join(name))
                .find(|candidate| candidate.is_file())
        });
    let mut command = Command::new("rustfmt");
    command.args(["--edition", "2024", "--emit", "stdout"]);
    if let Some(config) = config {
        command
            .arg("--config-path")
            .arg(config)
            .args(["--config", "skip_children=true"]);
    } else {
        command.args([
            "--config",
            "skip_children=true,max_width=100,use_small_heuristics=Max",
        ]);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("rustfmt stdin unavailable")?
        .write_all(source.as_bytes())?;
    let output = child.wait_with_output()?;
    anyhow::ensure!(
        output.status.success(),
        "rustfmt could not format generated source"
    );
    Ok(String::from_utf8(output.stdout)?)
}

// HTML documentation is additive in 0.1.1. An explicitly selected older
// framework retains its available public OpenAPI helper. Applications without a
// selection get the current release; unknown local versions keep compatibility.
fn supports_native_docs(directory: &Path) -> bool {
    fn workspace_version(path: &Path) -> Option<String> {
        read_to_string(path).ok()
            .and_then(|source| toml::from_str::<Value>(&source).ok())
            .and_then(|manifest| manifest.get("workspace")?.get("package")?.get("version")?.as_str().map(str::to_owned))
    }
    let Some(config) = directory.ancestors().find_map(crate::core::forklaunch_state::framework) else {
        return true;
    };
    let version = if let Some(path) = config.get("path").and_then(Value::as_str) {
        workspace_version(&Path::new(path).join("Cargo.toml"))
    } else {
        config.get("version").and_then(Value::as_str).map(str::to_owned)
    };
    let Some(version) = version else { return false; };
    let version = version.split('+').next().unwrap_or(&version);
    let (release, prerelease) = version.split_once('-').map_or((version, false), |(release, _)| (release, true));
    let Ok(parts) = release.split('.').map(str::parse::<u64>).collect::<Result<Vec<_>, _>>() else { return false; };
    parts.len() == 3 && (parts.as_slice() > [0, 1, 1].as_slice()
        || (parts == [0, 1, 1] && !prerelease))
}

pub(super) fn service_templates(
    dir: &Path,
    name: &str,
    description: &str,
) -> Vec<RenderedTemplate> {
    let data = rust_crate_template_data(name, description, true);
    let native_docs = supports_native_docs(dir);
    let files = [
        "lib.rs",
        "main.rs",
        "bootstrapper.rs",
        "application.rs",
        "registrations.rs",
        "server.rs",
        "api/mod.rs",
        "api/controllers/mod.rs",
        "api/controllers/health.rs",
        "api/policy.rs",
        "api/routes/mod.rs",
        "api/routes/health.rs",
        "api/contracts/mod.rs",
        "api/contracts/health.rs",
        "domain/mod.rs",
        "domain/mappers/mod.rs",
        "domain/services/mod.rs",
        "domain/services/health.rs",
        "persistence/mod.rs",
        "persistence/entities/mod.rs",
        "persistence/compliance_schema.rs",
        "sdk.rs",
        "build.rs",
        "build_assets.rs",
        "__test__/http.rs",
        "migrations/README.md",
    ];
    let mut output: Vec<_> = files
        .into_iter()
        .map(|file| RenderedTemplate {
            path: dir.join(file),
            content: {
                let source = render_rust_template(&format!("rust/project/native-service/{file}"), &data);
                if file == "application.rs" && native_docs {
                    source.replace("native::with_openapi(", "native::with_docs(")
                } else { source }
            },
            context: None,
        })
        .collect();
    let mut cargo = service_cargo_toml(name, description)
        .replace("path = \"src/main.rs\"", "path = \"main.rs\"");
    cargo = cargo.replace("[package]", "[package]\nautotests = false");
    cargo = cargo.replace("[dependencies]", &format!("[lib]\npath = \"lib.rs\"\n\n[[test]]\nname = \"http\"\npath = \"__test__/http.rs\"\n\n[dependencies]\ncontracts = {{ package = \"{name}-contracts\", path = \"domain\" }}"));
    cargo = cargo.replace("[build-dependencies]", &format!("[build-dependencies]\ncontracts = {{ package = \"{name}-contracts\", path = \"domain\" }}"));
    cargo = cargo.replace(
        "features = [\"mcp\"]",
        "default-features = false, features = [\"native-server\", \"native-jwt\"]",
    );
    cargo = cargo
        .lines()
        .filter(|line| !line.starts_with("forklaunch-common ="))
        .collect::<Vec<_>>()
        .join("\n");
    let host = format!(
        "[package]\nname = \"{name}-contracts\"\nversion.workspace = true\nedition.workspace = true\nlicense.workspace = true\n[lib]\npath = \"../sdk.rs\"\n[dependencies]\nforklaunch-core = {{ path = \"../../../.forklaunch/framework-rs/crates/core\", features = [\"typed-schema\"] }}\n"
    );
    let schemas = format!(
        "{}\n{}",
        render_static_rust_template("rust/project/service/src/contracts/models.rs"),
        // The schema half of the health contract; its imports serve `contract()`.
        render_static_rust_template("rust/project/service/src/contracts/health.rs")
            .split("pub fn contract()")
            .next()
            .unwrap()
            .replace("use forklaunch_core::contract::{BodyContract, RouteContract};\n", "")
    );
    for (file, content) in [
        ("Cargo.toml", cargo),
        ("domain/Cargo.toml", host),
        (
            "domain/Makefile",
            makefile(&format!("{name}-contracts"), false),
        ),
        ("domain/schemas/mod.rs", schemas),
        ("config.rs", service_config_rs(name)),
        ("error.rs", service_error_rs()),
        ("persistence/repository.rs", service_persistence_rs()),
        ("Makefile", makefile(name, true)),
    ] {
        output.push(RenderedTemplate {
            path: dir.join(file),
            content,
            context: None,
        });
    }
    contracts_layout::split(dir, name, output)
}

pub(super) fn flatten(
    dir: &Path,
    name: &str,
    kind: RustProjectKind,
    templates: Vec<RenderedTemplate>,
) -> Vec<RenderedTemplate> {
    let mut output = Vec::new();
    for mut template in templates {
        let relative = template
            .path
            .strip_prefix(dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let relative = relative.strip_prefix("src/").unwrap_or(&relative);
        let relative = match relative {
            "config/mod.rs" => "config.rs",
            "error/mod.rs" => "error.rs",
            other => other,
        }
        .to_owned();
        template.path = dir.join(&relative);
        if relative == "Cargo.toml" {
            template.content = template
                .content
                .replace("path = \"src/main.rs\"", "path = \"main.rs\"");
            template.content.push_str("\n[lib]\npath = \"lib.rs\"\n");
        }
        if relative == "main.rs" {
            let content = template
                .content
                .replace("mod signals;", "use crate::signals;")
                .replace(
                    &format!("use {}::", name.to_case(Case::Snake)),
                    "use crate::",
                )
                .replace("async fn run()", "async fn start_runtime()")
                .replace("fn run()", "fn start_runtime()")
                .replace("let result = run()", "let result = start_runtime()")
                .replace("#[tokio::main]\nasync fn main()", "pub async fn run()");
            template.content = format!(
                "use std::error::Error;\n\n#[tokio::main]\nasync fn main() -> Result<(), Box<dyn Error>> {{\n    {}::bootstrapper::run().await\n}}\n",
                name.to_case(Case::Snake)
            );
            output.push(RenderedTemplate {
                path: dir.join("bootstrapper.rs"),
                content,
                context: None,
            });
        }
        if relative == "lib.rs" && kind == RustProjectKind::Library {
            if let Some(split) = template.content.find("pub const LIBRARY_NAME") {
                let metadata = template.content[split..].to_owned();
                template.content = format!(
                    "{}\nmod metadata;\npub use metadata::{{LIBRARY_NAME, library_name}};\n",
                    &template.content[..split]
                );
                output.push(RenderedTemplate {
                    path: dir.join("metadata.rs"),
                    content: metadata,
                    context: None,
                });
            }
        }
        if relative == "lib.rs" && kind == RustProjectKind::Worker {
            // Keep the established worker adapters but put implementation in its
            // named module; lib.rs remains an explicit export surface.
            let split = template
                .content
                .find("\nuse ")
                .unwrap_or(template.content.len());
            let implementation = template.content[split..]
                .trim_start()
                .replace("use config::", "use crate::config::")
                .replace("use error::", "use crate::error::")
                .replace("use jobs::", "use crate::jobs::")
                .replace("jobs::process_next", "crate::jobs::process_next");
            template.content = format!(
                "{}\npub mod bootstrapper;\npub mod registrations;\npub mod worker;\npub use worker::{{WORKER_NAME, WORKER_VERSION, WorkerContext, build_context, run_once, contract_registry, write_contract_artifacts}};\n",
                &template.content[..split]
            );
            if templates_have_signals(dir) {
                template.content.push_str("pub mod signals;\n");
            }
            output.push(RenderedTemplate {
                path: dir.join("worker.rs"),
                content: implementation,
                context: None,
            });
            output.push(RenderedTemplate {
                path: dir.join("registrations.rs"),
                content: "pub use crate::worker::{WorkerContext, build_context};\n".into(),
                context: None,
            });
        }
        output.push(template);
    }
    // A backend template supplies signals, unlike the simple worker.
    if output
        .iter()
        .any(|file| file.path == dir.join("signals.rs"))
    {
        if let Some(lib) = output
            .iter_mut()
            .find(|file| file.path == dir.join("lib.rs"))
        {
            if !lib.content.contains("pub mod signals;") {
                lib.content.push_str("pub mod signals;\n");
            }
        }
    }
    if kind == RustProjectKind::Worker
        && output
            .iter()
            .any(|file| file.path == dir.join("signals.rs"))
    {
        // Separate lifecycle from queue processing. Keep backend-native control,
        // retention and retry behavior intact while exporting only the public API.
        if let Some(index) = output
            .iter()
            .position(|file| file.path == dir.join("bootstrapper.rs"))
        {
            let source = output[index].content.clone();
            if let (Some(entry), Some(runtime)) = (
                source.find("pub async fn run()"),
                source.find("async fn start_runtime()"),
            ) {
                // Telemetry moves with the lifecycle; queue processing keeps the rest.
                let header = source[..entry]
                    .replace(", write_contract_artifacts", "")
                    .replace(", telemetry::otlp::TelemetryRuntime", "")
                    .replace("use forklaunch_core::telemetry::otlp::TelemetryRuntime;\n", "");
                let lifecycle =
                    source[entry..runtime].replace("start_runtime()", "crate::worker::run()");
                let runtime =
                    source[runtime..].replacen("async fn start_runtime()", "pub async fn run()", 1);
                let std_imports = match (lifecycle.contains("env::"), lifecycle.contains("dyn Error")) {
                    (true, true) => "use std::{env, error::Error};\n",
                    (true, false) => "use std::env;\n",
                    (false, true) => "use std::error::Error;\n",
                    (false, false) => "",
                };
                let core = match (lifecycle.contains("config::"), lifecycle.contains("TelemetryRuntime::")) {
                    (true, true) => "use forklaunch_core::{config, telemetry::otlp::TelemetryRuntime};\n",
                    (true, false) => "use forklaunch_core::config;\n",
                    (false, true) => "use forklaunch_core::telemetry::otlp::TelemetryRuntime;\n",
                    (false, false) => "",
                };
                output[index].content = format!(
                    "{std_imports}{core}use crate::{{WORKER_NAME, write_contract_artifacts}};\n{lifecycle}"
                );
                let worker_index = output
                    .iter()
                    .position(|file| file.path == dir.join("worker.rs"))
                    .unwrap();
                let registrations = output[worker_index].content.clone();
                output[worker_index].content = format!("{header}{runtime}");
                let registrations_index = output
                    .iter()
                    .position(|file| file.path == dir.join("registrations.rs"))
                    .unwrap();
                output[registrations_index].content = registrations;
                let lib = output
                    .iter_mut()
                    .find(|file| file.path == dir.join("lib.rs"))
                    .unwrap();
                lib.content = lib
                    .content
                    .replace("pub use worker::{", "pub use registrations::{");
            }
        }
    }
    if kind == RustProjectKind::Worker {
        let has_handler = output
            .iter()
            .any(|file| file.path == dir.join("handler.rs"));
        for file in &mut output {
            if file.path == dir.join("jobs/mod.rs") {
                file.path = dir.join("domain/services/jobs.rs");
            } else if file.path == dir.join("event_contract.rs") {
                file.path = dir.join("domain/schemas/event_contract.rs");
            } else if file.path == dir.join("events.rs") {
                file.path = dir.join("domain/schemas/events.rs");
            } else if file.path == dir.join("handler.rs") {
                file.path = dir.join("domain/services/handler.rs");
            } else if file.path == dir.join("error.rs") {
                file.path = dir.join("domain/error.rs");
            } else if file.path == dir.join("lib.rs") {
                file.content = file
                    .content
                    .replace(
                        "pub mod jobs;",
                        "pub mod domain;\npub use domain::services::jobs;",
                    )
                    .replace("pub mod error;", "pub use domain::error;")
                    .replace("pub mod handler;", "pub use domain::services::handler;")
                    .replace(
                        "pub mod event_contract;",
                        "pub use domain::schemas::event_contract;",
                    )
                    .replace("pub mod events;", "pub use domain::schemas::events;");
            }
        }
        for (path, content) in [
            ("domain/mod.rs", "pub mod services;\npub mod error;\npub mod schemas;\npub mod mappers;\n".into()),
            ("domain/services/mod.rs", if has_handler { "pub mod jobs;\npub mod handler;\n" } else { "pub mod jobs;\n" }.into()),
            ("domain/schemas/mod.rs", if has_handler { "pub mod event_contract;\npub mod events;\n" } else { "//! Typed job contracts shared with producers belong here.\n" }.into()),
            ("domain/mappers/mod.rs", "//! Map transport job payloads to domain inputs here.\n".into()),
            ("persistence/mod.rs", "//! Service-owned queue persistence extensions.\n".into()),
            ("migrations/README.md", "# Worker migrations\n\nDurable adapter queue migrations run with `cargo run -- --migrate` from this project. Queue implementation and schema ownership remain in the selected ForkLaunch adapter. Add application-specific migrations here.\n".into()),
        ] { output.push(RenderedTemplate { path: dir.join(path), content, context: None }); }
    }
    output.push(RenderedTemplate {
        path: dir.join("Makefile"),
        content: makefile(name, kind != RustProjectKind::Library),
        context: None,
    });
    output
}

fn templates_have_signals(dir: &Path) -> bool {
    dir.join("signals.rs").exists()
}

pub(super) fn makefile(name: &str, executable: bool) -> String {
    let mut tasks = format!(
        "CARGO ?= cargo\nPACKAGE := {name}\n.PHONY: fmt fmt-check lint lint-fix check test build docs clean run dev\nfmt:\n\t$(CARGO) fmt --package $(PACKAGE)\nfmt-check:\n\t$(CARGO) fmt --package $(PACKAGE) -- --check\nlint:\n\t$(CARGO) clippy --package $(PACKAGE) --all-targets --no-deps -- -D warnings\nlint-fix:\n\t$(CARGO) clippy --fix --allow-no-vcs --package $(PACKAGE) --all-targets --no-deps -- -D warnings\ncheck:\n\t$(CARGO) check --package $(PACKAGE) --all-targets\ntest:\n\t$(CARGO) test --package $(PACKAGE)\nbuild:\n\t$(CARGO) build --package $(PACKAGE)\ndocs:\n\t$(CARGO) doc --package $(PACKAGE) --no-deps\nclean:\n\t$(CARGO) clean --package $(PACKAGE)\n"
    );
    if executable {
        tasks.push_str("run:\n\t$(CARGO) run --package $(PACKAGE)\ndev:\n\t$(CARGO) watch -x 'run --package $(PACKAGE)'\n");
    }
    tasks
}

pub(super) fn generate_router(
    dir: &Path,
    name: &str,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    let module = name.to_case(Case::Snake);
    let response = format!("{}Response", name.to_case(Case::Pascal));
    let route = escape_rust_string(name);
    let title = escape_rust_string(&name.to_case(Case::Title));
    let postgres = dir.join("persistence/database.rs").is_file();
    if postgres {
        let controller = contracts_layout::source(dir, &format!("api/controllers/{module}.rs"));
        if let Ok(source) = read_to_string(&controller) {
            let compact: String = source.chars().filter(|character| !character.is_whitespace()).collect();
            if compact.contains("state.repository.list().map_err(|_|EndpointError::InvalidResponse)?") {
                bail!(
                    "Existing PostgreSQL router {} calls the in-memory repository list method. Its source was preserved. Update the handler and response schema to a tenant-scoped async count, or implement an explicit audited projection for protected records before reinitializing this router.",
                    controller.display()
                );
            }
        }
    }
    let (operation, binding, label, description, records_field, query, response_fields) = if postgres {
        (
            "summary", "Summary", "Summarize", "Summarize tenant-scoped",
            "",
            "    // Protected records require an application-owned projection with explicit audited disclosure.\n    // This endpoint exposes only the current tenant's count.\n    let total = state.repository.count().await.map_err(|_| EndpointError::Unavailable)?;\n",
            "",
        )
    } else {
        (
            "list", "List", "List", "List",
            ", pub records: Vec<super::Record>",
            "    let records = state.repository.list().map_err(|_| EndpointError::InvalidResponse)?;\n    let total = records.len();\n",
            ", records",
        )
    };
    let mut files = vec![
        (
            format!("domain/schemas/{module}.rs"),
            format!(
                "#[forklaunch_core::contract::schema]\n#[derive(Debug, Clone)]\npub struct {response} {{ pub route: String, pub summary: super::RecordSummary{records_field} }}\n"
            ),
        ),
        (
            format!("api/controllers/{module}.rs"),
            format!(
                "use forklaunch_axum::native::State;\nuse forklaunch_axum::native::EndpointError;\nuse crate::registrations::AppState;\nuse crate::domain::schemas::RecordSummary;\nuse crate::domain::schemas::{module}::{response};\npub async fn handle(State(state): State<AppState>) -> Result<{response}, EndpointError> {{\n{query}    let summary = RecordSummary {{ service: state.config.name.clone(), total }};\n    Ok({response} {{ route: \"{route}\".into(), summary{response_fields} }})\n}}\n"
            ),
        ),
        (
            format!("api/routes/{module}.rs"),
            format!(
                "use forklaunch_axum::native::ContractRouter;\nuse crate::{{registrations::AppState, api::controllers::{module}::handle}};\npub fn mount(router: ContractRouter<AppState>) -> Result<ContractRouter<AppState>, String> {{ contracts::{module}::Bindings {{ {operation}: contracts::{module}::{binding}::bind(handle) }}.mount(router) }}\n"
            ),
        ),
        (
            format!("api/contracts/{module}.rs"),
            format!(
                "#[forklaunch_core::contract::api]\nmod operations {{\nuse forklaunch_core::contract::{{HttpMethod, endpoint::{{Endpoint, NoBody, JsonBody}}}};\npub fn {operation}() -> Endpoint<NoBody, JsonBody<crate::schemas::{module}::{response}>> {{\n    Endpoint::new(\"{label} {title}\", \"{description} {route} records\", HttpMethod::Get, \"/{route}\", 200).expect(\"valid {operation} endpoint\")\n}}\n}}\npub use operations::{{binding_manifest, Bindings, {binding}, {operation} as endpoint}};\npub fn contract() -> forklaunch_core::contract::RouteContract {{ endpoint().contract() }}\n"
            ),
        ),
    ];
    if contracts_layout::directory(dir).is_some_and(|path| path == contracts_layout::new_host_directory(dir)) {
        let contract_path=format!("api/contracts/{module}.rs");
        let declaration=contracts_layout::inline_contract(&files.iter().find(|(path,_)|path==&contract_path).unwrap().1);
        files.retain(|(path,_)|path!=&contract_path);
        for (path,content) in &mut files {
            if path==&format!("api/controllers/{module}.rs") { content.push_str(&format!("\n{declaration}")); }
            if path==&format!("api/routes/{module}.rs") { *content=content.replace(&format!("contracts::{module}::"),&format!("crate::api::controllers::{module}::contract::")); }
        }
    }
    for (path, content) in files {
        let path = contracts_layout::source(dir, &path);
        if !path.exists() {
            write_text_file(&path, &content, dryrun, stdout)?;
        }
    }
    for path in ["domain/schemas/mod.rs", "api/controllers/mod.rs"] {
        let file = contracts_layout::source(dir, path);
        let mut content = read_to_string(&file)?;
        let declaration = format!("pub mod {module};");
        if !content.lines().any(|line| line == declaration) {
            content.push_str(&format!("\n{declaration}\n"));
        }
        write_text_file(&file, &content, dryrun, stdout)?;
    }
    rewrite_router_exports(dir, Some(&module), dryrun, stdout)
}

fn rewrite_router_exports(
    dir: &Path,
    extra: Option<&str>,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    let mut modules = rust_route_modules(&dir.join("api/routes"))?;
    if let Some(module) = extra {
        modules.push(module.to_owned());
    }
    modules.sort();
    modules.dedup();
    let declarations: String = modules
        .iter()
        .map(|module| format!("pub mod {module};\n"))
        .collect();
    let merges: String = modules
        .iter()
        .map(|module| format!("let router = {module}::mount(router)?;\n"))
        .collect();
    let capability_mount = if read_to_string(dir.join("api/routes/mod.rs")).is_ok_and(|source| source.contains(".with_managed_capabilities()")) {
        ".with_managed_capabilities()?"
    } else { "" };
    let capability_import = if capability_mount.is_empty() { "" } else {
        "use crate::capabilities::ManagedCapabilitiesRouterExt;\n"
    };
    let mut routes = format!(
        "{declarations}\n{capability_import}use forklaunch_axum::native::Router;\nuse crate::registrations::AppState;\npub fn router() -> Result<Router<AppState>, String> {{ let router = forklaunch_axum::native::ContractRouter::new(contracts::registry()); {merges} router{capability_mount}.finish() }}\n"
    );
    // Agents bind their routes on the same contract router.
    if dir.join("agent/mod.rs").is_file() {
        routes = super::agent::routes_with_agent(&routes)?;
    }
    write_text_file(&dir.join("api/routes/mod.rs"), &routes, dryrun, stdout)?;
    if contracts_layout::directory(dir).is_some_and(|path|path==contracts_layout::new_host_directory(dir)) {
        let cargo:Value=toml::from_str(&read_to_string(dir.join("Cargo.toml"))?)?;
        let name=cargo["package"]["name"].as_str().context("package name")?;
        if dir.join("api/controllers/platform_events.rs").exists() { modules.push("platform_events".into()); }
        let mut host=contracts_layout::host_source(name,&modules);
        if dir.join("domain/schemas/jobs.rs").is_file() {host.push_str("\n#[path=\"../../domain/schemas/jobs.rs\"] pub mod jobs;\n");}
        if dir.join("agent/mod.rs").is_file() { host = super::agent::host_with_agent(&host)?; }
        return write_text_file(&contracts_layout::new_host_directory(dir).join("lib.rs"),&host,dryrun,stdout);
    }
    let contracts: String = modules
        .iter()
        .map(|module| format!("{module}::contract(),"))
        .collect();
    let bindings: String = modules.iter().map(|module| format!("bindings.extend({module}::binding_manifest()?);\n")).collect();
    // Preserve service identity from the existing registry rather than assuming
    // the package and directory still have their original names.
    let registry = read_to_string(contracts_layout::source(dir, "api/contracts/mod.rs"))?;
    let registry = registry
        .split("pub fn registry()")
        .nth(1)
        .context("Expected service registry")?
        .split("pub fn binding_manifest()")
        .next()
        .expect("registry prefix exists");
    write_text_file(
        &contracts_layout::source(dir, "api/contracts/mod.rs"),
        &format!(
            "{declarations}\npub fn routes() -> Vec<forklaunch_core::contract::RouteContract> {{ vec![{contracts}] }}\npub fn binding_manifest() -> Result<Vec<forklaunch_core::contract::assembly::BindingDeclaration>, String> {{ let mut bindings = Vec::new(); {bindings} Ok(bindings) }}\npub fn registry(){registry}"
        ),
        dryrun,
        stdout,
    )
}

pub(super) fn change_router(
    dir: &Path,
    old: &str,
    new: Option<&str>,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    let old_module = old.to_case(Case::Snake);
    let new_module = new.map(|name| name.to_case(Case::Snake));
    if dryrun {
        writeln!(stdout, "Would update native router {old}")?;
        return Ok(());
    }
    for section in [
        "api/controllers",
        "api/routes",
        "api/contracts",
        "domain/schemas",
    ] {
        let path = contracts_layout::source(dir, &format!("{section}/{old_module}.rs"));
        if !path.exists() {
            continue;
        }
        if let Some(new) = new {
            let module = new_module.as_ref().unwrap();
            let source = read_to_string(&path)?
                .replace(
                    &format!("List {}", old.to_case(Case::Title)),
                    &format!("List {}", new.to_case(Case::Title)),
                )
                .replace(
                    &format!("Summarize {}", old.to_case(Case::Title)),
                    &format!("Summarize {}", new.to_case(Case::Title)),
                )
                .replace(
                    &format!("Summarize tenant-scoped {old} records"),
                    &format!("Summarize tenant-scoped {new} records"),
                )
                .replace(
                    &format!("{}Response", old.to_case(Case::Pascal)),
                    &format!("{}Response", new.to_case(Case::Pascal)),
                )
                .replace(&format!("::{old_module}::"), &format!("::{module}::"))
                .replace(
                    &format!("\"{}\"", escape_rust_string(old)),
                    &format!("\"{}\"", escape_rust_string(new)),
                )
                .replace(
                    &format!("\"/{}\"", escape_rust_string(old)),
                    &format!("\"/{}\"", escape_rust_string(new)),
                );
            let destination = contracts_layout::source(dir, &format!("{section}/{module}.rs"));
            if destination.exists() && destination != path {
                bail!(
                    "Router destination already exists: {}",
                    destination.display()
                );
            }
            write(&destination, source)?;
            if path != destination {
                remove_file(path)?;
            }
        } else {
            remove_file(path)?;
        }
    }
    for section in ["api/controllers", "domain/schemas"] {
        let path = contracts_layout::source(dir, &format!("{section}/mod.rs"));
        let declaration = format!("pub mod {old_module};");
        let replacement = new_module
            .as_ref()
            .map(|name| format!("pub mod {name};"))
            .unwrap_or_default();
        write(
            &path,
            read_to_string(&path)?.replace(&declaration, &replacement),
        )?;
    }
    rewrite_router_exports(dir, None, false, stdout)
}

pub(super) fn framework_paths(
    root: &Path,
    mut templates: Vec<RenderedTemplate>,
) -> Vec<RenderedTemplate> {
    let paths = regex::Regex::new(r#"path\s*=\s*"[^"]*(\.forklaunch/framework-rs|\.forklaunch/blueprint-rs)(/[^"]*)?""#)
        .expect("fixed dependency path pattern");
    for template in &mut templates {
        if template
            .path
            .file_name()
            .is_some_and(|name| name == "Cargo.toml")
        {
            let directory = template.path.parent().expect("Cargo manifest directory");
            template.content = paths
                .replace_all(&template.content, |captures: &regex::Captures<'_>| {
                    let base = &captures[1];
                    let suffix = captures
                        .get(2)
                        .map_or("", |suffix| suffix.as_str())
                        .trim_start_matches('/');
                    let relative = crate::core::relative_path::get_relative_path(
                        directory,
                        &root.join(base).join(suffix),
                    );
                    format!("path = \"{}\"", relative.display())
                })
                .into_owned();
        }
    }
    templates
}

pub(super) fn replace_namespace(source: &str, old: &str, new: &str) -> String {
    let namespace = regex::Regex::new(&format!(r"(^|[^:\w]){}::", regex::escape(old)))
        .expect("escaped Rust namespace");
    let local_group = regex::Regex::new(r"\b(?:crate|self|super)\s*::\s*\{").unwrap();
    let mut output = String::new();
    let mut offset = 0;
    while let Some(group) = local_group.find(&source[offset..]) {
        let start = offset + group.start();
        output.push_str(&namespace.replace_all(&source[offset..start], format!("${{1}}{new}::")));
        let mut end = offset + group.end();
        let mut depth = 1usize;
        for (index, character) in source[end..].char_indices() {
            if character == '{' {
                depth += 1;
            }
            if character == '}' {
                depth -= 1;
            }
            if depth == 0 {
                end += index + 1;
                break;
            }
        }
        output.push_str(&source[start..end]);
        offset = end;
    }
    output.push_str(&namespace.replace_all(&source[offset..], format!("${{1}}{new}::")));
    output
}

pub(super) fn rename_project(dir: &Path, old: &str, new: &str) -> Result<()> {
    let old_module = old.to_case(Case::Snake);
    let new_module = new.to_case(Case::Snake);
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rename_project(&path, old, new)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "rs" | "toml"))
            || path.file_name().is_some_and(|file| file == "Makefile")
        {
            let source = read_to_string(&path)?;
            let changed = replace_namespace(&source, &old_module, &new_module)
                .replace(&format!("\"{old}\""), &format!("\"{new}\""))
                .replace(
                    &format!("\"{old}-contracts\""),
                    &format!("\"{new}-contracts\""),
                )
                .replace(&format!("\"{old}-worker\""), &format!("\"{new}-worker\""))
                .replace(&format!("--bin {old}-worker"), &format!("--bin {new}-worker"))
                .replace(&format!("../{old}-contracts"), &format!("../{new}-contracts"))
                .replace(&format!("rust/{old}.rs\""), &format!("rust/{new}.rs\""))
                .replace(&format!("\"{old}-sdk\""), &format!("\"{new}-sdk\""))
                .replace(
                    &format!("typescript/{old}\""),
                    &format!("typescript/{new}\""),
                )
                .replace(
                    &format!("PACKAGE := {old}-contracts\n"),
                    &format!("PACKAGE := {new}-contracts\n"),
                )
                .replace(
                    &format!("PACKAGE := {old}\n"),
                    &format!("PACKAGE := {new}\n"),
                )
                .replace(&format!("PACKAGE := {old}-sdk\n"), &format!("PACKAGE := {new}-sdk\n"))
                .replace(&format!("\"{old} listening on "), &format!("\"{new} listening on "));
            if changed != source {
                write(path, changed)?;
            }
        } else if path.file_name().is_some_and(|file| file == "README.md") {
            // Generated project documentation names the project's binaries.
            let source = read_to_string(&path)?;
            let changed = source
                .replacen(&format!("# {old} "), &format!("# {new} "), 1)
                .replace(&format!("--bin {old}`"), &format!("--bin {new}`"))
                .replace(&format!("--bin {old}-worker`"), &format!("--bin {new}-worker`"))
                .replace(&format!("-p {old}`"), &format!("-p {new}`"));
            if changed != source {
                write(path, changed)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_updates_startup_message_and_sdk_package_task() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join("sdk/rust")).unwrap();
        write(root.path().join("bootstrapper.rs"), "println!(\"notes listening on http://{}\", address);\n").unwrap();
        write(root.path().join("sdk/rust/Makefile"), "PACKAGE := notes-sdk\n").unwrap();
        write(root.path().join("README.md"), "# notes worker\n\nRun `cargo run --bin notes` and `cargo run --bin notes-worker`; notes stay.\n").unwrap();
        rename_project(root.path(), "notes", "records").unwrap();
        assert_eq!(read_to_string(root.path().join("README.md")).unwrap(), "# records worker\n\nRun `cargo run --bin records` and `cargo run --bin records-worker`; notes stay.\n");
        assert_eq!(read_to_string(root.path().join("bootstrapper.rs")).unwrap(), "println!(\"records listening on http://{}\", address);\n");
        assert_eq!(read_to_string(root.path().join("sdk/rust/Makefile")).unwrap(), "PACKAGE := records-sdk\n");
    }

    #[test]
    fn rename_updates_host_contract_package_tasks() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join("domain")).unwrap();
        write(
            root.path().join("domain/Makefile"),
            makefile("notes-contracts", false),
        )
        .unwrap();
        rename_project(root.path(), "notes", "records").unwrap();
        assert!(
            read_to_string(root.path().join("domain/Makefile"))
                .unwrap()
                .contains("PACKAGE := records-contracts\n")
        );
    }

    #[test]
    fn rename_keeps_local_group_imports_when_service_matches_module_name() {
        assert_eq!(
            replace_namespace(
                "use crate::{api::routes, self_type};\nuse self::{api::models};\napi::bootstrapper::run();",
                "api",
                "renamed"
            ),
            "use crate::{api::routes, self_type};\nuse self::{api::models};\nrenamed::bootstrapper::run();"
        );
    }

    fn materialize(root: &Path, files: Vec<RenderedTemplate>) {
        for file in files {
            create_dir_all(file.path.parent().unwrap()).unwrap();
            write(file.path, file.content).unwrap();
        }
        assert!(root.join("lib.rs").exists());
    }

    #[test]
    fn framework_dependencies_follow_registered_project_depth() {
        let root = tempfile::tempdir().unwrap();
        let files = framework_paths(root.path(), vec![RenderedTemplate {
            path: root.path().join("workers/team/jobs/Cargo.toml"),
            content: "[dependencies]\nforklaunch-core = { path = \"../../.forklaunch/framework-rs/crates/core\" }\n".into(),
            context: None,
        }]);
        assert!(
            files[0]
                .content
                .contains("\"../../../.forklaunch/framework-rs/crates/core\"")
        );
    }

    #[test]
    fn generated_source_formatting_is_idempotent_and_does_not_visit_modules() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path().join("rustfmt.toml"),
            "max_width = 100\nuse_small_heuristics = \"Max\"\n",
        )
        .unwrap();
        let path = root.path().join("src/notes/lib.rs");
        let source = "pub mod application_owned;pub fn answer()->u32{42}";
        let formatted = match rustfmt_source(&path, source) {
            Ok(formatted) => formatted,
            Err(_) => {
                eprintln!("rustfmt unavailable; skipping external formatter probe");
                return;
            }
        };
        assert!(formatted.contains("pub mod application_owned;\n"));
        assert!(formatted.contains("pub fn answer() -> u32 {"));
        assert_eq!(rustfmt_source(&path, &formatted).unwrap(), formatted);
        assert!(
            !root.path().join("src").exists(),
            "formatting must operate on memory only"
        );
    }

    #[test]
    fn project_tasks_limit_linting_to_owned_code_and_support_fresh_repositories() {
        for executable in [false, true] {
            let make = makefile("notes", executable);
            assert!(
                make.contains("clippy --package $(PACKAGE) --all-targets --no-deps -- -D warnings")
            );
            assert!(make.contains("clippy --fix --allow-no-vcs --package $(PACKAGE) --all-targets --no-deps -- -D warnings"));
            assert!(make.contains("fmt --package $(PACKAGE)"));
            assert!(!make.contains("fmt --all"));
        }
        let root = tempfile::tempdir().unwrap();
        let files = mixed_typescript_sdk_templates(root.path(), "notes", &default_sdk_targets());
        let package = files
            .iter()
            .find(|file| file.path == root.path().join("package.json"))
            .unwrap();
        let package: serde_json::Value = serde_json::from_str(&package.content).unwrap();
        assert_eq!(package["scripts"]["rust:fmt"], "cargo fmt");
        assert_eq!(
            package["scripts"]["sdk:generate"],
            "forklaunch sdk generate --target all --from contract"
        );
        assert_eq!(
            package["scripts"]["rust:lint"],
            "cargo clippy --workspace --all-targets --no-deps"
        );
    }

    #[test]
    fn namespace_relocation_preserves_framework_dependency_names() {
        let source = "use iam_base::service_info; use forklaunch_iam_base::postgres::PostgresIamService; let _ = iam_base::SERVICE_NAME;";
        let relocated = replace_namespace(source, "iam_base", "crate");
        assert!(relocated.contains("use crate::service_info;"));
        assert!(relocated.contains("forklaunch_iam_base::postgres::PostgresIamService"));
        assert!(relocated.contains("crate::SERVICE_NAME"));
        assert!(!relocated.contains("forklaunch_crate"));
        let renamed = replace_namespace(source, "iam_base", "identity");
        assert!(renamed.contains("forklaunch_iam_base::postgres::PostgresIamService"));
        assert!(renamed.contains("identity::SERVICE_NAME"));
        assert_eq!(
            replace_namespace("crate::jobs::run(); jobs::start();", "jobs", "email_jobs"),
            "crate::jobs::run(); email_jobs::start();"
        );
    }

    #[test]
    fn generated_docs_follow_selected_framework_version_without_overwriting_sources() {
        for (version, local, expected) in [
            (None, false, "with_docs"),
            (Some("0.1.0"), false, "with_openapi"),
            (Some("0.1.1"), false, "with_docs"),
            (Some("0.1.10"), false, "with_docs"),
            (Some("0.10.0"), false, "with_docs"),
            (Some("0.1.0"), true, "with_openapi"),
            (Some("0.1.1"), true, "with_docs"),
            (Some("0.1.1-beta.1"), true, "with_openapi"),
            (Some("invalid"), true, "with_openapi"),
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path();
            if let Some(version) = version {
                create_dir_all(root.join(".forklaunch")).unwrap();
                let config = if local {
                    let source = root.join("custom-framework");
                    create_dir_all(&source).unwrap();
                    write(source.join("Cargo.toml"), format!("[workspace.package]\nversion={:?}\n", version)).unwrap();
                    format!("path={}\n", toml::Value::String(source.to_string_lossy().into_owned()))
                } else { format!("version={:?}\n", version) };
                { let manifest = root.join(".forklaunch/manifest.toml"); let prior = std::fs::read_to_string(&manifest).unwrap_or_default(); std::fs::write(&manifest, format!("{prior}\n[framework]\n{}\n", config)).unwrap(); }
            }
            let directory = root.join("modules/team/notes");
            let render = || service_templates(&directory, "notes", "Notes").into_iter()
                .find(|file| file.path == directory.join("application.rs")).unwrap().content;
            let source = render();
            assert!(source.contains(&format!("native::{expected}(")), "{version:?} local={local}");
            assert!(!directory.exists(), "Rendering must not create project files");
            create_dir_all(&directory).unwrap();
            let authored = "// Application-owned composition\npub const CUSTOM: bool = true;\n";
            write(directory.join("application.rs"), authored).unwrap();
            assert_eq!(render(), source);
            assert_eq!(read_to_string(directory.join("application.rs")).unwrap(), authored);
        }
    }

    #[test]
    fn selected_frameworks_keep_their_available_docs_api() {
        for local in [false, true] {
            for (version, expected) in [("0.1.0", "with_openapi"), ("0.1.1", "with_docs")] {
                let root = tempfile::tempdir().unwrap();
                create_dir_all(root.path().join(".forklaunch")).unwrap();
                let selection = if local {
                    let checkout = root.path().join("framework-rs");
                    create_dir_all(&checkout).unwrap();
                    write(checkout.join("Cargo.toml"), format!("[workspace.package]\nversion={version:?}\n")).unwrap();
                    format!("path = {:?}", checkout.to_string_lossy())
                } else {
                    format!("version = {version:?}")
                };
                write(root.path().join(".forklaunch/manifest.toml"), format!("[framework]\n{selection}\n")).unwrap();
                let directory = root.path().join("src/notes");
                let source = service_templates(&directory, "notes", "Notes").into_iter()
                    .find(|file| file.path == directory.join("application.rs")).unwrap().content;
                assert!(source.contains(&format!("native::{expected}(")), "local={local} {version}");
            }
        }
    }

    #[test]
    fn native_service_has_host_contract_boundary_and_secure_native_routes() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().join("notes");
        create_dir_all(&root).unwrap();
        materialize(
            root.as_path(),
            service_templates(root.as_path(), "notes", "Notes"),
        );
        let cargo: Value =
            toml::from_str(&read_to_string(root.as_path().join("Cargo.toml")).unwrap()).unwrap();
        assert_eq!(cargo["lib"]["path"].as_str(), Some("lib.rs"));
        assert_eq!(
            cargo["build-dependencies"]["contracts"]["path"].as_str(),
            Some(".forklaunch/contracts")
        );
        let host: Value =
            toml::from_str(&read_to_string(root.join(".forklaunch/contracts/Cargo.toml")).unwrap())
                .unwrap();
        assert_eq!(host["lib"]["path"].as_str(), Some("lib.rs"));
        assert_eq!(host["package"]["publish"].as_bool(), Some(false));
        assert!(!root.with_file_name("notes-contracts").exists());
        let host_source=read_to_string(root.join(".forklaunch/contracts/lib.rs")).unwrap();
        assert!(host_source.contains("../../domain/schemas/mod.rs"));
        assert!(host_source.contains("../../api/controllers/health.rs"));
        assert!(read_to_string(root.join("domain/schemas/mod.rs")).unwrap().contains("pub struct HealthResponse"));
        assert!(read_to_string(root.join("api/controllers/health.rs")).unwrap().contains("#[forklaunch_core::contract::api]"));
        assert!(!root.join("api/contracts").exists());
        let build = read_to_string(root.as_path().join("build.rs")).unwrap();
        let declaration = build
            .find("include_assets(&build_assets::declared_assets(&output)?")
            .unwrap();
        assert!(declaration < build.find("build_assets::emit(&output)?").unwrap());
        assert!(
            read_to_string(root.as_path().join("build_assets.rs"))
                .unwrap()
                .contains("pub fn declared_assets")
        );
        let server = read_to_string(root.as_path().join("server.rs")).unwrap();
        assert!(
            server.contains("application::http(state)?.serve(listener, shutdown).await")
        );
        let application = read_to_string(root.as_path().join("application.rs")).unwrap();
        assert!(application.contains("NativeApplication::new("));
        assert!(!server.contains("server::mount_http("));
        let bootstrap = read_to_string(root.as_path().join("bootstrapper.rs")).unwrap();
        assert!(bootstrap.contains("run_application("));
        assert!(!application.contains("pub async fn run"));
        assert!(!application.contains("std::env::var"));
        assert!(!application.contains("TcpListener"));
        assert!(application.contains("state.config.origin()"));
        assert!(bootstrap.contains("ServiceConfig::from_env()"));
        assert!(bootstrap.contains("registrations::register(config)"));
        assert!(bootstrap.contains("server::serve(state, listener, stopping.signal())"));
        assert!(!bootstrap.contains("application::run()"));
        assert!(!bootstrap.contains("axum::serve("));
        assert!(!server.contains("LocalRouter"));
        assert!(!root.as_path().join("src").exists());
        assert!(
            !read_to_string(root.as_path().join("lib.rs"))
                .unwrap()
                .contains("fn ")
        );
    }

    #[test]
    fn router_lifecycle_keeps_contract_and_native_routes_in_sync() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().join("notes");
        create_dir_all(&root).unwrap();
        materialize(
            root.as_path(),
            service_templates(root.as_path(), "notes", "Notes"),
        );
        let mut stdout = StandardStream::stdout(ColorChoice::Never);
        generate_router(root.as_path(), "comments", false, &mut stdout).unwrap();
        let contract = read_to_string(root.join("api/controllers/comments.rs")).unwrap();
        assert!(contract.contains("#[forklaunch_core::contract::api]"));
        assert!(read_to_string(root.as_path().join("api/routes/comments.rs")).unwrap().contains("List::bind(handle)"));
        let controller = root.as_path().join("api/controllers/comments.rs");
        let customized = read_to_string(&controller)
            .unwrap()
            .replace("Ok(", "// custom\n    Ok(");
        write(&controller, &customized).unwrap();
        generate_router(root.as_path(), "comments", false, &mut stdout).unwrap();
        assert_eq!(read_to_string(&controller).unwrap(), customized);
        change_router(root.as_path(), "comments", Some("replies"), false, &mut stdout).unwrap();
        assert!(
            read_to_string(root.as_path().join("api/controllers/replies.rs"))
                .unwrap()
                .contains("// custom")
        );
        assert!(
            read_to_string(contracts_layout::new_host_directory(&root).join("lib.rs"))
                .unwrap()
                .contains("replies::contract::registry()")
        );
        let aggregate = read_to_string(contracts_layout::new_host_directory(&root).join("lib.rs")).unwrap();
        assert_eq!(aggregate.matches("pub fn binding_manifest()").count(), 1);
        assert!(aggregate.contains("replies::contract::binding_manifest()?"));
        assert!(aggregate.contains("health::contract::binding_manifest()?"));
        assert!(!aggregate.contains("comments::contract::binding_manifest"));
        assert!(!controller.exists());
        change_router(root.as_path(), "replies", None, false, &mut stdout).unwrap();
        assert!(
            !read_to_string(root.as_path().join("api/routes/mod.rs"))
                .unwrap()
                .contains("replies")
        );
        assert!(
            !read_to_string(contracts_layout::new_host_directory(&root).join("lib.rs"))
                .unwrap()
                .contains("replies")
        );
    }

    #[test]
    fn postgres_router_uses_tenant_summary_without_projecting_protected_records() {
        {
            let root = tempfile::tempdir().unwrap();
            let templates = service_templates(root.path(), "notes", "Notes");
            materialize(
                root.path(),
                service_database::templates(root.path(), "notes", templates).unwrap(),
            );
            let mut stdout = StandardStream::stdout(ColorChoice::Never);
            generate_router(root.path(), "users", false, &mut stdout).unwrap();
            let controller = read_to_string(root.path().join("api/controllers/users.rs")).unwrap();
            let compact: String = controller.chars().filter(|character| !character.is_whitespace()).collect();
            assert!(compact.contains("state.repository.count().await"));
            assert!(controller.contains("EndpointError::Unavailable"));
            assert!(controller.contains("explicit audited disclosure"));
            assert!(controller.contains("pub fn summary()"));
            assert!(controller.contains("Summarize Users"));
            for forbidden in ["repository.list()", "protected_name(", ".reveal(", "records:"] {
                assert!(!compact.contains(forbidden), "{forbidden}");
            }
            let schema = read_to_string(root.path().join("domain/schemas/users.rs")).unwrap();
            assert!(schema.contains("pub summary: super::RecordSummary"));
            assert!(!schema.contains("records"));
            let route = read_to_string(root.path().join("api/routes/users.rs")).unwrap();
            let route: String = route.chars().filter(|character| !character.is_whitespace()).collect();
            assert!(route.contains("summary:crate::api::controllers::users::contract::Summary::bind(handle)"));
            let custom = format!("{controller}\n// Application-owned audited projection belongs here.\n");
            write(root.path().join("api/controllers/users.rs"), &custom).unwrap();
            generate_router(root.path(), "users", false, &mut stdout).unwrap();
            assert_eq!(read_to_string(root.path().join("api/controllers/users.rs")).unwrap(), custom);
            change_router(root.path(), "users", Some("members"), false, &mut stdout).unwrap();
            let renamed = read_to_string(root.path().join("api/controllers/members.rs")).unwrap();
            assert!(renamed.contains("Summarize Members"));
            assert!(renamed.contains("Summarize tenant-scoped members records"));
            assert!(renamed.contains("Application-owned audited projection"));
        }
    }

    #[test]
    fn postgres_router_rejects_legacy_memory_handler_without_overwriting_sources() {
        let root = tempfile::tempdir().unwrap();
        materialize(root.path(), service_templates(root.path(), "notes", "Notes"));
        let mut stdout = StandardStream::stdout(ColorChoice::Never);
        generate_router(root.path(), "users", false, &mut stdout).unwrap();
        let templates = service_templates(root.path(), "notes", "Notes");
        materialize(
            root.path(),
            service_database::templates(root.path(), "notes", templates).unwrap(),
        );
        let files = [
            "api/controllers/users.rs", "domain/schemas/users.rs", "api/routes/users.rs",
            "api/controllers/mod.rs", "domain/schemas/mod.rs", "api/routes/mod.rs",
            ".forklaunch/contracts/lib.rs",
        ];
        let before: Vec<_> = files.iter().map(|path| read_to_string(root.path().join(path)).unwrap()).collect();
        let error = generate_router(root.path(), "users", false, &mut stdout).unwrap_err();
        assert!(error.to_string().contains("in-memory repository list method"));
        assert!(error.to_string().contains("source was preserved"));
        for (path, original) in files.iter().zip(before) {
            assert_eq!(read_to_string(root.path().join(path)).unwrap(), original, "{path}");
        }
    }

    #[test]
    fn flattened_worker_retains_backend_processing_and_thin_entrypoints() {
        let root = tempfile::tempdir().unwrap();
        let data = rust_crate_template_data("insights", "Worker", true);
        let mut templates = super::super::worker_templates(root.path(), "insights", "Worker");
        for file in &mut templates {
            if file.path == root.path().join("src/main.rs") {
                file.content = render_rust_template("rust/postgres-worker/main.rs", &data);
            }
            if file.path == root.path().join("src/lib.rs") {
                file.content = render_rust_template("rust/leased-worker/lib.rs", &data);
            }
        }
        templates.push(RenderedTemplate {
            path: root.path().join("src/signals.rs"),
            content: String::new(),
            context: None,
        });
        materialize(
            root.path(),
            flatten(root.path(), "insights", RustProjectKind::Worker, templates),
        );
        let runtime = read_to_string(root.path().join("worker.rs")).unwrap();
        assert!(runtime.contains("PostgresWorkerQueue"));
        assert!(runtime.contains("pub async fn run()"));
        assert!(
            !read_to_string(root.path().join("main.rs"))
                .unwrap()
                .contains("PostgresWorkerQueue")
        );
        assert!(
            read_to_string(root.path().join("bootstrapper.rs"))
                .unwrap()
                .contains("crate::worker::run()")
        );
        assert!(
            !read_to_string(root.path().join("lib.rs"))
                .unwrap()
                .contains("fn ")
        );
    }
}

pub(super) fn convert_shape(dir: &Path, name: &str, kind: RustProjectKind) -> Result<()> {
    let root = find_manifest_root(dir)?;
    let description = cargo_description(&dir.join("Cargo.toml"))?.unwrap_or_default();
    let templates = if kind == RustProjectKind::Service {
        let relative = dir.strip_prefix(&root)?;
        add_cargo_member(&root, &format!("{}/.forklaunch/contracts", relative.display()), false)?;
        add_cargo_member(&root, &format!("{}/sdk/rust", relative.display()), false)?;
        service_templates(dir, name, &description)
    } else {
        flatten(
            dir,
            name,
            kind,
            rust_crate_templates(dir, name, &description, kind),
        )
    };
    apply_conversion(dir, kind, framework_paths(&root, templates))
}

pub(super) fn apply_conversion(
    dir: &Path,
    kind: RustProjectKind,
    templates: Vec<RenderedTemplate>,
) -> Result<()> {
    let previous: toml::Value = toml::from_str(&read_to_string(dir.join("Cargo.toml"))?)?;
    for mut file in templates {
        let relative = file.path.strip_prefix(dir).unwrap_or(&file.path).to_string_lossy();
        if relative == "Cargo.toml" {
            let mut cargo: toml::Value = toml::from_str(&file.content)?;
            if let Some(metadata) = previous
                .get("package")
                .and_then(|package| package.get("metadata"))
            {
                cargo["package"]
                    .as_table_mut()
                    .context("Expected package")?
                    .insert("metadata".into(), metadata.clone());
            }
            if kind != RustProjectKind::Service && cargo.get("build-dependencies").is_none() {
                cargo["package"]
                    .as_table_mut()
                    .context("Expected package")?
                    .insert("build".into(), toml::Value::Boolean(false));
            }
            if let Some(dependencies) = previous.get("dependencies").and_then(Value::as_table) {
                for (key, value) in dependencies {
                    if kind != RustProjectKind::Service
                        && key == "contracts"
                        && value.get("path").and_then(Value::as_str) == Some("domain")
                        && value
                            .get("package")
                            .and_then(Value::as_str)
                            .is_some_and(|package| package.ends_with("-contracts"))
                    {
                        continue;
                    }
                    cargo["dependencies"]
                        .as_table_mut()
                        .context("Expected dependencies")?
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
            }
            file.content = toml::to_string_pretty(&cargo)?;
        } else if relative == "domain/schemas/mod.rs"
            && kind == RustProjectKind::Worker
            && file.path.exists()
        {
            let mut previous = read_to_string(&file.path)?;
            for declaration in file
                .content
                .lines()
                .filter(|line| line.starts_with("pub mod "))
            {
                if !previous.lines().any(|line| line.trim() == declaration) {
                    previous.push_str(&format!("\n{declaration}\n"));
                }
            }
            file.content = previous;
        } else if relative == "domain/schemas/mod.rs"
            && kind == RustProjectKind::Service
            && file.path.exists()
        {
            let previous = read_to_string(&file.path)?;
            if previous.contains("pub struct HealthResponse") {
                continue;
            }
            file.content = format!("{previous}\n{}", file.content);
        } else if file.path.exists() && !conversion::replaces(relative.as_ref()) {
            continue;
        }
        create_dir_all(
            file.path
                .parent()
                .context("Conversion output requires parent")?,
        )?;
        let rendered = dependencies::render_for_path(&file.path, &file.content)?;
        let content = format_source(&file.path, &rendered);
        write(file.path, content)?;
    }
    Ok(())
}

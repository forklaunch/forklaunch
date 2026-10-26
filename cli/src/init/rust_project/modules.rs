use super::*;
use crate::core::rendered_template::TEMPLATES_DIR;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BuiltinModule {
    Iam,
    Cac,
    EcommerceStripe,
    BillingBase,
    BillingStripe,
}

impl BuiltinModule {
    pub(super) fn from_variant(variant: &str) -> Option<Self> {
        match variant {
            "iam-base" => Some(Self::Iam),
            "cac-base" => Some(Self::Cac),
            "ecommerce-stripe" => Some(Self::EcommerceStripe),
            "billing-base" => Some(Self::BillingBase),
            "billing-stripe" => Some(Self::BillingStripe),
            _ => None,
        }
    }
    fn variant(self) -> &'static str {
        match self {
            Self::Iam => "iam-base",
            Self::Cac => "cac-base",
            Self::EcommerceStripe => "ecommerce-stripe",
            Self::BillingBase => "billing-base",
            Self::BillingStripe => "billing-stripe",
        }
    }
    pub(super) fn template(self) -> &'static str {
        match self {
            Self::Iam => "iam",
            Self::Cac => "cac",
            Self::EcommerceStripe => "ecommerce",
            Self::BillingBase | Self::BillingStripe => "billing",
        }
    }
    pub(super) fn service_name(self) -> &'static str {
        self.template()
    }
    fn description(self) -> &'static str {
        match self {
            Self::Iam => "Persistent PostgreSQL IAM",
            Self::Cac => "CAC code-set service",
            Self::EcommerceStripe => "Persistent ecommerce with Stripe and optional PayPal",
            Self::BillingBase => "Persistent billing services",
            Self::BillingStripe => "Persistent billing with Stripe",
        }
    }
}

/// Modules implemented only in TypeScript. Messaging is not a module in either
/// language: services send through the managed gateway (`forklaunch infra add`).
pub(super) const TYPESCRIPT_ONLY_MODULES: &[&str] = &["iam-better-auth", "relay"];

#[derive(Content)]
struct RustModuleTemplate {
    name: String,
    description: String,
    crate_module: String,
    stripe: bool,
}

pub(crate) fn generate_rust_module(matches: &ArgMatches) -> Result<()> {
    let requested = option(matches, "module").unwrap_or_default();
    if let Ok(module) = requested.parse::<crate::constants::Module>() {
        crate::constants::ensure_module_offered(&module)?;
    }
    if TYPESCRIPT_ONLY_MODULES.contains(&requested.as_str()) {
        bail!("{requested} is a TypeScript module; omit --language rust. Rust services call it through its generated Rust SDK");
    }
    let kind = BuiltinModule::from_variant(&requested)
        .context("Rust module generation supports --module iam-base, cac-base, ecommerce-stripe, billing-base or billing-stripe")?;
    if option(matches, "database").is_some_and(|database| database != "postgresql") {
        bail!("Rust modules currently support --database postgresql");
    }
    let name = option(matches, "name").unwrap_or_else(|| kind.service_name().to_owned());
    if !validate_name(&name) {
        bail!("Invalid module service name");
    }
    let root = find_manifest_root(
        &option(matches, "base_path")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir()?),
    )?;
    let dryrun = matches.get_flag("dryrun");
    generate_module_at(&root, &name, matches, dryrun, kind)?;
    if !dryrun {
        let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
        let crate_path = crate_path_for_project(&manifest, &name).unwrap_or_else(|| name.clone());
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        log_ok!(stdout, "Generated Rust {} module {} at {}", kind.variant(), name, root.join(crate_path).display());
    }
    Ok(())
}

pub(super) fn generate_module_at(
    root: &Path,
    name: &str,
    matches: &ArgMatches,
    dryrun: bool,
    kind: BuiltinModule,
) -> Result<()> {
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let manifest = if manifest_path.exists() {
        read_manifest_value(&manifest_path)?
    } else {
        Value::Table(Map::new())
    };
    if let Some(project) = project_table(&manifest, &name) {
        if project.get("variant").and_then(Value::as_str) != Some(kind.variant())
            || !is_rust_project(root, name)?
        {
            bail!("Project {name} already exists and is not the requested Rust module");
        }
    }
    let crate_path = crate_path_for_project(&manifest, name)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src")).join(name));
    let crate_dir = root.join(&crate_path);
    if project_table(&manifest, name).is_none() && crate_dir.join("Cargo.toml").exists() {
        bail!(
            "An existing crate occupies the module destination: {}",
            crate_dir.display()
        );
    }
    let shared_contracts = !crate_dir.join("Cargo.toml").exists();
    if shared_contracts {
        for destination in [".forklaunch/contracts", "sdk/rust", "sdk/typescript"] {
            anyhow::ensure!(!crate_dir.join(destination).exists(),
                "Module SDK destination already exists: {}", crate_dir.join(destination).display());
        }
    }
    if manifest_path.exists() {
        mixed::ensure_workspace(root, dryrun)?;
    }
    let data = RustModuleTemplate {
        name: escape_toml_string(name),
        description: escape_toml_string(kind.description()),
        crate_module: name.to_case(Case::Snake),
        stripe: kind == BuiltinModule::BillingStripe,
    };
    ensure_embedded_framework(&root, dryrun)?;
    let mut templates = module_templates(&crate_dir, name, kind, &data, shared_contracts)
    .into_iter()
    .filter(|template| !template.path.exists())
    .collect::<Vec<_>>();
    if shared_contracts {
        add_local_sdk_templates(&crate_dir, name, &mut templates)?;
        add_typescript_surfacing(&crate_dir, kind, &mut templates)?;
    }
    // Older generated modules have no build script. Add only the required host
    // dependency alongside the new exporter; preserve user source and settings.
    if crate_dir.join("Cargo.toml").exists() && !crate_dir.join("build.rs").exists() {
        let cargo_path = crate_dir.join("Cargo.toml");
        let mut cargo: toml_edit::DocumentMut = read_to_string(&cargo_path)?.parse()?;
        let mut dependency = cargo["dependencies"]["forklaunch-core"].clone();
        dependency["features"] = toml_edit::value(toml_edit::Array::from_iter(["compliance-artifact"]));
        cargo["build-dependencies"]["forklaunch-core"] = dependency;
        templates.push(RenderedTemplate { path: cargo_path, content: cargo.to_string(), context: None });
    }
    let templates = native_layout::framework_paths(root, templates);
    write_rust_rendered_templates(
        &templates,
        dryrun,
        &mut StandardStream::stdout(ColorChoice::Always),
    )?;
    add_cargo_member(&root, &crate_path.to_string_lossy(), dryrun)?;
    let local_sdk = shared_contracts || crate_dir.join("sdk/rust/.forklaunch-sdk-local").is_file();
    if local_sdk {
        for relative in [".forklaunch/contracts", "sdk/rust"] {
            add_cargo_member(root, &crate_path.join(relative).to_string_lossy(), dryrun)?;
        }
    }
    add_project_to_manifest(
        &root,
        RustProjectKind::Service,
        &name,
        kind.description(),
        matches,
        dryrun,
    )?;
    if !dryrun {
        let path = root.join(".forklaunch/manifest.toml");
        let mut manifest = read_manifest_value(&path)?;
        let metadata = project_table_mut(&mut manifest, name)
            .context("Expected generated module")?
            .get_mut("metadata")
            .and_then(Value::as_table_mut)
            .context("Expected module metadata")?;
        metadata.insert(
                "cratePath".into(),
                Value::String(crate_path.to_string_lossy().into_owned()),
            );
        if local_sdk {
            metadata.insert("sdkLayout".into(), "project-local-v1".into());
            contracts_layout::set_artifact_paths(metadata, name);
        }
        write_manifest_value(&path, &manifest)?;
        mark_module(&root, &name, kind)?;
    }
    ensure_module_compose(&root, &name, kind, dryrun)?;
    ensure_rust_package_scripts(&root, RustProjectKind::Service, &name, dryrun)?;
    Ok(())
}

/// Service-shaped module sources: files every module shares, then the
/// module's own configuration, registrations, domain, persistence and API.
fn module_templates(dir: &Path, name: &str, kind: BuiltinModule, data: &RustModuleTemplate, shared_contracts: bool) -> Vec<RenderedTemplate> {
    fn collect(directory: &Dir<'_>, root: &Path, files: &mut Vec<PathBuf>) {
        for file in directory.files() {
            files.push(file.path().strip_prefix(root).expect("module template path").to_path_buf());
        }
        for child in directory.dirs() {
            collect(child, root, files);
        }
    }
    let mut output = Vec::new();
    for root in ["rust/project/module".to_owned(), format!("rust/{}", kind.template())] {
        let mut files = Vec::new();
        collect(TEMPLATES_DIR.get_dir(&root).expect("embedded module templates"), Path::new(&root), &mut files);
        for relative in files {
            let source = format!("{root}/{}", relative.to_string_lossy().replace('\\', "/"));
            let relative = match relative.to_str() {
                Some("Cargo.toml.template") => PathBuf::from("Cargo.toml"),
                Some("env.example") => PathBuf::from(".env.example"),
                _ => relative,
            };
            let content = render_rust_template(&source, data);
            // Variant-only sources (Stripe callbacks) render empty elsewhere.
            if !content.trim().is_empty() {
                output.push(RenderedTemplate { path: dir.join(relative), content, context: None });
            }
        }
    }
    let build = if shared_contracts { "rust/project/module-sdk-build.rs" } else { "rust/project/module-compliance-build.rs" };
    for (path, content) in [
        ("build.rs", render_rust_template(build, data)),
        ("api/contracts/mod.rs", "pub mod registry;\npub mod extensions;\n".to_owned()),
        ("api/contracts/extensions/mod.rs", extension_contracts_mod(&[])),
        ("api/routes/extensions/mod.rs", native_routes_mod_rs(&[])),
        ("Makefile", native_layout::makefile(name, true)),
    ] {
        output.push(RenderedTemplate { path: dir.join(path), content, context: None });
    }
    let cargo = output.iter_mut().find(|file| file.path == dir.join("Cargo.toml")).expect("module manifest");
    cargo.content.push_str(&format!(
        "\n[lib]\npath = \"lib.rs\"\n[[bin]]\nname = \"{name}\"\npath = \"main.rs\"\n\n[build-dependencies]\nforklaunch-core = {{ path = \"../../.forklaunch/framework-rs/crates/core\", features = [\"compliance-artifact\"] }}\n"
    ));
    if shared_contracts {
        cargo.content = with_contract_build_dependencies(&cargo.content).expect("valid generated module manifest");
    }
    output
}

const REGISTRY_ENVIRONMENT: &[&str] = &[
    "BILLING_REQUEST_DATE_PROFILE", "BILLING_IDENTITY_PROFILE", "BILLING_BOUND_CHECKOUT_CONFIG",
    "BILLING_BOUND_BILLING_CONFIG", "BILLING_PUBLISHER_DATABASE_URL", "BILLING_HMAC_PROFILE",
    "BILLING_PAYMENT_LINK_UPDATE_PROFILE",
];

/// Fresh modules use the ordinary service-owned SDK packages. Existing modules
/// retain their generated output layout until the application explicitly migrates.
fn add_local_sdk_templates(dir: &Path, name: &str, templates: &mut Vec<RenderedTemplate>) -> Result<()> {
    let host = contracts_layout::new_host_directory(dir);
    let service = templates.iter_mut().find(|file| file.path == dir.join("Cargo.toml"))
        .context("Missing fresh module manifest")?;
    let mut cargo: toml_edit::DocumentMut = service.content.parse()?;
    let mut host_cargo: toml_edit::DocumentMut = format!(
        "[package]\nname = \"{name}-contracts\"\nversion.workspace = true\nedition.workspace = true\nlicense.workspace = true\npublish = false\n[lib]\npath = \"lib.rs\"\n"
    ).parse()?;
    host_cargo["dependencies"] = cargo["build-dependencies"].clone();
    let features = host_cargo["dependencies"]["forklaunch-core"]["features"].as_array_mut()
        .context("Missing registry core features")?;
    if !features.iter().any(|feature| feature.as_str() == Some("typed-schema")) {
        features.push("typed-schema");
    }
    // The module serves MCP, which keeps schema members in declaration order;
    // compiled contracts and SDKs use the same order.
    let mut ordered = toml_edit::InlineTable::new();
    ordered.insert("features", toml_edit::Array::from_iter(["preserve_order"]).into());
    host_cargo["dependencies"]["serde_json"] = toml_edit::value(ordered);
    let mut contracts = toml_edit::InlineTable::new();
    contracts.insert("package", format!("{name}-contracts").into());
    contracts.insert("path", ".forklaunch/contracts".into());
    cargo["dependencies"]["contracts"] = toml_edit::value(contracts.clone());
    cargo["build-dependencies"]["contracts"] = toml_edit::value(contracts);
    service.content = cargo.to_string();
    let build = templates.iter_mut().find(|file| file.path == dir.join("build.rs"))
        .context("Missing fresh module build script")?;
    build.content = build.content.replace(
        "#[allow(dead_code)]\n#[path = \"api/contracts/mod.rs\"]\nmod contracts;\n", ""
    ).replace("contracts::registry::configured_registry()?", "contracts::configured_registry()?");
    templates.extend([
        RenderedTemplate { path: host.join("Cargo.toml"), content: host_cargo.to_string(), context: None },
        RenderedTemplate { path: host.join("lib.rs"), content: "//! Private registry compiler host; declarations remain owned by the service.\n#[path = \"../../api/contracts/registry.rs\"]\npub mod registry;\n#[path = \"../../api/contracts/extensions/mod.rs\"]\npub mod extensions;\npub use registry::configured_registry;\n".into(), context: None },
    ]);
    let exports = templates.iter_mut().find(|file| file.path == dir.join("api/contracts/mod.rs"))
        .context("Missing fresh module contract exports")?;
    exports.content = "//! Shared declarations used by both runtime and contract compilation.\npub use contracts::{extensions, registry};\n".into();
    templates.extend(sdk_consumer::local_templates_with_registry(
        dir, name, &host, "contracts::configured_registry()?", REGISTRY_ENVIRONMENT,
    ));
    Ok(())
}

/// TypeScript servers resolve roles, permissions, subscriptions and features
/// through `@<app>/iam` and `@<app>/billing` whichever language the module is
/// written in. The Rust modules' TypeScript SDK packages therefore export the
/// same `createSurface*` helpers as the TypeScript modules, calling the HMAC
/// surfacing routes directly. `module.ts` is CLI-owned: the Cargo build
/// rewrites `index.ts`, `sdk.ts` and the JSON artifacts, never these files.
fn add_typescript_surfacing(dir: &Path, kind: BuiltinModule, templates: &mut Vec<RenderedTemplate>) -> Result<()> {
    let surfacing = match kind {
        BuiltinModule::Iam => include_str!("../../templates/rust/module-sdk/iam/surfacing.ts"),
        BuiltinModule::BillingBase | BuiltinModule::BillingStripe => {
            include_str!("../../templates/rust/module-sdk/billing/surfacing.ts")
        }
        BuiltinModule::Cac | BuiltinModule::EcommerceStripe => return Ok(()),
    };
    let sdk = dir.join("sdk/typescript");
    let package = templates.iter_mut().find(|file| file.path == sdk.join("package.json"))
        .context("Missing module TypeScript SDK package")?;
    let mut manifest: serde_json::Value = serde_json::from_str(&package.content)?;
    manifest["exports"]["."] = serde_json::json!({"types": "./module.ts", "default": "./module.ts"});
    manifest["dependencies"] = serde_json::json!({
        "@forklaunch/core": crate::core::package_json::package_json_constants::CORE_VERSION
    });
    package.content = serde_json::to_string_pretty(&manifest)? + "\n";
    let tsconfig = templates.iter_mut().find(|file| file.path == sdk.join("tsconfig.json"))
        .context("Missing module TypeScript SDK tsconfig")?;
    // The helpers call fetch; check them against the DOM fetch types.
    tsconfig.content = "{\"compilerOptions\":{\"strict\":true,\"module\":\"NodeNext\",\"moduleResolution\":\"NodeNext\",\"target\":\"ES2022\",\"lib\":[\"ES2022\",\"DOM\"],\"skipLibCheck\":true,\"noEmit\":true},\"include\":[\"*.ts\"]}\n".into();
    templates.extend([
        RenderedTemplate {
            path: sdk.join("module.ts"),
            content: "/**\n * Generated by ForkLaunch\n * File: module.ts\n * Package entry: the generated SDK types plus the surfacing helpers.\n */\nexport type * from './sdk.js';\nexport * from './surfacing.js';\n".into(),
            context: None,
        },
        RenderedTemplate { path: sdk.join("surfacing.ts"), content: surfacing.into(), context: None },
        // Lets the production bundler take this package's TypeScript sources.
        RenderedTemplate { path: sdk.join(".forklaunch-sdk-local"), content: "forklaunch.sdk.local.v1\n".into(), context: None },
    ]);
    Ok(())
}

fn mark_module(root: &Path, name: &str, kind: BuiltinModule) -> Result<()> {
    let path = root.join(".forklaunch/manifest.toml");
    let mut manifest = read_manifest_value(&path)?;
    let projects = manifest
        .get_mut("projects")
        .and_then(Value::as_array_mut)
        .context("Expected manifest projects")?;
    let project = projects
        .iter_mut()
        .find(|project| project.get("name").and_then(Value::as_str) == Some(name))
        .and_then(Value::as_table_mut)
        .context("Expected Rust module project")?;
    project.insert(
        "variant".to_owned(),
        Value::String(kind.variant().to_owned()),
    );
    if kind == BuiltinModule::Cac {
        if let Some(resources) = project.get_mut("resources").and_then(Value::as_table_mut) {
            resources.remove("database");
        }
    }
    write_manifest_value(&path, &manifest)
}

pub(super) fn module_kind(root: &Path, name: &str) -> Result<Option<BuiltinModule>> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    Ok(project_table(&manifest, name)
        .and_then(|project| project.get("variant"))
        .and_then(Value::as_str)
        .and_then(BuiltinModule::from_variant))
}

pub(super) fn ensure_module_compose(
    root: &Path,
    name: &str,
    kind: BuiltinModule,
    dryrun: bool,
) -> Result<()> {
    ensure_module_compose_sources(root, name, kind, dryrun)?;
    if !dryrun {
        // Modules may add another process (ecommerce inventory) or rename the
        // existing binary after generic project scaffolding has run.
        language_images::reconcile(root)?;
    }
    Ok(())
}

fn ensure_module_compose_sources(
    root: &Path,
    name: &str,
    kind: BuiltinModule,
    dryrun: bool,
) -> Result<()> {
    if kind == BuiltinModule::Iam {
        return ensure_iam_compose(root, name, dryrun);
    }
    if kind == BuiltinModule::EcommerceStripe {
        return ensure_ecommerce_compose(root, name, dryrun);
    }
    if matches!(
        kind,
        BuiltinModule::BillingBase | BuiltinModule::BillingStripe
    ) {
        return ensure_billing_compose(root, name, kind, dryrun);
    }
    if dryrun {
        return Ok(());
    }
    let path = root.join("docker-compose.yaml");
    let compose = read_or_create_rust_docker_compose(&path)?;
    if !compose.services.contains_key(name) {
        ensure_rust_docker_compose_project(root, RustProjectKind::Service, name, false)?;
    }
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let service = compose
        .services
        .get_mut(name)
        .context("Expected CAC Compose service")?;
    let env = service.environment.get_or_insert_with(IndexMap::new);
    env.entry("HMAC_SECRET_KEY".to_owned())
        .or_insert_with(|| "${CAC_HMAC_SECRET_KEY:?Configure a service HMAC secret}".to_owned());
    env.entry("JWKS_PUBLIC_KEY_URL".to_owned())
        .or_insert_with(|| "${CAC_JWKS_PUBLIC_KEY_URL:?Configure trusted JWKS}".to_owned());
    write(path, yaml_to_string(&compose)?)?;
    Ok(())
}

fn ensure_ecommerce_compose(root: &Path, name: &str, dryrun: bool) -> Result<()> {
    if dryrun {
        return Ok(());
    }
    let path = root.join("docker-compose.yaml");
    if !read_or_create_rust_docker_compose(&path)?
        .services
        .contains_key(name)
    {
        ensure_rust_docker_compose_project(root, RustProjectKind::Service, name, false)?;
    }
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let service = compose
        .services
        .get_mut(name)
        .context("Expected ecommerce Compose service")?;
    let env = service.environment.get_or_insert_with(IndexMap::new);
    for (key, value) in [
        (
            "DATABASE_URL",
            "${ECOMMERCE_DATABASE_URL:?Configure a restricted PostgreSQL role}",
        ),
        (
            "TENANT_ID",
            "${ECOMMERCE_TENANT_ID:?Configure a trusted tenant}",
        ),
        (
            "ENCRYPTION_KEY",
            "${ENCRYPTION_KEY:?Configure a field encryption key}",
        ),
        (
            "HMAC_SECRET_KEY",
            "${ECOMMERCE_HMAC_SECRET_KEY:?Configure a service HMAC secret}",
        ),
        (
            "JWKS_PUBLIC_KEY_URL",
            "${ECOMMERCE_JWKS_PUBLIC_KEY_URL:?Configure trusted JWKS}",
        ),
        (
            "STRIPE_API_KEY",
            "${STRIPE_API_KEY:-}",
        ),
        (
            "STRIPE_WEBHOOK_SECRET",
            "${STRIPE_WEBHOOK_SECRET:-}",
        ),
        ("WORKER_MAX_ATTEMPTS", "3"),
        ("WORKER_BACKOFF_SECONDS", "30"),
    ] {
        env.entry(key.to_owned())
            .or_insert_with(|| value.to_owned());
    }
    let service = service.clone();
    let worker_name = format!("{name}-inventory-worker");
    if let Some(existing) = compose.services.get(&worker_name) {
        anyhow::ensure!(
            ecommerce_worker_owner(existing) == Some(name),
            "Compose service {worker_name} already exists and is not this module's inventory worker"
        );
    } else {
        let app_name = rust_app_name(root)?;
        let mut worker =
            rust_docker_service(&app_name, &worker_name, RustProjectKind::Worker, None);
        worker.image = service.image.clone();
        worker.build = service.build.clone();
        worker.networks = service.networks.clone();
        worker.command = Some(DockerCommand::Multiple(vec![
            format!("./target/release/{name}"),
            "--worker".to_owned(),
        ]));
        let mut worker_env = IndexMap::new();
        for (key, value) in service.environment.as_ref().into_iter().flatten() {
            if matches!(
                key.as_str(),
                "DATABASE_URL"
                    | "TENANT_ID"
                    | "ENCRYPTION_KEY"
                    | "LEGACY_ENCRYPTION_KEYS"
                    | "RUST_LOG"
                    | "WORKER_MAX_ATTEMPTS"
                    | "WORKER_BACKOFF_SECONDS"
            ) || key.starts_with("OTEL_")
            {
                worker_env.insert(key.clone(), value.clone());
            }
        }
        // Runner settings are required at startup; these match `.env.example`.
        for (key, value) in [
            ("WORKER_CONCURRENCY", "1"),
            ("WORKER_BATCH_SIZE", "1"),
            ("WORKER_POLL_MS", "250"),
            ("WORKER_LEASE_MS", "30000"),
            ("WORKER_RENEW_MS", "10000"),
            ("WORKER_DRAIN_MS", "30000"),
        ] {
            worker_env.insert(key.to_owned(), value.to_owned());
        }
        worker_env.insert("WORKER_HTTP_ADDR".to_owned(), "0.0.0.0:9090".to_owned());
        worker.environment = Some(worker_env);
        worker.healthcheck = Some(Healthcheck {
            test: HealthTest::List(vec![
                "CMD".to_owned(),
                "curl".to_owned(),
                "-fsS".to_owned(),
                "http://localhost:9090/ready".to_owned(),
            ]),
            interval: "10s".to_owned(),
            timeout: "5s".to_owned(),
            retries: 3,
            start_period: "10s".to_owned(),
            additional_properties: Default::default(),
        });
        worker.additional_properties.insert(
            ECOMMERCE_WORKER_OWNER.to_owned(),
            serde_yml::Value::String(name.to_owned()),
        );
        compose.services.insert(worker_name, worker);
    }
    write(path, yaml_to_string(&compose)?)?;
    Ok(())
}

const ECOMMERCE_WORKER_OWNER: &str = "x-forklaunch-ecommerce-worker-for";
pub(super) fn ecommerce_worker_owner(service: &DockerService) -> Option<&str> {
    service
        .additional_properties
        .get(ECOMMERCE_WORKER_OWNER)?
        .as_str()
}

fn ensure_billing_compose(
    root: &Path,
    name: &str,
    kind: BuiltinModule,
    dryrun: bool,
) -> Result<()> {
    if dryrun {
        return Ok(());
    }
    let path = root.join("docker-compose.yaml");
    if !read_or_create_rust_docker_compose(&path)?
        .services
        .contains_key(name)
    {
        ensure_rust_docker_compose_project(root, RustProjectKind::Service, name, false)?;
    }
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let service = compose
        .services
        .get_mut(name)
        .context("Expected billing Compose service")?;
    let env = service.environment.get_or_insert_with(IndexMap::new);
    for (key, value) in [
        (
            "DATABASE_URL",
            "${BILLING_DATABASE_URL:?Configure a restricted PostgreSQL role}",
        ),
        (
            "TENANT_ID",
            "${BILLING_TENANT_ID:?Configure a trusted tenant}",
        ),
        (
            "ENCRYPTION_KEY",
            "${ENCRYPTION_KEY:?Configure a field encryption key}",
        ),
        (
            "HMAC_SECRET_KEY",
            "${BILLING_HMAC_SECRET_KEY:?Configure a service HMAC secret}",
        ),
        (
            "JWKS_PUBLIC_KEY_URL",
            "${BILLING_JWKS_PUBLIC_KEY_URL:?Configure trusted JWKS}",
        ),
        ("BILLING_DATABASE_BACKUP", "false"),
    ] {
        env.entry(key.to_owned())
            .or_insert_with(|| value.to_owned());
    }
    if kind == BuiltinModule::BillingStripe {
        for key in [
            "STRIPE_SECRET_KEY",
            "STRIPE_API_KEY",
            "STRIPE_WEBHOOK_SECRET",
            "PLATFORM_GATEWAY_URL",
            "INSTANCE_ID",
            "INSTANCE_HMAC_KEY",
            "FORKLAUNCH_LOCAL_GATEWAY_MOCK",
        ] {
            env.entry(key.to_owned())
                .or_insert_with(|| format!("${{{key}:-}}"));
        }
    }
    write(path, yaml_to_string(&compose)?)?;
    Ok(())
}

fn ensure_iam_compose(root: &Path, name: &str, dryrun: bool) -> Result<()> {
    if dryrun {
        return Ok(());
    }
    let path = root.join("docker-compose.yaml");
    let compose = read_or_create_rust_docker_compose(&path)?;
    if !compose.services.contains_key(name) {
        ensure_rust_docker_compose_project(root, RustProjectKind::Service, name, false)?;
    }
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let service = compose
        .services
        .get_mut(name)
        .context("Expected IAM Compose service")?;
    let env = service.environment.get_or_insert_with(IndexMap::new);
    env.entry("DATABASE_URL".to_owned()).or_insert_with(|| {
        "${IAM_DATABASE_URL:?Configure a restricted PostgreSQL role}".to_owned()
    });
    env.entry("TENANT_ID".to_owned())
        .or_insert_with(|| "${IAM_TENANT_ID:?Configure a trusted tenant}".to_owned());
    env.entry("IAM_DIRECTORY_AUTHORIZATION".to_owned())
        .or_insert_with(|| "jwt-claims".to_owned());
    for (key, value) in [
        (
            "ENCRYPTION_KEY",
            "${IAM_ENCRYPTION_KEY:?Configure field encryption}",
        ),
        (
            "HMAC_SECRET_KEY",
            "${IAM_HMAC_SECRET_KEY:?Configure service authentication}",
        ),
        (
            "JWKS_PUBLIC_KEY_URL",
            "${IAM_JWKS_PUBLIC_KEY_URL:?Configure JWT verification}",
        ),
    ] {
        env.entry(key.to_owned())
            .or_insert_with(|| value.to_owned());
    }
    write(path, yaml_to_string(&compose)?)?;
    Ok(())
}

/// Whether `service` is the one-shot `--seed` job running module `name`'s binary.
pub(super) fn is_seed_job(service: &DockerService, name: &str) -> bool {
    matches!(&service.command, Some(DockerCommand::Multiple(args))
        if args.len() == 2 && args[1] == "--seed"
            && ["./target/release/", "/app/bin/"].iter().any(|prefix| args[0] == format!("{prefix}{name}")))
}

/// The image the renamed module's own service runs, which its seed job shares.
fn service_image_after_rename(compose: &DockerCompose, name: &str) -> Option<String> {
    compose.services.get(name).and_then(|service| service.image.clone())
}

pub(super) fn rename_sources(root: &Path, old: &str, new: &str) -> Result<()> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let crate_dir = root
        .join(crate_path_for_project(&manifest, new).unwrap_or_else(|| format!("crates/{new}")));
    for filename in ["src/lib.rs", "src/main.rs"] {
        let path = crate_dir.join(filename);
        if path.exists() {
            let namespace = regex::Regex::new(&format!(
                r"\b{}::",
                regex::escape(&old.to_case(Case::Snake))
            ))?;
            let source = namespace
                .replace_all(
                    &read_to_string(&path)?,
                    format!("{}::", new.to_case(Case::Snake)),
                )
                .into_owned()
                .replace(
                    &format!("pub const SERVICE_NAME: &str = \"{old}\";"),
                    &format!("pub const SERVICE_NAME: &str = \"{new}\";"),
                );
            write(path, source)?;
        }
    }
    let path = root.join("docker-compose.yaml");
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    if let Some(mut service) = compose.services.shift_remove(old) {
        service.hostname = Some(new.to_owned());
        service.command = Some(DockerCommand::Multiple(vec![format!(
            "./target/release/{new}"
        )]));
        compose.services.insert(new.to_owned(), service);
        let old_worker = format!("{old}-inventory-worker");
        if compose
            .services
            .get(&old_worker)
            .is_some_and(|worker| ecommerce_worker_owner(worker) == Some(old))
        {
            let new_worker = format!("{new}-inventory-worker");
            anyhow::ensure!(
                !compose.services.contains_key(&new_worker),
                "Compose service {new_worker} already exists"
            );
            let mut worker = compose
                .services
                .shift_remove(&old_worker)
                .context("Expected module inventory worker")?;
            worker.hostname = Some(new_worker.clone());
            worker.container_name = Some(format!("{}-{new_worker}-rust", rust_app_name(root)?));
            if let Some(DockerCommand::Multiple(args)) = &mut worker.command {
                if args.len() == 2 && args[1] == "--worker" {
                    for prefix in ["./target/release/", "/app/bin/"] {
                        if args[0] == format!("{prefix}{old}") {
                            args[0] = format!("{prefix}{new}");
                            break;
                        }
                    }
                }
            }
            worker.additional_properties.insert(
                ECOMMERCE_WORKER_OWNER.to_owned(),
                serde_yml::Value::String(new.to_owned()),
            );
            compose.services.insert(new_worker, worker);
        }
        // The IAM seed job runs the module's own binary with --seed.
        let old_seed = format!("{old}-seed");
        if compose.services.get(&old_seed).is_some_and(|service| is_seed_job(service, old)) {
            let new_seed = format!("{new}-seed");
            anyhow::ensure!(
                !compose.services.contains_key(&new_seed),
                "Compose service {new_seed} already exists"
            );
            let mut seed = compose.services.shift_remove(&old_seed).context("Expected IAM seed job")?;
            if let Some(DockerCommand::Multiple(args)) = &mut seed.command {
                args[0] = args[0].replacen(&format!("/{old}"), &format!("/{new}"), 1);
            }
            seed.image = service_image_after_rename(&compose, new).or(seed.image);
            compose.services.insert(new_seed, seed);
        }
        write(path, yaml_to_string(&compose)?)?;
    }
    Ok(())
}

/// Registry compilation shares the runtime descriptors; no server or database starts at build time.
fn with_contract_build_dependencies(source: &str) -> Result<String> {
    let mut cargo: toml_edit::DocumentMut = source.parse()?;
    for name in ["forklaunch-core", "forklaunch-iam-interface", "forklaunch-cac-interface", "forklaunch-ecommerce-interface", "forklaunch-billing-interface", "forklaunch-billing-stripe"] {
        if let Some(dependency) = cargo["dependencies"].get(name).cloned() {
            cargo["build-dependencies"][name] = dependency;
        }
    }
    let features = cargo["build-dependencies"]["forklaunch-core"]["features"].as_array_mut().context("Core build features are required")?;
    for required in ["build", "compliance-artifact"] {
        if !features.iter().any(|v| v.as_str() == Some(required)) { features.push(required); }
    }
    Ok(cargo.to_string())
}

pub(super) fn extension_contracts_mod(modules: &[String]) -> String {
    native_routes_mod_rs(modules).split("pub fn register_router(").next().unwrap()
        .replace("use forklaunch_axum::ForkLaunchRouter;", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_iam_and_billing_sdks_export_typescript_surfacing() {
        let root = tempfile::tempdir().unwrap();
        for (kind, helpers) in [
            (BuiltinModule::Iam, &["createSurfaceRoles", "createSurfacePermissions"][..]),
            (BuiltinModule::BillingStripe, &["createSurfaceFeatures", "createSurfaceSubscription"][..]),
            (BuiltinModule::Cac, &[][..]),
        ] {
            let dir = root.path().join("src/module");
            let mut files = sdk_consumer::local_templates_with_registry(&dir, "module", &dir.join(".forklaunch/contracts"), "contracts::registry()", &[]);
            add_typescript_surfacing(&dir, kind, &mut files).unwrap();
            let file = |name: &str| files.iter().find(|file| file.path == dir.join("sdk/typescript").join(name)).map(|file| file.content.clone());
            let package: serde_json::Value = serde_json::from_str(&file("package.json").unwrap()).unwrap();
            if helpers.is_empty() {
                assert!(file("surfacing.ts").is_none());
                assert_eq!(package["exports"]["."]["types"], "./index.ts");
                continue;
            }
            let surfacing = file("surfacing.ts").unwrap();
            for helper in helpers {
                assert!(surfacing.contains(&format!("export async function {helper}(")), "{helper}");
            }
            // The Cargo build owns index.ts; the package entry is CLI-owned.
            assert_eq!(package["exports"]["."], serde_json::json!({"types": "./module.ts", "default": "./module.ts"}));
            assert!(package["dependencies"]["@forklaunch/core"].is_string());
            assert!(file("module.ts").unwrap().contains("export * from './surfacing.js';"));
            assert_eq!(file(".forklaunch-sdk-local").unwrap(), "forklaunch.sdk.local.v1\n");
            assert!(file("tsconfig.json").unwrap().contains("\"DOM\""));
        }
    }

    #[test]
    fn ecommerce_rename_preserves_worker_settings_in_shared_and_legacy_images() {
        for executable in ["/app/bin/shop", "./target/release/shop", "/custom/inventory"] {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir(root.path().join(".forklaunch")).unwrap();
            write(root.path().join(".forklaunch/manifest.toml"), "app_name = 'app'\n").unwrap();
            write(root.path().join("docker-compose.yaml"), format!(
                "services:\n  shop:\n    image: app-rust\n    command: ['/app/bin/shop']\n  shop-inventory-worker:\n    image: app-rust\n    command: ['{executable}', '--worker']\n    x-forklaunch-ecommerce-worker-for: shop\n    x-user-marker: preserved\n    environment:\n      WORKER_BATCH_SIZE: '2'\n"
            )).unwrap();
            rename_sources(root.path(), "shop", "store").unwrap();
            let compose = read_or_create_rust_docker_compose(&root.path().join("docker-compose.yaml")).unwrap();
            assert!(!compose.services.contains_key("shop-inventory-worker"));
            let worker = &compose.services["store-inventory-worker"];
            let expected = if executable == "/custom/inventory" { executable.to_owned() }
                else { executable.replace("shop", "store") };
            assert!(matches!(&worker.command, Some(DockerCommand::Multiple(args)) if args == &vec![expected, "--worker".to_owned()]));
            assert_eq!(ecommerce_worker_owner(worker), Some("store"));
            assert_eq!(worker.additional_properties["x-user-marker"].as_str(), Some("preserved"));
            assert_eq!(worker.environment.as_ref().unwrap()["WORKER_BATCH_SIZE"], "2");
        }
    }

    #[test]
    fn service_shaped_modules_bind_each_operation_once_through_owned_controllers() {
        for kind in [BuiltinModule::Iam, BuiltinModule::Cac, BuiltinModule::EcommerceStripe, BuiltinModule::BillingBase, BuiltinModule::BillingStripe] {
            let root = tempfile::tempdir().unwrap();
            let dir = root.path().join("src/module");
            let data = RustModuleTemplate {
                name: "module".into(),
                description: escape_toml_string(kind.description()),
                crate_module: "module".into(),
                stripe: kind == BuiltinModule::BillingStripe,
            };
            let files = module_templates(&dir, "module", kind, &data, true);
            let source = |relative: &str| files.iter().find(|file| file.path == dir.join(relative))
                .unwrap_or_else(|| panic!("{} lacks {relative}", kind.variant())).content.as_str();
            for file in ["main.rs", "lib.rs", "bootstrapper.rs", "application.rs", "registrations.rs", "server.rs",
                "config.rs", "error.rs", "domain/mod.rs", "persistence/mod.rs", "api/contracts/registry.rs",
                "api/routes/extensions/mod.rs", "Makefile", "build.rs", ".env.example", "README.md"] {
                let _ = source(file);
            }
            assert!(!files.iter().any(|file| file.path.starts_with(dir.join("api/http"))));
            assert_eq!(source("server.rs").matches(".serve_with_default_mcp(").count(), 1);
            assert!(source("api/routes/mod.rs").contains("extensions::register_router("));
            for file in files.iter().filter(|file| file.path.extension().is_some_and(|extension| extension == "rs")) {
                syn::parse_file(&file.content).unwrap_or_else(|error| panic!("{}: {error}", file.path.display()));
            }
            let route = regex::Regex::new(r#"\.route\("(\w+)", (\w+)::(\w+)\(state\)\)"#).unwrap();
            let mut operations = Vec::new();
            for file in files.iter().filter(|file| file.path.parent() == Some(&dir.join("api/routes"))) {
                let resource = file.path.file_stem().unwrap().to_str().unwrap();
                if resource == "mod" {
                    continue;
                }
                assert!(source("api/routes/mod.rs").contains(&format!("{resource}::mount(")), "{resource} is mounted");
                for capture in route.captures_iter(&file.content) {
                    let controller = source(&format!("api/controllers/{}.rs", &capture[2]));
                    // Controllers own their handler code instead of returning a
                    // packaged handler by operation name.
                    assert!(!controller.contains(".handler(\""), "{} controller {} owns its handler", kind.variant(), &capture[2]);
                    let start = [format!("pub fn {}(state: &AppState)", &capture[3]), format!("pub fn {}(_state: &AppState)", &capture[3])]
                        .iter().find_map(|signature| controller.find(signature.as_str()))
                        .unwrap_or_else(|| panic!("{} lacks controller {}", kind.variant(), &capture[3]));
                    let body = &controller[start..start + controller[start..].find("\n}\n").expect("controller body")];
                    assert!(body.contains("any("), "{} binds a handler", &capture[3]);
                    // Billing operations carry their configured contract; it must be the routed one.
                    if body.contains(".operation(") {
                        assert!(body.contains(&format!(".operation(\"{}\")", &capture[1])), "{} serves {}", &capture[3], &capture[1]);
                    }
                    operations.push(capture[1].to_owned());
                }
            }
            let count = operations.len();
            operations.sort();
            operations.dedup();
            assert_eq!(operations.len(), count, "{} routes an operation twice", kind.variant());
        }
    }

    #[test]
    fn all_builtin_variants_render_valid_runtime_and_contract_host_manifests() {
        for kind in [
            BuiltinModule::Iam,
            BuiltinModule::Cac,
            BuiltinModule::EcommerceStripe,
            BuiltinModule::BillingBase,
            BuiltinModule::BillingStripe,
        ] {
            let data = RustModuleTemplate {
                name: "test-module".into(),
                description: escape_toml_string(kind.description()),
                crate_module: "test_module".into(),
                        stripe: kind == BuiltinModule::BillingStripe,
            };
            let mut runtime = render_rust_template(
                &format!("rust/{}/Cargo.toml.template", kind.template()),
                &data,
            );
            runtime.push_str("\n[build-dependencies]\nforklaunch-core = { path = \"../../.forklaunch/framework-rs/crates/core\", features = [\"compliance-artifact\"] }\n");
            let runtime_manifest: toml::Value = toml::from_str(&runtime)
                .unwrap_or_else(|error| panic!("{} runtime manifest: {error}", kind.variant()));
            let runtime_features = runtime_manifest["dependencies"]["forklaunch-core"]["features"]
                .as_array().unwrap();
            for feature in ["openapi", "auth", "otlp", "remote-jwks"] {
                assert!(runtime_features.iter().any(|value| value.as_str() == Some(feature)),
                    "{} lost {feature}", kind.variant());
            }
            assert_eq!(runtime_features.iter().any(|value| value.as_str() == Some("postgres")), kind != BuiltinModule::Cac);
            let compiled = with_contract_build_dependencies(&runtime)
                .unwrap_or_else(|error| panic!("{} contract host manifest: {error}", kind.variant()));
            let host: toml::Value = toml::from_str(&compiled).unwrap();
            assert_eq!(host["dependencies"], runtime_manifest["dependencies"],
                "Contract compilation must preserve {} runtime dependencies", kind.variant());
            let build_features = host["build-dependencies"]["forklaunch-core"]["features"]
                .as_array().unwrap();
            assert!(runtime_features.iter().all(|feature| build_features.contains(feature)));
            assert!(build_features.iter().any(|value| value.as_str() == Some("build")));
            assert!(build_features.iter().any(|value| value.as_str() == Some("compliance-artifact")),
                "{} must compile its compliance manifest independently", kind.variant());
            for (name, dependency) in runtime_manifest["dependencies"].as_table().unwrap() {
                // Ecommerce's worker interface is used only by its inventory
                // process; the declaration host imports the domain interface.
                if name.ends_with("-interface") && name != "forklaunch-worker-interface" {
                    assert_eq!(&host["build-dependencies"][name.as_str()], dependency,
                        "{} declarations need the same interface dependency", kind.variant());
                }
            }
            let root = tempfile::tempdir().unwrap();
            let dir = root.path().join("backend/domains/test-module");
            let mut files = vec![
                RenderedTemplate { path: dir.join("Cargo.toml"), content: compiled, context: None },
                RenderedTemplate { path: dir.join("build.rs"), content: render_rust_template("rust/project/module-sdk-build.rs", &data), context: None },
                RenderedTemplate { path: dir.join("api/contracts/mod.rs"), content: "pub mod registry;\npub mod extensions;\n".into(), context: None },
            ];
            add_local_sdk_templates(&dir, "test-module", &mut files).unwrap();
            let files = native_layout::framework_paths(root.path(), files);
            let source = |relative: &str| files.iter().find(|file| file.path == dir.join(relative)).unwrap().content.as_str();
            let service: Value = toml::from_str(source("Cargo.toml")).unwrap();
            // The host boundary changes no runtime dependencies; paths alone
            // follow the configured module root.
            let normalized = native_layout::framework_paths(root.path(), vec![RenderedTemplate {
                path: dir.join("Cargo.toml"), content: runtime.clone(), context: None,
            }]);
            let expected: Value = toml::from_str(&normalized[0].content).unwrap();
            let mut runtime_dependencies = service["dependencies"].as_table().unwrap().clone();
            runtime_dependencies.remove("contracts");
            assert_eq!(Value::Table(runtime_dependencies), expected["dependencies"]);
            assert_eq!(service["build-dependencies"]["contracts"]["path"].as_str(), Some(".forklaunch/contracts"));
            assert_eq!(service["dependencies"]["contracts"], service["build-dependencies"]["contracts"],
                "Runtime and compiler must use exactly the same declaration package");
            let host: Value = toml::from_str(source(".forklaunch/contracts/Cargo.toml")).unwrap();
            assert_eq!(host["package"]["publish"].as_bool(), Some(false));
            assert!(host["dependencies"].get("test-module").is_none());
            assert!(host["dependencies"]["forklaunch-core"]["features"].as_array().unwrap().iter().any(|value| value.as_str() == Some("typed-schema")));
            assert_eq!(host["dependencies"]["serde_json"]["features"].as_array().unwrap()[0].as_str(), Some("preserve_order"),
                "Compiled contracts keep the declaration order MCP serves");
            assert_eq!(source("sdk/rust/.forklaunch-sdk-local"), "forklaunch.sdk.local.v1\n");
            let sdk: Value = toml::from_str(source("sdk/rust/Cargo.toml")).unwrap();
            assert_eq!(sdk["build-dependencies"]["contracts"]["path"].as_str(), Some("../../.forklaunch/contracts"));
            assert!(sdk["dependencies"].get("test-module").is_none());
            assert!(!source("build.rs").contains("mod contracts;"));
            for build in [source("build.rs"), source("sdk/rust/build.rs")] {
                syn::parse_file(build).unwrap();
                assert!(build.contains("contracts::configured_registry()?"));
                for environment in REGISTRY_ENVIRONMENT {
                    assert!(build.contains(environment));
                }
            }
            assert!(source("sdk/rust/build.rs").contains("\"Client\""));
            assert!(source(".forklaunch/contracts/lib.rs").contains("../../api/contracts/registry.rs"));
            assert!(source(".forklaunch/contracts/lib.rs").contains("../../api/contracts/extensions/mod.rs"));
            assert!(source("api/contracts/mod.rs").contains("pub use contracts::{extensions, registry};"));
            assert!(!source(".forklaunch/contracts/lib.rs").contains("server"));
            for file in &files {
                create_dir_all(file.path.parent().unwrap()).unwrap();
                write(&file.path, &file.content).unwrap();
            }
            create_dir_all(root.path().join(".forklaunch")).unwrap();
            write(root.path().join(".forklaunch/manifest.toml"), "modules_path='backend/domains'\n").unwrap();
            assert_eq!(
                forklaunch_core::build::provenance::contract_directory(&dir).unwrap(),
                Some(dir.join(".forklaunch/contracts").canonicalize().unwrap()),
                "The actual compiler provenance guard must accept the generated host boundary",
            );
            create_dir_all(dir.join("api/routes")).unwrap();
            create_dir_all(dir.join("api/contracts")).unwrap();
            write(dir.join("api/contracts/registry.rs"), "// Existing module registry\n").unwrap();
            assert!(contracts_layout::directory(&dir).unwrap().join("Cargo.toml").is_file());
            assert!(contracts_layout::has_local_sdk(&dir));
            assert!(!contracts_layout::is_native_service(&dir), "Module extension routers must retain their module adapter");
            let mut metadata = Map::new();
            metadata.insert("cratePath".into(), "backend/domains/test-module".into());
            metadata.insert("sdkLayout".into(), "project-local-v1".into());
            contracts_layout::set_artifact_paths(&mut metadata, "test-module");
            assert_eq!(metadata["contractArtifactPath"].as_str(), Some("backend/domains/test-module/sdk/typescript/contract.json"));
        }
    }
}

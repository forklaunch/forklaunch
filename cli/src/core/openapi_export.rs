mod compiler_metadata;
mod outputs;
mod portable;

use std::{
    env, fs,
    io::Read as _,
    path::Path,
    process::{Command as ProcessCommand, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

use crate::core::artifact_paths::{self, Artifact};

pub(crate) const OBSERVABILITY_SERVICES: &[&str] = &[
    "mimir",
    "loki",
    "tempo",
    "grafana",
    "otel",
    "otel-collector",
    "memcached",
    "minio",
    "minio-init",
];

/// Resolve a command name to its full path by searching PATH.
/// Falls back to the bare command name if not found (lets the OS try).
pub(crate) fn resolve_command(name: &str) -> String {
    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                if let Some(s) = candidate.to_str() {
                    return s.to_string();
                }
            }
        }
    }

    // Common install locations as fallback
    let home = env::var("HOME").unwrap_or_default();
    let mut fallback_paths = vec![
        format!("{}/Library/pnpm/{}", home, name),
        format!("{}/.local/share/pnpm/{}", home, name),
        format!("{}/.corepack/bin/{}", home, name),
        format!("/opt/homebrew/bin/{}", name),
        format!("/usr/local/bin/{}", name),
        format!("{}/.bun/bin/{}", home, name),
    ];

    // Check nvm directories for node/npm/npx
    if let Ok(nvm_dir) = env::var("NVM_DIR").or_else(|_| Ok::<String, ()>(format!("{}/.nvm", home)))
    {
        let nvm_versions = Path::new(&nvm_dir).join("versions").join("node");
        if nvm_versions.is_dir() {
            // Find the latest installed node version
            if let Ok(entries) = fs::read_dir(&nvm_versions) {
                let mut versions: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .collect();
                versions.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
                for version_entry in versions {
                    fallback_paths.push(format!("{}/bin/{}", version_entry.path().display(), name));
                }
            }
        }
    }

    for path in &fallback_paths {
        if Path::new(path).is_file() {
            return path.clone();
        }
    }

    name.to_string()
}

use crate::{
    constants::Runtime,
    core::{
        ast::infrastructure::env::{EnvVarUsage, find_all_env_vars},
        manifest::{ProjectType, application::ApplicationManifestData},
        rendered_template::RenderedTemplatesCache,
    },
};

fn generate_dummy_value(var_name: &str, var_type: &str, iam_port: Option<u16>) -> String {
    match var_type {
        "string" => {
            if var_name.ends_with("_PATH") || var_name.ends_with("_FILE") {
                "/dev/null".to_string()
            } else if var_name.contains("DB_URL") {
                "postgresql://dummy:dummy@localhost:5432/dummy".to_string()
            } else if var_name.contains("REDIS_URL") {
                "redis://localhost:6379".to_string()
            } else if var_name.contains("KAFKA") {
                "localhost:9092".to_string()
            } else if var_name == "HOST" {
                "localhost".to_string()
            } else if var_name == "PROTOCOL" {
                "http".to_string()
            } else if var_name == "VERSION" {
                "v1".to_string()
            } else if var_name.contains("DOCS_PATH") {
                "/docs".to_string()
            } else if var_name.contains("OTEL_EXPORTER") {
                "http://localhost:4318".to_string()
            } else if var_name.contains("OTEL_SERVICE_NAME") {
                "service".to_string()
            } else if var_name.contains("OTEL_LEVEL") {
                "info".to_string()
            } else if var_name == "NODE_ENV" {
                "development".to_string()
            } else if var_name.contains("S3_BUCKET") || var_name.contains("BUCKET_NAME") {
                "dummy-bucket".to_string()
            } else if var_name.contains("S3_REGION") || var_name.contains("AWS_REGION") {
                "us-east-1".to_string()
            } else if var_name == "JWKS_PUBLIC_KEY_URL" {
                if let Some(port) = iam_port {
                    format!("http://localhost:{}/api/auth/jwks", port)
                } else {
                    "dummy-jwks-url".to_string()
                }
            } else if var_name.contains("URL") {
                "http://localhost:3000".to_string()
            } else if var_name.contains("_SECRET")
                || var_name.contains("_KEY")
                || var_name.starts_with("SECRET")
                || var_name.starts_with("KEY")
            {
                if var_name == "HMAC_SECRET_KEY" {
                    format!("{:x}", {
                        use std::{
                            collections::hash_map::DefaultHasher,
                            hash::{Hash, Hasher},
                        };
                        let mut hasher = DefaultHasher::new();
                        var_name.hash(&mut hasher);
                        hasher.finish()
                    })
                } else {
                    "dummy-secret-key".to_string()
                }
            } else {
                "1".to_string()
            }
        }
        "number" => {
            if var_name.contains("PORT") {
                "3000".to_string()
            } else {
                "1".to_string()
            }
        }
        "boolean" => "true".to_string(),
        _ => "1".to_string(),
    }
}

pub(crate) fn export_service_openapi(
    service_path: &Path,
    service_name: &str,
    output_file: &Path,
    runtime: &str,
    env_vars: &[EnvVarUsage],
    iam_port: Option<u16>,
) -> Result<()> {
    if !service_path.join("package.json").exists() {
        bail!("No package.json found in {}", service_path.display());
    }

    let tsconfig_path = service_path.join("tsconfig.json");

    // Resolve the runtime binary and build the command.
    // For Node, run node directly with tsx's CLI module to avoid shebang PATH issues
    // (e.g. `#!/usr/bin/env node` fails when node isn't in PATH).
    let (resolved_bin, args) = match runtime.parse::<Runtime>()? {
        Runtime::Bun => {
            let bun = resolve_command("bun");
            (bun, vec!["server.ts".to_string()])
        }
        Runtime::Node => {
            let node = resolve_command("node");
            let tsx_cli = service_path
                .join("node_modules/tsx/dist/cli.mjs")
                .to_string_lossy()
                .to_string();

            if !Path::new(&tsx_cli).exists() {
                bail!(
                    "tsx not found at {} for service {}. Run pnpm install first.",
                    tsx_cli,
                    service_name
                );
            }

            (
                node,
                vec![
                    tsx_cli,
                    "--tsconfig".to_string(),
                    tsconfig_path.to_str().unwrap().to_string(),
                    "server.ts".to_string(),
                ],
            )
        }
    };

    let mut cmd = ProcessCommand::new(&resolved_bin);
    cmd.args(&args).current_dir(service_path);

    for env_var in env_vars {
        let dummy_value = generate_dummy_value(&env_var.var_name, "string", iam_port);
        cmd.env(&env_var.var_name, &dummy_value);
    }

    // Application scans may include these variables. The explicit export mode
    // and destination must take precedence over generated dummy values.
    cmd.env("FORKLAUNCH_MODE", "openapi")
        .env("FORKLAUNCH_OPENAPI_OUTPUT", output_file);

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn {} command", resolved_bin))?;

    let timeout = Duration::from_secs(60);
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let stderr = child
                        .stderr
                        .take()
                        .map(|mut s| {
                            let mut buf = String::new();
                            s.read_to_string(&mut buf).ok();
                            buf
                        })
                        .unwrap_or_default();
                    bail!("Service {} failed to export: {}", service_name, stderr);
                }
                break;
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    bail!(
                        "Service {} timed out after {}s during OpenAPI export. \
                        This usually means server.ts has top-level await calls (e.g. universalSdk, DB connections) \
                        that block before the FORKLAUNCH_MODE check in listen(). \
                        Wrap them in a guard: if (process.env.FORKLAUNCH_MODE !== 'openapi') {{ ... }}",
                        service_name,
                        timeout.as_secs()
                    );
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => bail!("Error waiting for {} subprocess: {}", service_name, e),
        }
    }

    if !output_file.exists() {
        bail!(
            "OpenAPI file not created at {:?} for service {}",
            output_file,
            service_name
        );
    }

    Ok(())
}

/// Export every service's OpenAPI document. `output_dir` additionally lays the
/// documents out as `<dir>/<service>/openapi.json`; without it each service
/// writes only its registered artifact (see `outputs::Outputs::plan`).
pub(crate) fn export_all_services(
    app_root: &Path,
    manifest: &ApplicationManifestData,
    output_dir: Option<&Path>,
) -> Result<Vec<String>> {
    // Validate all languages together before starting any exporter, including
    // collisions between one service's configured path and another's default.
    let outputs = outputs::Outputs::plan(
        app_root,
        output_dir,
        manifest
            .projects
            .iter()
            .filter(|project| {
                matches!(project.r#type, ProjectType::Service | ProjectType::Worker)
                    && project
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.contract_export)
                        != Some(false)
                    && (project
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.language.as_deref())
                        == Some("rust")
                        || !OBSERVABILITY_SERVICES.contains(&project.name.as_str()))
            })
            .map(|project| {
                let metadata = project
                    .metadata
                    .as_ref()
                    .and_then(|metadata| toml::Table::try_from(metadata).ok());
                (
                    project.name.as_str(),
                    artifact_paths::path(&project.name, metadata.as_ref(), Artifact::OpenApi),
                )
            })
            .collect::<Vec<_>>()
            .iter()
            .map(|(name, path)| (*name, Some(path.as_str()))),
    )?;
    let mut exported_services = export_all_rust_services(app_root, output_dir)?;

    // Tokio is the Rust runtime; TypeScript services in that mixed workspace
    // still export through Node. Bun/Node manifests keep their selected runtime.
    let runtime = if manifest.runtime == "tokio" {
        "node"
    } else {
        manifest.runtime.as_str()
    };
    let modules_path = app_root.join(&manifest.modules_path);

    // Include both services and workers (workers have a server component)
    // Exclude observability services as they are not user application components

    let services: Vec<_> = manifest
        .projects
        .iter()
        .filter(|p| {
            (p.r#type == ProjectType::Service || p.r#type == ProjectType::Worker)
                && !OBSERVABILITY_SERVICES.contains(&p.name.as_str())
                && p.metadata
                    .as_ref()
                    .and_then(|metadata| metadata.contract_export)
                    != Some(false)
                && p.metadata.as_ref().and_then(|m| m.language.as_deref()) != Some("rust")
        })
        .collect();

    if services.is_empty() {
        return Ok(exported_services);
    }

    let rendered_templates_cache = RenderedTemplatesCache::new();
    let all_env_vars = find_all_env_vars(&modules_path, &rendered_templates_cache)?;

    // Find IAM service to get its port
    let iam_port = manifest
        .projects
        .iter()
        .find(|p| {
            p.r#type == ProjectType::Service && (p.name == "iam" || p.name == "iam-better-auth")
        })
        .and_then(|iam_project| {
            // Try to get port from docker-compose by looking at services
            let docker_compose_path = app_root.join("docker-compose.yaml");
            if docker_compose_path.exists() {
                if let Ok(content) = fs::read_to_string(&docker_compose_path) {
                    if let Ok(compose) = serde_yml::from_str::<serde_yml::Value>(&content) {
                        if let Some(services) = compose.get("services") {
                            if let Some(i_services) = services.as_mapping() {
                                if let Some(iam) = i_services.get(&iam_project.name) {
                                    if let Some(ports) = iam.get("ports") {
                                        if let Some(port_str) = ports[0].as_str() {
                                            if let Some(host_port) = port_str.split(':').next() {
                                                if let Ok(port) = host_port.parse::<u16>() {
                                                    return Some(port);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        });

    for service in services {
        let service_path = app_root.join(&manifest.modules_path).join(&service.name);
        let openapi_file = outputs.exported(&service.name)?.to_path_buf();
        let service_output_dir = openapi_file
            .parent()
            .context("OpenAPI output parent missing")?;

        fs::create_dir_all(service_output_dir)
            .with_context(|| format!("Failed to create directory: {:?}", service_output_dir))?;

        let service_env_vars = all_env_vars
            .get(&service.name)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        export_service_openapi(
            &service_path,
            &service.name,
            &openapi_file,
            runtime,
            service_env_vars,
            iam_port,
        )?;

        let mut document: serde_json::Value = serde_json::from_str(&fs::read_to_string(&openapi_file)?)?;
        // The framework exports either one document or a map keyed by API
        // version (the unversioned API is keyed ""); normalize every document.
        let documents: Vec<&mut serde_json::Value> = if document.get("openapi").is_some() {
            vec![&mut document]
        } else {
            document
                .as_object_mut()
                .map(|versions| versions.values_mut().collect())
                .unwrap_or_default()
        };
        for document in documents {
            portable::enrich_imported(document, &service.name).with_context(|| {
                format!("Failed to normalize TypeScript OpenAPI for {}", service.name)
            })?;
        }
        fs::write(&openapi_file, serde_json::to_string_pretty(&document)?)?;

        outputs.refresh(&service.name)?;
        exported_services.push(service.name.clone());
    }

    Ok(exported_services)
}

// Typed manifests and raw Rust metadata share the same opt-out semantics.
pub(crate) fn raw_contract_export_enabled(project: &toml::Value) -> bool {
    project
        .get("metadata")
        .and_then(|metadata| {
            metadata
                .get("contractExport")
                .or_else(|| metadata.get("contract_export"))
        })
        .and_then(toml::Value::as_bool)
        != Some(false)
}

pub(crate) fn export_all_rust_services(
    app_root: &Path,
    output_dir: Option<&Path>,
) -> Result<Vec<String>> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    let manifest_content = fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest = toml::from_str::<toml::Value>(&manifest_content)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
    let projects = manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let outputs = outputs::Outputs::plan(
        app_root,
        output_dir,
        projects
            .iter()
            .filter(|project| {
                matches!(
                    project.get("type").and_then(toml::Value::as_str),
                    Some("Service" | "Worker")
                ) && raw_contract_export_enabled(project)
                    && project
                        .get("metadata")
                        .and_then(|metadata| metadata.get("language"))
                        .and_then(toml::Value::as_str)
                        == Some("rust")
            })
            .filter_map(|project| {
                project
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .map(|name| {
                        (
                            name,
                            artifact_paths::project_path(project, Artifact::OpenApi),
                        )
                    })
            })
            .collect::<Vec<_>>()
            .iter()
            .map(|(name, path)| (*name, path.as_deref())),
    )?;
    let mut exported_services = Vec::new();
    for project in projects {
        let project_type = project.get("type").and_then(toml::Value::as_str);
        if !matches!(project_type, Some("Service") | Some("Worker"))
            || !raw_contract_export_enabled(&project)
        {
            continue;
        }

        let Some(service_name) = project.get("name").and_then(toml::Value::as_str) else {
            continue;
        };

        let metadata = project.get("metadata").and_then(toml::Value::as_table);
        if metadata
            .and_then(|metadata| metadata.get("language"))
            .and_then(toml::Value::as_str)
            != Some("rust")
        {
            continue;
        }

        let output_file = outputs.exported(service_name)?.to_path_buf();
        let service_output_dir = output_file.parent().context("OpenAPI output parent missing")?;
        fs::create_dir_all(service_output_dir)
            .with_context(|| format!("Failed to create directory: {:?}", service_output_dir))?;
        let contract_path =
            app_root.join(artifact_paths::path(service_name, metadata, Artifact::Contract));
        if let Some(contract_dir) = contract_path.parent() {
            fs::create_dir_all(contract_dir)
                .with_context(|| format!("Failed to create {}", contract_dir.display()))?;
        }
        let crate_path = metadata
            .and_then(|metadata| metadata.get("cratePath"))
            .and_then(toml::Value::as_str)
            .map(|path| app_root.join(path))
            .unwrap_or_else(|| {
                let native = app_root.join("src").join(service_name);
                if native.join("Cargo.toml").is_file() {
                    native
                } else {
                    app_root.join("crates").join(service_name)
                }
            });

        let artifact = export_rust_service_artifacts(
            app_root,
            service_name,
            &crate_path,
            &output_file,
            &contract_path,
        )?;

        outputs.refresh(service_name)?;
        export_registered_sockets(app_root, service_name, metadata, &artifact)?;

        exported_services.push(service_name.to_string());
    }

    Ok(exported_services)
}

fn export_rust_service_artifacts(
    app_root: &Path,
    service_name: &str,
    crate_path: &Path,
    openapi_output: &Path,
    contract_output: &Path,
) -> Result<serde_json::Value> {
    if !crate_path.join("Cargo.toml").exists() {
        bail!(
            "Rust project {} does not have a Cargo.toml at {}",
            service_name,
            crate_path.display()
        );
    }

    // Export into separate files: the runtime registry has no SDK layout, while
    // Cargo may refresh that metadata in the service's normal build outputs.
    let staging = tempfile::tempdir()?;
    let staged_openapi = staging.path().join("openapi.json");
    let staged_contract = staging.path().join("contract.json");
    let cargo = resolve_command("cargo");
    let mut child = ProcessCommand::new(&cargo)
        .args(["run", "--manifest-path"])
        .arg(crate_path.join("Cargo.toml"))
        .arg("--quiet")
        .current_dir(app_root)
        .env("FORKLAUNCH_MODE", "contract")
        .env("FORKLAUNCH_OPENAPI_OUTPUT", &staged_openapi)
        .env("FORKLAUNCH_CONTRACT_OUTPUT", &staged_contract)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn cargo for Rust service {service_name}"))?;

    let stderr = child.stderr.take();
    let stderr_reader = thread::spawn(move || read_child_pipe(stderr));
    // A first contract export may compile the generated service and its dependencies.
    let timeout = Duration::from_secs(300);
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr = stderr_reader.join().unwrap_or_default();
                if !status.success() {
                    bail!(
                        "Rust service {} failed to export framework contract artifacts: {}",
                        service_name,
                        stderr
                    );
                }
                break;
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    bail!(
                        "Rust service {} timed out after {}s during framework contract export",
                        service_name,
                        timeout.as_secs()
                    );
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => bail!("Error waiting for Rust service {}: {}", service_name, error),
        }
    }

    if !staged_openapi.exists() {
        bail!(
            "Rust OpenAPI file was not created at {} for service {}",
            openapi_output.display(),
            service_name
        );
    }
    if !staged_contract.exists() {
        bail!(
            "Rust contract artifact was not created at {} for service {}",
            contract_output.display(),
            service_name
        );
    }

    let contract = fs::read_to_string(&staged_contract)
        .with_context(|| format!("Failed to read {}", staged_contract.display()))?;
    let mut artifact: serde_json::Value =
        serde_json::from_str(&contract).context("Invalid exported Rust contract artifact")?;
    if artifact["generator"] != "framework-rs" {
        bail!(
            "Rust contract artifact for {} was not generated by framework-rs",
            service_name
        );
    }
    compiler_metadata::restore(
        app_root, crate_path, service_name, contract_output, &mut artifact,
    )?;
    let mut document = serde_json::from_slice(&fs::read(&staged_openapi)?)?;
    if artifact["sdkCompiler"] == "forklaunch.sdk.http.v1" {
        portable::enrich(&mut document, &artifact, service_name)?;
    }
    outputs::write_json(contract_output, &artifact)?;
    outputs::write_json(openapi_output, &document)?;
    Ok(artifact)
}

pub(crate) fn export_registered_sockets(
    app_root: &Path,
    service_name: &str,
    metadata: Option<&toml::map::Map<String, toml::Value>>,
    artifact: &serde_json::Value,
) -> Result<()> {
    let root = app_root.canonicalize()?;
    let marker = crate::core::application_output::output_path(
        &root,
        &artifact_paths::socket_registry_marker(service_name, metadata),
    )?;
    let previous = read_socket_export_marker(&marker)?;
    let sockets = artifact
        .get("sockets")
        .map(|sockets| {
            sockets
                .as_object()
                .context("Exported socket contracts must be an object")
        })
        .transpose()?;
    if sockets.is_none_or(serde_json::Map::is_empty) {
        cleanup_socket_export(&root, previous.as_ref(), None)?;
        if previous.is_some() {
            fs::remove_file(marker)?;
        }
        return Ok(());
    }
    let sockets = sockets.unwrap();
    let relative = artifact_paths::path(service_name, metadata, Artifact::SocketContract);
    let output = crate::core::application_output::output_path(app_root, Path::new(&relative))?;
    let service = artifact
        .get("service")
        .context("Socket contract export requires service metadata")?;
    let title = service
        .get("name")
        .and_then(serde_json::Value::as_str)
        .context("Socket service name missing")?;
    let version = service
        .get("version")
        .and_then(serde_json::Value::as_str)
        .context("Socket service version missing")?;
    let portable =
        serde_json::json!({"formatVersion":1,"title":title,"version":version,"sockets":sockets});
    let content = format!("{}\n", serde_json::to_string_pretty(&portable)?);
    if content.len() > 16 * 1024 * 1024 {
        bail!("Socket contract artifact exceeds 16 MiB");
    }
    if output == marker
        || output
            .canonicalize()
            .ok()
            .zip(marker.canonicalize().ok())
            .is_some_and(|(output, marker)| output == marker)
    {
        bail!("Socket artifact path conflicts with registry export metadata");
    }
    fs::create_dir_all(output.parent().context("Socket output parent missing")?)?;
    write_socket_export(&output, content.as_bytes())?;
    cleanup_socket_export(&root, previous.as_ref(), Some(&output))?;
    let ownership = serde_json::json!({"formatVersion":1,"outputPath":output.strip_prefix(&root)?,"sha256":socket_export_digest(content.as_bytes())});
    fs::create_dir_all(marker.parent().context("Socket marker parent missing")?)?;
    write_socket_export(
        &marker,
        format!("{}\n", serde_json::to_string_pretty(&ownership)?).as_bytes(),
    )?;
    Ok(())
}

// Readers must observe either complete version, and identical exports keep timestamps.
fn write_socket_export(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;
    if fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    let path = if path.exists() {
        path.canonicalize()?
    } else {
        path.to_path_buf()
    };
    let mut pending =
        tempfile::NamedTempFile::new_in(path.parent().context("Socket export parent missing")?)?;
    pending.write_all(bytes)?;
    pending.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub(crate) fn rename_registered_sockets(
    app_root: &Path,
    old: &str,
    new: &str,
    metadata: Option<&toml::map::Map<String, toml::Value>>,
    artifact: Option<&serde_json::Value>,
) -> Result<bool> {
    let root = app_root.canonicalize()?;
    // `metadata` is the renamed project's: a service-owned SDK folder (and its
    // marker) moved with the crate, while legacy markers are named per service.
    let marker = crate::core::application_output::output_path(
        &root,
        &artifact_paths::socket_registry_marker(old, metadata),
    )?;
    let Some(previous) = read_socket_export_marker(&marker)? else {
        return Ok(false);
    };
    let artifact =
        artifact.context("Registered socket rename requires its exported framework contract")?;
    export_registered_sockets(&root, new, metadata, artifact)?;
    let output = artifact_paths::path(new, metadata, Artifact::SocketContract);
    let output = crate::core::application_output::output_path(&root, Path::new(&output))?;
    cleanup_socket_export(&root, Some(&previous), Some(&output))?;
    if marker
        == crate::core::application_output::output_path(
            &root,
            &artifact_paths::socket_registry_marker(new, metadata),
        )?
    {
        return Ok(true);
    }
    fs::remove_file(&marker)?;
    if let Some(parent) = marker.parent() {
        if parent.canonicalize().ok().as_deref() == Some(parent) {
            match fs::remove_dir(parent) {
                Ok(()) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(true)
}

fn socket_export_digest(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn read_socket_export_marker(marker: &Path) -> Result<Option<serde_json::Value>> {
    let metadata = match fs::metadata(marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.len() > 4096 {
        bail!("Invalid registry socket export metadata");
    }
    let value: serde_json::Value = serde_json::from_slice(&fs::read(marker)?)?;
    if value["formatVersion"] != 1
        || !value["outputPath"].is_string()
        || value["sha256"].as_str().is_none_or(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        bail!("Invalid registry socket export metadata");
    }
    Ok(Some(value))
}
fn cleanup_socket_export(
    root: &Path,
    previous: Option<&serde_json::Value>,
    keep: Option<&Path>,
) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let output = crate::core::application_output::output_path(
        root,
        Path::new(
            previous["outputPath"]
                .as_str()
                .context("Socket export path missing")?,
        ),
    )?;
    if keep.is_some_and(|keep| {
        keep == output
            || keep
                .canonicalize()
                .ok()
                .zip(output.canonicalize().ok())
                .is_some_and(|(keep, output)| keep == output)
    }) {
        return Ok(());
    }
    let metadata = match fs::metadata(&output) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    // Only remove the exact bounded bytes previously written by this exporter.
    if metadata.is_file()
        && metadata.len() <= 16 * 1024 * 1024
        && socket_export_digest(&fs::read(&output)?) == previous["sha256"].as_str().unwrap()
    {
        fs::remove_file(output)?;
    }
    Ok(())
}

fn read_child_pipe(pipe: Option<impl std::io::Read>) -> String {
    pipe.map(|mut output| {
        let mut buffer = String::new();
        let _ = output.read_to_string(&mut buffer);
        buffer
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod socket_export_tests {
    use serde_json::{Value, json};

    use super::*;

    fn registered() -> Value {
        json!({"service":{"name":"Realtime 水","version":"2.0.0"},"sockets":{"chat":{"path":"/chat","clientMessages":{"request":{"shape":{"type":"string","const":"water 水"}}}}}})
    }

    #[test]
    fn registry_exports_portable_artifacts_with_default_and_custom_destinations() {
        let root = tempfile::tempdir().unwrap();
        let artifact = registered();
        export_registered_sockets(root.path(), "realtime", None, &artifact).unwrap();
        let output = root
            .path()
            .join(".forklaunch/socket-contracts/realtime/socket-contract.json");
        let exported: Value = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
        assert_eq!(exported["formatVersion"], 1);
        assert_eq!(exported["title"], artifact["service"]["name"]);
        assert_eq!(exported["version"], "2.0.0");
        assert_eq!(exported["sockets"], artifact["sockets"]);
        let metadata: toml::Table =
            toml::from_str("socketContractArtifactPath = './custom/socket.json'").unwrap();
        export_registered_sockets(root.path(), "realtime", Some(&metadata), &artifact).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("custom/socket.json")).unwrap(),
            format!("{}\n", serde_json::to_string_pretty(&exported).unwrap())
        );
    }

    #[test]
    fn services_without_registered_sockets_preserve_manual_socket_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(".forklaunch/socket-contracts/realtime");
        fs::create_dir_all(&directory).unwrap();
        let output = directory.join("socket-contract.json");
        fs::write(&output, "manual artifact").unwrap();
        export_registered_sockets(root.path(), "realtime", None, &json!({"routes":[]})).unwrap();
        assert_eq!(fs::read_to_string(output).unwrap(), "manual artifact");
    }

    #[test]
    fn registry_rename_updates_custom_output_ownership_without_removing_active_file() {
        let root = tempfile::tempdir().unwrap();
        let metadata: toml::Table =
            toml::from_str("socketContractArtifactPath = 'custom/socket.json'").unwrap();
        let mut artifact = registered();
        export_registered_sockets(root.path(), "old", Some(&metadata), &artifact).unwrap();
        artifact["service"]["name"] = json!("new");
        assert!(
            rename_registered_sockets(root.path(), "old", "new", Some(&metadata), Some(&artifact))
                .unwrap()
        );
        let output: Value =
            serde_json::from_slice(&fs::read(root.path().join("custom/socket.json")).unwrap())
                .unwrap();
        assert_eq!(output["title"], "new");
        assert!(
            !root
                .path()
                .join(".forklaunch/socket-contracts/old")
                .exists()
        );
        assert!(
            root.path()
                .join(".forklaunch/socket-contracts/new/registry-export.json")
                .exists()
        );
        export_registered_sockets(root.path(), "new", None, &json!({"sockets":{}})).unwrap();
        assert!(!root.path().join("custom/socket.json").exists());
    }

    #[test]
    fn registry_export_removes_only_unchanged_owned_files_on_move_or_removal() {
        let root = tempfile::tempdir().unwrap();
        let artifact = registered();
        let default = root
            .path()
            .join(".forklaunch/socket-contracts/realtime/socket-contract.json");
        let marker = root
            .path()
            .join(".forklaunch/socket-contracts/realtime/registry-export.json");
        export_registered_sockets(root.path(), "realtime", None, &artifact).unwrap();
        assert!(default.exists());
        assert!(marker.exists());
        let metadata: toml::Table =
            toml::from_str("socketContractArtifactPath = 'custom/socket.json'").unwrap();
        export_registered_sockets(root.path(), "realtime", Some(&metadata), &artifact).unwrap();
        assert!(!default.exists());
        let custom = root.path().join("custom/socket.json");
        assert!(custom.exists());
        export_registered_sockets(
            root.path(),
            "realtime",
            Some(&metadata),
            &json!({"routes":[]}),
        )
        .unwrap();
        assert!(!custom.exists());
        assert!(!marker.exists());
        export_registered_sockets(root.path(), "realtime", None, &artifact).unwrap();
        fs::write(&default, "manually edited metadata").unwrap();
        export_registered_sockets(root.path(), "realtime", None, &json!({"sockets":{}})).unwrap();
        assert_eq!(
            fs::read_to_string(default).unwrap(),
            "manually edited metadata"
        );
        assert!(!marker.exists());
    }

    #[cfg(unix)]
    #[test]
    fn registry_output_aliases_preserve_the_active_artifact() {
        let root = tempfile::tempdir().unwrap();
        let artifact = registered();
        export_registered_sockets(root.path(), "realtime", None, &artifact).unwrap();
        let output = root
            .path()
            .join(".forklaunch/socket-contracts/realtime/socket-contract.json");
        std::os::unix::fs::symlink(&output, root.path().join("alias.json")).unwrap();
        let metadata: toml::Table =
            toml::from_str("socketContractArtifactPath = 'alias.json'").unwrap();
        export_registered_sockets(root.path(), "realtime", Some(&metadata), &artifact).unwrap();
        assert_eq!(
            fs::read(root.path().join("alias.json")).unwrap(),
            fs::read(output).unwrap()
        );
    }

    #[test]
    fn registry_exports_reject_external_paths_and_symlinks_before_writing() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let artifact = registered();
        for path in [
            outside.path().join("socket.json"),
            Path::new("../escape.json").to_path_buf(),
        ] {
            let metadata: toml::Table = toml::from_str(&format!(
                "socketContractArtifactPath = {}",
                serde_json::to_string(&path).unwrap()
            ))
            .unwrap();
            assert!(
                export_registered_sockets(root.path(), "realtime", Some(&metadata), &artifact)
                    .is_err()
            );
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
            let metadata: toml::Table =
                toml::from_str("socketContractArtifactPath = 'linked/socket.json'").unwrap();
            assert!(
                export_registered_sockets(root.path(), "realtime", Some(&metadata), &artifact)
                    .is_err()
            );
        }
        assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    }
}

#[cfg(test)]
mod native_package_tests {
    use super::*;

    #[test]
    fn opted_out_auth_and_pure_worker_are_never_planned_or_invoked() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for path in [".forklaunch", "src/auth", "src/insights"] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        fs::write(
            root.join("src/auth/server.ts"),
            "throw new Error('opted-out auth exporter was invoked');",
        )
        .unwrap();
        fs::write(
            root.join("src/insights/Cargo.toml"),
            r#"
[package]
name = "insights"
version = "0.0.0"
edition = "2024"
[[bin]]
name = "insights"
path = "main.rs"
[workspace]
"#,
        )
        .unwrap();
        fs::write(
            root.join("src/insights/main.rs"),
            "fn main() { panic!(\"opted-out worker exporter was invoked\"); }",
        )
        .unwrap();
        let source = r#"
id = "test"
cli_version = "0.1.0"
app_name = "native-review"
modules_path = "src"
app_description = "Native review"
linter = "clippy"
formatter = "rustfmt"
validator = "garde"
http_framework = "axum"
runtime = "tokio"
author = "test"
license = "MIT"
project_peer_topology = {}
[[projects]]
type = "Service"
name = "auth"
description = "Bare auth sidecar"
[projects.metadata]
language = "typescript"
contractExport = false
openapiPath = "../must-not-plan-auth.json"
[[projects]]
type = "Worker"
name = "insights"
description = "Pure queue worker"
[projects.metadata]
language = "rust"
cratePath = "src/insights"
contract_export = false
openapiPath = "../must-not-plan-worker.json"
"#;
        fs::write(root.join(".forklaunch/manifest.toml"), source).unwrap();
        let manifest: ApplicationManifestData = toml::from_str(source).unwrap();
        let output = root.join("exports");
        assert!(
            export_all_services(root, &manifest, Some(&output))
                .unwrap()
                .is_empty()
        );
        assert!(export_all_rust_services(root, Some(&output)).unwrap().is_empty());
        assert!(!output.exists());
        let serialized = toml::to_string(&manifest).unwrap();
        assert_eq!(serialized.matches("contractExport = false").count(), 2);
    }

    #[test]
    fn exports_registered_crate_with_different_package_and_default_binary_names() {
        assert_export_layout(false);
    }

    #[test]
    fn exports_flat_native_project_without_explicit_crate_metadata() {
        assert_export_layout(true);
    }

    fn assert_export_layout(native: bool) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let crate_path = root.join(if native {
            "src/registered-service"
        } else {
            "custom/native api"
        });
        let source_prefix = if native { "" } else { "src/" };
        fs::create_dir_all(crate_path.join(source_prefix)).unwrap();
        fs::create_dir_all(root.join(".forklaunch")).unwrap();
        fs::write(
            crate_path.join("Cargo.toml"),
            r#"[package]
name = "custom-native-package"
version = "0.0.0"
edition = "2024"
default-run = "custom-server"
[[bin]]
name = "custom-server"
path = "main.rs"
[[bin]]
name = "admin-tool"
path = "admin.rs"
[workspace]
"#
            .replace(
                "path = \"main.rs\"",
                &format!("path = \"{source_prefix}main.rs\""),
            )
            .replace(
                "path = \"admin.rs\"",
                &format!("path = \"{source_prefix}admin.rs\""),
            ),
        )
        .unwrap();
        fs::write(crate_path.join(format!("{source_prefix}main.rs")), r#"fn main() {
    assert_eq!(std::env::var("FORKLAUNCH_MODE").unwrap(), "contract");
    std::fs::write(std::env::var("FORKLAUNCH_OPENAPI_OUTPUT").unwrap(), "{\"openapi\":\"3.1.0\"}").unwrap();
    std::fs::write(std::env::var("FORKLAUNCH_CONTRACT_OUTPUT").unwrap(), "{\"generator\":\"framework-rs\",\"routes\":[]}").unwrap();
}"#).unwrap();
        fs::write(
            crate_path.join(format!("{source_prefix}admin.rs")),
            "fn main() { panic!(\"wrong binary\"); }",
        )
        .unwrap();
        fs::write(
            root.join(".forklaunch/manifest.toml"),
            r#"[[projects]]
name = "registered-service"
type = "Service"
[projects.metadata]
language = "rust"
cratePath = "custom/native api"
contractArtifactPath = "custom/contracts/native.json"
openapiPath = "custom/specs/native.json"
"#
            .replace(
                "cratePath = \"custom/native api\"",
                if native {
                    ""
                } else {
                    "cratePath = \"custom/native api\""
                },
            ),
        )
        .unwrap();
        let output = root.join("exported");
        assert_eq!(
            export_all_rust_services(root, Some(&output)).unwrap(),
            ["registered-service"]
        );
        assert_eq!(
            fs::read(root.join("custom/specs/native.json")).unwrap(),
            fs::read(output.join("registered-service/openapi.json")).unwrap()
        );
        assert!(root.join("custom/contracts/native.json").is_file());
    }
}

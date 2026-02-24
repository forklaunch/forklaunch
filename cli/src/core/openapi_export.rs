use std::{env, fs::create_dir_all, path::Path, process::Command as ProcessCommand};

use anyhow::{Context, Result, bail};

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
    if let Ok(nvm_dir) = env::var("NVM_DIR").or_else(|_| Ok::<String, ()>(format!("{}/.nvm", home))) {
        let nvm_versions = Path::new(&nvm_dir).join("versions").join("node");
        if nvm_versions.is_dir() {
            // Find the latest installed node version
            if let Ok(entries) = std::fs::read_dir(&nvm_versions) {
                let mut versions: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_dir())
                    .collect();
                versions.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
                for version_entry in versions {
                    fallback_paths.push(
                        format!("{}/bin/{}", version_entry.path().display(), name)
                    );
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
            } else if var_name.contains("SECRET") || var_name.contains("KEY") {
                if var_name == "HMAC_SECRET_KEY" {
                    // Generate a deterministic HMAC secret
                    // This ensures consistent values for testing/documentation
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
            } else if var_name == "JWKS_PUBLIC_KEY_URL" {
                if let Some(port) = iam_port {
                    format!("http://localhost:{}/api/auth/jwks", port)
                } else {
                    // Don't generate JWKS URL if no IAM service exists
                    "dummy-jwks-url".to_string()
                }
            } else if var_name.contains("URL") {
                "http://localhost:3000".to_string()
            } else {
                "dummy-value".to_string()
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
        _ => "dummy-value".to_string(),
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
    cmd.args(&args)
        .current_dir(service_path)
        .env("FORKLAUNCH_MODE", "openapi")
        .env("FORKLAUNCH_OPENAPI_OUTPUT", output_file);

    for env_var in env_vars {
        let dummy_value = generate_dummy_value(&env_var.var_name, "string", iam_port);
        cmd.env(&env_var.var_name, &dummy_value);
    }

    let output = cmd
        .output()
        .with_context(|| format!("Failed to run {} command", resolved_bin))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Service {} failed to export: {}", service_name, stderr);
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

pub(crate) fn export_all_services(
    app_root: &Path,
    manifest: &ApplicationManifestData,
    output_dir: &Path,
) -> Result<Vec<String>> {
    use crate::core::rendered_template::RenderedTemplatesCache;

    let mut exported_services = Vec::new();

    let runtime = manifest.runtime.as_str();
    let modules_path = app_root.join(&manifest.modules_path);

    let rendered_templates_cache = RenderedTemplatesCache::new();
    let all_env_vars = find_all_env_vars(&modules_path, &rendered_templates_cache)?;

    // Include both services and workers (workers have a server component)
    // Exclude observability services as they are not user application components
    const OBSERVABILITY_SERVICES: &[&str] = &[
        "prometheus",
        "loki",
        "tempo",
        "grafana",
        "otel",
        "otel-collector",
    ];

    let services: Vec<_> = manifest
        .projects
        .iter()
        .filter(|p| {
            (p.r#type == ProjectType::Service || p.r#type == ProjectType::Worker)
                && !OBSERVABILITY_SERVICES.contains(&p.name.as_str())
        })
        .collect();

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
                if let Ok(content) = std::fs::read_to_string(&docker_compose_path) {
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
        let service_output_dir = output_dir.join(&service.name);

        create_dir_all(&service_output_dir)
            .with_context(|| format!("Failed to create directory: {:?}", service_output_dir))?;

        let openapi_file = service_output_dir.join("openapi.json");

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

        exported_services.push(service.name.clone());
    }

    Ok(exported_services)
}

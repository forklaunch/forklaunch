use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    fs::{self, create_dir_all, read_to_string},
    io::Write,
    path::{Path, PathBuf},
};

/// Scope guard that removes a directory when dropped, warning on failure.
struct RemoveDirGuard {
    path: Option<PathBuf>,
}

impl RemoveDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    /// Disarm the guard so it does not remove the directory on drop.
    #[allow(dead_code)]
    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for RemoveDirGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            if let Err(e) = fs::remove_dir_all(&path) {
                eprintln!(
                    "Warning: failed to clean up temporary directory {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct ApplicationGitInfo {
    #[serde(rename = "gitRepository")]
    git_repository: Option<String>,
}

/// Opens the platform git integration page and polls until the git repository
/// URL is configured. Returns the git repository URL.
fn poll_for_git_repository(
    auth_mode: &AuthMode,
    application_id: &str,
    stdout: &mut StandardStream,
) -> Result<String> {
    let integration_url = format!(
        "{}/apps/{}/settings",
        get_platform_ui_url(),
        application_id
    );

    log_info!(
        stdout,
        "Opening git integration page in your browser..."
    );
    writeln!(stdout, "  {}", integration_url)?;

    if let Err(e) = opener::open(&integration_url) {
        log_warn!(
            stdout,
            "Could not open browser automatically: {}",
            e
        );
        log_info!(stdout, "Please open the URL above manually.");
    }

    log_info!(stdout, "Waiting for git repository to be connected...");

    let url = format!(
        "{}/applications/{}",
        get_platform_management_api_url(),
        application_id
    );

    loop {
        sleep(Duration::from_secs(3));

        let response = http_client::get_with_auth(auth_mode, &url);

        match response {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(app) = resp.json::<ApplicationGitInfo>() {
                    if let Some(git_repo) = app.git_repository {
                        if !git_repo.is_empty() {
                            log_ok!(stdout, "Git repository connected: {}", git_repo);
                            return Ok(git_repo);
                        }
                    }
                }
            }
            Ok(resp) => {
                log_warn!(
                    stdout,
                    "Unexpected response while polling (HTTP {}). Retrying...",
                    resp.status()
                );
            }
            Err(_) => {
                // Transient network error — keep polling
            }
        }
    }
}

use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use dialoguer::{Select, theme::ColorfulTheme};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};
use toml::to_string_pretty;

use super::{
    git::{get_git_branch, get_git_commit, is_git_repo},
    manifest_generator::{
        EnvironmentVariableComponent, EnvironmentVariableComponentProperty,
        EnvironmentVariableComponentType, EnvironmentVariableRequirement, EnvironmentVariableScope,
        ReleaseManifest, generate_release_manifest,
    },
};
use crate::{
    CliCommand,
    constants::{get_platform_management_api_url, get_platform_ui_url},
    core::{
        ast::infrastructure::{
            env::find_all_env_vars,
            integrations::find_all_integrations,
            runtime_deps::{find_all_runtime_deps, get_unique_resource_types},
            service_dependencies::find_all_service_dependencies,
            worker_config::find_all_worker_configs,
        },
        command::command,
        docker::{DockerCompose, DockerService, find_docker_compose_path},
        env::{find_workspace_root, get_modules_path},
        env_scope::{
            EnvironmentVariableScope as EnvScope, ScopedEnvVar, determine_env_var_scopes,
            is_application_scoped_var, is_inter_service_url, is_never_application_scoped,
            parse_inter_service_url_var,
        },
        hmac::AuthMode,
        http_client,
        manifest::{ProjectType, application::ApplicationManifestData},
        openapi_export::export_all_services,
        rendered_template::RenderedTemplatesCache,
        validate::{require_active_account, require_integration, require_manifest, resolve_auth},
    },
    sync::all::sync_all_projects,
};

#[derive(Debug, Serialize)]
struct CreateReleaseRequest {
    #[serde(rename = "applicationId")]
    application_id: String,
    manifest: ReleaseManifest,
    #[serde(rename = "releasedBy", skip_serializing_if = "Option::is_none")]
    released_by: Option<String>,
}

#[derive(Debug)]
pub(crate) struct CreateCommand;

impl CreateCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command("create", "Create a new release")
            .disable_version_flag(true)
            .arg(
                Arg::new("release_version")
                    .long("version")
                    .short('v')
                    .required(true)
                    .help("Release version (e.g., 1.0.0)"),
            )
            .arg(
                Arg::new("notes")
                    .long("notes")
                    .short('n')
                    .help("Release notes (optional)"),
            )
            .arg(
                Arg::new("base_path")
                    .long("path")
                    .short('p')
                    .help("Path to application root (optional)"),
            )
            .arg(
                Arg::new("dry-run")
                    .long("dry-run")
                    .action(clap::ArgAction::SetTrue)
                    .help("Simulate release creation without uploading"),
            )
            .arg(
                Arg::new("local")
                    .long("local")
                    .action(clap::ArgAction::SetTrue)
                    .help("Package local code and upload to S3 (for CI/CD testing without GitHub)"),
            )
            .arg(
                Arg::new("skip-sync")
                    .long("skip-sync")
                    .action(clap::ArgAction::SetTrue)
                    .help("Skip automatic sync of projects with manifest before creating release"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let auth_mode = resolve_auth()?;
        require_active_account(&auth_mode)?;
        let (app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;

        let version = matches
            .get_one::<String>("release_version")
            .ok_or_else(|| anyhow::anyhow!("Version is required"))?;

        let dry_run = matches.get_flag("dry-run");
        let local_mode = matches.get_flag("local");
        let skip_sync = matches.get_flag("skip-sync");

        let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
        let mut manifest = manifest;

        // Step 0: Sync projects with manifest (unless skipped)
        if !skip_sync {
            log_header!(stdout, Color::Cyan, "Syncing projects with manifest...");
            writeln!(stdout)?;

            let mut rendered_templates_cache = RenderedTemplatesCache::new();

            // Perform sync with confirm_all=true to avoid prompts during release
            let changes_made = match sync_all_projects(
                &app_root,
                &mut manifest,
                &mut rendered_templates_cache,
                true, // confirm_all - no interactive prompts
                &HashMap::new(),
                &mut stdout,
            ) {
                Ok(changed) => changed,
                Err(e) => {
                    log_error!(stdout, "Sync failed: {}", e);
                    bail!("Failed to sync projects with manifest: {}", e);
                }
            };

            if changes_made {
                // Update manifest.toml with synced data
                let updated_manifest_content = to_string_pretty(&manifest)
                    .with_context(|| "Failed to serialize updated manifest")?;
                fs::write(&manifest_path, updated_manifest_content)
                    .with_context(|| "Failed to write updated manifest")?;

                log_ok!(stdout, "Sync completed with changes");

                log_header!(stdout, Color::Yellow, "Manifest was updated. Please commit the changes to manifest.toml");
                writeln!(stdout)?;
            } else {
                log_ok!(stdout, "Sync completed - no changes detected");
                writeln!(stdout)?;
            }
        } else {
            log_info!(stdout, "Skipping project sync (--skip-sync flag set)");
            writeln!(stdout)?;
        }

        // When not in a git repo and not already in local mode, let the user choose
        let mut local_mode = local_mode;
        if !is_git_repo() && !local_mode {
            log_warn!(stdout, "Not a git repository");
            writeln!(stdout)?;

            let options = [
                "Connect GitHub repository (opens browser)",
                "Package locally (upload code directly)",
            ];
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("How would you like to proceed?")
                .items(&options)
                .default(0)
                .interact()?;

            match selection {
                0 => {
                    // Open integration page and poll for git repository
                    let git_repo = poll_for_git_repository(
                        &auth_mode,
                        &application_id,
                        &mut stdout,
                    )?;

                    manifest.git_repository = Some(git_repo);

                    let manifest_str =
                        to_string_pretty(&manifest).with_context(|| "Failed to serialize manifest")?;
                    fs::write(&manifest_path, manifest_str)
                        .with_context(|| "Failed to write manifest")?;

                    log_ok!(stdout, "Git repository saved to manifest.toml");
                }
                _ => {
                    local_mode = true;
                }
            }
        }

        // Prompt for git repository URL if not set and not using local mode
        if !local_mode && manifest.git_repository.is_none() {
            log_info!(stdout, "Git repository URL not set in manifest");

            print!("Enter git repository URL (e.g., https://github.com/user/repo.git): ");
            std::io::stdout().flush()?;

            let mut git_repo = String::new();
            std::io::stdin().read_line(&mut git_repo)?;
            let git_repo = git_repo.trim().to_string();

            if !git_repo.is_empty() {
                manifest.git_repository = Some(git_repo);

                let manifest_str =
                    to_string_pretty(&manifest).with_context(|| "Failed to serialize manifest")?;
                fs::write(&manifest_path, manifest_str)
                    .with_context(|| "Failed to write manifest")?;

                log_ok!(stdout, "Git repository saved to manifest.toml");
            }
        }

        if local_mode {
            log_info!(stdout, "Using local mode - packaging code directly");
        }

        log_header!(stdout, Color::Cyan, "Creating release {}...", version);
        writeln!(stdout)?;

        // Step 1: Detect git metadata
        log_progress!(stdout, "  Detecting git metadata...");

        let (git_commit, git_branch) = if is_git_repo() {
            let commit = get_git_commit()?;
            let branch = get_git_branch().ok();
            log_ok_suffix!(stdout);
            (commit, branch)
        } else if local_mode {
            log_warn!(stdout, "Not a git repository (using local defaults)");
            ("local-build".to_string(), Some("local".to_string()))
        } else {
            log_warn!(stdout, "Not a git repository");
            bail!("Current directory is not a git repository. Initialize git first.");
        };

        writeln!(
            stdout,
            "[INFO] Commit: {} ({})",
            if git_commit == "local-build" {
                "local"
            } else {
                &git_commit[..8]
            },
            git_branch.as_deref().unwrap_or("unknown")
        )?;

        // Step 2: Export OpenAPI specs
        log_progress!(stdout, "Exporting OpenAPI specifications...");

        let openapi_path = app_root.join(".forklaunch").join("openapi");
        create_dir_all(&openapi_path).with_context(|| "Failed to create openapi directory")?;
        let _openapi_guard = RemoveDirGuard::new(openapi_path.clone());

        let exported_services = export_all_services(&app_root, &manifest, &openapi_path)?;

        log_ok!(stdout, "({} services)", exported_services.len());

        let mut openapi_specs = HashMap::new();
        for project in &manifest.projects {
            let openapi_file = openapi_path.join(&project.name).join("openapi.json");
            if openapi_file.exists() {
                let content = read_to_string(&openapi_file)?;
                let spec: Value = serde_json::from_str(&content)?;
                openapi_specs.insert(project.name.clone(), spec);
            }
        }

        // openapi cleanup is handled by _openapi_guard (Drop)

        log_progress!(stdout, "Detecting required environment variables...");

        let workspace_root = find_workspace_root(&app_root)?;
        let modules_path = get_modules_path(&workspace_root)?;

        let rendered_templates_cache = RenderedTemplatesCache::new();
        let project_env_vars = find_all_env_vars(&modules_path, &rendered_templates_cache)?;

        let mut scoped_env_vars = determine_env_var_scopes(&project_env_vars, &manifest)?;

        // For vars that must never be application-scoped, replace the single
        // application entry with per-component entries for each project that
        // declares the var in registrations.ts, source files, or .env.local.
        // The docker-compose loop below may add additional per-component entries.
        {
            let never_app_vars: Vec<String> = scoped_env_vars
                .iter()
                .filter(|v| {
                    v.scope == EnvScope::Application && is_never_application_scoped(&v.name)
                })
                .map(|v| v.name.clone())
                .collect();

            scoped_env_vars.retain(|v| {
                !(v.scope == EnvScope::Application && is_never_application_scoped(&v.name))
            });

            // Collect .env.local vars per project
            let mut env_local_vars: HashMap<String, HashSet<String>> = HashMap::new();
            for project in &manifest.projects {
                let env_local_path = app_root
                    .join(&manifest.modules_path)
                    .join(&project.name)
                    .join(".env.local");
                if let Ok(contents) = read_to_string(&env_local_path) {
                    let vars: HashSet<String> = contents
                        .lines()
                        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
                        .filter_map(|line| {
                            line.split('=').next().map(|k| k.trim().to_string())
                        })
                        .collect();
                    env_local_vars.insert(project.name.clone(), vars);
                }
            }

            for var_name in &never_app_vars {
                for (project_name, env_vars) in &project_env_vars {
                    let in_source = env_vars.iter().any(|v| v.var_name == *var_name);
                    let in_env_local = env_local_vars
                        .get(project_name)
                        .is_some_and(|vars| vars.contains(var_name));

                    if !in_source && !in_env_local {
                        continue;
                    }

                    let project_type = manifest
                        .projects
                        .iter()
                        .find(|p| &p.name == project_name)
                        .map(|p| &p.r#type);

                    let (scope, scope_id) = match project_type {
                        Some(ProjectType::Service) => (
                            EnvScope::Service,
                            Some(project_name.clone()),
                        ),
                        Some(ProjectType::Worker) => (
                            EnvScope::Worker,
                            Some(format!("{}-worker", project_name)),
                        ),
                        _ => continue,
                    };

                    scoped_env_vars.push(ScopedEnvVar {
                        name: var_name.clone(),
                        scope,
                        scope_id,
                        used_by: vec![project_name.clone()],
                        value: None,
                    });
                }
            }
        }

        let (mut env_var_components, docker_compose_env_vars) =
            build_env_var_component_map(app_root.as_path(), &manifest);

        // Collect all var names from docker-compose before consuming the map
        let _docker_compose_var_names: HashSet<String> = docker_compose_env_vars
            .values()
            .flat_map(|vars| vars.iter().map(|(k, _)| k.clone()))
            .collect();

        // Add all env vars from docker-compose for each service/worker
        let mut existing_vars: HashSet<(String, Option<String>)> =
            scoped_env_vars
                .iter()
                .map(|v| (v.name.clone(), v.scope_id.clone()))
                .collect();

        // Inject platform defaults at APPLICATION scope.
        // These are hardcoded defaults that apply to all services/workers.
        // Users can override via config push. NODE_ENV uses {{environment}} template.
        // Docker-compose values for these keys are ignored (no component overrides).
        // Platform defaults at APPLICATION scope with hardcoded values.
        // Empty string = Pulumi injects the real value at deploy time.
        let platform_defaults: Vec<(&str, &str)> = vec![
            ("PORT", "8000"),
            ("HOST", "0.0.0.0"),
            ("PROTOCOL", "http"),
            ("VERSION", "v1"),
            ("DB_SSL", "true"),
            ("DOCS_PATH", "/docs"),
            ("OTEL_LEVEL", "info"),
            ("NODE_ENV", "{{environment}}"),
            ("IAM_DB_NAME", "iam_database"),
            // Pulumi-injected app-scoped vars (empty — Pulumi fills at deploy time)
            ("DB_HOST", ""),
            ("DB_PORT", ""),
            ("DB_USER", ""),
            ("DB_PASSWORD", ""),
            ("PGSSLMODE", "no-verify"),
        ];
        let platform_default_keys: HashSet<String> = platform_defaults
            .iter()
            .map(|(k, _)| k.to_string())
            .collect();

        // Component-scoped vars injected by Pulumi at deploy time.
        // Stay in manifest at service/worker scope but with empty values.
        let pulumi_injected_component_vars: HashSet<&str> = [
            "REDIS_URL",
            "DB_NAME",
            "KAFKA_BROKERS", "KAFKA_BOOTSTRAP_SERVERS",
        ].iter().copied().collect();

        for (default_key, default_value) in &platform_defaults {
            let app_key = (default_key.to_string(), None);
            if !existing_vars.contains(&app_key) {
                scoped_env_vars.push(ScopedEnvVar {
                    name: default_key.to_string(),
                    scope: EnvScope::Application,
                    scope_id: None,
                    used_by: vec!["platform".to_string()],
                    value: Some(default_value.to_string()),
                });
                existing_vars.insert(app_key);
            }
        }

        for (service_name, env_vars) in docker_compose_env_vars {
            let project_types: HashMap<String, ProjectType> = manifest
                .projects
                .iter()
                .map(|p| (p.name.clone(), p.r#type.clone()))
                .collect();

            let worker_alias_info = classify_worker_alias(&service_name, &project_types);

            let (scope, scope_id) =
                if let Some((component_type, base_worker_name)) = worker_alias_info {
                    match component_type {
                        EnvironmentVariableComponentType::Service => (
                            EnvScope::Service,
                            Some(format!("{}-service", base_worker_name)),
                        ),
                        EnvironmentVariableComponentType::Worker => (
                            EnvScope::Worker,
                            Some(format!("{}-worker", base_worker_name)),
                        ),
                        _ => continue,
                    }
                } else {
                    // Check if it's a direct project match
                    let project_type = manifest
                        .projects
                        .iter()
                        .find(|p| p.name == service_name)
                        .map(|p| &p.r#type);

                    match project_type {
                        Some(ProjectType::Service) => (
                            EnvScope::Service,
                            Some(service_name.clone()),
                        ),
                        Some(ProjectType::Worker) => (
                            EnvScope::Worker,
                            Some(format!("{}-worker", service_name)),
                        ),
                        _ => continue, // Skip if not a service or worker
                    }
                };

            let project_names_for_scope: Vec<String> =
                manifest.projects.iter().map(|p| p.name.clone()).collect();

            for (key, value) in env_vars {
                // Skip platform defaults — they're already set at APPLICATION scope
                // with hardcoded values, no component overrides needed
                if platform_default_keys.contains(&key.to_ascii_uppercase()) {
                    continue;
                }

                // Pulumi-injected component vars: keep in manifest for component metadata.
                // DB_NAME is computed from the component name (same logic as Pulumi):
                //   strip -service/-worker suffix, replace - with _, append _database
                // Other Pulumi-injected vars get empty values — Pulumi fills at deploy time.
                let effective_value = if key.eq_ignore_ascii_case("DB_NAME") {
                    if let Some(ref sid) = scope_id {
                        let base = sid
                            .trim_end_matches("-service")
                            .trim_end_matches("-worker")
                            .replace('-', "_");
                        format!("{}_database", base)
                    } else {
                        String::new()
                    }
                } else if pulumi_injected_component_vars.contains(key.to_ascii_uppercase().as_str()) {
                    String::new()
                } else if is_inter_service_url(&key, &project_names_for_scope) {
                    // Inter-service URL vars are injected by Pulumi at deploy time
                    String::new()
                } else if is_pulumi_injected_url_var(&key) {
                    // Auth/infrastructure URL vars computed by Pulumi
                    String::new()
                } else {
                    value.clone()
                };

                let is_app_scoped = is_application_scoped_var(
                    &key,
                    &project_names_for_scope,
                ) && !is_never_application_scoped(&key);

                if is_app_scoped {
                    // Ensure an APPLICATION-scoped entry exists
                    let app_key = (key.clone(), None);
                    if !existing_vars.contains(&app_key) {
                        scoped_env_vars.push(ScopedEnvVar {
                            name: key.clone(),
                            scope: EnvScope::Application,
                            scope_id: None,
                            used_by: vec![service_name.clone()],
                            value: Some(effective_value.clone()),
                        });
                        existing_vars.insert(app_key);
                    }

                    // If this component has a different value, add a component-scoped override
                    let app_value = scoped_env_vars
                        .iter()
                        .find(|v| {
                            v.name == key
                                && v.scope
                                    == EnvScope::Application
                        })
                        .and_then(|v| v.value.as_deref());

                    if app_value != Some(effective_value.as_str()) {
                        let comp_key = (key.clone(), scope_id.clone());
                        if !existing_vars.contains(&comp_key) {
                            scoped_env_vars.push(ScopedEnvVar {
                                name: key.clone(),
                                scope: scope.clone(),
                                scope_id: scope_id.clone(),
                                used_by: vec![service_name.clone()],
                                value: Some(effective_value.clone()),
                            });
                            existing_vars.insert(comp_key);
                        }
                    }

                    continue;
                }

                if existing_vars.contains(&(key.clone(), scope_id.clone())) {
                    continue;
                }

                scoped_env_vars.push(ScopedEnvVar {
                    name: key.clone(),
                    scope: scope.clone(),
                    scope_id: scope_id.clone(),
                    used_by: vec![service_name.clone()],
                    value: Some(effective_value.clone()),
                });

                existing_vars.insert((key, scope_id.clone()));
            }
        }

        // Fallback: infer service/worker URL component from env var name
        // For _URL vars not already in env_var_components, check if the name
        // (minus _URL, converted to kebab-case) matches a project Service or Worker
        {
            let project_types: HashMap<String, ProjectType> = manifest
                .projects
                .iter()
                .map(|p| (p.name.clone(), p.r#type.clone()))
                .collect();

            for scoped_var in &scoped_env_vars {
                if env_var_components.contains_key(&scoped_var.name) {
                    continue;
                }

                let upper = scoped_var.name.to_ascii_uppercase();
                if !upper.ends_with("_URL") {
                    continue;
                }

                // BILLING_URL -> billing, PLATFORM_MANAGEMENT_URL -> platform-management
                let stripped = upper.trim_end_matches("_URL");
                let kebab = stripped.to_ascii_lowercase().replace('_', "-");

                if let Some(project_type) = project_types.get(&kebab) {
                    let component_type = match project_type {
                        ProjectType::Service => EnvironmentVariableComponentType::Service,
                        ProjectType::Worker => EnvironmentVariableComponentType::Worker,
                        _ => continue,
                    };
                    env_var_components.insert(
                        scoped_var.name.clone(),
                        (
                            component_type,
                            EnvironmentVariableComponentProperty::Url,
                            Some(kebab),
                            None,
                            None,
                        ),
                    );
                }
            }
        }

        // Filter out TEST_ prefixed vars (test-only, not needed in deployment)
        scoped_env_vars.retain(|v| !v.name.starts_with("TEST_"));

        // Filter out vars that are irrelevant in deployed environments
        const EXCLUDED_VARS: &[&str] = &["DOTENV_FILE_PATH"];
        scoped_env_vars.retain(|v| !EXCLUDED_VARS.contains(&v.name.as_str()));

        // Only keep application-level variables if they match the allowed criteria
        let project_names_for_retain: Vec<String> =
            manifest.projects.iter().map(|p| p.name.clone()).collect();
        scoped_env_vars.retain(|v| {
            if v.scope != EnvScope::Application {
                return true;
            }

            is_allowed_application_var(&v.name, &env_var_components, &project_names_for_retain)
        });

        // Cross-scope deduplication: remove service/worker copies when an application-scope copy exists
        deduplicate_cross_scope(&mut scoped_env_vars);

        log_ok!(stdout, "({} variables)", scoped_env_vars.len());

        log_progress!(stdout, "Detecting runtime dependencies...");

        let all_runtime_deps = find_all_runtime_deps(&modules_path, &rendered_templates_cache)?;

        // Convert to resource types per project
        let mut project_runtime_deps: HashMap<String, Vec<String>> = HashMap::new();
        for (project_name, deps) in &all_runtime_deps {
            let resource_types = get_unique_resource_types(deps);
            // Filter out "monitoring" as it's not a provisionable resource
            let filtered_types: Vec<String> = resource_types
                .into_iter()
                .filter(|t| t != "monitoring")
                .collect();
            if !filtered_types.is_empty() {
                project_runtime_deps.insert(project_name.clone(), filtered_types);
            }
        }

        let total_resources: usize = project_runtime_deps.values().map(|v| v.len()).sum();
        log_ok!(stdout, "({} resources)", total_resources);

        log_progress!(stdout, "Detecting integrations...");

        let all_integrations = find_all_integrations(&modules_path, &rendered_templates_cache)?;

        let total_integrations: usize = all_integrations.values().map(|v| v.len()).sum();
        log_ok!(stdout, "({} integrations)", total_integrations);

        log_progress!(stdout, "Detecting worker configurations...");

        let all_worker_configs =
            find_all_worker_configs(&modules_path, &rendered_templates_cache)?;

        let total_worker_configs = all_worker_configs.len();
        log_ok!(stdout, "({} workers)", total_worker_configs);

        log_progress!(stdout, "Detecting service mesh connections...");

        let all_service_deps =
            find_all_service_dependencies(&modules_path, &rendered_templates_cache)?;

        let total_service_deps: usize = all_service_deps.values().map(|v| v.len()).sum();
        log_ok!(stdout, "({} connections)", total_service_deps);

        let project_names_for_origin: Vec<String> =
            manifest.projects.iter().map(|p| p.name.clone()).collect();

        let required_env_vars: Vec<EnvironmentVariableRequirement> = scoped_env_vars
            .iter()
            .map(|v| EnvironmentVariableRequirement {
                name: v.name.clone(),
                scope: match v.scope {
                    EnvScope::Application => {
                        EnvironmentVariableScope::Application
                    }
                    EnvScope::Service => {
                        EnvironmentVariableScope::Service
                    }
                    EnvScope::Worker => {
                        EnvironmentVariableScope::Worker
                    }
                },
                scope_id: v.scope_id.clone(),
                component: env_var_components.get(&v.name).map(
                    |(component_type, property, target, path, passthrough)| {
                        EnvironmentVariableComponent {
                            r#type: component_type.clone(),
                            property: property.clone(),
                            target: target.clone(),
                            path: path.clone(),
                            passthrough: passthrough.clone(),
                        }
                    },
                ),
                origin: {
                    let inter_service = parse_inter_service_url_var(
                        &v.name,
                        &project_names_for_origin,
                    );
                    if is_platform_managed_var(&v.name)
                        || is_pulumi_provisioned_component(&v.name, &env_var_components)
                        || inter_service.is_some()
                    {
                        Some("platform".to_string())
                    } else {
                        Some("user".to_string())
                    }
                },
                inter_service_url: parse_inter_service_url_var(
                    &v.name,
                    &project_names_for_origin,
                )
                .map(|(target_service, transport, port_env_var)| {
                    super::manifest_generator::InterServiceUrlInfo {
                        target_service,
                        transport,
                        port_env_var,
                    }
                }),
            })
            .collect();

        let app_vars = scoped_env_vars
            .iter()
            .filter(|v| v.scope == EnvScope::Application)
            .count();
        let service_vars = scoped_env_vars
            .iter()
            .filter(|v| v.scope == EnvScope::Service)
            .count();
        let worker_vars = scoped_env_vars
            .iter()
            .filter(|v| v.scope == EnvScope::Worker)
            .count();
        if app_vars > 0 {
            log_info!(stdout, "Application-level: {}", app_vars);
        }
        if service_vars > 0 {
            log_info!(stdout, "Service-level: {}", service_vars);
        }
        if worker_vars > 0 {
            log_info!(stdout, "Worker-level: {}", worker_vars);
        }

        // Handle local mode: create tarball and upload to S3
        let code_source_url = if local_mode && !dry_run {
            log_info!(stdout, "\nPackaging local code...");

            let tarball_path = app_root.join(".forklaunch").join("release-code.tar.gz");
            super::s3_upload::create_app_tarball(&app_root, &modules_path, &tarball_path)?;

            log_ok!(stdout, "Tarball created");

            // Get presigned upload URL from platform
            log_progress!(stdout, "Getting upload URL from platform...");

            let upload_response =
                super::s3_upload::get_presigned_upload_url(&application_id, version, &auth_mode)?;

            log_ok_suffix!(stdout);

            log_progress!(stdout, "Uploading code to S3...");

            super::s3_upload::upload_to_s3(&tarball_path, &upload_response.upload_url)?;

            log_ok_suffix!(stdout);

            fs::remove_file(&tarball_path).ok();

            Some(upload_response.code_source_url)
        } else {
            None
        };

        // Upload OpenAPI specs to S3 (skip for dry-run, specs stay inline)
        let openapi_s3_keys: HashMap<String, String> = if !dry_run && !openapi_specs.is_empty() {
            let service_names: Vec<String> = openapi_specs.keys().cloned().collect();

            log_progress!(stdout, "Uploading OpenAPI specs to S3...");

            let upload_urls = super::s3_upload::get_openapi_upload_urls(
                &application_id,
                version,
                &service_names,
                &auth_mode,
            )?;

            let mut s3_keys = HashMap::new();
            for (service_name, spec_value) in &openapi_specs {
                if let Some(entry) = upload_urls.get(service_name) {
                    // Wrap the spec as { "v1": specObject } to match the expected Record<string, OpenAPIObject> shape
                    let wrapped_spec = serde_json::json!({ "v1": spec_value });
                    super::s3_upload::upload_json_to_s3(&wrapped_spec, &entry.upload_url)?;
                    s3_keys.insert(service_name.clone(), entry.s3_key.clone());
                }
            }

            log_ok!(stdout, "({} specs uploaded)", s3_keys.len());

            s3_keys
        } else {
            HashMap::new()
        };

        log_progress!(stdout, "Generating release manifest...");

        let release_manifest = generate_release_manifest(
            &app_root,
            application_id.clone(),
            version.clone(),
            git_commit.clone(),
            git_branch.clone(),
            code_source_url,
            &manifest,
            &openapi_specs,
            required_env_vars,
            &project_runtime_deps,
            &all_integrations,
            &all_worker_configs,
            &all_service_deps,
            &openapi_s3_keys,
        )?;

        log_ok_suffix!(stdout);

        if dry_run {
            log_warn!(stdout, "\n  [DRY RUN] Skipping upload to platform");

            let manifest_file = app_root.join(".forklaunch").join("release-manifest.json");
            fs::write(
                &manifest_file,
                serde_json::to_string_pretty(&release_manifest)?,
            )?;
            writeln!(
                stdout,
                "[INFO] Manifest written to: {}",
                manifest_file.display()
            )?;
        } else {
            let manifest_json = serde_json::to_string(&release_manifest)?;
            log_info!(
                stdout,
                "[INFO] Release manifest size: {} bytes",
                manifest_json.len()
            );

            log_progress!(stdout, "Uploading release to platform...");

            upload_release(&application_id, release_manifest, &auth_mode)?;

            log_ok_suffix!(stdout);

            manifest.release_version = Some(version.clone());

            let updated_manifest = to_string_pretty(&manifest)
                .with_context(|| "Failed to serialize updated manifest")?;

            fs::write(&manifest_path, updated_manifest)
                .with_context(|| "Failed to write updated manifest")?;
        }

        writeln!(stdout)?;
        log_header!(stdout, Color::Green, "Release {} created successfully!", version);

        if !dry_run {
            log_info!(stdout, "\nNext steps:");
            writeln!(stdout, "  1. Set environment variables in Platform UI")?;
            writeln!(
                stdout,
                "  2. forklaunch deploy create --release {} --environment <env> --region <region>",
                version
            )?;
        }

        Ok(())
    }
}

fn upload_release(
    application_id: &str,
    manifest: ReleaseManifest,
    auth_mode: &AuthMode,
) -> Result<()> {
    let request_body = CreateReleaseRequest {
        application_id: application_id.to_string(),
        manifest,
        released_by: None, // TODO: Get from token
    };

    let url = if auth_mode.is_hmac() {
        format!("{}/releases/internal", get_platform_management_api_url())
    } else {
        format!("{}/releases", get_platform_management_api_url())
    };

    let response =
        http_client::post_with_auth(auth_mode, &url, serde_json::to_value(&request_body)?)
            .with_context(|| "Failed to create release")?;

    let status = response.status();
    let response_body = response.text().unwrap_or_else(|_| "{}".to_string());

    let parsed_response: serde_json::Value = serde_json::from_str(&response_body)
        .unwrap_or_else(|_| serde_json::json!({ "raw": response_body }));

    if !status.is_success() {
        if status.as_u16() == 409 {
            bail!("Release version already exists. Bump the version in your manifest and try again.");
        }
        bail!(
            "Failed to create release: {} (Status: {})",
            parsed_response,
            status
        );
    }

    if let Some(warnings) = parsed_response.get("warnings") {
        if let Some(array) = warnings.as_array() {
            if !array.is_empty() {
                let mut stdout = StandardStream::stdout(ColorChoice::Always);
                log_warn!(stdout, "\nRelease created with warnings:");
                for warning in array {
                    log_warn!(stdout, "  - {}", warning);
                }
            }
        }
    }

    Ok(())
}

fn build_env_var_component_map(
    app_root: &Path,
    manifest: &ApplicationManifestData,
) -> (
    HashMap<
        String,
        (
            EnvironmentVariableComponentType,
            EnvironmentVariableComponentProperty,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >,
    HashMap<String, Vec<(String, String)>>, // service/worker name -> vec of (key, value)
) {
    let mut mapping = HashMap::new();
    let mut docker_compose_env_vars: HashMap<String, Vec<(String, String)>> = HashMap::new();

    if let Some(compose_path) = manifest
        .docker_compose_path
        .as_ref()
        .map(|path| app_root.join(path))
        .filter(|path| path.exists())
        .or_else(|| {
            find_docker_compose_path(app_root).map(|relative_path| app_root.join(relative_path))
        })
        .or_else(|| {
            let default_yaml = app_root.join("docker-compose.yaml");
            if default_yaml.exists() {
                Some(default_yaml)
            } else {
                let default_yml = app_root.join("docker-compose.yml");
                if default_yml.exists() {
                    Some(default_yml)
                } else {
                    None
                }
            }
        })
    {
        if let Ok(contents) = read_to_string(&compose_path) {
            match serde_yml::from_str::<DockerCompose>(&contents) {
                Ok(docker_compose) => {
                    let project_types: HashMap<_, _> = manifest
                        .projects
                        .iter()
                        .map(|project| (project.name.clone(), project.r#type.clone()))
                        .collect();

                    let mut service_lookup: HashMap<
                        String,
                        (EnvironmentVariableComponentType, String),
                    > = HashMap::new();

                    for (compose_key, compose_service) in docker_compose.services.iter() {
                        if let Some((component_type, canonical_target)) =
                            classify_compose_service(compose_key, &project_types)
                        {
                            let is_worker =
                                matches!(&component_type, EnvironmentVariableComponentType::Worker);

                            insert_service_identifier(
                                &mut service_lookup,
                                compose_key,
                                &component_type,
                                &canonical_target,
                            );

                            if let Some(hostname) = &compose_service.hostname {
                                insert_service_identifier(
                                    &mut service_lookup,
                                    hostname,
                                    &component_type,
                                    &canonical_target,
                                );
                            }

                            if let Some(container_name) = &compose_service.container_name {
                                insert_service_identifier(
                                    &mut service_lookup,
                                    container_name,
                                    &component_type,
                                    &canonical_target,
                                );
                            }

                            if is_worker {
                                insert_service_identifier(
                                    &mut service_lookup,
                                    &canonical_target,
                                    &component_type,
                                    &canonical_target,
                                );
                            }
                        }
                    }

                    for (service_name, service) in docker_compose.services.iter() {
                        if let Some(environment) = &service.environment {
                            // Collect all env vars for this service/worker
                            let mut service_env_vars = Vec::new();

                            for (key, value) in environment.iter() {
                                // Store the env var for this service
                                service_env_vars.push((key.clone(), value.clone()));

                                // Infer component details
                                let mut passthrough = None;
                                let key_upper = key.to_ascii_uppercase();

                                // Handle PORT specially - set passthrough to "8000"
                                if key_upper == "PORT" {
                                    passthrough = Some("8000".to_string());
                                }

                                if let Some((
                                    component_type,
                                    property,
                                    target,
                                    path,
                                    inferred_passthrough,
                                )) = infer_component_details(
                                    service_name,
                                    service,
                                    &project_types,
                                    &service_lookup,
                                    key,
                                    value,
                                ) {
                                    // Use the passthrough from PORT handling if set
                                    let final_passthrough = passthrough.or(inferred_passthrough);

                                    match mapping.entry(key.clone()) {
                                        Entry::Vacant(entry) => {
                                            entry.insert((
                                                component_type,
                                                property,
                                                target,
                                                path,
                                                final_passthrough,
                                            ));
                                        }
                                        Entry::Occupied(mut entry) => {
                                            let current = entry.get();
                                            // Preserve existing passthrough unless we have a new one
                                            let passthrough = final_passthrough.clone().or_else(|| current.4.clone());
                                            entry.insert((
                                                component_type,
                                                property,
                                                target,
                                                path,
                                                passthrough,
                                            ));
                                        }
                                    }
                                } else if passthrough.is_some() {
                                    // PORT that wasn't inferred, but we want to set passthrough
                                    let property = infer_component_property(&key_upper)
                                        .unwrap_or_else(|| {
                                            default_component_property(
                                                &EnvironmentVariableComponentType::Service,
                                                &key_upper,
                                            )
                                        });
                                    mapping.insert(
                                        key.clone(),
                                        (
                                            EnvironmentVariableComponentType::Service,
                                            property,
                                            None,
                                            None,
                                            passthrough,
                                        ),
                                    );
                                }
                            }

                            if project_types.contains_key(service_name) {
                                docker_compose_env_vars
                                    .insert(service_name.clone(), service_env_vars);
                            } else if classify_worker_alias(service_name, &project_types).is_some()
                            {
                                docker_compose_env_vars
                                    .insert(service_name.clone(), service_env_vars);
                            }
                        }
                    }
                }
                Err(err) => {
                    eprintln!(
                        "Failed to parse docker-compose at {}: {err}",
                        compose_path.display()
                    );
                }
            }
        }
    }

    (mapping, docker_compose_env_vars)
}

fn classify_compose_service(
    compose_key: &str,
    project_types: &HashMap<String, ProjectType>,
) -> Option<(EnvironmentVariableComponentType, String)> {
    if let Some(project_type) = project_types.get(compose_key) {
        return match project_type {
            ProjectType::Service => Some((
                EnvironmentVariableComponentType::Service,
                compose_key.to_string(),
            )),
            ProjectType::Worker => Some((
                EnvironmentVariableComponentType::Worker,
                compose_key.to_string(),
            )),
            _ => None,
        };
    }

    classify_worker_alias(compose_key, project_types)
}

fn classify_worker_alias(
    compose_key: &str,
    project_types: &HashMap<String, ProjectType>,
) -> Option<(EnvironmentVariableComponentType, String)> {
    const WORKER_ALIAS_SUFFIXES: &[(&str, EnvironmentVariableComponentType)] = &[
        ("-worker", EnvironmentVariableComponentType::Worker),
        ("-service", EnvironmentVariableComponentType::Service),
        ("-server", EnvironmentVariableComponentType::Service),
    ];

    for (suffix, component_type) in WORKER_ALIAS_SUFFIXES {
        if let Some(base) = compose_key.strip_suffix(suffix) {
            if matches!(project_types.get(base), Some(ProjectType::Worker)) {
                return Some((component_type.clone(), base.to_string()));
            }
        }
    }

    None
}

fn infer_component_details(
    service_name: &str,
    service: &DockerService,
    project_types: &HashMap<String, ProjectType>,
    service_lookup: &HashMap<String, (EnvironmentVariableComponentType, String)>,
    key: &str,
    value: &str,
) -> Option<(
    EnvironmentVariableComponentType,
    EnvironmentVariableComponentProperty,
    Option<String>,
    Option<String>,
    Option<String>,
)> {
    let key_upper = key.to_ascii_uppercase();

    // OTEL_SERVICE_NAME is per-service/worker and must NOT use passthrough.
    // The platform sets it dynamically per component at deploy time.
    // Without component metadata, resolveComponentInitialValue returns undefined.
    if key_upper == "OTEL_SERVICE_NAME" {
        return None;
    }

    // Pulumi-injected URL vars have known production targets that differ from
    // docker-compose dev targets. Override the target to reflect the actual
    // service the URL resolves to in production.
    if let Some(target) = pulumi_url_var_target(&key_upper) {
        let property = infer_component_property(&key_upper).unwrap_or_else(|| {
            default_component_property(&EnvironmentVariableComponentType::Service, &key_upper)
        });
        let path = extract_path_from_value(value);
        return Some((
            EnvironmentVariableComponentType::Service,
            property,
            Some(target),
            path,
            None,
        ));
    }

    // Inter-service URL vars: target is the service the URL resolves to,
    // derived from the var name pattern (e.g. BILLING_URL → "billing").
    {
        let project_names: Vec<String> = project_types.keys().cloned().collect();
        if let Some((target_service, ..)) =
            parse_inter_service_url_var(key, &project_names)
        {
            let property = infer_component_property(&key_upper).unwrap_or_else(|| {
                default_component_property(&EnvironmentVariableComponentType::Service, &key_upper)
            });
            let path = extract_path_from_value(value);
            return Some((
                EnvironmentVariableComponentType::Service,
                property,
                Some(target_service),
                path,
                None,
            ));
        }
    }

    if !is_url_like(value) && is_cli_generated_key_var(&key_upper) {
        if let Some(property) = infer_key_component_property(&key_upper, value) {
            return Some((
                EnvironmentVariableComponentType::Key,
                property,
                None,
                None,
                None,
            ));
        }
    }

    let service_hint = classify_compose_service(service_name, project_types);

    if let Some((component_type, target)) = infer_component_type(
        &key_upper,
        value,
        service_name,
        service.image.as_deref(),
        service.ports.as_ref().map(|ports| ports.as_slice()),
        service_hint.as_ref(),
        service_lookup,
    ) {
        let property = infer_component_property(&key_upper)
            .unwrap_or_else(|| default_component_property(&component_type, &key_upper));
        let path = extract_path_from_value(value);
        return Some((component_type, property, target, path, None));
    }

    if should_passthrough(key, value) {
        let property = infer_component_property(&key_upper).unwrap_or_else(|| {
            default_component_property(&EnvironmentVariableComponentType::Service, &key_upper)
        });
        let path = None;
        let passthrough_value = Some(value.to_string());
        return Some((
            EnvironmentVariableComponentType::Service,
            property,
            None,
            path,
            passthrough_value,
        ));
    }

    None
}

fn infer_component_property(key_upper: &str) -> Option<EnvironmentVariableComponentProperty> {
    let tokens: Vec<&str> = key_upper
        .split('_')
        .filter(|segment| !segment.is_empty())
        .collect();

    let contains = |needle: &str| tokens.iter().any(|token| *token == needle);

    if contains("CONNECTIONSTRING") || (contains("CONNECTION") && contains("STRING")) {
        Some(EnvironmentVariableComponentProperty::ConnectionString)
    } else if contains("CONNECTION") {
        Some(EnvironmentVariableComponentProperty::Connection)
    } else if contains("HOSTNAME") {
        Some(EnvironmentVariableComponentProperty::Hostname)
    } else if contains("HOST") {
        Some(EnvironmentVariableComponentProperty::Host)
    } else if contains("PORT") {
        Some(EnvironmentVariableComponentProperty::Port)
    } else if contains("URL") || contains("URI") {
        Some(EnvironmentVariableComponentProperty::Url)
    } else if contains("USERNAME") {
        Some(EnvironmentVariableComponentProperty::Username)
    } else if contains("USER") {
        Some(EnvironmentVariableComponentProperty::User)
    } else if contains("PASSWORD") {
        Some(EnvironmentVariableComponentProperty::Password)
    } else if (contains("DB") || contains("DATABASE")) && contains("NAME") {
        Some(EnvironmentVariableComponentProperty::DbName)
    } else if contains("DATABASE") || contains("DB") {
        Some(EnvironmentVariableComponentProperty::Database)
    } else if contains("FQDN") {
        Some(EnvironmentVariableComponentProperty::Fqdn)
    } else if contains("BUCKET") {
        Some(EnvironmentVariableComponentProperty::Bucket)
    } else if contains("ENDPOINT") {
        Some(EnvironmentVariableComponentProperty::Endpoint)
    } else if contains("REGION") {
        Some(EnvironmentVariableComponentProperty::Region)
    } else if contains("PATH") {
        Some(EnvironmentVariableComponentProperty::Endpoint)
    } else if contains("BROKER") || contains("BROKERS") || contains("QUEUE") {
        Some(EnvironmentVariableComponentProperty::Connection)
    } else if contains("SECRET") || contains("TOKEN") {
        Some(EnvironmentVariableComponentProperty::Password)
    } else if contains("ACCESS") && contains("KEY") {
        Some(EnvironmentVariableComponentProperty::User)
    } else if contains("CLIENT") {
        Some(EnvironmentVariableComponentProperty::User)
    } else {
        None
    }
}

fn infer_key_component_property(
    key_upper: &str,
    value: &str,
) -> Option<EnvironmentVariableComponentProperty> {
    if key_upper.contains("ENCRYPTION") || key_upper.contains("PRIVATE_KEY") {
        return Some(EnvironmentVariableComponentProperty::PrivatePem);
    }

    if key_upper.contains("PUBLIC_KEY") {
        return Some(EnvironmentVariableComponentProperty::PublicPem);
    }

    if looks_pem(value) {
        if value.contains("BEGIN PUBLIC KEY") {
            return Some(EnvironmentVariableComponentProperty::PublicPem);
        }
        if value.contains("BEGIN PRIVATE KEY") || value.contains("BEGIN ENCRYPTED PRIVATE KEY") {
            return Some(EnvironmentVariableComponentProperty::PrivatePem);
        }
    }

    if key_upper.contains("HMAC") {
        return Some(EnvironmentVariableComponentProperty::Base64Bytes32);
    }

    if key_upper.contains("JWT") {
        return Some(EnvironmentVariableComponentProperty::Base64Bytes32);
    }

    if key_upper.contains("ACCESS_KEY") || key_upper.contains("CLIENT_SECRET") {
        if looks_hex_key(value) {
            return Some(EnvironmentVariableComponentProperty::HexKey);
        }
        if looks_base64(value) && value.len() >= 60 {
            return Some(EnvironmentVariableComponentProperty::Base64Bytes64);
        }
        return Some(EnvironmentVariableComponentProperty::KeyMaterial);
    }

    if key_upper.contains("SECRET")
        || key_upper.contains("TOKEN")
        || key_upper.ends_with("_KEY")
        || key_upper.contains("API_KEY")
    {
        if looks_hex_key(value) {
            return Some(EnvironmentVariableComponentProperty::HexKey);
        }
        if looks_base64(value) {
            return Some(EnvironmentVariableComponentProperty::Base64Bytes32);
        }
        return Some(EnvironmentVariableComponentProperty::KeyMaterial);
    }

    None
}

fn default_component_property(
    component_type: &EnvironmentVariableComponentType,
    key_upper: &str,
) -> EnvironmentVariableComponentProperty {
    if key_upper.contains("HOST") {
        EnvironmentVariableComponentProperty::Host
    } else if key_upper.contains("PORT") {
        EnvironmentVariableComponentProperty::Port
    } else if key_upper.contains("URL") || key_upper.contains("URI") {
        EnvironmentVariableComponentProperty::Url
    } else if key_upper.contains("USER")
        || key_upper.contains("USERNAME")
        || key_upper.contains("CLIENT")
    {
        EnvironmentVariableComponentProperty::User
    } else if key_upper.contains("PASSWORD")
        || key_upper.contains("SECRET")
        || key_upper.contains("TOKEN")
    {
        EnvironmentVariableComponentProperty::Password
    } else {
        match component_type {
            EnvironmentVariableComponentType::ObjectStore => {
                EnvironmentVariableComponentProperty::Endpoint
            }
            EnvironmentVariableComponentType::Key => {
                EnvironmentVariableComponentProperty::KeyMaterial
            }
            _ => EnvironmentVariableComponentProperty::Connection,
        }
    }
}

fn infer_component_type(
    key_upper: &str,
    value: &str,
    service_name: &str,
    service_image: Option<&str>,
    service_ports: Option<&[String]>,
    service_hint: Option<&(EnvironmentVariableComponentType, String)>,
    service_lookup: &HashMap<String, (EnvironmentVariableComponentType, String)>,
) -> Option<(EnvironmentVariableComponentType, Option<String>)> {
    let value_lower = value.to_ascii_lowercase();
    let service_lower = service_name.to_ascii_lowercase();
    let image_lower = service_image.map(|image| image.to_ascii_lowercase());

    const DATABASE_KEY_HINTS: &[&str] = &[
        "DB_", "DATABASE", "POSTGRES", "MYSQL", "MARIADB", "MSSQL", "PG", "SQLITE",
    ];
    const DATABASE_VALUE_HINTS: &[&str] = &[
        "postgres",
        "postgresql",
        "mysql",
        "mariadb",
        "mssql",
        "sqlserver",
        "sqlite",
        "cockroach",
    ];
    const DATABASE_IMAGE_HINTS: &[&str] = &[
        "postgres",
        "postgresql",
        "mysql",
        "mariadb",
        "mssql",
        "sqlserver",
        "mongo",
        "mongodb",
        "cockroach",
        "timescale",
    ];
    if matches_hints(
        key_upper,
        &value_lower,
        &service_lower,
        image_lower.as_deref(),
        DATABASE_KEY_HINTS,
        DATABASE_VALUE_HINTS,
        DATABASE_IMAGE_HINTS,
    ) {
        // Exclude config flags that happen to start with DB_ but aren't database properties
        // e.g. DB_SSL, DB_TLS, DB_VERIFY, DB_CA_CERT
        let tokens: Vec<&str> = key_upper.split('_').filter(|s| !s.is_empty()).collect();
        let has_config_token = tokens
            .iter()
            .any(|t| matches!(*t, "SSL" | "TLS" | "CERT" | "CA" | "VERIFY" | "SSLMODE"));
        if !has_config_token {
            return Some((EnvironmentVariableComponentType::Database, None));
        }
    }

    const CACHE_KEY_HINTS: &[&str] = &["REDIS", "CACHE", "MEMCACHE", "MEMCACHED"];
    const CACHE_VALUE_HINTS: &[&str] = &["redis", "cache", "memcache", "memcached"];
    const CACHE_IMAGE_HINTS: &[&str] = &["redis", "memcache", "memcached"];
    if matches_hints(
        key_upper,
        &value_lower,
        &service_lower,
        image_lower.as_deref(),
        CACHE_KEY_HINTS,
        CACHE_VALUE_HINTS,
        CACHE_IMAGE_HINTS,
    ) {
        return Some((EnvironmentVariableComponentType::Cache, None));
    }

    const OBJECT_STORE_KEY_HINTS: &[&str] = &["S3", "MINIO", "OBJECT", "BUCKET"];
    const OBJECT_STORE_VALUE_HINTS: &[&str] = &["s3", "minio", "object", "bucket"];
    const OBJECT_STORE_IMAGE_HINTS: &[&str] = &["minio", "localstack", "seaweedfs"];
    if matches_hints(
        key_upper,
        &value_lower,
        &service_lower,
        image_lower.as_deref(),
        OBJECT_STORE_KEY_HINTS,
        OBJECT_STORE_VALUE_HINTS,
        OBJECT_STORE_IMAGE_HINTS,
    ) {
        return Some((EnvironmentVariableComponentType::ObjectStore, None));
    }

    const QUEUE_KEY_HINTS: &[&str] = &[
        "QUEUE", "KAFKA", "BROKER", "BULLMQ", "RABBIT", "RABBITMQ", "NATS", "SQS",
    ];
    const QUEUE_VALUE_HINTS: &[&str] = &[
        "queue", "kafka", "broker", "bullmq", "rabbit", "rabbitmq", "nats", "sqs",
    ];
    const QUEUE_IMAGE_HINTS: &[&str] =
        &["kafka", "redpanda", "rabbitmq", "nats", "activemq", "sqs"];
    if matches_hints(
        key_upper,
        &value_lower,
        &service_lower,
        image_lower.as_deref(),
        QUEUE_KEY_HINTS,
        QUEUE_VALUE_HINTS,
        QUEUE_IMAGE_HINTS,
    ) {
        return Some((EnvironmentVariableComponentType::Queue, None));
    }

    if let Some((component_type, ref_target)) = infer_service_reference(value, service_lookup) {
        // For _URL vars, only trust the service reference if the var name prefix
        // matches the inferred target. This prevents external service URLs
        // (e.g. LIVEKIT_URL=ws://matching:7880) from being misclassified as
        // internal service references (target=matching).
        if key_upper.ends_with("_URL") {
            let prefix = key_upper
                .trim_end_matches("_URL")
                .to_ascii_lowercase()
                .replace('_', "-");
            if ref_target.as_deref() == Some(prefix.as_str()) {
                return Some((component_type, ref_target));
            }
            // Name mismatch — skip this inference (e.g. LIVEKIT != matching)
        } else {
            return Some((component_type, ref_target));
        }
    }

    let has_generated_port = has_generated_app_port(service_ports);

    if has_generated_port && is_url_like(value) {
        if let Some((component_type_hint, canonical_target)) = service_hint {
            return Some((component_type_hint.clone(), Some(canonical_target.clone())));
        }
    }

    None
}

fn matches_hints(
    key_upper: &str,
    value_lower: &str,
    service_lower: &str,
    image_lower: Option<&str>,
    key_hints: &[&str],
    value_hints: &[&str],
    image_hints: &[&str],
) -> bool {
    key_hints.iter().any(|hint| key_upper.contains(hint))
        || value_hints.iter().any(|hint| value_lower.contains(hint))
        || value_hints.iter().any(|hint| service_lower.contains(hint))
        || image_hints
            .iter()
            .any(|hint| image_lower.map_or(false, |image| image.contains(hint)))
}

fn infer_service_reference(
    value: &str,
    service_lookup: &HashMap<String, (EnvironmentVariableComponentType, String)>,
) -> Option<(EnvironmentVariableComponentType, Option<String>)> {
    let parsed = parse_url(value)?;
    let host = parsed.host?;

    service_lookup
        .get(&host)
        .map(|(component_type, target)| (component_type.clone(), Some(target.clone())))
}

fn insert_service_identifier(
    lookup: &mut HashMap<String, (EnvironmentVariableComponentType, String)>,
    identifier: &str,
    component_type: &EnvironmentVariableComponentType,
    canonical_target: &str,
) {
    if identifier.is_empty() {
        return;
    }

    lookup
        .entry(identifier.to_ascii_lowercase())
        .or_insert((component_type.clone(), canonical_target.to_string()));
}

fn has_generated_app_port(service_ports: Option<&[String]>) -> bool {
    if let Some(ports) = service_ports {
        for binding in ports {
            for segment in binding.split(&[':', '/'][..]) {
                if let Ok(port) = segment.parse::<u16>() {
                    if (8000..9000).contains(&port) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn is_url_like(value: &str) -> bool {
    parse_url(value).is_some()
}

fn should_passthrough(key: &str, value: &str) -> bool {
    if is_url_like(value) {
        return false;
    }

    // Don't passthrough values containing localhost
    if value.contains("localhost") {
        return false;
    }

    let key_upper = key.to_ascii_uppercase();

    if infer_key_component_property(&key_upper, value).is_some() {
        return false;
    }

    if key_upper.contains("KEY")
        || key_upper.contains("TOKEN")
        || key_upper.contains("SECRET")
        || key_upper.contains("PASSWORD")
        || key_upper.contains("ACCESS")
    {
        return false;
    }

    if key_upper.contains("PORT") {
        return false;
    }

    // NODE_ENV should not be passthrough
    if key_upper == "NODE_ENV" {
        return false;
    }

    true
}

const CLI_GENERATED_KEY_VARS: &[&str] = &[
    "HMAC_SECRET_KEY",
];

fn is_cli_generated_key_var(key_upper: &str) -> bool {
    CLI_GENERATED_KEY_VARS
        .iter()
        .any(|allowed| allowed == &key_upper)
}

fn is_allowed_application_var(
    var_name: &str,
    components: &HashMap<
        String,
        (
            EnvironmentVariableComponentType,
            EnvironmentVariableComponentProperty,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >,
    project_names: &[String],
) -> bool {
    // Allow all vars that are inherently application-scoped (observability, shared keys, inter-service URLs)
    if is_application_scoped_var(var_name, project_names) {
        return true;
    }

    if let Some((component_type, property, _, _, _)) = components.get(var_name) {
        if matches!(
            component_type,
            EnvironmentVariableComponentType::Service | EnvironmentVariableComponentType::Worker
        ) && matches!(property, EnvironmentVariableComponentProperty::Url)
        {
            return true;
        }
    }

    false
}

struct ParsedUrl {
    host: Option<String>,
    path: Option<String>,
}

fn parse_url(value: &str) -> Option<ParsedUrl> {
    let scheme_split: Vec<&str> = value.splitn(2, "://").collect();
    if scheme_split.len() != 2 {
        return None;
    }

    let remainder = scheme_split[1];
    let mut host = remainder;
    let mut path_start = remainder.len();

    if let Some(idx) = remainder.find(&['/', '?', '#'][..]) {
        host = &remainder[..idx];
        path_start = idx;
    }

    let mut host_parts = host.splitn(2, ':');
    let hostname = host_parts
        .next()
        .map(|h| h.trim().trim_matches(|c: char| c == '[' || c == ']'))
        .filter(|h| !h.is_empty())
        .map(|h| h.to_ascii_lowercase());

    let path = if path_start < remainder.len() {
        Some(remainder[path_start..].to_string())
    } else {
        None
    };

    Some(ParsedUrl {
        host: hostname,
        path,
    })
}

fn extract_path_from_value(value: &str) -> Option<String> {
    parse_url(value).and_then(|parsed| parsed.path)
}

fn looks_pem(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with("-----BEGIN ") && trimmed.contains("-----END ")
}

fn looks_hex_key(value: &str) -> bool {
    let trimmed = value.trim();
    let hex_chars = trimmed.chars().all(|c| c.is_ascii_hexdigit());
    hex_chars && (trimmed.len() >= 32)
}

fn looks_base64(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.len() < 16 {
        return false;
    }
    trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "+/=_-".contains(c))
}

/// Cross-scope deduplication: if a var exists at application scope AND at service/worker scope
/// with the same value (or no value), remove the service/worker copies.
/// Promotes service/worker vars to application scope when 2+ copies share the same value,
/// keeping minority overrides at service/worker scope.
fn deduplicate_cross_scope(scoped_env_vars: &mut Vec<ScopedEnvVar>) {

    // Pre-pass: fill in blank/empty values from sibling copies.
    // If VAR is blank in service A but "abc" in services B and C, set A to "abc".
    // This ensures consolidation sees consistent values across components.
    {
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (idx, var) in scoped_env_vars.iter().enumerate() {
            by_name.entry(var.name.clone()).or_default().push(idx);
        }
        for (_name, indices) in &by_name {
            if indices.len() <= 1 {
                continue;
            }
            // Find the majority non-empty value
            let mut value_counts: HashMap<String, usize> = HashMap::new();
            for &idx in indices {
                if let Some(v) = &scoped_env_vars[idx].value {
                    let trimmed = v.trim().to_string();
                    if !trimmed.is_empty() {
                        *value_counts.entry(trimmed).or_insert(0) += 1;
                    }
                }
            }
            if let Some(&max_count) = value_counts.values().max() {
                let max_entries: Vec<_> = value_counts
                    .iter()
                    .filter(|&(_, &c)| c == max_count)
                    .collect();
                // Only backfill when there is a single unambiguous majority value
                if max_entries.len() == 1 {
                    let fill_value = max_entries[0].0.clone();
                    // Fill blank/empty entries with the majority value
                    for &idx in indices {
                        let is_blank = match &scoped_env_vars[idx].value {
                            None => true,
                            Some(v) => v.trim().is_empty(),
                        };
                        if is_blank {
                            scoped_env_vars[idx].value = Some(fill_value.clone());
                        }
                    }
                }
            }
        }
    }

    // Group vars by name
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, var) in scoped_env_vars.iter().enumerate() {
        by_name.entry(var.name.clone()).or_default().push(idx);
    }

    let mut indices_to_remove = HashSet::new();

    for (_name, indices) in &by_name {
        if indices.len() <= 1 {
            continue;
        }

        let has_app_scope = indices.iter().any(|&i| {
            scoped_env_vars[i].scope == EnvScope::Application
        });

        if has_app_scope {
            // Compute the majority value across all service/worker copies.
            // The app-scope entry gets this value; service/worker copies matching
            // the majority are removed. Minority diversions are kept as overrides.
            let app_idx = indices
                .iter()
                .find(|&&i| {
                    scoped_env_vars[i].scope
                        == EnvScope::Application
                })
                .copied()
                .unwrap();

            // Count occurrences of each non-empty value across service/worker copies
            let mut value_counts: HashMap<String, usize> = HashMap::new();
            for &idx in indices {
                if scoped_env_vars[idx].scope
                    != EnvScope::Application
                {
                    if let Some(v) = &scoped_env_vars[idx].value {
                        let trimmed = v.trim().to_string();
                        if !trimmed.is_empty() {
                            *value_counts.entry(trimmed).or_insert(0) += 1;
                        }
                    }
                }
            }

            // Most common value becomes the application-scope value
            let majority_value = value_counts
                .iter()
                .max_by_key(|(_, count)| *count)
                .map(|(val, _)| val.clone());

            // Set the app-scope entry's value to the majority
            if let Some(ref val) = majority_value {
                scoped_env_vars[app_idx].value = Some(val.clone());
            }

            // Remove service/worker copies that match the majority or are blank.
            // Keep copies that differ from the majority (overrides).
            for &idx in indices {
                if scoped_env_vars[idx].scope
                    != EnvScope::Application
                {
                    let matches_majority = match &scoped_env_vars[idx].value {
                        None => true,
                        Some(v) => {
                            let trimmed = v.trim();
                            trimmed.is_empty() || majority_value.as_deref() == Some(trimmed)
                        }
                    };
                    if matches_majority {
                        indices_to_remove.insert(idx);
                    }
                }
            }
        } else if indices.len() >= 2 {
            // All copies are at service/worker scope (no app entry).
            // Skip vars that must never be promoted to application scope.
            let name = &scoped_env_vars[indices[0]].name;
            if is_never_application_scoped(name) {
                continue;
            }
            // If 2+ copies share the same value (including empty), promote to application scope.
            let mut value_counts: HashMap<String, usize> = HashMap::new();
            for &idx in indices {
                let trimmed = scoped_env_vars[idx]
                    .value
                    .as_deref()
                    .map(|v| v.trim())
                    .unwrap_or("")
                    .to_string();
                *value_counts.entry(trimmed).or_insert(0) += 1;
            }

            if let Some((majority_val, majority_count)) =
                value_counts.iter().max_by_key(|(_, count)| *count)
            {
                // Only promote if at least 2 copies share the same value
                if *majority_count >= 2 {
                    let majority_val = majority_val.clone();

                    // Create an application-scope entry with the majority value as hint
                    // (docker-compose values are dev-only but serve as useful defaults)
                    let first_idx = indices[0];
                    let app_entry = ScopedEnvVar {
                        name: scoped_env_vars[first_idx].name.clone(),
                        scope: EnvScope::Application,
                        scope_id: None,
                        used_by: scoped_env_vars[first_idx].used_by.clone(),
                        value: Some("".to_string()),
                    };
                    scoped_env_vars.push(app_entry);

                    // Remove service/worker copies that match the majority value or are blank.
                    // Keep copies with minority values as per-service overrides.
                    for &idx in indices {
                        let matches_majority = match &scoped_env_vars[idx].value {
                            Some(v) => {
                                let trimmed = v.trim();
                                trimmed.is_empty() || trimmed == majority_val
                            }
                            None => true,
                        };
                        if matches_majority {
                            indices_to_remove.insert(idx);
                        }
                    }
                }
            }
        }
    }

    // Remove in reverse order to preserve indices
    let mut sorted_removals: Vec<usize> = indices_to_remove.into_iter().collect();
    sorted_removals.sort_unstable_by(|a, b| b.cmp(a));
    for idx in sorted_removals {
        scoped_env_vars.remove(idx);
    }
}

/// Auth/infrastructure URL vars that Pulumi computes at deploy time.
/// These are not inter-service URLs but are still auto-generated.
fn is_pulumi_injected_url_var(var_name: &str) -> bool {
    let upper = var_name.to_ascii_uppercase();
    pulumi_url_var_target(&upper).is_some()
}

/// Maps Pulumi-injected URL vars to their production target service.
/// The target is the service the URL resolves to in production (via ALB),
/// which may differ from the docker-compose dev service name.
fn pulumi_url_var_target(key_upper: &str) -> Option<String> {
    match key_upper {
        "OTEL_EXPORTER_OTLP_ENDPOINT" => Some("monitoring".to_string()),
        "JWKS_PUBLIC_KEY_URL" => Some("iam".to_string()),
        "BETTER_AUTH_BASE_URL" => Some("iam".to_string()),
        _ => None,
    }
}

/// Check if a var belongs to an infrastructure component that Pulumi provisions and injects.
/// Only Database, Cache, and Queue are provisioned by Pulumi.
/// ObjectStore (S3), Service, Worker, and Key components are NOT — those vars are user-supplied.
fn is_pulumi_provisioned_component(
    var_name: &str,
    env_var_components: &HashMap<
        String,
        (
            EnvironmentVariableComponentType,
            EnvironmentVariableComponentProperty,
            Option<String>,
            Option<String>,
            Option<String>,
        ),
    >,
) -> bool {
    if let Some((component_type, ..)) = env_var_components.get(var_name) {
        matches!(
            component_type,
            EnvironmentVariableComponentType::Database
                | EnvironmentVariableComponentType::Cache
                | EnvironmentVariableComponentType::Queue
        )
    } else {
        false
    }
}

/// Explicit registry of platform-managed env var names/prefixes.
/// Fallback for vars that don't match heuristic checks (docker-compose, component, scoped).
/// Update this list when adding new platform-generated env vars.
fn is_platform_managed_var(var_name: &str) -> bool {
    const PLATFORM_VARS: &[&str] = &[
        // CLI-generated: scaffolded with default values during `forklaunch init`
        "NODE_ENV",
        "HOST",
        "PROTOCOL",
        "PORT",
        "WS_PORT",
        "VERSION",
        "DOCS_PATH",
        "QUEUE_NAME",
        "OTEL_SERVICE_NAME",
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_EXPORTER_OTLP_PROTOCOL",
        "BETTER_AUTH_BASE_PATH",
        // CLI-generated: random secrets produced during `forklaunch init`
        "HMAC_SECRET_KEY",
        "BETTER_AUTH_SECRET",
        // Pulumi-generated: derived from other vars at deploy time
        "JWKS_PUBLIC_KEY_URL",
        "BETTER_AUTH_BASE_URL",
        // Pulumi-generated: IAM-specific database
        "IAM_DB_NAME",
        // Pulumi-generated: database
        "DB_NAME",
        "DB_HOST",
        "DB_PORT",
        "DB_USER",
        "DB_PASSWORD",
        "DB_URL",
        "DB_SSL",
        "PGSSLMODE",
        // Pulumi-generated: redis
        "REDIS_URL",
        "REDIS_HOST",
        "REDIS_PORT",
        "REDIS_TLS",
        // Pulumi-generated: kafka
        "KAFKA_BROKERS",
        "KAFKA_BOOTSTRAP_SERVERS",
        "KAFKA_BOOTSTRAP_SERVERS_TLS",
        "KAFKA_ZOOKEEPER_CONNECT",
        "KAFKA_CLIENT_ID",
        "KAFKA_GROUP_ID",
        "KAFKA_SSL",
        // Pulumi-generated: observability
        "OTEL_APPLICATION_ID",
        "OTEL_RESOURCE_ATTRIBUTES",
        "OTEL_EXPORTER_OTLP_HEADERS",
        "OTEL_TRACES_EXPORTER",
        "OTEL_METRICS_EXPORTER",
        "OTEL_LOGS_EXPORTER",
        "OTEL_LEVEL",
        "PROMETHEUS_URL",
        "LOKI_URL",
        "TEMPO_URL",
        "NODE_TLS_REJECT_UNAUTHORIZED",
        "MONITORING_SECRET",
        // Pulumi-generated: docs
        "DOCS_SERVER_URLS",
        "DOCS_SERVER_DESCRIPTIONS",
    ];
    let upper = var_name.to_ascii_uppercase();
    PLATFORM_VARS.iter().any(|&v| v == upper)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::core::manifest::{
        ProjectEntry, ResourceInventory, application::ApplicationManifestData,
    };

    fn create_test_manifest(projects: Vec<(&str, ProjectType)>) -> ApplicationManifestData {
        ApplicationManifestData {
            id: "test-id".to_string(),
            cli_version: "1.0.0".to_string(),
            app_name: "test-app".to_string(),
            camel_case_app_name: "testApp".to_string(),
            pascal_case_app_name: "TestApp".to_string(),
            kebab_case_app_name: "test-app".to_string(),
            title_case_app_name: "Test App".to_string(),
            modules_path: "src/modules".to_string(),
            docker_compose_path: Some("docker-compose.yaml".to_string()),
            dockerfile: None,
            git_repository: None,
            runtime: "node".to_string(),
            formatter: "prettier".to_string(),
            linter: "eslint".to_string(),
            validator: "zod".to_string(),
            http_framework: "express".to_string(),
            test_framework: None,
            app_description: "Test application".to_string(),
            author: "Test Author".to_string(),
            license: "MIT".to_string(),
            projects: projects
                .into_iter()
                .map(|(name, r#type)| ProjectEntry {
                    name: name.to_string(),
                    r#type,
                    description: format!("Test {}", name),
                    variant: None,
                    resources: Some(ResourceInventory {
                        database: None,
                        cache: None,
                        queue: None,
                        object_store: None,
                        redis_partition: None,
                    }),
                    routers: None,
                    metadata: None,
                })
                .collect(),
            project_peer_topology: HashMap::new(),
            database: "postgresql".to_string(),
            is_postgres: true,
            is_sqlite: false,
            is_mysql: false,
            is_mariadb: false,
            is_better_sqlite: false,
            is_libsql: false,
            is_mssql: false,
            is_mongo: false,
            is_in_memory_database: false,
            is_eslint: true,
            is_biome: false,
            is_oxlint: false,
            is_prettier: true,
            is_express: true,
            is_hyper_express: false,
            is_zod: true,
            is_typebox: false,
            is_bun: false,
            is_node: true,
            is_vitest: false,
            is_jest: false,
            platform_application_id: None,
            platform_organization_id: None,
            release_version: None,
        }
    }

    fn create_test_docker_compose(services: Vec<(&str, Vec<(&str, &str)>)>) -> String {
        let mut yaml = "version: '3.8'\nservices:\n".to_string();
        for (service_name, env_vars) in services {
            yaml.push_str(&format!("  {}:\n", service_name));
            yaml.push_str("    image: test-image:latest\n");
            if !env_vars.is_empty() {
                yaml.push_str("    environment:\n");
                for (key, value) in env_vars {
                    yaml.push_str(&format!("      {}: {}\n", key, value));
                }
            }
        }
        yaml
    }

    #[test]
    fn test_build_env_var_component_map_collects_docker_compose_env_vars() {
        let temp_dir = TempDir::new().unwrap();
        let compose_path = temp_dir.path().join("docker-compose.yaml");

        let compose_content = create_test_docker_compose(vec![
            (
                "my-service",
                vec![
                    ("DB_HOST", "postgres"),
                    ("DB_PORT", "5432"),
                    ("PORT", "8080"),
                    ("HOST", "0.0.0.0"),
                ],
            ),
            (
                "my-worker",
                vec![
                    ("REDIS_HOST", "redis"),
                    ("REDIS_PORT", "6379"),
                    ("PORT", "9000"),
                ],
            ),
        ]);

        fs::write(&compose_path, compose_content).unwrap();

        let manifest = create_test_manifest(vec![
            ("my-service", ProjectType::Service),
            ("my-worker", ProjectType::Worker),
        ]);

        let (_components, docker_compose_env_vars) =
            build_env_var_component_map(temp_dir.path(), &manifest);

        // Verify docker-compose env vars are collected
        assert!(docker_compose_env_vars.contains_key("my-service"));
        assert!(docker_compose_env_vars.contains_key("my-worker"));

        let service_vars = docker_compose_env_vars.get("my-service").unwrap();
        assert_eq!(service_vars.len(), 4);
        let service_var_names: Vec<&String> = service_vars.iter().map(|(k, _)| k).collect();
        assert!(service_var_names.contains(&&"DB_HOST".to_string()));
        assert!(service_var_names.contains(&&"DB_PORT".to_string()));
        assert!(service_var_names.contains(&&"PORT".to_string()));
        assert!(service_var_names.contains(&&"HOST".to_string()));

        let worker_vars = docker_compose_env_vars.get("my-worker").unwrap();
        assert_eq!(worker_vars.len(), 3);
        let worker_var_names: Vec<&String> = worker_vars.iter().map(|(k, _)| k).collect();
        assert!(worker_var_names.contains(&&"REDIS_HOST".to_string()));
        assert!(worker_var_names.contains(&&"REDIS_PORT".to_string()));
        assert!(worker_var_names.contains(&&"PORT".to_string()));
    }

    #[test]
    fn test_build_env_var_component_map_sets_port_passthrough() {
        let temp_dir = TempDir::new().unwrap();
        let compose_path = temp_dir.path().join("docker-compose.yaml");

        let compose_content = create_test_docker_compose(vec![(
            "my-service",
            vec![("PORT", "8080"), ("HOST", "0.0.0.0")],
        )]);

        fs::write(&compose_path, compose_content).unwrap();

        let manifest = create_test_manifest(vec![("my-service", ProjectType::Service)]);

        let (components, _docker_compose_env_vars) =
            build_env_var_component_map(temp_dir.path(), &manifest);

        // Verify PORT has passthrough "8000"
        if let Some((_type, _property, _target, _path, passthrough)) = components.get("PORT") {
            assert_eq!(passthrough, &Some("8000".to_string()));
        } else {
            panic!("PORT should be in components map");
        }
    }

    #[test]
    fn test_docker_compose_env_vars_included_in_scoped_vars() {
        let temp_dir = TempDir::new().unwrap();
        let compose_path = temp_dir.path().join("docker-compose.yaml");

        let compose_content = create_test_docker_compose(vec![(
            "my-service",
            vec![
                ("DB_HOST", "postgres"),
                ("PORT", "8080"),
                ("HOST", "0.0.0.0"),
            ],
        )]);

        fs::write(&compose_path, compose_content).unwrap();

        let manifest = create_test_manifest(vec![("my-service", ProjectType::Service)]);

        let (mut components, docker_compose_env_vars) =
            build_env_var_component_map(temp_dir.path(), &manifest);

        // Simulate the logic from the main function
        let mut scoped_env_vars = Vec::new();
        let mut existing_vars: HashSet<(String, Option<String>)> =
            HashSet::new();

        for (service_name, env_vars) in docker_compose_env_vars {
            let project_type = manifest
                .projects
                .iter()
                .find(|p| p.name == service_name)
                .map(|p| &p.r#type);

            let (scope, scope_id) = match project_type {
                Some(ProjectType::Service) => (
                    EnvScope::Service,
                    Some(service_name.clone()),
                ),
                Some(ProjectType::Worker) => (
                    EnvScope::Worker,
                    Some(format!("{}-worker", service_name)),
                ),
                _ => continue,
            };

            for (key, _value) in env_vars {
                if existing_vars.contains(&(key.clone(), scope_id.clone())) {
                    continue;
                }

                let key_upper = key.to_ascii_uppercase();
                if key_upper == "PORT" {
                    let property = infer_component_property(&key_upper).unwrap_or_else(|| {
                        default_component_property(
                            &EnvironmentVariableComponentType::Service,
                            &key_upper,
                        )
                    });
                    components.insert(
                        key.clone(),
                        (
                            EnvironmentVariableComponentType::Service,
                            property,
                            None,
                            None,
                            Some("8000".to_string()),
                        ),
                    );
                }

                scoped_env_vars.push(ScopedEnvVar {
                    name: key.clone(),
                    scope: scope.clone(),
                    scope_id: scope_id.clone(),
                    used_by: vec![service_name.clone()],
                    value: None,
                });

                existing_vars.insert((key, scope_id.clone()));
            }
        }

        // Verify env vars are in scoped_env_vars
        assert_eq!(scoped_env_vars.len(), 3);

        let var_names: Vec<&String> = scoped_env_vars.iter().map(|v| &v.name).collect();
        assert!(var_names.contains(&&"DB_HOST".to_string()));
        assert!(var_names.contains(&&"PORT".to_string()));
        assert!(var_names.contains(&&"HOST".to_string()));

        // Verify they're scoped to the service
        for var in &scoped_env_vars {
            assert_eq!(
                var.scope,
                EnvScope::Service
            );
            assert_eq!(var.scope_id, Some("my-service".to_string()));
        }

        // Verify PORT has passthrough in components
        if let Some((_type, _property, _target, _path, passthrough)) = components.get("PORT") {
            assert_eq!(passthrough, &Some("8000".to_string()));
        } else {
            panic!("PORT should be in components map with passthrough");
        }
    }

    #[test]
    fn test_docker_compose_env_vars_not_duplicated() {
        let temp_dir = TempDir::new().unwrap();
        let compose_path = temp_dir.path().join("docker-compose.yaml");

        let compose_content = create_test_docker_compose(vec![(
            "my-service",
            vec![("PORT", "8080"), ("HOST", "0.0.0.0")],
        )]);

        fs::write(&compose_path, compose_content).unwrap();

        let manifest = create_test_manifest(vec![("my-service", ProjectType::Service)]);

        let (_components, docker_compose_env_vars) =
            build_env_var_component_map(temp_dir.path(), &manifest);

        // Simulate existing env vars from code
        let mut scoped_env_vars = vec![ScopedEnvVar {
            name: "PORT".to_string(),
            scope: EnvScope::Service,
            scope_id: Some("my-service".to_string()),
            used_by: vec!["my-service".to_string()],
            value: None,
        }];

        let mut existing_vars: HashSet<(String, Option<String>)> =
            scoped_env_vars
                .iter()
                .map(|v| (v.name.clone(), v.scope_id.clone()))
                .collect();

        // Add docker-compose env vars (simulating the main function logic)
        for (service_name, env_vars) in docker_compose_env_vars {
            let project_type = manifest
                .projects
                .iter()
                .find(|p| p.name == service_name)
                .map(|p| &p.r#type);

            let (scope, scope_id) = match project_type {
                Some(ProjectType::Service) => (
                    EnvScope::Service,
                    Some(service_name.clone()),
                ),
                _ => continue,
            };

            for (key, _value) in env_vars {
                // Skip if already exists (this is the deduplication logic)
                if existing_vars.contains(&(key.clone(), scope_id.clone())) {
                    continue;
                }

                scoped_env_vars.push(ScopedEnvVar {
                    name: key.clone(),
                    scope: scope.clone(),
                    scope_id: scope_id.clone(),
                    used_by: vec![service_name.clone()],
                    value: None,
                });

                existing_vars.insert((key, scope_id.clone()));
            }
        }

        // Verify PORT is not duplicated
        let port_count = scoped_env_vars.iter().filter(|v| v.name == "PORT").count();
        assert_eq!(port_count, 1, "PORT should not be duplicated");

        // Verify HOST is added
        assert!(scoped_env_vars.iter().any(|v| v.name == "HOST"));
    }

    #[test]
    fn test_deduplicate_cross_scope_promotes_same_value_services() {
        // Two service copies with the same value → promoted to application scope
        let mut vars = vec![
            ScopedEnvVar {
                name: "S3_BUCKET".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("billing".to_string()),
                used_by: vec!["billing".to_string()],
                value: Some("my-bucket".to_string()),
            },
            ScopedEnvVar {
                name: "S3_BUCKET".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("platform-management".to_string()),
                used_by: vec!["platform-management".to_string()],
                value: Some("my-bucket".to_string()),
            },
        ];

        deduplicate_cross_scope(&mut vars);

        // Should have one application-scoped entry with blank value
        assert_eq!(vars.len(), 1, "Expected 1 var after dedup, got {}", vars.len());
        assert_eq!(vars[0].scope, EnvScope::Application);
        assert_eq!(vars[0].scope_id, None);
        assert_eq!(vars[0].value, Some("".to_string()));
        assert_eq!(vars[0].name, "S3_BUCKET");
    }

    #[test]
    fn test_deduplicate_cross_scope_keeps_different_values() {
        // Two service copies with different values → kept as-is
        let mut vars = vec![
            ScopedEnvVar {
                name: "DB_NAME".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("billing".to_string()),
                used_by: vec!["billing".to_string()],
                value: Some("billing_db".to_string()),
            },
            ScopedEnvVar {
                name: "DB_NAME".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("iam".to_string()),
                used_by: vec!["iam".to_string()],
                value: Some("iam_db".to_string()),
            },
        ];

        deduplicate_cross_scope(&mut vars);

        // Both should remain — no majority (each value appears once)
        assert_eq!(vars.len(), 2, "Expected 2 vars after dedup, got {}", vars.len());
        assert!(vars.iter().all(|v| v.scope == EnvScope::Service));
    }

    #[test]
    fn test_deduplicate_cross_scope_existing_app_scope_unchanged() {
        // Mix of app-scope + service copies → existing behavior (app gets majority, matching service copies removed)
        let mut vars = vec![
            ScopedEnvVar {
                name: "REDIS_URL".to_string(),
                scope: EnvScope::Application,
                scope_id: None,
                used_by: vec![],
                value: None,
            },
            ScopedEnvVar {
                name: "REDIS_URL".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("billing".to_string()),
                used_by: vec!["billing".to_string()],
                value: Some("redis://localhost:6379".to_string()),
            },
            ScopedEnvVar {
                name: "REDIS_URL".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("iam".to_string()),
                used_by: vec!["iam".to_string()],
                value: Some("redis://localhost:6379".to_string()),
            },
        ];

        deduplicate_cross_scope(&mut vars);

        // Should have one application-scoped entry with the majority value
        assert_eq!(vars.len(), 1, "Expected 1 var after dedup, got {}", vars.len());
        assert_eq!(vars[0].scope, EnvScope::Application);
        assert_eq!(vars[0].scope_id, None);
        // The has_app_scope branch sets the app entry's value to the majority
        assert_eq!(vars[0].value, Some("redis://localhost:6379".to_string()));
    }

    #[test]
    fn test_deduplicate_cross_scope_majority_with_minority_override() {
        // Three service copies: 2 same, 1 different → promote majority, keep minority
        let mut vars = vec![
            ScopedEnvVar {
                name: "S3_REGION".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("billing".to_string()),
                used_by: vec!["billing".to_string()],
                value: Some("us-east-1".to_string()),
            },
            ScopedEnvVar {
                name: "S3_REGION".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("iam".to_string()),
                used_by: vec!["iam".to_string()],
                value: Some("us-east-1".to_string()),
            },
            ScopedEnvVar {
                name: "S3_REGION".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("special".to_string()),
                used_by: vec!["special".to_string()],
                value: Some("eu-west-1".to_string()),
            },
        ];

        deduplicate_cross_scope(&mut vars);

        // Should have: 1 application-scoped (blank) + 1 minority service override
        assert_eq!(vars.len(), 2, "Expected 2 vars after dedup, got {}", vars.len());

        let app_var = vars.iter().find(|v| v.scope == EnvScope::Application);
        assert!(app_var.is_some(), "Should have an application-scoped entry");
        assert_eq!(app_var.unwrap().value, Some("".to_string()));

        let svc_var = vars.iter().find(|v| v.scope == EnvScope::Service);
        assert!(svc_var.is_some(), "Should keep minority service override");
        assert_eq!(svc_var.unwrap().value, Some("eu-west-1".to_string()));
        assert_eq!(svc_var.unwrap().scope_id, Some("special".to_string()));
    }

    #[test]
    fn test_deduplicate_cross_scope_single_entry_unchanged() {
        // Single service entry → no promotion
        let mut vars = vec![
            ScopedEnvVar {
                name: "CUSTOM_VAR".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("billing".to_string()),
                used_by: vec!["billing".to_string()],
                value: Some("some-value".to_string()),
            },
        ];

        deduplicate_cross_scope(&mut vars);

        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].scope, EnvScope::Service);
    }

    #[test]
    fn test_deduplicate_cross_scope_fills_blank_from_siblings() {
        // Service A has blank value, services B and C have "abc" → blank gets filled,
        // then all three match and promote to application scope
        let mut vars = vec![
            ScopedEnvVar {
                name: "API_KEY".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("billing".to_string()),
                used_by: vec!["billing".to_string()],
                value: Some("".to_string()), // blank in docker-compose
            },
            ScopedEnvVar {
                name: "API_KEY".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("iam".to_string()),
                used_by: vec!["iam".to_string()],
                value: Some("abc123".to_string()),
            },
            ScopedEnvVar {
                name: "API_KEY".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("platform-management".to_string()),
                used_by: vec!["platform-management".to_string()],
                value: Some("abc123".to_string()),
            },
        ];

        deduplicate_cross_scope(&mut vars);

        // All three had the same value after fill-in → promoted to application scope
        assert_eq!(vars.len(), 1, "Expected 1 var after dedup, got {}", vars.len());
        assert_eq!(vars[0].scope, EnvScope::Application);
        assert_eq!(vars[0].name, "API_KEY");
    }

    #[test]
    fn test_deduplicate_cross_scope_blank_with_minority_override() {
        // Service A blank, B and C have "abx", D has "abc"
        // → blank gets filled with "abx" (majority), D kept as override
        let mut vars = vec![
            ScopedEnvVar {
                name: "SETTING".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("svc-a".to_string()),
                used_by: vec!["svc-a".to_string()],
                value: Some("".to_string()),
            },
            ScopedEnvVar {
                name: "SETTING".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("svc-b".to_string()),
                used_by: vec!["svc-b".to_string()],
                value: Some("abx".to_string()),
            },
            ScopedEnvVar {
                name: "SETTING".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("svc-c".to_string()),
                used_by: vec!["svc-c".to_string()],
                value: Some("abx".to_string()),
            },
            ScopedEnvVar {
                name: "SETTING".to_string(),
                scope: EnvScope::Service,
                scope_id: Some("svc-d".to_string()),
                used_by: vec!["svc-d".to_string()],
                value: Some("abc".to_string()),
            },
        ];

        deduplicate_cross_scope(&mut vars);

        // Should have: 1 application-scoped (blank) + 1 minority service override for svc-d
        assert_eq!(vars.len(), 2, "Expected 2 vars after dedup, got {}", vars.len());

        let app_var = vars.iter().find(|v| v.scope == EnvScope::Application);
        assert!(app_var.is_some(), "Should have an application-scoped entry");

        let svc_var = vars.iter().find(|v| v.scope == EnvScope::Service);
        assert!(svc_var.is_some(), "Should keep minority service override");
        assert_eq!(svc_var.unwrap().value, Some("abc".to_string()));
        assert_eq!(svc_var.unwrap().scope_id, Some("svc-d".to_string()));
    }

    #[test]
    fn test_should_passthrough_rejects_localhost() {
        assert!(!should_passthrough("SOME_VAR", "localhost:3000"));
        assert!(!should_passthrough("HOST", "http://localhost"));
        assert!(!should_passthrough("ENDPOINT", "localhost"));
    }

    #[test]
    fn test_should_passthrough_allows_non_localhost() {
        assert!(should_passthrough("SOME_VAR", "production"));
        assert!(should_passthrough("APP_NAME", "my-app"));
    }
}

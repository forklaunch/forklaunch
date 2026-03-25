use std::{collections::{HashMap, HashSet}, fs, io::Write, path::Path};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use rustyline::{Editor, history::DefaultHistory};
use serde_json::from_str as json_from_str;
use serde_yml::{from_str as yaml_from_str, to_string as yaml_to_string};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};
use toml::from_str as toml_from_str;

use crate::{
    CliCommand,
    constants::{
        DIRS_TO_IGNORE, ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE, ERROR_FAILED_TO_PARSE_MANIFEST,
        InitializeType,
    },
    core::{
        ast::infrastructure::{
            compliance::scan_all_compliance,
            env::find_all_env_vars,
        },
        base_path::{RequiredLocation, find_app_root_path},
        command::command,
        docker::{DockerCompose, sync_docker_compose_env_vars},
        env_template::{generate_env_templates, sync_env_local_files},
        manifest::{
            RetentionManifestConfig,
            ProjectType, application::ApplicationManifestData,
        },
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
        sync::{
            artifacts::{ArtifactType, remove_project_from_artifacts},
            detection::detect_project_type,
        },
    },
    prompt::{ArrayCompleter, prompt_for_confirmation, prompt_with_validation_with_answers},
};

/// Performs a full sync of all projects in the modules directory with the manifest.
/// This function can be called programmatically from other commands (e.g., release).
///
/// # Arguments
/// * `app_root_path` - Root path of the application
/// * `manifest_data` - Mutable reference to the manifest data to be updated
/// * `rendered_templates_cache` - Cache for rendered templates
/// * `confirm_all` - If true, skips interactive confirmation prompts
/// * `prompts_map` - Pre-provided answers for prompts
/// * `stdout` - Output stream for messages
///
/// # Returns
/// Returns `Ok(true)` if changes were made to the manifest, `Ok(false)` otherwise
pub fn sync_all_projects(
    app_root_path: &Path,
    manifest_data: &mut ApplicationManifestData,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    confirm_all: bool,
    prompts_map: &HashMap<String, HashMap<String, String>>,
    stdout: &mut StandardStream,
) -> Result<bool> {
    let modules_path = app_root_path.join(&manifest_data.modules_path);
    let mut changes_made = false;

    if !modules_path.exists() {
        log_warn!(stdout, "Modules path does not exist: {}", modules_path.display());
        return Ok(false);
    }

    log_info!(stdout, "Scanning modules directory: {}", modules_path.display());

    let existing_folders: HashSet<String> = fs::read_dir(&modules_path)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if entry.path().is_dir() {
                entry.file_name().to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();

    let mut orphaned_projects = vec![];
    for project in &manifest_data.projects {
        if !DIRS_TO_IGNORE.contains(&project.name.as_str())
            && !existing_folders.contains(&project.name)
        {
            orphaned_projects.push(project.name.clone());
        }
    }

    if !orphaned_projects.is_empty() {
        writeln!(stdout)?;
        log_warn!(stdout, "Found {} orphaned project(s) in manifest:", orphaned_projects.len());
        for project_name in &orphaned_projects {
            writeln!(stdout, "  - {}", project_name)?;
        }

        let should_cleanup = if confirm_all {
            true
        } else {
            let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
            prompt_for_confirmation(
                &mut line_editor,
                "Remove these orphaned projects from all artifacts? (y/N) ",
            )?
        };

        if should_cleanup {
            for project_name in &orphaned_projects {
                log_info!(stdout, "Removing '{}'...", project_name);

                let project = manifest_data
                    .projects
                    .iter()
                    .find(|p| &p.name == project_name);
                let project_type = project
                    .map(|p| p.r#type.clone())
                    .unwrap_or(ProjectType::Library);

                remove_project_from_artifacts(
                    rendered_templates_cache,
                    manifest_data,
                    project_name,
                    project_type,
                    &[
                        ArtifactType::Manifest,
                        ArtifactType::DockerCompose,
                        ArtifactType::Runtime,
                        ArtifactType::ClientSdk,
                    ],
                    app_root_path,
                    &modules_path,
                    stdout,
                )?;
                changes_made = true;
            }

            log_ok!(stdout, "Cleaned up {} orphaned project(s)", orphaned_projects.len());
            writeln!(stdout)?;
        } else {
            log_warn!(stdout, "Skipping cleanup of orphaned projects");
        }
    }

    for entry in fs::read_dir(&modules_path)? {
        let entry = entry?;
        let project_path = entry.path();

        if !project_path.is_dir() {
            continue;
        }

        let Some(project_name) = project_path.file_name() else {
            continue;
        };

        let project_name = project_name.to_string_lossy().to_string();

        if DIRS_TO_IGNORE.contains(&project_name.as_str()) {
            continue;
        }

        writeln!(stdout)?;
        log_info!(stdout, "Processing: {}", project_name);

        // If the project is already in the manifest, use its known type and skip prompts
        let manifest_project = manifest_data
            .projects
            .iter()
            .find(|p| p.name == project_name);

        let project_type = if let Some(existing) = manifest_project {
            let init_type: InitializeType = match existing.r#type {
                ProjectType::Service => InitializeType::Service,
                ProjectType::Worker => InitializeType::Worker,
                ProjectType::Library => InitializeType::Library,
            };
            log_info!(stdout, "Known as: {}", init_type.to_string());
            init_type
        } else {
            // New folder not in manifest — try to detect, then prompt if needed
            let detected_type = detect_project_type(&project_path)?;

            let project_type = if let Some(detected) = detected_type {
                log_info!(stdout, "Detected as: {}", detected.to_string());
                detected
            } else {
                log_warn!(stdout, "Could not auto-detect project type");

                if confirm_all {
                    log_warn!(stdout, "Skipping '{}' (cannot auto-detect and no interaction allowed)", project_name);
                    continue;
                }

                // Include "skip" option to allow excluding non-forklaunch folders
                const SYNC_TYPE_OPTIONS: &[&str] =
                    &["service", "library", "worker", "module", "router", "skip"];

                let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
                let type_str = prompt_with_validation_with_answers(
                    &mut line_editor,
                    stdout,
                    "category",
                    &ArgMatches::default(),
                    &format!(
                        "Project type for '{}' (or 'skip' to ignore)",
                        project_name
                    ),
                    Some(SYNC_TYPE_OPTIONS),
                    |input| SYNC_TYPE_OPTIONS.contains(&input),
                    |_| "Invalid option. Please try again.".to_string(),
                    &project_name,
                    prompts_map,
                )?;

                if type_str == "skip" {
                    log_info!(stdout, "Skipping '{}' (not a forklaunch project)", project_name);
                    continue;
                }

                type_str.parse()?
            };

            // Only prompt for confirmation on new/unrecognized projects
            let should_sync = if confirm_all {
                true
            } else {
                let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
                prompt_for_confirmation(
                    &mut line_editor,
                    &format!(
                        "Sync '{}' as {}? (y/N) ",
                        project_name,
                        project_type.to_string()
                    ),
                )?
            };

            if !should_sync {
                log_info!(stdout, "Skipped");
                continue;
            }

            project_type
        };

        // Take a snapshot of the project state before syncing to detect changes
        let project_snapshot_before = manifest_data
            .projects
            .iter()
            .find(|p| p.name == project_name)
            .map(|p| toml::to_string(p).unwrap_or_default());

        match project_type {
            InitializeType::Service => {
                log_info!(stdout, "Syncing service...");

                match crate::sync::service::sync_service_with_cache(
                    &project_name,
                    app_root_path,
                    manifest_data,
                    &ArgMatches::default(),
                    prompts_map,
                    rendered_templates_cache,
                    stdout,
                ) {
                    Ok(_) => {
                        // Compare snapshot to detect changes
                        let project_snapshot_after = manifest_data
                            .projects
                            .iter()
                            .find(|p| p.name == project_name)
                            .map(|p| toml::to_string(p).unwrap_or_default());

                        if project_snapshot_before != project_snapshot_after {
                            changes_made = true;
                        }
                    }
                    Err(e) => {
                        log_error!(stdout, "{}", e);
                    }
                }
            }
            InitializeType::Worker => {
                log_info!(stdout, "Syncing worker...");

                match crate::sync::worker::sync_worker_with_cache(
                    &project_name,
                    app_root_path,
                    manifest_data,
                    &ArgMatches::default(),
                    prompts_map,
                    rendered_templates_cache,
                    stdout,
                ) {
                    Ok(_) => {
                        // Compare snapshot to detect changes
                        let project_snapshot_after = manifest_data
                            .projects
                            .iter()
                            .find(|p| p.name == project_name)
                            .map(|p| toml::to_string(p).unwrap_or_default());

                        if project_snapshot_before != project_snapshot_after {
                            changes_made = true;
                        }
                    }
                    Err(e) => {
                        log_error!(stdout, "{}", e);
                    }
                }
            }
            InitializeType::Library => {
                log_info!(stdout, "Syncing library...");

                match crate::sync::library::sync_library_with_cache(
                    &project_name,
                    app_root_path,
                    manifest_data,
                    &ArgMatches::default(),
                    prompts_map,
                    rendered_templates_cache,
                    stdout,
                ) {
                    Ok(_) => {
                        // Compare snapshot to detect changes
                        let project_snapshot_after = manifest_data
                            .projects
                            .iter()
                            .find(|p| p.name == project_name)
                            .map(|p| toml::to_string(p).unwrap_or_default());

                        if project_snapshot_before != project_snapshot_after {
                            changes_made = true;
                        }
                    }
                    Err(e) => {
                        log_error!(stdout, "{}", e);
                    }
                }
            }
            InitializeType::Router | InitializeType::Module => {
                writeln!(
                    stdout,
                    "[INFO] Skipped (routers and modules are synced as part of their parent service)"
                )?;
            }
        }
    }

    // Scan entity files for compliance classifications and retention policies
    let modules_path_buf = app_root_path.join(&manifest_data.modules_path);
    match scan_all_compliance(&modules_path_buf) {
        Ok((field_classifications, retention_policies)) => {
            let mut compliance = manifest_data.compliance.take().unwrap_or_default();

            if !field_classifications.is_empty() || !retention_policies.is_empty() {
                compliance.entities = field_classifications;
                compliance.retention = retention_policies
                    .into_iter()
                    .map(|(name, info)| {
                        (
                            name,
                            RetentionManifestConfig {
                                duration: info.duration,
                                action: info.action,
                            },
                        )
                    })
                    .collect();

                log_ok!(
                    stdout,
                    "Scanned {} entities, {} with retention policies",
                    compliance.entities.len(),
                    compliance.retention.len()
                );

                changes_made = true;
            }

            manifest_data.compliance = Some(compliance);
        }
        Err(e) => {
            log_warn!(
                stdout,
                "Failed to scan entity compliance metadata: {}",
                e
            );
        }
    }

    Ok(changes_made)
}

#[derive(Debug)]
pub(crate) struct SyncAllCommand;

impl SyncAllCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for SyncAllCommand {
    fn command(&self) -> Command {
        command(
            "all",
            "Sync all projects in the modules directory to application artifacts",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("The application path"),
        )
        .arg(
            Arg::new("confirm")
                .short('c')
                .long("confirm")
                .action(ArgAction::SetTrue)
                .help("Skip confirmation prompts"),
        )
        .arg(
            Arg::new("prompts")
                .short('P')
                .long("prompts")
                .help("JSON object with pre-provided answers for prompts")
                .value_name("JSON"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let prompts_map: HashMap<String, HashMap<String, String>> =
            if let Some(prompts_json) = matches.get_one::<String>("prompts") {
                json_from_str(prompts_json).with_context(|| "Failed to parse prompts JSON")?
            } else {
                HashMap::new()
            };

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let mut rendered_templates_cache = RenderedTemplatesCache::new();
        rendered_templates_cache.get(&manifest_path)?;

        let manifest_template = rendered_templates_cache.get(&manifest_path)?.unwrap();
        let mut manifest_data =
            toml_from_str::<ApplicationManifestData>(&manifest_template.content)
                .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let confirm_all = matches.get_flag("confirm");

        let _changes_made = sync_all_projects(
            &app_root_path,
            &mut manifest_data,
            &mut rendered_templates_cache,
            confirm_all,
            &prompts_map,
            &mut stdout,
        )?;

        let modules_path = app_root_path.join(&manifest_data.modules_path);
        generate_env_templates(
            &modules_path,
            &manifest_data,
            &mut rendered_templates_cache,
            &mut stdout,
        )?;
        sync_env_local_files(&modules_path, &manifest_data, &mut stdout)?;

        // Sync docker-compose environment sections with discovered env vars
        sync_docker_compose_with_env_vars(
            &app_root_path,
            &modules_path,
            &manifest_data,
            &mut rendered_templates_cache,
            &mut stdout,
        )?;

        rendered_templates_cache.insert(
            manifest_path.to_string_lossy().to_string(),
            RenderedTemplate {
                path: manifest_path.clone(),
                content: toml::to_string_pretty(&manifest_data)
                    .context("Failed to serialize manifest")?,
                context: Some("Failed to write manifest".to_string()),
            },
        );

        let rendered_templates: Vec<_> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .collect();

        write_rendered_templates(&rendered_templates, false, &mut stdout)?;

        writeln!(stdout)?;
        log_header!(stdout, Color::Green, "Sync all completed");

        Ok(())
    }
}

/// Sync docker-compose environment sections with env vars discovered from code scanning.
fn sync_docker_compose_with_env_vars(
    app_root_path: &Path,
    modules_path: &Path,
    manifest_data: &ApplicationManifestData,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    stdout: &mut StandardStream,
) -> Result<()> {
    let docker_path = app_root_path.join(
        manifest_data
            .docker_compose_path
            .clone()
            .unwrap_or_else(|| "docker-compose.yaml".to_string()),
    );

    if !docker_path.exists() {
        return Ok(());
    }

    let docker_content = if let Some(template) = rendered_templates_cache.get(&docker_path)? {
        template.content.clone()
    } else {
        return Ok(());
    };

    let mut docker_compose: DockerCompose =
        yaml_from_str(&docker_content).context(ERROR_FAILED_TO_PARSE_DOCKER_COMPOSE)?;

    // Re-discover env vars (uses the same enhanced scanning with process.env)
    let env_vars_cache = RenderedTemplatesCache::new();
    let project_env_vars = find_all_env_vars(modules_path, &env_vars_cache)?;

    if project_env_vars.is_empty() {
        return Ok(());
    }

    let changes_made = sync_docker_compose_env_vars(
        &mut docker_compose,
        &project_env_vars,
        manifest_data,
        modules_path,
        stdout,
    )?;

    if changes_made {
        rendered_templates_cache.insert(
            docker_path.to_string_lossy().to_string(),
            RenderedTemplate {
                path: docker_path.clone(),
                content: yaml_to_string(&docker_compose)
                    .context("Failed to serialize docker-compose")?,
                context: Some("Failed to write docker-compose".to_string()),
            },
        );

        log_ok!(stdout, "Synchronized docker-compose environment variables");
    }

    Ok(())
}

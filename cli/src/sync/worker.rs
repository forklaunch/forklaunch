use std::{collections::HashMap, io::Write};

use anyhow::{Context, Result, bail};
use clap::ArgMatches;
use rustyline::{Editor, history::DefaultHistory};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    constants::{ERROR_FAILED_TO_PARSE_MANIFEST, WorkerType},
    core::{
        base_path::{RequiredLocation, find_app_root_path},
        manifest::{ProjectType, application::ApplicationManifestData},
        rendered_template::{RenderedTemplatesCache, write_rendered_templates},
        sync::{
            artifacts::{ArtifactType, ProjectSyncMetadata, sync_project_to_artifacts},
            detection::detect_worker_config,
            resolvers::{
                display_detection_results, resolve_database_config, resolve_description,
                resolve_worker_type,
            },
        },
    },
    prompt::{ArrayCompleter, prompt_for_confirmation},
};

#[derive(Debug)]
pub(crate) struct WorkerSyncCommand;

impl WorkerSyncCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

pub(crate) fn sync_worker_with_cache(
    worker_name: &str,
    app_root_path: &std::path::Path,
    manifest_data: &mut ApplicationManifestData,
    matches: &ArgMatches,
    prompts_map: &HashMap<String, HashMap<String, String>>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    stdout: &mut StandardStream,
) -> Result<()> {
    let modules_path = app_root_path.join(&manifest_data.modules_path);
    let worker_path = modules_path.join(worker_name);

    if !worker_path.exists() {
        use crate::core::sync::artifacts::remove_project_from_artifacts;

        if let Some(project) = manifest_data
            .projects
            .iter()
            .find(|p| p.name == worker_name)
        {
            log_warn!(stdout, "[WARN] Worker directory not found, but exists in manifest");

            let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
            let should_cleanup = prompt_for_confirmation(
                &mut line_editor,
                &format!("Remove worker '{}' from all artifacts? (y/N) ", worker_name),
            )?;

            if !should_cleanup {
                log_warn!(stdout, "[INFO] Skipping cleanup");
                bail!("Worker directory not found: {}", worker_path.display());
            }

            writeln!(
                stdout,
                "[INFO] Removing '{}' from artifacts...",
                worker_name
            )?;

            remove_project_from_artifacts(
                rendered_templates_cache,
                manifest_data,
                worker_name,
                project.r#type.clone(),
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

            log_ok!(stdout, "[OK] Removed orphaned worker '{}'", worker_name);
            return Ok(());
        } else {
            log_error!(stdout, "[ERROR] Worker directory not found: {}", worker_path.display());
            bail!("Worker directory not found: {}", worker_path.display());
        }
    }

    if manifest_data.projects.iter().any(|p| p.name == worker_name) {
        log_ok!(stdout, "[INFO] Worker '{}' already synced", worker_name);
        return Ok(());
    }

    log_info!(stdout, "[INFO] Detecting worker configuration from files...");

    let detected = detect_worker_config(&worker_path)?;
    display_detection_results(&detected, stdout)?;

    let worker_type = resolve_worker_type(worker_name, &detected, matches, prompts_map, stdout)?;

    let database = match worker_type {
        WorkerType::Database => Some(resolve_database_config(
            worker_name,
            &worker_path,
            &detected,
            matches,
            prompts_map,
            stdout,
        )?),
        _ => None,
    };

    let description = resolve_description(worker_name, &worker_path, matches, prompts_map, stdout)?;

    let sync_metadata = ProjectSyncMetadata {
        project_type: ProjectType::Worker,
        project_name: worker_name.to_string(),
        description,
        database,
        infrastructure: vec![],
        worker_type: Some(worker_type),
    };

    sync_project_to_artifacts(
        rendered_templates_cache,
        manifest_data,
        &sync_metadata,
        &[
            ArtifactType::Manifest,
            ArtifactType::DockerCompose,
            ArtifactType::Runtime,
            ArtifactType::ModulesTsconfig,
            ArtifactType::ClientSdk,
        ],
        &app_root_path,
        &modules_path,
        stdout,
    )?;

    log_ok!(stdout, "[OK] Worker '{}' synced successfully", worker_name);

    Ok(())
}

impl crate::CliCommand for WorkerSyncCommand {
    fn command(&self) -> clap::Command {
        use clap::Arg;
        crate::core::command::command("worker", "Sync a specific worker to application artifacts")
            .arg(
                Arg::new("name")
                    .help("The name of the worker")
                    .required(true),
            )
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path"),
            )
            .arg(
                Arg::new("prompts")
                    .short('P')
                    .long("prompts")
                    .help("JSON object with pre-provided answers")
                    .value_name("JSON"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let worker_name = matches.get_one::<String>("name").unwrap();

        let prompts_map: HashMap<String, HashMap<String, String>> =
            if let Some(prompts_json) = matches.get_one::<String>("prompts") {
                serde_json::from_str(prompts_json).context("Failed to parse prompts JSON")?
            } else {
                HashMap::new()
            };

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let mut rendered_templates_cache = RenderedTemplatesCache::new();
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");
        rendered_templates_cache.get(&manifest_path)?;

        let manifest_template = rendered_templates_cache.get(&manifest_path)?.unwrap();
        let mut manifest_data: ApplicationManifestData =
            toml::from_str(&manifest_template.content).context(ERROR_FAILED_TO_PARSE_MANIFEST)?;

        sync_worker_with_cache(
            worker_name,
            &app_root_path,
            &mut manifest_data,
            matches,
            &prompts_map,
            &mut rendered_templates_cache,
            &mut stdout,
        )?;

        rendered_templates_cache.insert(
            manifest_path.to_string_lossy().to_string(),
            crate::core::rendered_template::RenderedTemplate {
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

        Ok(())
    }
}

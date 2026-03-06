use std::{collections::{HashMap, HashSet}, env, fs, io::Write, path::{Path, PathBuf}};

use anyhow::{Context, Result};
use clap::{ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::ERROR_FAILED_TO_PARSE_MANIFEST,
    core::{
        ast::infrastructure::env::find_all_env_vars,
        env::{
            add_env_vars_to_file, find_workspace_root, get_modules_path, get_target_env_file,
            is_env_var_defined,
        },
        env_scope::is_pulumi_injected,
        env_template::generate_env_templates,
        manifest::application::ApplicationManifestData,
        rendered_template::{RenderedTemplatesCache, write_rendered_templates},
    },
};

#[derive(Debug)]
pub(crate) struct SyncCommand;

impl SyncCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for SyncCommand {
    fn command(&self) -> Command {
        Command::new("sync")
            .about("Sync missing environment variables by adding them with blank values")
            .long_about("Uses validate to find missing environment variables and adds them to appropriate .env files with blank values. Respects the .env hierarchy by placing common variables in root .env.local")
            .arg(
                clap::Arg::new("dry-run")
                    .long("dry-run")
                    .short('n')
                    .help("Show what would be done without making changes")
                    .action(clap::ArgAction::SetTrue)
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let dry_run = matches.get_flag("dry-run");

        if dry_run {
            log_warn!(stdout, "Dry run mode - no changes will be made");
        } else {
            log_info!(stdout, "Syncing environment variables...");
        }

        let current_dir = env::current_dir()?;
        let workspace_root = find_workspace_root(&current_dir)?;
        let modules_path = get_modules_path(&workspace_root)?;

        writeln!(stdout, "Workspace: {}", workspace_root.display())?;
        writeln!(stdout, "Modules path: {}", modules_path.display())?;

        let manifest_path = workspace_root.join(".forklaunch").join("manifest.toml");
        let manifest_content = fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read manifest: {}", manifest_path.display()))?;
        let manifest_data: ApplicationManifestData =
            toml::from_str(&manifest_content)
                .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;
        let project_names: Vec<String> = manifest_data
            .projects
            .iter()
            .map(|p| p.name.clone())
            .collect();

        let rendered_templates_cache = RenderedTemplatesCache::new();
        let project_env_vars = find_all_env_vars(&modules_path, &rendered_templates_cache)?;

        if project_env_vars.is_empty() {
            log_warn!(stdout, "No projects with registrations.ts found");
            return Ok(());
        }

        log_info!(stdout, "\nRunning validation first...");
        let mut missing_vars_by_project = HashMap::new();

        for (project_name, env_vars) in &project_env_vars {
            let project_path = modules_path.join(project_name);
            let mut missing_set = HashSet::new();

            for env_var in env_vars {
                // Skip Pulumi-injected vars (inter-service URLs, auth URLs) —
                // these are injected at deploy time
                if is_pulumi_injected(&env_var.var_name, &project_names) {
                    continue;
                }
                if !is_env_var_defined(&project_path, &env_var.var_name)? {
                    missing_set.insert(env_var.var_name.clone());
                }
            }

            if !missing_set.is_empty() {
                let mut vars: Vec<String> = missing_set.into_iter().collect();
                vars.sort();
                missing_vars_by_project.insert(project_name.clone(), vars);
            }
        }

        if missing_vars_by_project.is_empty() {
            log_ok!(stdout, "No missing environment variables found!");
            return Ok(());
        }

        let sync_plan = create_sync_plan(&missing_vars_by_project, &workspace_root, &modules_path)?;

        display_sync_plan(&sync_plan, &mut stdout)?;

        if !dry_run {
            execute_sync_plan(&sync_plan, &mut stdout)?;

            // Generate .env.template files
            let mut env_template_cache = RenderedTemplatesCache::new();
            generate_env_templates(
                &modules_path,
                &manifest_data,
                &mut env_template_cache,
                &mut stdout,
            )?;
            let env_templates: Vec<_> = env_template_cache
                .drain()
                .map(|(_, template)| template)
                .collect();
            write_rendered_templates(&env_templates, false, &mut stdout)?;

            log_ok!(stdout, "\nEnvironment sync completed!");
            writeln!(
                stdout,
                "Remember to fill in the actual values for the added variables. Empty vars can be satisfied with \"placeholder\"."
            )?;
        } else {
            log_info!(
                stdout,
                "\nThis was a dry run. Use 'forklaunch environment sync' to apply changes."
            );
        }

        Ok(())
    }
}

#[derive(Debug)]
struct SyncPlan {
    root_vars: Vec<String>,
    project_vars: HashMap<String, Vec<String>>,
    root_env_file: PathBuf,
    project_env_files: HashMap<String, PathBuf>,
}

fn create_sync_plan(
    missing_vars_by_project: &HashMap<String, Vec<String>>,
    workspace_root: &Path,
    modules_path: &Path,
) -> Result<SyncPlan> {
    let mut var_usage_count: HashMap<String, Vec<String>> = HashMap::new();

    for (project_name, missing_vars) in missing_vars_by_project {
        for var_name in missing_vars {
            var_usage_count
                .entry(var_name.clone())
                .or_insert_with(Vec::new)
                .push(project_name.clone());
        }
    }

    let mut root_vars = Vec::new();
    let mut project_specific_vars: HashMap<String, Vec<String>> = HashMap::new();

    for (var_name, projects) in var_usage_count {
        if projects.len() >= 2 {
            root_vars.push(var_name);
        } else {
            let project_name = &projects[0];
            project_specific_vars
                .entry(project_name.clone())
                .or_insert_with(Vec::new)
                .push(var_name);
        }
    }

    let root_env_file = workspace_root.join(".env.local");
    let mut project_env_files = HashMap::new();

    for project_name in missing_vars_by_project.keys() {
        let project_path = modules_path.join(project_name);
        let target_file = get_target_env_file(&project_path)?;
        project_env_files.insert(project_name.clone(), target_file);
    }

    Ok(SyncPlan {
        root_vars,
        project_vars: project_specific_vars,
        root_env_file,
        project_env_files,
    })
}

fn display_sync_plan(plan: &SyncPlan, stdout: &mut StandardStream) -> Result<()> {
    log_info!(stdout, "\nSync Plan");
    writeln!(stdout, "{}", "=".repeat(40))?;

    if !plan.root_vars.is_empty() {
        writeln!(
            stdout,
            "\n{} variables to add to root .env.local:",
            plan.root_vars.len()
        )?;
        writeln!(stdout, "   {}", plan.root_env_file.display())?;
        for var_name in &plan.root_vars {
            log_info!(stdout, "   - {}", var_name);
        }
    }

    if !plan.project_vars.is_empty() {
        writeln!(stdout, "\nProject-specific variables:")?;
        for (project_name, vars) in &plan.project_vars {
            if let Some(env_file) = plan.project_env_files.get(project_name) {
                writeln!(stdout, "\n   {} ({} variables):", project_name, vars.len())?;
                writeln!(stdout, "   {}", env_file.display())?;
                for var_name in vars {
                    log_info!(stdout, "   - {}", var_name);
                }
            }
        }
    }

    let total_vars =
        plan.root_vars.len() + plan.project_vars.values().map(|v| v.len()).sum::<usize>();

    log_warn!(stdout, "\nTotal variables to add: {}", total_vars);

    Ok(())
}

fn execute_sync_plan(plan: &SyncPlan, stdout: &mut StandardStream) -> Result<()> {
    log_info!(stdout, "\nExecuting sync plan...");

    if !plan.root_vars.is_empty() {
        writeln!(
            stdout,
            "Adding {} variables to root .env.local...",
            plan.root_vars.len()
        )?;

        let mut root_vars_map = HashMap::new();
        for var_name in &plan.root_vars {
            root_vars_map.insert(var_name.clone(), String::new());
        }

        add_env_vars_to_file(&plan.root_env_file, &root_vars_map)?;
        log_ok!(
            stdout,
            "   Root variables added to {}",
            plan.root_env_file.display()
        );
    }

    for (project_name, vars) in &plan.project_vars {
        if let Some(env_file) = plan.project_env_files.get(project_name) {
            writeln!(
                stdout,
                "Adding {} variables to {}...",
                vars.len(),
                project_name
            )?;

            let mut project_vars_map = HashMap::new();
            for var_name in vars {
                project_vars_map.insert(var_name.clone(), String::new());
            }

            add_env_vars_to_file(env_file, &project_vars_map)?;
            log_ok!(stdout, "   Variables added to {}", env_file.display());
        }
    }

    Ok(())
}

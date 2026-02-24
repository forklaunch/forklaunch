use std::{collections::HashMap, io::Write, path::Path};

use anyhow::Result;
use clap::{ArgMatches, Command};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{
        ast::infrastructure::env::{EnvVarUsage, find_all_env_vars},
        env::{find_workspace_root, get_modules_path, is_env_var_defined},
        env_scope::{EnvironmentVariableScope, ScopedEnvVar, determine_env_var_scopes},
        rendered_template::RenderedTemplatesCache,
    },
};

#[derive(Debug)]
pub(crate) struct ValidateCommand;

impl ValidateCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ValidateCommand {
    fn command(&self) -> Command {
        Command::new("validate")
            .about("Check all workspace projects for missing environment variables")
            .long_about("Validates that all environment variables referenced in registrations.ts files have corresponding entries in .env files")
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let (app_root, manifest) = crate::core::validate::require_manifest(matches)?;

        log_info!(stdout, "Validating environment variables...");

        let workspace_root = find_workspace_root(&app_root)?;
        let modules_path = get_modules_path(&workspace_root)?;

        writeln!(stdout, "Workspace: {}", workspace_root.display())?;
        writeln!(stdout, "Modules path: {}", modules_path.display())?;

        let rendered_templates_cache = RenderedTemplatesCache::new();
        let project_env_vars = find_all_env_vars(&modules_path, &rendered_templates_cache)?;

        if project_env_vars.is_empty() {
            log_warn!(stdout, "No projects with registrations.ts found");
            return Ok(());
        }

        writeln!(stdout, "\n{} projects found:", project_env_vars.len())?;
        for project_name in project_env_vars.keys() {
            writeln!(stdout, "  - {}", project_name)?;
        }

        let scoped_env_vars = determine_env_var_scopes(&project_env_vars, &manifest)?;

        let mut validation_results = ValidationResults::new();

        for (project_name, env_vars) in &project_env_vars {
            let project_path = modules_path.join(project_name);
            let project_result = validate_project(&project_path, env_vars)?;
            validation_results.add_project_result(project_name.clone(), project_result);
        }

        display_validation_results(&validation_results, &scoped_env_vars, &mut stdout)?;

        analyze_env_hierarchy(
            &project_env_vars,
            &modules_path,
            &workspace_root,
            &mut stdout,
        )?;

        if validation_results.has_missing_vars() {
            std::process::exit(1);
        }

        Ok(())
    }
}

#[derive(Debug)]
struct ProjectValidationResult {
    missing_vars: Vec<EnvVarUsage>,
    defined_vars: Vec<String>,
}

#[derive(Debug)]
struct ValidationResults {
    projects: HashMap<String, ProjectValidationResult>,
}

impl ValidationResults {
    fn new() -> Self {
        Self {
            projects: HashMap::new(),
        }
    }

    fn add_project_result(&mut self, project_name: String, result: ProjectValidationResult) {
        self.projects.insert(project_name, result);
    }

    fn has_missing_vars(&self) -> bool {
        self.projects
            .values()
            .any(|result| !result.missing_vars.is_empty())
    }

    fn total_missing_count(&self) -> usize {
        self.projects
            .values()
            .map(|result| result.missing_vars.len())
            .sum()
    }
}

fn validate_project(
    project_path: &Path,
    env_vars: &[EnvVarUsage],
) -> Result<ProjectValidationResult> {
    let mut missing_vars = Vec::new();
    let mut defined_vars = Vec::new();

    for env_var in env_vars {
        if is_env_var_defined(project_path, &env_var.var_name)? {
            defined_vars.push(env_var.var_name.clone());
        } else {
            missing_vars.push(env_var.clone());
        }
    }

    Ok(ProjectValidationResult {
        missing_vars,
        defined_vars,
    })
}

fn display_validation_results(
    results: &ValidationResults,
    scoped_env_vars: &[ScopedEnvVar],
    stdout: &mut StandardStream,
) -> Result<()> {
    log_info!(stdout, "\nValidation Results");
    writeln!(stdout, "{}", "=".repeat(50))?;

    let app_vars: Vec<_> = scoped_env_vars
        .iter()
        .filter(|v| v.scope == EnvironmentVariableScope::Application)
        .collect();
    let service_vars: Vec<_> = scoped_env_vars
        .iter()
        .filter(|v| v.scope == EnvironmentVariableScope::Service)
        .collect();
    let worker_vars: Vec<_> = scoped_env_vars
        .iter()
        .filter(|v| v.scope == EnvironmentVariableScope::Worker)
        .collect();

    let mut has_any_missing = false;

    if !app_vars.is_empty() {
        log_header!(
            stdout,
            Color::Cyan,
            "\nApplication-Level Variables ({}):",
            app_vars.len()
        );
        for var in &app_vars {
            display_scoped_var_status(var, results, stdout, &mut has_any_missing)?;
        }
    }

    if !service_vars.is_empty() {
        log_header!(
            stdout,
            Color::Cyan,
            "\nService-Level Variables ({}):",
            service_vars.len()
        );
        for var in &service_vars {
            display_scoped_var_status(var, results, stdout, &mut has_any_missing)?;
        }
    }

    if !worker_vars.is_empty() {
        log_header!(stdout, Color::Cyan, "\nWorker-Level Variables ({}):", worker_vars.len());
        for var in &worker_vars {
            display_scoped_var_status(var, results, stdout, &mut has_any_missing)?;
        }
    }

    log_info!(stdout, "\nSummary");
    writeln!(stdout, "{}", "-".repeat(30))?;

    let total_projects = results.projects.len();
    let projects_with_issues = results
        .projects
        .values()
        .filter(|result| !result.missing_vars.is_empty())
        .count();
    let total_missing = results.total_missing_count();

    writeln!(stdout, "Projects scanned: {}", total_projects)?;

    let issues_color = if projects_with_issues > 0 { Color::Red } else { Color::Green };
    log_write!(stdout, issues_color, "Projects with missing vars: {}\n", projects_with_issues);

    let missing_color = if total_missing > 0 { Color::Red } else { Color::Green };
    log_write!(stdout, missing_color, "Total missing variables: {}\n", total_missing);

    if has_any_missing {
        log_info!(
            stdout,
            "\nRun 'forklaunch environment sync' to automatically add missing variables with blank values"
        );
    } else {
        log_ok!(
            stdout,
            "\nAll environment variables are properly configured!"
        );
    }

    Ok(())
}

fn display_scoped_var_status(
    var: &ScopedEnvVar,
    results: &ValidationResults,
    stdout: &mut StandardStream,
    has_any_missing: &mut bool,
) -> Result<()> {
    // Check if this variable is defined in any of the projects that use it
    let mut is_defined = false;
    let mut is_missing = false;

    for project_name in &var.used_by {
        if let Some(project_result) = results.projects.get(project_name) {
            if project_result.defined_vars.contains(&var.name) {
                is_defined = true;
            }
            if project_result
                .missing_vars
                .iter()
                .any(|v| v.var_name == var.name)
            {
                is_missing = true;
            }
        }
    }

    if is_missing {
        *has_any_missing = true;
        log_write!(stdout, Color::Red, "  [MISSING] {}", var.name);
    } else if is_defined {
        log_write!(stdout, Color::Green, "  [OK] {}", var.name);
    } else {
        log_write!(stdout, Color::Yellow, "  [UNKNOWN] {}", var.name);
    }

    if let Some(scope_id) = &var.scope_id {
        log_write!(stdout, Color::Yellow, " ({})", scope_id);
    }

    writeln!(stdout)?;
    Ok(())
}

fn analyze_env_hierarchy(
    project_env_vars: &HashMap<String, Vec<EnvVarUsage>>,
    _modules_path: &Path,
    workspace_root: &Path,
    stdout: &mut StandardStream,
) -> Result<()> {
    log_info!(stdout, "\nEnvironment Hierarchy Analysis");
    writeln!(stdout, "{}", "=".repeat(50))?;

    let mut var_counts: HashMap<String, Vec<String>> = HashMap::new();

    for (project_name, env_vars) in project_env_vars {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for env_var in env_vars {
            if seen.insert(env_var.var_name.as_str()) {
                var_counts
                    .entry(env_var.var_name.clone())
                    .or_insert_with(Vec::new)
                    .push(project_name.clone());
            }
        }
    }

    let mut common_vars: Vec<(&String, &Vec<String>)> = var_counts
        .iter()
        .filter(|(_, projects)| projects.len() > 1)
        .collect();
    common_vars.sort_by_key(|(_, projects)| std::cmp::Reverse(projects.len()));

    if common_vars.is_empty() {
        writeln!(
            stdout,
            "No common environment variables found across projects."
        )?;
        return Ok(());
    }

    writeln!(
        stdout,
        "Common variables that could be moved to root .env.local:"
    )?;
    writeln!(stdout)?;

    for (var_name, projects) in &common_vars {
        log_info!(stdout, "{} (used in {} projects)", var_name, projects.len());
        for project in projects.iter() {
            writeln!(stdout, "   - {}", project)?;
        }
        writeln!(stdout)?;
    }

    let root_env_local = workspace_root.join(".env.local");
    if root_env_local.exists() {
        writeln!(
            stdout,
            "Root .env.local exists at: {}",
            root_env_local.display()
        )?;
    } else {
        writeln!(
            stdout,
            "Root .env.local not found. Consider creating one for common variables."
        )?;
    }

    Ok(())
}

use std::{collections::HashMap, io::Write, path::Path};

use anyhow::{Result, bail};
use clap::ArgMatches;
use rustyline::{Editor, history::DefaultHistory};
use serde_json::from_str as json_from_str;
use termcolor::{StandardStream, WriteColor};

use super::detection::{DetectedConfig, detect_description_from_package_json};
use crate::{
    constants::{Database, Infrastructure, WorkerType},
    core::package_json::project_package_json::ProjectPackageJson,
    prompt::{
        ArrayCompleter, prompt_comma_separated_list_with_answers,
        prompt_with_validation_with_answers, prompt_without_validation_with_answers,
    },
};

pub fn resolve_database_config(
    project_name: &str,
    project_path: &Path,
    detected: &DetectedConfig,
    matches: &ArgMatches,
    prompts_map: &HashMap<String, HashMap<String, String>>,
    stdout: &mut StandardStream,
) -> Result<Database> {
    let override_value = prompts_map
        .get(project_name)
        .and_then(|p| p.get("database"))
        .map(|s| s.as_str());

    if let Some(detected_db) = &detected.database {
        if override_value.is_none() {
            return Ok(*detected_db);
        }
    }

    if let Some(override_str) = override_value {
        if override_str == "none" {
            return fallback_to_package_json(project_path, stdout);
        } else {
            return Ok(override_str.parse()?);
        }
    }

    log_warn!(stdout, "Could not detect database configuration. Please specify:");

    let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
    let mut database_options = vec!["none"];
    database_options.extend_from_slice(&Database::VARIANTS);

    let database_input = prompt_with_validation_with_answers(
        &mut line_editor,
        stdout,
        "database",
        matches,
        "database type",
        Some(&database_options),
        |input| database_options.contains(&input),
        |_| "Invalid database type. Please try again".to_string(),
        project_name,
        prompts_map,
    )?;

    if database_input == "none" {
        fallback_to_package_json(project_path, stdout)
    } else {
        Ok(database_input.parse()?)
    }
}

pub fn resolve_infrastructure_config(
    project_name: &str,
    detected: &DetectedConfig,
    matches: &ArgMatches,
    prompts_map: &HashMap<String, HashMap<String, String>>,
    _stdout: &mut StandardStream,
) -> Result<Vec<Infrastructure>> {
    let override_value = prompts_map
        .get(project_name)
        .and_then(|p| p.get("infrastructure"));

    if let Some(override_str) = override_value {
        if override_str == "none" {
            return Ok(vec![]);
        } else {
            return Ok(override_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect());
        }
    }

    if !detected.infrastructure.is_empty() {
        return Ok(detected.infrastructure.clone());
    }

    let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
    Ok(prompt_comma_separated_list_with_answers(
        &mut line_editor,
        "infrastructure",
        matches,
        &Infrastructure::VARIANTS,
        None,
        "additional infrastructure components",
        true,
        project_name,
        prompts_map,
    )?
    .iter()
    .map(|s| s.parse().unwrap())
    .collect())
}

pub fn resolve_description(
    project_name: &str,
    project_path: &Path,
    matches: &ArgMatches,
    prompts_map: &HashMap<String, HashMap<String, String>>,
    stdout: &mut StandardStream,
) -> Result<String> {
    let detected_description = detect_description_from_package_json(project_path)
        .ok()
        .flatten();

    if let Some(detected) = &detected_description {
        let override_value = prompts_map
            .get(project_name)
            .and_then(|p| p.get("description"));

        if override_value.is_none() {
            log_ok!(stdout, "[INFO] Detected description: {}", detected);

            let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
            let override_choice = prompt_with_validation_with_answers(
                &mut line_editor,
                stdout,
                "override_description",
                matches,
                "Override detected description? (y/n)",
                Some(&["y", "n", "yes", "no"]),
                |input| matches!(input, "y" | "n" | "yes" | "no"),
                |_| "Please enter 'y' or 'n'".to_string(),
                project_name,
                prompts_map,
            )?;

            if matches!(override_choice.as_str(), "n" | "no") {
                return Ok(detected.clone());
            }
        } else {
            return Ok(override_value.unwrap().clone());
        }
    }

    let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
    prompt_without_validation_with_answers(
        &mut line_editor,
        stdout,
        "description",
        matches,
        "project description",
        None,
        project_name,
        prompts_map,
    )
}

pub fn fallback_to_package_json(
    project_path: &Path,
    stdout: &mut StandardStream,
) -> Result<Database> {
    let package_json_path = project_path.join("package.json");

    if !package_json_path.exists() {
        log_error!(stdout, "[ERROR] package.json not found");
        bail!("Cannot determine database configuration: package.json not found");
    }

    let content = std::fs::read_to_string(&package_json_path)?;
    let package_json: ProjectPackageJson = json_from_str(&content)?;

    if let Some(deps) = &package_json.dependencies {
        let found_db = deps
            .databases
            .iter()
            .find(|db| Database::VARIANTS.contains(&db.to_string().as_str()));

        if let Some(db) = found_db {
            return Ok(*db);
        }
    }

    bail!("No database found in package.json dependencies");
}

pub fn display_detection_results(
    detected: &DetectedConfig,
    stdout: &mut StandardStream,
) -> Result<()> {
    if let Some(ref db) = detected.database {
        log_ok!(stdout, "[INFO] Detected database: {}", db.to_string());
    }

    if !detected.infrastructure.is_empty() {
        log_ok!(stdout, "Detected infrastructure: {}", detected.infrastructure.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", "));
    }

    if let Some(ref desc) = detected.description {
        log_ok!(stdout, "[INFO] Detected description: {}", desc);
    }

    if detected.database.is_none()
        && detected.infrastructure.is_empty()
        && detected.description.is_none()
        && detected.worker_type.is_none()
    {
        log_warn!(stdout, "[INFO] No configuration auto-detected");
    }

    Ok(())
}

pub fn resolve_worker_type(
    worker_name: &str,
    detected: &DetectedConfig,
    matches: &ArgMatches,
    prompts_map: &HashMap<String, HashMap<String, String>>,
    stdout: &mut StandardStream,
) -> Result<WorkerType> {
    if let Some(project_prompts) = prompts_map.get(worker_name) {
        if let Some(worker_type_str) = project_prompts.get("type") {
            return Ok(worker_type_str.parse()?);
        }
    }

    if let Some(detected_type) = &detected.worker_type {
        log_ok!(stdout, "Detected worker type: {}", detected_type.to_string());

        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let override_choice = prompt_with_validation_with_answers(
            &mut line_editor,
            stdout,
            "override_worker_type",
            matches,
            "Override detected worker type? (y/n)",
            Some(&["y", "n", "yes", "no"]),
            |input| matches!(input, "y" | "n" | "yes" | "no"),
            |_| "Please enter 'y' or 'n'".to_string(),
            worker_name,
            prompts_map,
        )?;

        if matches!(override_choice.as_str(), "n" | "no") {
            return Ok(*detected_type);
        }
    }

    let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
    let worker_type_str = prompt_with_validation_with_answers(
        &mut line_editor,
        stdout,
        "type",
        matches,
        "worker type",
        Some(&WorkerType::VARIANTS),
        |input| WorkerType::VARIANTS.contains(&input),
        |_| "Invalid worker type. Please try again".to_string(),
        worker_name,
        prompts_map,
    )?;

    Ok(worker_type_str.parse()?)
}

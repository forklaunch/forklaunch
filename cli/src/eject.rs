use std::{
    borrow::Cow,
    collections::HashMap,
    fs::{read_dir, read_to_string},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use fs_extra::dir::{CopyOptions, copy};
use rustyline::{Editor, history::DefaultHistory};
use serde_json::{Map, Value, from_str, to_string_pretty};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use walkdir::{DirEntry, WalkDir};

use crate::{
    CliCommand,
    constants::{
        ERROR_FAILED_TO_EJECT_DIRECTORY_NOT_EJECTABLE, ERROR_FAILED_TO_PARSE_MANIFEST,
        ERROR_FAILED_TO_PARSE_PACKAGE_JSON, ERROR_FAILED_TO_READ_MANIFEST,
        ERROR_FAILED_TO_READ_PACKAGE_JSON,
    },
    core::{
        ast::transformations::transform_domain_schemas_index::transform_domain_schemas_index_ts,
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        manifest::{
            ApplicationInitializationMetadata, InitializableManifestConfig,
            InitializableManifestConfigMetadata, ManifestData,
            application::ApplicationManifestData,
        },
        relative_path::get_relative_path,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
        validate::require_manifest,
    },
    prompt::{ArrayCompleter, prompt_comma_separated_list, prompt_for_confirmation},
};

// TODO: add injected token into struct
#[derive(Debug)]
pub(crate) struct EjectCommand {}

impl EjectCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

fn should_skip_file(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some("index.ts")
}

fn copy_directory_selective(
    source_dir: &Path,
    target_dir: &Path,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    for entry in WalkDir::new(source_dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let source_path = entry.path();

            if should_skip_file(source_path) {
                continue;
            }

            let relative_path = source_path.strip_prefix(source_dir)?;
            let target_path = target_dir.join(relative_path);

            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            if dryrun {
                writeln!(
                    stdout,
                    "Would copy {} to {}",
                    source_path.display(),
                    target_path.display()
                )?;
            } else {
                std::fs::copy(source_path, target_path)?;
            }
        }
    }
    Ok(())
}

fn eject_dependencies(
    project_variant: &Option<String>,
    package_json: &mut Value,
    matches: &ArgMatches,
    line_editor: &mut Editor<ArrayCompleter, DefaultHistory>,
    base_path: &Path,
    stdout: &mut StandardStream,
    dryrun: bool,
) -> Result<(Vec<String>, Map<String, Value>)> {
    let dependencies = package_json
        .get("dependencies")
        .and_then(|deps| deps.as_object())
        .unwrap();

    let mut ejection_candidates = Vec::new();
    for dependency in dependencies {
        let dependency_name = dependency.0;
        if (dependency_name.starts_with("@forklaunch/implementation-")
            && dependency_name.contains(project_variant.as_ref().unwrap()))
            || dependency_name.starts_with("@forklaunch/interfaces-")
            || dependency_name.starts_with("@forklaunch/infrastructure-")
        {
            ejection_candidates.push(dependency_name.to_string());
        }
    }

    let dependencies_to_eject: Vec<String> = prompt_comma_separated_list(
        line_editor,
        "dependencies",
        matches,
        &ejection_candidates
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        None,
        "dependencies to eject",
        true,
    )?
    .iter()
    .filter(|dependency| {
        if ejection_candidates.contains(dependency) {
            true
        } else {
            let _ = stdout.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)));
            let _ = writeln!(stdout, "Ignoring {} ejection", &dependency);
            let _ = stdout.reset();
            false
        }
    })
    .map(|dependency| dependency.to_string())
    .collect();

    detect_collisions_and_prepare_merges(&base_path, &dependencies_to_eject)?;

    let filtered_dependencies: Map<String, Value> = dependencies
        .iter()
        .filter(|(name, _)| !dependencies_to_eject.contains(&name.to_string()))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();

    let node_modules_path = if base_path.join("node_modules").exists() {
        base_path.join("node_modules")
    } else {
        base_path.parent().unwrap().join("node_modules")
    };

    for ejectable_dependency in &dependencies_to_eject {
        let dependency_path = node_modules_path
            .join(&ejectable_dependency)
            .join("lib")
            .join("eject");

        if !dependency_path.exists() {
            bail!(
                "{}",
                ERROR_FAILED_TO_EJECT_DIRECTORY_NOT_EJECTABLE.to_string()
            );
        }

        let mut options = CopyOptions::new();
        options.overwrite = false;
        options.copy_inside = true;

        for entry in read_dir(&dependency_path)? {
            let entry = entry?;
            let source_path = entry.path();
            let target_path = base_path.join(entry.file_name());

            if source_path.is_dir() {
                copy_directory_selective(&source_path, &target_path, dryrun, stdout)?;
            } else {
                if should_skip_file(&source_path) {
                    if dryrun {
                        writeln!(
                            stdout,
                            "Would skip {} (will be merged)",
                            source_path.display()
                        )?;
                    }
                    continue;
                }

                if dryrun {
                    writeln!(
                        stdout,
                        "Would copy {} to {}",
                        source_path.display(),
                        target_path.display()
                    )?;
                } else {
                    copy(&source_path, &target_path, &options).with_context(|| {
                        format!("Failed to copy files from {}", source_path.display())
                    })?;
                }
            }
        }
    }

    Ok((dependencies_to_eject, filtered_dependencies))
}

fn domain_prefix(package_name: &str) -> &str {
    match package_name {
        "/enum" => "/domain/enum",
        "/schemas" => "/domain/schemas",
        "/interfaces" => "/domain/interfaces",
        "/types" => "/domain/types",
        _ => package_name,
    }
}

fn perform_string_replacements(
    app_files: &Vec<DirEntry>,
    base_path: &Path,
    manifest_data: &ApplicationManifestData,
    dependencies_to_eject: &Vec<String>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    for file in app_files {
        if file.file_name() != "package.json" {
            let file_path = file.path();
            let relative_path_difference = get_relative_path(file_path, base_path);

            let content = rendered_templates_cache
                .get(file_path)?
                .unwrap()
                .content
                .clone();

            let mut new_content = content.clone();

            new_content = new_content.replace(
                "@{{app_name}}/core",
                format!("@{}/core", manifest_data.app_name).as_str(),
            );

            let relative_path_difference_prefix = match relative_path_difference.to_string_lossy() {
                Cow::Borrowed("") => Cow::Borrowed("."),
                Cow::Borrowed(s) => Cow::Borrowed(s),
                Cow::Owned(s) => Cow::Owned(s),
            };

            for dependency in dependencies_to_eject {
                let module_types = [
                    "/enum",
                    "/schemas",
                    "/interfaces",
                    "/types",
                    "/services",
                    "/consumers",
                    "/producers",
                ];
                for module_type in module_types {
                    new_content = new_content.replace(
                        format!("{}{}", dependency, module_type).as_str(),
                        format!(
                            "{}{}",
                            relative_path_difference_prefix,
                            domain_prefix(module_type)
                        )
                        .as_str(),
                    );
                }

                if new_content.contains("@forklaunch/infrastructure-")
                    && dependency.contains("@forklaunch/infrastructure-")
                {
                    let module = dependency.replace("@forklaunch/infrastructure-", "");
                    new_content = new_content.replace(
                        dependency,
                        format!(
                            "{}/infrastructure/{}",
                            relative_path_difference_prefix, module
                        )
                        .as_str(),
                    );
                }
            }

            rendered_templates_cache.insert(
                file_path.to_string_lossy(),
                RenderedTemplate {
                    path: file_path.to_path_buf(),
                    content: new_content,
                    context: Some(format!(
                        "Failed to write file {}",
                        file_path.to_string_lossy()
                    )),
                },
            );
        }
    }

    Ok(())
}

fn detect_collisions_and_prepare_merges(
    base_path: &Path,
    dependencies_to_eject: &Vec<String>,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let node_modules_path = if base_path.join("node_modules").exists() {
        base_path.join("node_modules")
    } else {
        base_path.parent().unwrap().join("node_modules")
    };

    let mut collisions = Vec::new();
    let mut index_ts_files = Vec::new();

    let mut target_path_dependencies: HashMap<PathBuf, Vec<String>> = HashMap::new();

    for ejectable_dependency in dependencies_to_eject {
        let dependency_path = node_modules_path
            .join(&ejectable_dependency)
            .join("lib")
            .join("eject");

        if !dependency_path.exists() {
            continue;
        }

        for entry in WalkDir::new(&dependency_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let source_path = entry.path();
                let relative_path = source_path.strip_prefix(&dependency_path)?;
                let target_path = base_path.join(relative_path);

                target_path_dependencies
                    .entry(target_path)
                    .or_insert_with(Vec::new)
                    .push(ejectable_dependency.clone());
            }
        }
    }

    for (target_path, source_dependencies) in target_path_dependencies {
        let target_exists = target_path.exists();
        let is_index_ts = target_path.file_name().and_then(|n| n.to_str()) == Some("index.ts");
        let multiple_dependencies = source_dependencies.len() > 1;

        if is_index_ts {
            index_ts_files.push(target_path);
        } else if target_exists || multiple_dependencies {
            if multiple_dependencies {
                bail!(
                    "Multiple dependencies would create the same file: {} (from dependencies: {}). Cannot proceed with ejection.",
                    target_path.display(),
                    source_dependencies.join(", ")
                );
            } else {
                collisions.push(target_path);
            }
        }
    }

    if !collisions.is_empty() {
        let collision_list = collisions
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "File collisions detected (files already exist): {}. Cannot proceed with ejection.",
            collision_list
        );
    }

    Ok((collisions, index_ts_files))
}

fn merge_index_ts_files(
    base_path: &Path,
    dependencies_to_eject: &Vec<String>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    let node_modules_path = if base_path.join("node_modules").exists() {
        base_path.join("node_modules")
    } else {
        base_path.parent().unwrap().join("node_modules")
    };

    let mut index_files_to_merge: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for ejectable_dependency in dependencies_to_eject {
        let dependency_path = node_modules_path
            .join(&ejectable_dependency)
            .join("lib")
            .join("eject");

        if !dependency_path.exists() {
            continue;
        }

        for entry in WalkDir::new(&dependency_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let source_path = entry.path();
                if source_path.file_name().and_then(|n| n.to_str()) == Some("index.ts") {
                    let relative_path = source_path.strip_prefix(&dependency_path)?;
                    let target_path = base_path.join(relative_path);

                    index_files_to_merge
                        .entry(target_path.clone())
                        .or_insert_with(Vec::new)
                        .push(source_path.to_path_buf());
                }
            }
        }
    }

    for (target_path, source_files) in index_files_to_merge {
        let mut merged_content = String::new();

        if target_path.exists() {
            let existing_content = rendered_templates_cache
                .get(&target_path)?
                .unwrap()
                .content
                .clone();
            merged_content.push_str(&existing_content);
            merged_content.push('\n');
        }

        for source_file in source_files {
            let content = rendered_templates_cache
                .get(&source_file)?
                .unwrap()
                .content
                .clone();
            merged_content.push_str(&content);
            merged_content.push('\n');
        }

        rendered_templates_cache.insert(
            target_path.clone().to_string_lossy(),
            RenderedTemplate {
                path: target_path.clone(),
                content: merged_content,
                context: Some(format!(
                    "Failed to write merged index.ts file {}",
                    target_path.display()
                )),
            },
        );
    }

    Ok(())
}

fn transform_schema_index_ts(
    base_path: &Path,
    ejectable_dependencies: &Vec<String>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    let schema_index_ts_path = base_path.join("domain").join("schemas").join("index.ts");

    let new_index_ts_text = transform_domain_schemas_index_ts(
        rendered_templates_cache,
        &base_path,
        &ejectable_dependencies,
    )?;

    rendered_templates_cache.insert(
        schema_index_ts_path.to_string_lossy(),
        RenderedTemplate {
            path: schema_index_ts_path.clone(),
            content: new_index_ts_text,
            context: None,
        },
    );

    Ok(())
}

impl CliCommand for EjectCommand {
    fn command(&self) -> Command {
        command("eject", "Eject a forklaunch project")
            .alias("ej")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application root path"),
            )
            .arg(
                Arg::new("continue")
                    .short('c')
                    .long("continue")
                    .help("Continue the eject operation")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("dependencies")
                    .short('d')
                    .long("dependencies")
                    .help("The dependencies to eject")
                    .num_args(0..)
                    .action(ArgAction::Append),
            )
            .arg(
                Arg::new("dryrun")
                    .short('n')
                    .long("dryrun")
                    .help("Dry run the application")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _ = require_manifest(matches)?;

        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let continue_eject_override = matches.get_flag("continue");
        let dryrun = matches.get_flag("dryrun");

        if !continue_eject_override && !dryrun {
            let continue_eject = prompt_for_confirmation(
                &mut line_editor,
                "This operation is irreversible. Do you want to continue? (y/N) ",
            )?;

            if !continue_eject {
                log_error!(stdout, "Ejection cancelled");
                return Ok(());
            }
        }

        let (app_root_path, project_name) = find_app_root_path(matches, RequiredLocation::Project)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let mut manifest_data = toml::from_str::<ApplicationManifestData>(
            &read_to_string(manifest_path).with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let base_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Application(&manifest_data),
            &project_name,
            &mut line_editor,
            &mut stdout,
            matches,
            1,
        )?;

        manifest_data = manifest_data.initialize(InitializableManifestConfigMetadata::Application(
            ApplicationInitializationMetadata {
                app_name: manifest_data.app_name.clone(),
                database: None,
            },
        ));

        let mut project_variant = None;
        if let Some(project) = manifest_data
            .projects
            .iter()
            .find(|project| project_name.clone().unwrap() == project.name)
        {
            project_variant = project.variant.clone();
        }

        let package_path = base_path.join("package.json");

        let package_data =
            read_to_string(&package_path).with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?;

        let mut package_json: Value =
            from_str(&package_data).with_context(|| ERROR_FAILED_TO_PARSE_PACKAGE_JSON)?;

        let mut rendered_templates_cache = RenderedTemplatesCache::new();

        let (dependencies_to_eject, filtered_dependencies) = eject_dependencies(
            &project_variant,
            &mut package_json,
            matches,
            &mut line_editor,
            &base_path,
            &mut stdout,
            dryrun,
        )?;

        let app_files = WalkDir::new(base_path.clone())
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                path.extension()
                    .map_or(false, |ext| ext == "ts" || ext == "tsx")
            })
            .collect::<Vec<_>>();

        package_json["dependencies"] = filtered_dependencies.into();

        if !dryrun {
            perform_string_replacements(
                &app_files,
                &base_path,
                &manifest_data,
                &dependencies_to_eject,
                &mut rendered_templates_cache,
            )?;
            merge_index_ts_files(
                &base_path,
                &dependencies_to_eject,
                &mut rendered_templates_cache,
            )?;
            transform_schema_index_ts(
                &base_path,
                &dependencies_to_eject,
                &mut rendered_templates_cache,
            )?;
        }

        let mut templates_to_render: Vec<RenderedTemplate> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .collect();

        templates_to_render.push(RenderedTemplate {
            path: package_path,
            content: to_string_pretty(&package_json)?,
            context: None,
        });

        write_rendered_templates(&templates_to_render, dryrun, &mut stdout)?;

        if !dryrun {
            log_ok!(stdout, "Ejected successfully!");
        }

        Ok(())
    }
}

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fs::{exists, read_to_string},
    io::Write,
    iter::zip,
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, Command};
use convert_case::{Case, Casing};
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use glob::Pattern;
use ramhorns::Template;
use rustyline::{Editor, history::DefaultHistory};
use termcolor::{ColorChoice, StandardStream, WriteColor};
use walkdir::WalkDir;

use super::core::clean_application::clean_application;
use crate::{
    CliCommand,
    constants::{
        Database, ERROR_FAILED_TO_PARSE_MANIFEST, ERROR_FAILED_TO_READ_MANIFEST,
        ERROR_FAILED_TO_READ_PACKAGE_JSON, Formatter, HttpFramework, License, Linter, Runtime,
        TestFramework, Validator,
    },
    core::{
        ast::transformations::transform_core_registrations_ts::{
            transform_core_registrations_ts_http_framework,
            transform_core_registrations_ts_validator,
        },
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        docker::{DockerCompose, update_dockerfile_contents},
        format::format_code,
        license::generate_license,
        manifest::{
            ApplicationInitializationMetadata, InitializableManifestConfig,
            InitializableManifestConfigMetadata, ManifestData, ProjectType,
            application::ApplicationManifestData,
        },
        name::validate_name,
        package_json::{
            application_package_json::{
                ApplicationDevDependencies, ApplicationPackageJson, ApplicationScripts,
            },
            package_json_constants::{
                BIOME_VERSION, ESLINT_VERSION, EXPRESS_VERSION, HYPER_EXPRESS_VERSION,
                JEST_TYPES_VERSION, JEST_VERSION, OXLINT_VERSION, PRETTIER_VERSION,
                TS_JEST_VERSION, TYPEBOX_VERSION, TYPESCRIPT_ESLINT_VERSION, VITEST_VERSION,
                ZOD_VERSION, application_build_script, application_clean_purge_script,
                application_clean_script, application_docs_script, application_format_script,
                application_lint_fix_script, application_lint_script, application_migrate_script,
                application_seed_script, application_setup_script, application_test_script,
                application_up_packages_script, project_clean_script, project_dev_local_script,
                project_dev_server_script, project_dev_worker_client_script, project_format_script,
                project_lint_fix_script, project_lint_script, project_start_server_script,
                project_start_worker_script, project_test_script,
            },
            project_package_json::{
                ProjectDependencies, ProjectDevDependencies, ProjectPackageJson, ProjectScripts,
            },
        },
        pnpm_workspace::PnpmWorkspace,
        removal_template::{RemovalTemplate, remove_template_files},
        rendered_template::{
            RenderedTemplate, RenderedTemplatesCache, TEMPLATES_DIR, write_rendered_templates,
        },
        symlink_template::{SymlinkTemplate, create_symlinks},
        watermark::apply_watermark,
    },
    prompt::{ArrayCompleter, prompt_field_from_selections_with_validation},
};

#[derive(Debug)]
pub(super) struct ApplicationCommand;

impl ApplicationCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

fn should_ignore(path: &Path, patterns: &[Vec<String>]) -> bool {
    patterns.iter().any(|pattern_set| {
        pattern_set.iter().any(|pattern| {
            Pattern::new(pattern)
                .unwrap()
                .matches(&path.to_string_lossy())
        })
    })
}

fn replace_name_in_content(content: &str, existing_name: &str, new_name: &str) -> String {
    let mut new_content = content
        .replace(
            format!("@{}", existing_name).as_str(),
            format!("@{}", new_name).as_str(),
        )
        .replace(
            format!("{}-", existing_name).as_str(),
            format!("{}-", new_name).as_str(),
        )
        .replace(
            format!("/{}", existing_name).as_str(),
            format!("/{}", new_name).as_str(),
        );

    for case in [Case::Kebab, Case::Camel, Case::Pascal] {
        new_content = new_content
            .replace(
                format!("@{}", existing_name.to_case(case)).as_str(),
                format!("@{}", new_name.to_case(case)).as_str(),
            )
            .replace(
                format!("{}-", existing_name.to_case(case)).as_str(),
                format!("{}-", new_name.to_case(case)).as_str(),
            )
            .replace(
                format!("/{}", existing_name.to_case(case)).as_str(),
                format!("/{}", new_name.to_case(case)).as_str(),
            );
    }

    new_content
}

fn change_name(
    manifest_data: &mut ApplicationManifestData,
    base_path: &Path,
    docker_compose_path: &Option<PathBuf>,
    name: &str,
    application_json_to_write: &mut ApplicationPackageJson,
    project_jsons_to_write: &mut HashMap<String, ProjectPackageJson>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    let existing_name = manifest_data.app_name.clone();
    let existing_kebab_name = manifest_data.kebab_case_app_name.clone();

    manifest_data.app_name = name.to_string();
    manifest_data.kebab_case_app_name = name.to_case(Case::Kebab);
    manifest_data.camel_case_app_name = name.to_case(Case::Camel);
    manifest_data.pascal_case_app_name = name.to_case(Case::Pascal);

    application_json_to_write.name = Some(
        application_json_to_write
            .name
            .as_ref()
            .unwrap()
            .replace(&existing_kebab_name, &manifest_data.kebab_case_app_name),
    );
    application_json_to_write.description = Some(
        application_json_to_write
            .description
            .as_ref()
            .unwrap()
            .replace(&existing_kebab_name, &manifest_data.kebab_case_app_name),
    );
    for project in project_jsons_to_write.values_mut() {
        project.name = Some(
            project
                .name
                .as_ref()
                .unwrap()
                .replace(&existing_kebab_name, &manifest_data.kebab_case_app_name),
        );
        project.description = Some(
            project
                .description
                .as_ref()
                .unwrap()
                .replace(&existing_name, &manifest_data.app_name),
        );
        project.dependencies = Some(ProjectDependencies {
            additional_deps: project
                .dependencies
                .as_ref()
                .unwrap()
                .additional_deps
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone()
                            .replace(&existing_kebab_name, &manifest_data.kebab_case_app_name)
                            .replace(&existing_name, &manifest_data.app_name),
                        value.clone(),
                    )
                })
                .collect(),
            ..project.dependencies.as_ref().unwrap().clone()
        });
        project.dev_dependencies = Some(ProjectDevDependencies {
            additional_deps: project
                .dev_dependencies
                .as_ref()
                .unwrap()
                .additional_deps
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone()
                            .replace(&existing_kebab_name, &manifest_data.kebab_case_app_name)
                            .replace(&existing_name, &manifest_data.app_name),
                        value.clone(),
                    )
                })
                .collect(),
            ..project.dev_dependencies.as_ref().unwrap().clone()
        });
    }

    let mut ignore_pattern_stack: Vec<Vec<String>> = Vec::new();
    ignore_pattern_stack.push(vec!["**/node_modules/**/*".to_string()]);

    for entry in WalkDir::new(base_path) {
        let entry = entry?;
        let path = entry.path();
        let relative_path = path.strip_prefix(base_path)?;

        if entry.file_type().is_dir() {
            let flignore_path = path.join(".flignore");
            if flignore_path.exists() {
                let new_patterns = read_to_string(flignore_path)?
                    .lines()
                    .filter(|line| !line.starts_with('#') && !line.is_empty())
                    .map(|line| line.to_string())
                    .collect::<Vec<String>>();
                ignore_pattern_stack.push(new_patterns);
            }

            if path != base_path && entry.depth() < ignore_pattern_stack.len() {
                let levels_to_pop = ignore_pattern_stack.len() - entry.depth();
                for _ in 0..levels_to_pop {
                    ignore_pattern_stack.pop();
                }
            }
        } else if entry.file_type().is_file()
            && entry.file_name().to_str().unwrap() != "package.json"
        {
            if should_ignore(relative_path, &ignore_pattern_stack) {
                continue;
            }

            if let Some(rendered_template) = rendered_templates_cache.get(path)? {
                let contents = rendered_template.content.clone();
                let new_contents = replace_name_in_content(&contents, &existing_name, name);

                if contents != new_contents {
                    rendered_templates_cache.insert(
                        path.to_string_lossy(),
                        RenderedTemplate {
                            path: path.to_path_buf(),
                            content: new_contents,
                            context: None,
                        },
                    );
                }
            }
        }
    }

    if let Some(docker_compose_path) = docker_compose_path {
        let docker_compose_contents = read_to_string(docker_compose_path)?;
        let new_docker_compose_contents =
            replace_name_in_content(&docker_compose_contents, &existing_name, name);

        if docker_compose_contents != new_docker_compose_contents {
            rendered_templates_cache.insert(
                docker_compose_path.to_string_lossy(),
                RenderedTemplate {
                    path: docker_compose_path.to_path_buf(),
                    content: new_docker_compose_contents,
                    context: None,
                },
            );
        }
    }

    Ok(())
}

fn change_description(
    description: &str,
    manifest_data: &mut ApplicationManifestData,
    application_json_to_write: &mut ApplicationPackageJson,
) {
    manifest_data.app_description = description.to_string();
    application_json_to_write.description = Some(description.to_string());
}

fn attempt_replacement(
    additional_scripts: &mut HashMap<String, String>,
    existing_script: Option<String>,
    existing_script_choice: Option<String>,
    existing_script_generation: Option<String>,
    script_key: &str,
    script_choice: &str,
    script_replacement: &str,
    choices_to_preserve: Option<&mut HashSet<String>>,
) -> String {
    if let Some(existing_script) = existing_script {
        if let Some(existing_script_generation) = existing_script_generation {
            if let Some(existing_script_choice) = existing_script_choice {
                if existing_script != existing_script_generation {
                    additional_scripts.insert(
                        format!("{}:{}", script_key, existing_script_choice),
                        existing_script.to_owned(),
                    );
                    if let Some(choices_to_preserve) = choices_to_preserve {
                        choices_to_preserve.insert(existing_script_choice.clone());
                    }
                }
            }
        }
    }

    let modified_format_script_key = format!("{}:{}", script_key, script_choice);
    if additional_scripts.contains_key(&modified_format_script_key) {
        let stashed_script = additional_scripts
            .get(&modified_format_script_key)
            .unwrap()
            .to_owned();
        additional_scripts.remove(&modified_format_script_key);
        stashed_script
    } else {
        script_replacement.to_owned()
    }
}

fn update_config_files(
    base_path: &Path,
    exclusive_files: Option<&[&str]>,
    existing_files: Vec<&str>,
    project_jsons_to_write: &mut HashMap<String, ProjectPackageJson>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<(Vec<RemovalTemplate>, Vec<SymlinkTemplate>, Vec<String>)> {
    let mut removal_templates = vec![];
    let mut symlink_templates = vec![];
    let mut preserved_files = vec![];

    if let Some(exclusive_files) = exclusive_files {
        for file in exclusive_files {
            let linter_config_file_contents = TEMPLATES_DIR
                .get_file(Path::new("application").join(file.to_string()))
                .unwrap()
                .contents_utf8()
                .unwrap();

            rendered_templates_cache.insert(
                base_path.join(file).to_string_lossy(),
                RenderedTemplate {
                    path: base_path.join(file),
                    content: linter_config_file_contents.to_string(),
                    context: None,
                },
            );

            for project in project_jsons_to_write.keys() {
                symlink_templates.push(SymlinkTemplate {
                    path: base_path.join(file),
                    target: base_path.join(project).join(file),
                });
            }
        }
    }

    for file in existing_files {
        let file_path = base_path.join(file);
        if exists(&file_path)? {
            let file_content = read_to_string(&file_path)?;
            let watermarked_file_content = apply_watermark(&RenderedTemplate {
                path: file_path.clone(),
                content: TEMPLATES_DIR
                    .get_file(Path::new("application").join(file.to_string()))
                    .unwrap()
                    .contents_utf8()
                    .unwrap()
                    .to_string(),
                context: None,
            })?;

            if file_content.trim() == watermarked_file_content.trim() {
                for project in project_jsons_to_write.keys() {
                    removal_templates.push(RemovalTemplate {
                        path: base_path.join(project).join(file),
                    });
                }
                removal_templates.push(RemovalTemplate {
                    path: file_path,
                });
            } else {
                preserved_files.push(file_path.to_string_lossy().to_string());
            }
        }
    }

    Ok((removal_templates, symlink_templates, preserved_files))
}

fn update_project_package_json<
    ProjectScriptsUpdateFunction,
    ProjectDependenciesUpdateFunction,
    ProjectDevDependenciesUpdateFunction,
>(
    project_json_to_write: &mut ProjectPackageJson,
    project_package_json_script_setters: &mut ProjectScriptsUpdateFunction,
    project_package_json_dependencies_setters: &mut ProjectDependenciesUpdateFunction,
    project_package_json_dev_dependencies_setters: &mut ProjectDevDependenciesUpdateFunction,
) -> Result<()>
where
    ProjectScriptsUpdateFunction: FnMut(&mut ProjectScripts),
    ProjectDependenciesUpdateFunction: FnMut(&mut ProjectDependencies),
    ProjectDevDependenciesUpdateFunction: FnMut(&mut ProjectDevDependencies),
{
    if let Some(scripts) = project_json_to_write.scripts.as_mut() {
        project_package_json_script_setters(scripts);
    }

    if let Some(dependencies) = project_json_to_write.dependencies.as_mut() {
        project_package_json_dependencies_setters(dependencies);
    }

    if let Some(dev_dependencies) = project_json_to_write.dev_dependencies.as_mut() {
        project_package_json_dev_dependencies_setters(dev_dependencies);
    }

    Ok(())
}

fn update_application_and_project_package_jsons<
    ApplicationScriptsUpdateFunction,
    ProjectScriptsUpdateFunction,
    ProjectDependenciesUpdateFunction,
    ApplicationDevDependenciesUpdateFunction,
    ProjectDevDependenciesUpdateFunction,
>(
    application_json_to_write: &mut ApplicationPackageJson,
    project_jsons_to_write: Vec<&mut ProjectPackageJson>,
    application_package_json_script_setters: &mut ApplicationScriptsUpdateFunction,
    project_package_json_script_setters: &mut ProjectScriptsUpdateFunction,
    project_package_json_dependencies_setters: &mut ProjectDependenciesUpdateFunction,
    application_package_json_dev_dependencies_setters: &mut ApplicationDevDependenciesUpdateFunction,
    project_package_json_dev_dependencies_setters: &mut ProjectDevDependenciesUpdateFunction,
) -> Result<()>
where
    ApplicationScriptsUpdateFunction: FnMut(&mut ApplicationScripts),
    ProjectScriptsUpdateFunction: FnMut(&mut ProjectScripts),
    ProjectDependenciesUpdateFunction: FnMut(&mut ProjectDependencies),
    ApplicationDevDependenciesUpdateFunction: FnMut(&mut ApplicationDevDependencies),
    ProjectDevDependenciesUpdateFunction: FnMut(&mut ProjectDevDependencies),
{
    if let Some(scripts) = application_json_to_write.scripts.as_mut() {
        application_package_json_script_setters(scripts);
    }

    if let Some(dev_dependencies) = application_json_to_write.dev_dependencies.as_mut() {
        application_package_json_dev_dependencies_setters(dev_dependencies);
    }

    for project_json in project_jsons_to_write {
        update_project_package_json(
            project_json,
            project_package_json_script_setters,
            project_package_json_dependencies_setters,
            project_package_json_dev_dependencies_setters,
        )?;
    }

    Ok(())
}

fn change_formatter(
    base_path: &Path,
    formatter: &Formatter,
    manifest_data: &mut ApplicationManifestData,
    application_json_to_write: &mut ApplicationPackageJson,
    project_jsons_to_write: &mut HashMap<String, ProjectPackageJson>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<(Vec<RemovalTemplate>, Vec<SymlinkTemplate>)> {
    let existing_formatter = manifest_data.formatter.parse::<Formatter>()?;

    manifest_data.formatter = formatter.to_string();

    let exclusive_files = formatter.metadata().exclusive_files;
    let existing_files = formatter.all_other_files();

    let (removal_templates, symlink_templates, preserved_files) = update_config_files(
        base_path,
        exclusive_files,
        existing_files,
        project_jsons_to_write,
        rendered_templates_cache,
    )?;

    let mut formatters_to_preserve = HashSet::new();
    formatters_to_preserve.insert(formatter.to_string());
    for file in preserved_files {
        if let Some(formatter_file) = Formatter::get_variant_from_exclusive_file(&file) {
            formatters_to_preserve.insert(formatter_file.to_string());
        }
    }

    let rc_formatters_to_preserve = Rc::new(RefCell::new(formatters_to_preserve));

    update_application_and_project_package_jsons(
        application_json_to_write,
        project_jsons_to_write.values_mut().collect(),
        &mut |scripts| {
            scripts.format = Some(attempt_replacement(
                &mut scripts.additional_scripts,
                scripts.format.clone(),
                Some(existing_formatter.to_string()),
                Some(application_format_script(&existing_formatter)),
                "format",
                &formatter.to_string(),
                &application_format_script(formatter),
                Some(&mut rc_formatters_to_preserve.borrow_mut()),
            ));
        },
        &mut |scripts| {
            scripts.format = Some(attempt_replacement(
                &mut scripts.additional_scripts,
                scripts.format.clone(),
                Some(existing_formatter.to_string()),
                Some(project_format_script(&existing_formatter)),
                "format",
                &formatter.to_string(),
                &project_format_script(formatter),
                Some(&mut rc_formatters_to_preserve.borrow_mut()),
            ));
        },
        &mut |_| {},
        &mut |dev_dependencies| {
            dev_dependencies.prettier = None;
            dev_dependencies.biome = None;

            for formatter in rc_formatters_to_preserve.borrow().iter() {
                match formatter.parse().unwrap() {
                    Formatter::Prettier => {
                        dev_dependencies.prettier = Some(PRETTIER_VERSION.to_string());
                    }
                    Formatter::Biome => {
                        dev_dependencies.biome = Some(BIOME_VERSION.to_string());
                    }
                }
            }
        },
        &mut |dev_dependencies| {
            dev_dependencies.prettier = None;
            dev_dependencies.biome = None;

            for formatter in rc_formatters_to_preserve.borrow().iter() {
                match formatter.parse().unwrap() {
                    Formatter::Prettier => {
                        dev_dependencies.prettier = Some(PRETTIER_VERSION.to_string());
                    }
                    Formatter::Biome => {
                        dev_dependencies.biome = Some(BIOME_VERSION.to_string());
                    }
                }
            }
        },
    )?;

    Ok((removal_templates, symlink_templates))
}

fn change_linter(
    base_path: &Path,
    linter: &Linter,
    manifest_data: &mut ApplicationManifestData,
    application_json_to_write: &mut ApplicationPackageJson,
    project_jsons_to_write: &mut HashMap<String, ProjectPackageJson>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<(Vec<RemovalTemplate>, Vec<SymlinkTemplate>)> {
    let existing_linter = manifest_data.linter.parse::<Linter>()?;

    manifest_data.linter = linter.to_string();

    let exclusive_files = linter.metadata().exclusive_files;
    let existing_files = linter.all_other_files();

    let (removal_templates, symlink_templates, preserved_files) = update_config_files(
        base_path,
        exclusive_files,
        existing_files,
        project_jsons_to_write,
        rendered_templates_cache,
    )?;

    let mut linters_to_preserve = HashSet::new();
    linters_to_preserve.insert(linter.to_string());
    for file in preserved_files {
        if let Some(linter_file) = Linter::get_variant_from_exclusive_file(&file) {
            linters_to_preserve.insert(linter_file.to_string());
        }
    }

    let rc_linters_to_preserve = Rc::new(RefCell::new(linters_to_preserve));

    update_application_and_project_package_jsons(
        application_json_to_write,
        project_jsons_to_write.values_mut().collect(),
        &mut |scripts| {
            scripts.lint = Some(attempt_replacement(
                &mut scripts.additional_scripts,
                scripts.lint.clone(),
                Some(existing_linter.to_string()),
                Some(application_lint_script(&existing_linter)),
                "lint",
                &linter.to_string(),
                &application_lint_script(linter),
                Some(&mut rc_linters_to_preserve.borrow_mut()),
            ));

            scripts.lint_fix = Some(attempt_replacement(
                &mut scripts.additional_scripts,
                scripts.lint_fix.clone(),
                Some(existing_linter.to_string()),
                Some(application_lint_fix_script(&existing_linter)),
                "lint-fix",
                &linter.to_string(),
                &application_lint_fix_script(linter),
                Some(&mut rc_linters_to_preserve.borrow_mut()),
            ));
        },
        &mut |scripts| {
            scripts.lint = Some(attempt_replacement(
                &mut scripts.additional_scripts,
                scripts.lint.clone(),
                Some(existing_linter.to_string()),
                Some(project_lint_script(&existing_linter)),
                "lint",
                &linter.to_string(),
                &project_lint_script(linter),
                Some(&mut rc_linters_to_preserve.borrow_mut()),
            ));

            scripts.lint_fix = Some(attempt_replacement(
                &mut scripts.additional_scripts,
                scripts.lint_fix.clone(),
                Some(existing_linter.to_string()),
                Some(project_lint_fix_script(&existing_linter)),
                "lint-fix",
                &linter.to_string(),
                &project_lint_fix_script(linter),
                Some(&mut rc_linters_to_preserve.borrow_mut()),
            ));
        },
        &mut |_| {},
        &mut |dev_dependencies| {
            dev_dependencies.eslint = None;
            dev_dependencies.eslint_js = None;
            dev_dependencies.typescript_eslint = None;
            dev_dependencies.oxlint = None;

            for linter in rc_linters_to_preserve.borrow().iter() {
                match linter.parse().unwrap() {
                    Linter::Eslint => {
                        dev_dependencies.eslint = Some(ESLINT_VERSION.to_string());
                        dev_dependencies.eslint_js = Some(ESLINT_VERSION.to_string());
                        dev_dependencies.typescript_eslint =
                            Some(TYPESCRIPT_ESLINT_VERSION.to_string());
                    }
                    Linter::Oxlint => {
                        dev_dependencies.oxlint = Some(OXLINT_VERSION.to_string());
                    }
                }
            }
        },
        &mut |dev_dependencies| {
            dev_dependencies.eslint = None;
            dev_dependencies.eslint_js = None;
            dev_dependencies.typescript_eslint = None;
            dev_dependencies.oxlint = None;

            for linter in rc_linters_to_preserve.borrow().iter() {
                match linter.parse().unwrap() {
                    Linter::Eslint => {
                        dev_dependencies.eslint = Some(ESLINT_VERSION.to_string());
                        dev_dependencies.eslint_js = Some(ESLINT_VERSION.to_string());
                        dev_dependencies.typescript_eslint =
                            Some(TYPESCRIPT_ESLINT_VERSION.to_string());
                    }
                    Linter::Oxlint => {
                        dev_dependencies.oxlint = Some(OXLINT_VERSION.to_string());
                    }
                }
            }
        },
    )?;

    Ok((removal_templates, symlink_templates))
}

fn change_validator(
    base_path: &Path,
    validator: &Validator,
    manifest_data: &mut ApplicationManifestData,
    project_jsons_to_write: &mut HashMap<String, ProjectPackageJson>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    let existing_validator = manifest_data.validator.clone().parse::<Validator>()?;
    manifest_data.validator = validator.to_string();

    let validator_file_key = base_path.join("core").join("registrations.ts");

    for project in project_jsons_to_write.values_mut() {
        let dependencies = project.dependencies.as_mut().unwrap();
        dependencies.zod = None;
        dependencies.typebox = None;

        match validator {
            Validator::Zod => {
                dependencies.zod = Some(ZOD_VERSION.to_string());
            }
            Validator::Typebox => {
                dependencies.typebox = Some(TYPEBOX_VERSION.to_string());
            }
        }
    }

    rendered_templates_cache.insert(
        validator_file_key.to_string_lossy(),
        RenderedTemplate {
            path: base_path.join("core").join("registrations.ts"),
            content: transform_core_registrations_ts_validator(
                rendered_templates_cache,
                &validator.to_string(),
                &existing_validator.to_string(),
                base_path,
            )?,
            context: None,
        },
    );

    Ok(())
}

fn change_http_framework(
    base_path: &Path,
    http_framework: &HttpFramework,
    manifest_data: &mut ApplicationManifestData,
    project_jsons_to_write: &mut HashMap<String, ProjectPackageJson>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    let existing_http_framework = manifest_data
        .http_framework
        .clone()
        .parse::<HttpFramework>()?;
    manifest_data.http_framework = http_framework.to_string();

    let http_framework_file_key = base_path.join("core").join("registrations.ts");

    for project in project_jsons_to_write.values_mut() {
        let dependencies = project.dependencies.as_mut().unwrap();
        dependencies.forklaunch_express = None;
        dependencies.forklaunch_hyper_express = None;

        match http_framework {
            HttpFramework::Express => {
                dependencies.forklaunch_express = Some(EXPRESS_VERSION.to_string());
            }
            HttpFramework::HyperExpress => {
                dependencies.forklaunch_hyper_express = Some(HYPER_EXPRESS_VERSION.to_string());
            }
        }
    }

    rendered_templates_cache.insert(
        http_framework_file_key.to_string_lossy(),
        RenderedTemplate {
            path: base_path.join("core").join("registrations.ts"),
            content: transform_core_registrations_ts_http_framework(
                rendered_templates_cache,
                &http_framework.to_string(),
                &existing_http_framework.to_string(),
                base_path,
            )?,
            context: None,
        },
    );

    Ok(())
}

fn change_runtime(
    line_editor: &mut Editor<ArrayCompleter, DefaultHistory>,
    stdout: &mut StandardStream,
    matches: &clap::ArgMatches,
    base_path: &Path,
    runtime: &Runtime,
    manifest_data: &mut ApplicationManifestData,
    application_json_to_write: &mut ApplicationPackageJson,
    project_jsons_to_write: &mut HashMap<String, ProjectPackageJson>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<(Vec<RemovalTemplate>, Vec<SymlinkTemplate>)> {
    let existing_runtime = manifest_data.runtime.parse::<Runtime>()?;
    let mut existing_databases = HashSet::new();

    for project in manifest_data.projects.iter() {
        if let Some(resources) = &project.resources {
            if let Some(database) = &resources.database {
                existing_databases.insert(database.parse()?);
            }
        }
    }

    let mut removal_templates = vec![];
    let mut symlink_templates = vec![];

    let existing_workspaces: Vec<String> = match manifest_data.runtime.parse()? {
        Runtime::Bun => application_json_to_write
            .workspaces
            .as_ref()
            .unwrap()
            .clone(),
        Runtime::Node => {
            removal_templates.push(RemovalTemplate {
                path: base_path.join("pnpm-workspace.yaml"),
            });
            serde_yml::from_str::<PnpmWorkspace>(&read_to_string(
                &base_path.join("pnpm-workspace.yaml"),
            )?)?
            .packages
        }
    };

    manifest_data.runtime = runtime.to_string();

    let test_framework = match runtime {
        Runtime::Bun => {
            manifest_data.test_framework = None;
            None
        }
        Runtime::Node => {
            if manifest_data.test_framework.is_none() {
                let test_framework = prompt_field_from_selections_with_validation(
                    "test-framework",
                    None,
                    &["test-framework"],
                    line_editor,
                    stdout,
                    matches,
                    "test framework",
                    Some(&TestFramework::VARIANTS),
                    |input: &str| TestFramework::VARIANTS.contains(&input),
                    |_| "Invalid test framework. Please try again".to_string(),
                )?
                .unwrap()
                .parse::<TestFramework>()?;

                Some(test_framework)
            } else {
                Some(
                    manifest_data
                        .test_framework
                        .as_ref()
                        .unwrap()
                        .parse::<TestFramework>()?,
                )
            }
        }
    };

    if let Some(test_framework) = test_framework {
        let (test_framework_removal_templates, test_framework_symlink_templates) =
            change_test_framework(
                base_path,
                &test_framework,
                manifest_data,
                application_json_to_write,
                project_jsons_to_write,
                rendered_templates_cache,
            )?;

        removal_templates.extend(test_framework_removal_templates);
        symlink_templates.extend(test_framework_symlink_templates);
    }

    let application_package_json_scripts = application_json_to_write.scripts.as_mut().unwrap();

    application_package_json_scripts.build = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.build.clone(),
        Some(existing_runtime.to_string()),
        Some(application_build_script(&existing_runtime)),
        "build",
        &runtime.to_string(),
        &application_build_script(runtime),
        None,
    ));
    application_package_json_scripts.clean = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.clean.clone(),
        Some(existing_runtime.to_string()),
        Some(application_clean_script(&existing_runtime)),
        "clean",
        &runtime.to_string(),
        &application_clean_script(runtime),
        None,
    ));
    application_package_json_scripts.clean_purge = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.clean_purge.clone(),
        Some(existing_runtime.to_string()),
        Some(application_clean_purge_script(&existing_runtime)),
        "clean-purge",
        &runtime.to_string(),
        &application_clean_purge_script(runtime),
        None,
    ));
    application_package_json_scripts.database_setup = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.database_setup.clone(),
        Some(existing_runtime.to_string()),
        Some(application_setup_script(&existing_runtime)),
        "database-setup",
        &runtime.to_string(),
        &application_setup_script(runtime),
        None,
    ));
    application_package_json_scripts.docs = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.docs.clone(),
        Some(existing_runtime.to_string()),
        Some(application_docs_script(&existing_runtime)),
        "docs",
        &runtime.to_string(),
        &application_docs_script(runtime),
        None,
    ));
    application_package_json_scripts.migrate_create = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.migrate_create.clone(),
        Some(existing_runtime.to_string()),
        Some(application_migrate_script(
            &existing_runtime,
            &existing_databases,
            "create",
        )),
        "migrate-create",
        &runtime.to_string(),
        &application_migrate_script(runtime, &existing_databases, "create"),
        None,
    ));
    application_package_json_scripts.migrate_down = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.migrate_down.clone(),
        Some(existing_runtime.to_string()),
        Some(application_migrate_script(
            &existing_runtime,
            &existing_databases,
            "down",
        )),
        "migrate-down",
        &runtime.to_string(),
        &application_migrate_script(runtime, &existing_databases, "down"),
        None,
    ));
    application_package_json_scripts.migrate_init = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.migrate_init.clone(),
        Some(existing_runtime.to_string()),
        Some(application_migrate_script(
            &existing_runtime,
            &existing_databases,
            "init",
        )),
        "migrate-init",
        &runtime.to_string(),
        &application_migrate_script(runtime, &existing_databases, "init"),
        None,
    ));
    application_package_json_scripts.migrate_up = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.migrate_up.clone(),
        Some(existing_runtime.to_string()),
        Some(application_migrate_script(
            &existing_runtime,
            &existing_databases,
            "up",
        )),
        "migrate-up",
        &runtime.to_string(),
        &application_migrate_script(runtime, &existing_databases, "up"),
        None,
    ));
    application_package_json_scripts.seed = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.seed.clone(),
        Some(existing_runtime.to_string()),
        Some(application_seed_script(
            &existing_runtime,
            &existing_databases,
        )),
        "seed",
        &runtime.to_string(),
        &application_seed_script(runtime, &existing_databases),
        None,
    ));
    application_package_json_scripts.up_packages = Some(attempt_replacement(
        &mut application_package_json_scripts.additional_scripts,
        application_package_json_scripts.up_packages.clone(),
        Some(existing_runtime.to_string()),
        Some(application_up_packages_script(&existing_runtime)),
        "up-packages",
        &runtime.to_string(),
        &application_up_packages_script(runtime),
        None,
    ));

    let mut is_in_memory_database = manifest_data.is_in_memory_database;
    for (project_name, project) in project_jsons_to_write {
        let project_entry = manifest_data
            .projects
            .iter()
            .find(|project_entry| &project_entry.name == project_name)
            .unwrap();
        let project_type = &project_entry.r#type;
        let database = if let Some(resources) = &project_entry.resources {
            if let Some(database) = &resources.database {
                is_in_memory_database = if !is_in_memory_database {
                    match database.parse::<Database>()? {
                        Database::SQLite => true,
                        Database::LibSQL => true,
                        Database::BetterSQLite => true,
                        _ => false,
                    }
                } else {
                    true
                };
            }
            resources
                .database
                .as_ref()
                .map(|db| db.parse::<Database>().unwrap())
        } else {
            None
        };

        if let Some(project_scripts) = &mut project.scripts {
            project_scripts.clean = Some(attempt_replacement(
                &mut project_scripts.additional_scripts,
                project_scripts.clean.clone(),
                Some(existing_runtime.to_string()),
                Some(project_clean_script(&existing_runtime)),
                "clean",
                &runtime.to_string(),
                &project_clean_script(runtime),
                None,
            ));
            match project_type {
                ProjectType::Service => {
                    project_scripts.dev = Some(attempt_replacement(
                        &mut project_scripts.additional_scripts,
                        project_scripts.dev.clone(),
                        Some(existing_runtime.to_string()),
                        Some(project_dev_server_script(&existing_runtime, database)),
                        "dev",
                        &runtime.to_string(),
                        &project_dev_server_script(runtime, database),
                        None,
                    ));
                    project_scripts.dev_local = Some(attempt_replacement(
                        &mut project_scripts.additional_scripts,
                        project_scripts.dev_local.clone(),
                        Some(existing_runtime.to_string()),
                        Some(project_dev_local_script(&existing_runtime, database)),
                        "dev-local",
                        &runtime.to_string(),
                        &project_dev_local_script(runtime, database),
                        None,
                    ));
                    project_scripts.start = Some(attempt_replacement(
                        &mut project_scripts.additional_scripts,
                        project_scripts.start.clone(),
                        Some(existing_runtime.to_string()),
                        Some(project_start_server_script(&existing_runtime, database)),
                        "start",
                        &runtime.to_string(),
                        &project_start_server_script(runtime, database),
                        None,
                    ));
                }
                ProjectType::Worker => {
                    project_scripts.dev_server = Some(attempt_replacement(
                        &mut project_scripts.additional_scripts,
                        project_scripts.dev_server.clone(),
                        Some(existing_runtime.to_string()),
                        Some(project_dev_server_script(&existing_runtime, database)),
                        "dev-server",
                        &runtime.to_string(),
                        &project_dev_server_script(runtime, database),
                        None,
                    ));

                    project_scripts.dev_worker = Some(attempt_replacement(
                        &mut project_scripts.additional_scripts,
                        project_scripts.dev_worker.clone(),
                        Some(existing_runtime.to_string()),
                        Some(project_dev_worker_client_script(&existing_runtime)),
                        "dev-client",
                        &runtime.to_string(),
                        &project_dev_worker_client_script(runtime),
                        None,
                    ));

                    project_scripts.start_server = Some(attempt_replacement(
                        &mut project_scripts.additional_scripts,
                        project_scripts.start_server.clone(),
                        Some(existing_runtime.to_string()),
                        Some(project_start_server_script(&existing_runtime, database)),
                        "start-server",
                        &runtime.to_string(),
                        &project_start_server_script(runtime, database),
                        None,
                    ));

                    project_scripts.start_worker = Some(attempt_replacement(
                        &mut project_scripts.additional_scripts,
                        project_scripts.start_worker.clone(),
                        Some(existing_runtime.to_string()),
                        Some(project_start_worker_script(&existing_runtime, database)),
                        "start-worker",
                        &runtime.to_string(),
                        &project_start_worker_script(runtime, database),
                        None,
                    ));
                }
                ProjectType::Library => {}
            }
        }
    }

    match runtime {
        Runtime::Bun => {
            application_json_to_write.workspaces = Some(existing_workspaces);
        }
        Runtime::Node => {
            rendered_templates_cache.insert(
                "pnpm-workspace.yaml".to_string(),
                RenderedTemplate {
                    path: base_path.join("pnpm-workspace.yaml"),
                    content: serde_yml::to_string(&PnpmWorkspace {
                        packages: existing_workspaces,
                    })?,
                    context: None,
                },
            );
        }
    }

    let dockerfile_template_path = TEMPLATES_DIR
        .get_file(Path::new("application").join("Dockerfile"))
        .unwrap();

    let dockerfile_template = Template::new(dockerfile_template_path.contents_utf8().unwrap())?;

    let existing_service_manifest_data = ApplicationManifestData {
        is_in_memory_database,
        is_node: existing_runtime == Runtime::Node,
        is_bun: existing_runtime == Runtime::Bun,
        ..manifest_data.clone()
    };

    let existing_dockerfile_contents = read_to_string(base_path.join("Dockerfile"))?;
    let watermarked_dockerfile_contents = apply_watermark(&RenderedTemplate {
        path: base_path.join("Dockerfile"),
        content: dockerfile_template.render(&existing_service_manifest_data),
        context: None,
    })?;

    if existing_dockerfile_contents.trim() != watermarked_dockerfile_contents.trim() {
        log_warn!(stdout, "Warning: Dockerfile is generating from template, you may need to manually migrate changes from Dockerfile.{}", runtime.to_string());
        rendered_templates_cache.insert(
            base_path
                .join(format!("Dockerfile.{}", runtime.to_string()))
                .to_string_lossy(),
            RenderedTemplate {
                path: base_path.join(format!("Dockerfile.{}", runtime.to_string())),
                content: existing_dockerfile_contents,
                context: None,
            },
        );
    }

    let dockerfile_cache_key = base_path.join("Dockerfile");
    rendered_templates_cache.insert(
        dockerfile_cache_key.to_string_lossy(),
        RenderedTemplate {
            path: base_path.join("Dockerfile"),
            content: if exists(base_path.join(format!("Dockerfile.{}", runtime.to_string())))? {
                read_to_string(base_path.join(format!("Dockerfile.{}", runtime.to_string())))?
            } else {
                update_dockerfile_contents(
                    &apply_watermark(&RenderedTemplate {
                        path: base_path.join("Dockerfile"),
                        content: dockerfile_template.render(&ApplicationManifestData {
                            is_in_memory_database,
                            is_node: runtime == &Runtime::Node,
                            is_bun: runtime == &Runtime::Bun,
                            ..manifest_data.clone()
                        }),
                        context: None,
                    })?,
                    &runtime,
                    is_in_memory_database,
                )?
            },
            context: None,
        },
    );

    if let Some(docker_compose_path) = &manifest_data.docker_compose_path {
        let docker_compose_file_path = base_path.join(&docker_compose_path);
        if exists(&docker_compose_file_path)? {
            let existing_docker_compose_content = read_to_string(&docker_compose_file_path)?;
            let mut docker_compose: DockerCompose =
                serde_yml::from_str(&existing_docker_compose_content)?;

            let mut updated = false;
            for (_, service) in docker_compose.services.iter_mut() {
                if let Some(entrypoint) = &mut service.entrypoint {
                    match existing_runtime {
                        Runtime::Node => {
                            if entrypoint.iter().any(|arg| arg.contains("pnpm")) {
                                for arg in entrypoint.iter_mut() {
                                    if arg.contains("pnpm") {
                                        *arg = arg.replace(
                                            "pnpm",
                                            match runtime {
                                                Runtime::Bun => "bun",
                                                Runtime::Node => "pnpm",
                                            },
                                        );
                                        updated = true;
                                    }
                                }
                            }
                        }
                        Runtime::Bun => {
                            if entrypoint.iter().any(|arg| arg.contains("bun")) {
                                for arg in entrypoint.iter_mut() {
                                    if arg.contains("bun") {
                                        *arg = arg.replace(
                                            "bun",
                                            match runtime {
                                                Runtime::Bun => "bun",
                                                Runtime::Node => "pnpm",
                                            },
                                        );
                                        updated = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if updated {
                let updated_docker_compose_content = serde_yml::to_string(&docker_compose)?;
                rendered_templates_cache.insert(
                    docker_compose_file_path.to_string_lossy(),
                    RenderedTemplate {
                        path: docker_compose_file_path.clone(),
                        content: updated_docker_compose_content,
                        context: None,
                    },
                );
            }
        }
    }

    let readme = rendered_templates_cache.get(&base_path.join("README.md"))?;
    if let Some(readme) = readme {
        let bun_replacements = ["bun run build".to_string(), "bun".to_string()];
        let pnpm_replacements = ["pnpm build".to_string(), "pnpm".to_string()];
        let mut replaced_content = readme.content.clone();

        for (existing, new) in zip(
            match existing_runtime {
                Runtime::Bun => &bun_replacements,
                Runtime::Node => &pnpm_replacements,
            },
            match runtime {
                Runtime::Bun => &bun_replacements,
                Runtime::Node => &pnpm_replacements,
            },
        ) {
            replaced_content = replaced_content.replace(existing, new);
        }

        rendered_templates_cache.insert(
            base_path.join("README.md").to_string_lossy(),
            RenderedTemplate {
                path: base_path.join("README.md"),
                content: replaced_content,
                context: None,
            },
        );
    }

    Ok((removal_templates, symlink_templates))
}

fn change_test_framework(
    base_path: &Path,
    test_framework: &TestFramework,
    manifest_data: &mut ApplicationManifestData,
    application_json_to_write: &mut ApplicationPackageJson,
    project_jsons_to_write: &mut HashMap<String, ProjectPackageJson>,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<(Vec<RemovalTemplate>, Vec<SymlinkTemplate>)> {
    let existing_test_framework =
        if let Some(manifest_test_framework) = manifest_data.test_framework.clone() {
            Some(manifest_test_framework.parse::<TestFramework>()?)
        } else {
            None
        };

    manifest_data.test_framework = Some(test_framework.to_string());

    let runtime = manifest_data.runtime.parse::<Runtime>()?;

    let exclusive_files = test_framework.metadata().exclusive_files;
    let existing_files = test_framework.all_other_files();

    let (removal_templates, symlink_templates, preserved_files) = update_config_files(
        base_path,
        exclusive_files,
        existing_files,
        project_jsons_to_write,
        rendered_templates_cache,
    )?;

    let mut test_frameworks_to_preserve = HashSet::new();
    test_frameworks_to_preserve.insert(test_framework.to_string());

    for file in preserved_files {
        if let Some(linter_file) = Linter::get_variant_from_exclusive_file(&file) {
            test_frameworks_to_preserve.insert(linter_file.to_string());
        }
    }

    let rc_test_frameworks_to_preserve = Rc::new(RefCell::new(test_frameworks_to_preserve));

    update_application_and_project_package_jsons(
        application_json_to_write,
        project_jsons_to_write.values_mut().collect(),
        &mut |scripts| {
            let modified_test_script_key = format!("test:{}", test_framework.to_string());

            scripts.test = if let Some(test_script) =
                application_test_script(&runtime, &Some(test_framework.clone()))
            {
                Some(attempt_replacement(
                    &mut scripts.additional_scripts,
                    scripts.test.clone(),
                    existing_test_framework.map(|test_framework| test_framework.to_string()),
                    application_test_script(&runtime, &existing_test_framework),
                    &modified_test_script_key,
                    &test_framework.to_string(),
                    &test_script,
                    Some(&mut rc_test_frameworks_to_preserve.borrow_mut()),
                ))
            } else {
                None
            }
        },
        &mut |scripts| {
            let modified_test_script_key = format!("test:{}", test_framework.to_string());
            scripts.test = if let Some(test_script) =
                project_test_script(&runtime, &Some(test_framework.clone()))
            {
                Some(attempt_replacement(
                    &mut scripts.additional_scripts,
                    scripts.test.clone(),
                    existing_test_framework.map(|test_framework| test_framework.to_string()),
                    project_test_script(&runtime, &existing_test_framework),
                    &modified_test_script_key,
                    &test_framework.to_string(),
                    &test_script,
                    Some(&mut rc_test_frameworks_to_preserve.borrow_mut()),
                ))
            } else {
                None
            }
        },
        &mut |_| {},
        &mut |dev_dependencies| {
            dev_dependencies.jest = None;
            dev_dependencies.ts_jest = None;
            dev_dependencies.vitest = None;
            dev_dependencies.types_jest = None;

            for test_framework in rc_test_frameworks_to_preserve.borrow().iter() {
                let test_framework = test_framework.parse::<TestFramework>().unwrap();
                match test_framework {
                    TestFramework::Vitest => {
                        dev_dependencies.vitest = Some(VITEST_VERSION.to_string());

                        let vitest_config_file_contents = TEMPLATES_DIR
                            .get_file(Path::new("application").join("vitest.config.ts"))
                            .unwrap()
                            .contents_utf8()
                            .unwrap();

                        rendered_templates_cache.insert(
                            base_path.join("vitest.config.ts").to_string_lossy(),
                            RenderedTemplate {
                                path: base_path.join("vitest.config.ts"),
                                content: vitest_config_file_contents.to_string(),
                                context: None,
                            },
                        );
                    }
                    TestFramework::Jest => {
                        dev_dependencies.jest = Some(JEST_VERSION.to_string());
                        dev_dependencies.ts_jest = Some(TS_JEST_VERSION.to_string());
                        dev_dependencies.types_jest = Some(JEST_TYPES_VERSION.to_string());

                        let jest_config_file_contents = TEMPLATES_DIR
                            .get_file(Path::new("application").join("jest.config.ts"))
                            .unwrap()
                            .contents_utf8()
                            .unwrap();

                        rendered_templates_cache.insert(
                            base_path.join("jest.config.ts").to_string_lossy(),
                            RenderedTemplate {
                                path: base_path.join("jest.config.ts"),
                                content: jest_config_file_contents.to_string(),
                                context: None,
                            },
                        );
                    }
                };
            }
        },
        &mut |_dev_dependencies| {},
    )?;

    Ok((removal_templates, symlink_templates))
}

fn change_author(
    author: &str,
    manifest_data: &mut ApplicationManifestData,
    application_json_to_write: &mut ApplicationPackageJson,
) {
    manifest_data.author = author.to_string();
    application_json_to_write.author = Some(author.to_string());
}

fn change_license(
    base_path: &Path,
    license: &License,
    manifest_data: &mut ApplicationManifestData,
    application_json_to_write: &mut ApplicationPackageJson,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<Option<RemovalTemplate>> {
    manifest_data.license = license.to_string();
    let license_path = base_path.join("LICENSE");

    let mut removal_template = None;
    if exists(&license_path)? {
        removal_template = Some(RemovalTemplate {
            path: license_path.clone(),
        });
    }

    application_json_to_write.license = Some(license.to_string());

    if let Some(license_file_contents) = generate_license(base_path, &manifest_data)? {
        rendered_templates_cache.insert(license_path.to_string_lossy(), license_file_contents);
    }

    Ok(removal_template)
}

impl CliCommand for ApplicationCommand {
    fn command(&self) -> Command {
        command("application", "Change a forklaunch Application")
            .alias("app")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application root path"),
            )
            .arg(
                Arg::new("name")
                    .short('N')
                    .help("The name of the application"),
            )
            .arg(
                Arg::new("validator")
                    .short('v')
                    .long("validator")
                    .help("The validator to use")
                    .value_parser(Validator::VARIANTS),
            )
            .arg(
                Arg::new("formatter")
                    .short('f')
                    .long("formatter")
                    .help("The formatter to use")
                    .value_parser(Formatter::VARIANTS),
            )
            .arg(
                Arg::new("linter")
                    .short('l')
                    .long("linter")
                    .help("The linter to use")
                    .value_parser(Linter::VARIANTS),
            )
            .arg(
                Arg::new("http-framework")
                    .short('F')
                    .long("http-framework")
                    .help("The http framework to use")
                    .value_parser(HttpFramework::VARIANTS),
            )
            .arg(
                Arg::new("runtime")
                    .short('r')
                    .long("runtime")
                    .help("The runtime to use")
                    .value_parser(Runtime::VARIANTS),
            )
            .arg(
                Arg::new("test-framework")
                    .short('t')
                    .long("test-framework")
                    .help("The test framework to use")
                    .value_parser(TestFramework::VARIANTS),
            )
            .arg(
                Arg::new("description")
                    .short('D')
                    .long("description")
                    .help("The description of the application"),
            )
            .arg(
                Arg::new("author")
                    .short('A')
                    .long("author")
                    .help("The author of the application"),
            )
            .arg(
                Arg::new("license")
                    .short('L')
                    .long("license")
                    .help("The license of the application")
                    .value_parser(License::VARIANTS),
            )
            .arg(
                Arg::new("dryrun")
                    .short('n')
                    .long("dryrun")
                    .help("Dry run the command")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("confirm")
                    .short('c')
                    .long("confirm")
                    .help("Flag to confirm any prompts")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &clap::ArgMatches) -> Result<()> {
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let mut rendered_templates_cache = RenderedTemplatesCache::new();

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let existing_manifest_data = toml::from_str::<ApplicationManifestData>(
            &read_to_string(&manifest_path).with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let app_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Application(&existing_manifest_data),
            &None,
            &mut line_editor,
            &mut stdout,
            matches,
            0,
        )?;

        let mut manifest_data = existing_manifest_data.initialize(
            InitializableManifestConfigMetadata::Application(ApplicationInitializationMetadata {
                app_name: existing_manifest_data.app_name.clone(),
                database: None,
            }),
        );

        let docker_compose_path = manifest_data
            .docker_compose_path
            .as_ref()
            .map(|path| app_root_path.join(path));

        let name = matches.get_one::<String>("name");
        let formatter = matches.get_one::<String>("formatter");
        let linter = matches.get_one::<String>("linter");
        let validator = matches.get_one::<String>("validator");
        let runtime = matches.get_one::<String>("runtime");
        let http_framework = matches.get_one::<String>("http-framework");
        let test_framework = matches.get_one::<String>("test-framework");
        let description = matches.get_one::<String>("description");
        let author = matches.get_one::<String>("author");
        let license = matches.get_one::<String>("license");
        let dryrun = matches.get_flag("dryrun");
        let confirm = matches.get_flag("confirm");

        let selected_options = if matches.ids().all(|id| id == "dryrun" || id == "confirm") {
            let options = vec![
                "name",
                "formatter",
                "linter",
                "http-framework",
                "runtime",
                "validator",
                "test-framework",
                "description",
                "author",
                "license",
            ];

            let selections = MultiSelect::with_theme(&ColorfulTheme::default())
                .with_prompt("What would you like to change?")
                .items(&options)
                .interact()?;

            if selections.is_empty() {
                writeln!(stdout, "No changes selected")?;
                return Ok(());
            }

            selections.iter().map(|i| options[*i]).collect()
        } else {
            vec![]
        };
        let name = prompt_field_from_selections_with_validation(
            "name",
            name,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "application name",
            None,
            |input: &str| validate_name(input),
            |_| {
                "Application name cannot be empty or include numbers or spaces. Please try again"
                    .to_string()
            },
        )?;

        let runtime = prompt_field_from_selections_with_validation(
            "runtime",
            runtime,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "runtime",
            Some(&Runtime::VARIANTS),
            |input| Runtime::VARIANTS.contains(&input),
            |_| "Invalid runtime. Please try again".to_string(),
        )?;

        let http_framework = prompt_field_from_selections_with_validation(
            "http-framework",
            http_framework,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "HTTP framework",
            Some(&HttpFramework::VARIANTS),
            |input| {
                HttpFramework::VARIANTS.contains(&input)
                    && !(runtime == Some("bun".to_string()) && input == "hyper-express")
            },
            |input| {
                if runtime == Some("bun".to_string()) && input == "hyper-express" {
                    "Hyper Express is not supported for bun".to_string()
                } else {
                    "Invalid HTTP framework. Please try again".to_string()
                }
            },
        )?;

        let formatter = prompt_field_from_selections_with_validation(
            "formatter",
            formatter,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "formatter",
            Some(&Formatter::VARIANTS),
            |input| Formatter::VARIANTS.contains(&input),
            |_| "Invalid formatter. Please try again".to_string(),
        )?;

        let linter = prompt_field_from_selections_with_validation(
            "linter",
            linter,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "linter",
            Some(&Linter::VARIANTS),
            |input| Linter::VARIANTS.contains(&input),
            |_| "Invalid linter. Please try again".to_string(),
        )?;

        let validator = prompt_field_from_selections_with_validation(
            "validator",
            validator,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "validator",
            Some(&Validator::VARIANTS),
            |input| Validator::VARIANTS.contains(&input),
            |_| "Invalid validator. Please try again".to_string(),
        )?;

        let test_framework = prompt_field_from_selections_with_validation(
            "test-framework",
            test_framework,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "test framework",
            Some(&TestFramework::VARIANTS),
            |input| TestFramework::VARIANTS.contains(&input),
            |_| "Invalid test framework. Please try again".to_string(),
        )?;

        let description = prompt_field_from_selections_with_validation(
            "description",
            description,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "project description",
            None,
            |_input: &str| true,
            |_| "Invalid description. Please try again".to_string(),
        )?;

        let author = prompt_field_from_selections_with_validation(
            "author",
            author,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "author name",
            None,
            |input: &str| !input.is_empty(),
            |_| "Author name cannot be empty. Please try again".to_string(),
        )?;

        let license = prompt_field_from_selections_with_validation(
            "license",
            license,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "license",
            Some(&License::VARIANTS),
            |input: &str| input.parse::<License>().is_ok(),
            |_| "Invalid license. Please try again".to_string(),
        )?;

        let mut removal_templates = vec![];
        let mut symlink_templates = vec![];

        let application_package_json_path = if app_path.join("package.json").exists() {
            app_path.join("package.json")
        } else {
            return Err(anyhow::anyhow!(
                "package.json not found in {}",
                app_path.display()
            ));
        };

        let application_package_json_data = read_to_string(&application_package_json_path)
            .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?;
        let mut application_json_to_write =
            serde_json::from_str::<ApplicationPackageJson>(&application_package_json_data)?;
        let mut project_jsons_to_write: HashMap<String, ProjectPackageJson> = manifest_data
            .projects
            .iter()
            .map(|project| {
                let project_package_json_path = app_path.join(&project.name).join("package.json");
                let project_package_json_data = read_to_string(&project_package_json_path)
                    .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)
                    .unwrap();

                (
                    project.name.clone(),
                    serde_json::from_str::<ProjectPackageJson>(&project_package_json_data).unwrap(),
                )
            })
            .collect();

        if let Some(name) = name {
            clean_application(
                &app_path,
                &manifest_data.runtime.parse()?,
                confirm,
                &mut stdout,
            )?;

            change_name(
                &mut manifest_data,
                &app_path,
                &docker_compose_path,
                &name,
                &mut application_json_to_write,
                &mut project_jsons_to_write,
                &mut rendered_templates_cache,
            )?;
        }

        if let Some(formatter) = formatter {
            let (formatter_removal_templates, formatter_symlink_templates) = change_formatter(
                &app_path,
                &formatter.parse()?,
                &mut manifest_data,
                &mut application_json_to_write,
                &mut project_jsons_to_write,
                &mut rendered_templates_cache,
            )?;
            removal_templates.extend(formatter_removal_templates);
            symlink_templates.extend(formatter_symlink_templates);
        }

        if let Some(linter) = linter {
            let (linter_removal_templates, linter_symlink_templates) = change_linter(
                &app_path,
                &linter.parse()?,
                &mut manifest_data,
                &mut application_json_to_write,
                &mut project_jsons_to_write,
                &mut rendered_templates_cache,
            )?;
            removal_templates.extend(linter_removal_templates);
            symlink_templates.extend(linter_symlink_templates);
        }

        if let Some(validator) = validator {
            change_validator(
                &app_path,
                &validator.parse()?,
                &mut manifest_data,
                &mut project_jsons_to_write,
                &mut rendered_templates_cache,
            )?;
        }

        if let Some(http_framework) = http_framework {
            change_http_framework(
                &app_path,
                &http_framework.parse()?,
                &mut manifest_data,
                &mut project_jsons_to_write,
                &mut rendered_templates_cache,
            )?;
        }

        if let Some(runtime) = runtime {
            clean_application(
                &app_path,
                &manifest_data.runtime.parse()?,
                confirm,
                &mut stdout,
            )?;

            let (runtime_removal_templates, runtime_symlink_templates) = change_runtime(
                &mut line_editor,
                &mut stdout,
                matches,
                &app_path,
                &runtime.parse()?,
                &mut manifest_data,
                &mut application_json_to_write,
                &mut project_jsons_to_write,
                &mut rendered_templates_cache,
            )?;
            removal_templates.extend(runtime_removal_templates);
            symlink_templates.extend(runtime_symlink_templates);
        }

        if let Some(test_framework) = test_framework {
            let (test_framework_removal_templates, test_framework_symlink_templates) =
                change_test_framework(
                    &app_path,
                    &test_framework.parse()?,
                    &mut manifest_data,
                    &mut application_json_to_write,
                    &mut project_jsons_to_write,
                    &mut rendered_templates_cache,
                )?;

            removal_templates.extend(test_framework_removal_templates);
            symlink_templates.extend(test_framework_symlink_templates);
        }

        if let Some(description) = description {
            change_description(
                &description,
                &mut manifest_data,
                &mut application_json_to_write,
            );
        }

        if let Some(author) = author {
            change_author(&author, &mut manifest_data, &mut application_json_to_write);
        }

        if let Some(license) = license {
            let removal_template = change_license(
                &app_path,
                &license.parse()?,
                &mut manifest_data,
                &mut application_json_to_write,
                &mut rendered_templates_cache,
            )?;

            if let Some(removal_template) = removal_template {
                removal_templates.push(removal_template);
            }
        }
        rendered_templates_cache.insert(
            manifest_path.to_string_lossy(),
            RenderedTemplate {
                path: manifest_path.to_path_buf(),
                content: toml::to_string_pretty(&manifest_data)?,
                context: None,
            },
        );
        rendered_templates_cache.insert(
            application_package_json_path.to_string_lossy(),
            RenderedTemplate {
                path: application_package_json_path.clone(),
                content: serde_json::to_string_pretty(&application_json_to_write)?,
                context: None,
            },
        );
        project_jsons_to_write
            .iter()
            .for_each(|(project_name, project_json)| {
                rendered_templates_cache.insert(
                    app_path
                        .join(project_name)
                        .join("package.json")
                        .to_string_lossy(),
                    RenderedTemplate {
                        path: app_path.join(project_name).join("package.json"),
                        content: serde_json::to_string_pretty(&project_json).unwrap(),
                        context: None,
                    },
                );
            });
        let rendered_templates: Vec<RenderedTemplate> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .collect();
        remove_template_files(&removal_templates, dryrun, &mut stdout)?;
        write_rendered_templates(&rendered_templates, dryrun, &mut stdout)?;
        create_symlinks(&symlink_templates, dryrun, &mut stdout)?;
        if !dryrun {
            log_ok!(stdout, "{} changed successfully!", &manifest_data.app_name);
            format_code(&app_path, &manifest_data.runtime.parse()?);
        }

        Ok(())
    }
}

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
        SELECTABLE_LINTERS, ensure_selectable_linter, linter_value_parser,
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
                BIOME_VERSION, ESLINT_VERSION, EXPRESS_VERSION, HYPER_EXPRESS_VERSION, UWEBSOCKETS_VERSION,
                JEST_TYPES_VERSION, JEST_VERSION, OXLINT_TSGOLINT_VERSION, OXLINT_VERSION, PRETTIER_VERSION,
                TS_JEST_VERSION, TYPEBOX_VERSION, TYPESCRIPT_ESLINT_VERSION, VITEST_VERSION,
                ZOD_VERSION, application_build_script, application_clean_purge_script,
                application_clean_script, application_docs_script, application_format_script,
                application_lint_fix_script, application_lint_script, application_migrate_script,
                application_seed_script, application_setup_script, application_test_script,
                application_up_packages_script, project_clean_script, project_dev_local_script,
                project_dev_server_script, project_dev_worker_client_script, project_format_script,
                project_lint_fix_script, project_lint_script, project_start_server_script,
                project_start_worker_script, project_test_script, project_up_latest_script, project_retention_enforce_script,
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
        tsconfig::update_tsconfig_test_framework_types,
        watermark::apply_watermark,
    },
    init::rust_project,
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
    rename_license_holder(base_path, &existing_name, name, rendered_templates_cache)?;
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

/// The MIT license names the application as its copyright holder; a rename
/// carries over to that line (and only that line, so an edited year or a
/// changed license text stays as written).
fn rename_license_holder(
    base_path: &Path,
    existing_name: &str,
    name: &str,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    let license_path = base_path.join("LICENSE");
    let Some(license) = rendered_templates_cache.get(&license_path)? else {
        return Ok(());
    };
    let holder = regex::Regex::new(&format!(
        r"(?m)^(Copyright \(c\).*?)\b{}\b([ \t]*)$",
        regex::escape(existing_name)
    ))?;
    let renamed = holder.replace_all(&license.content, |captures: &regex::Captures| {
        format!("{}{name}{}", &captures[1], &captures[2])
    });
    if renamed != license.content {
        rendered_templates_cache.insert(
            license_path.to_string_lossy(),
            RenderedTemplate { path: license_path.clone(), content: renamed.into_owned(), context: None },
        );
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
                removal_templates.push(RemovalTemplate { path: file_path });
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
            dev_dependencies.oxlint_tsgolint = None;

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
                        dev_dependencies.oxlint_tsgolint =
                            Some(OXLINT_TSGOLINT_VERSION.to_string());
                    }
                }
            }
        },
        &mut |dev_dependencies| {
            dev_dependencies.eslint = None;
            dev_dependencies.eslint_js = None;
            dev_dependencies.typescript_eslint = None;
            dev_dependencies.oxlint = None;
            dev_dependencies.oxlint_tsgolint = None;

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
                        dev_dependencies.oxlint_tsgolint =
                            Some(OXLINT_TSGOLINT_VERSION.to_string());
                    }
                }
            }
        },
    )?;

    Ok((removal_templates, symlink_templates))
}

/// Rewrite only static module specifiers; leave comments and application strings intact.
fn migrate_module_imports(
    base: &Path,
    projects: &HashMap<String, ProjectPackageJson>,
    from: &str,
    to: &str,
    cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    use oxc_ast::ast::{SourceType, Statement};
    for name in projects.keys() {
        for entry in walkdir::WalkDir::new(base.join(name)).follow_links(false).into_iter()
            .filter_entry(|e| !matches!(e.file_name().to_str(), Some("node_modules" | "dist" | "lib" | "production" | "target" | ".git"))) {
            let entry = entry?;
            if !entry.file_type().is_file() || entry.path().extension().is_none_or(|ext| ext != "ts" && ext != "tsx") { continue; }
            let mut template = cache.get(entry.path())?.context("Missing TypeScript source")?;
            if !template.content.contains(from) { continue; }
            let allocator = oxc_allocator::Allocator::default();
            let parsed = oxc_parser::Parser::new(&allocator, &template.content, SourceType::from_path(entry.path())?).parse();
            anyhow::ensure!(!parsed.diagnostics.has_errors(), "Cannot migrate imports in invalid TypeScript source {}", entry.path().display());
            let mut spans = Vec::new();
            for statement in &parsed.program.body {
                let source = match statement {
                    Statement::ImportDeclaration(import) => Some(&import.source),
                    Statement::ExportFromDeclaration(export) => Some(&export.source),
                    Statement::ExportAllDeclaration(export) => Some(&export.source),
                    _ => None,
                };
                if let Some(source) = source.filter(|source| source.value == from) { spans.push(source.span); }
            }
            drop(parsed);
            for span in spans.iter().rev() {
                template.content.replace_range(span.start as usize..span.end as usize, &serde_json::to_string(to)?);
            }
            if !spans.is_empty() { cache.insert(entry.path().to_string_lossy(), template); }
        }
    }
    Ok(())
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

    migrate_module_imports(base_path, project_jsons_to_write,
        &format!("@forklaunch/validator/{}", existing_validator.to_string()),
        &format!("@forklaunch/validator/{}", validator.to_string()), rendered_templates_cache)?;

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

fn preflight_http_framework(base_path: &Path, http_framework: &str, runtime: &str) -> Result<()> {
    if http_framework == "hyper-express" && runtime == "node" {
        let package: serde_json::Value = serde_json::from_str(&read_to_string(base_path.join("package.json"))?)?;
        // Moving a Bun workspace to Node writes the default pnpm policy, which
        // blocks the Git-distributed dependency.
        anyhow::ensure!(
            !package["packageManager"].as_str().is_some_and(|manager| manager.starts_with("bun@")),
            "HyperExpress on Node requires Git-distributed uWebSockets.js, which the default pnpm policy blocks. Change the HTTP framework to express in the same command (-F express), or keep the Bun runtime. Existing application files are preserved");
        let pinned_pnpm11 = package["packageManager"].as_str().is_some_and(|manager| manager.starts_with("pnpm@11."));
        if !pinned_pnpm11 { return Ok(()); }
        let path = base_path.join("pnpm-workspace.yaml");
        anyhow::ensure!(path.is_file(),
            "HyperExpress on Node requires Git-distributed uWebSockets.js, which the default pnpm policy blocks. Change the HTTP framework to express in the same command (-F express), or keep the Bun runtime. Existing application files are preserved");
        let workspace: serde_yml::Value = serde_yml::from_str(&read_to_string(&path)?)?;
        anyhow::ensure!(!pinned_pnpm11 || workspace["blockExoticSubdeps"].as_bool() == Some(false),
            "HyperExpress requires Git-distributed uWebSockets.js. The current pnpm policy blocks this dependency; explicitly review its dependency policy before selecting HyperExpress. Existing application files are preserved");
    }
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

    migrate_module_imports(base_path, project_jsons_to_write,
        &format!("@forklaunch/{}", existing_http_framework.to_string()),
        &format!("@forklaunch/{}", http_framework.to_string()), rendered_templates_cache)?;

    let http_framework_file_key = base_path.join("core").join("registrations.ts");

    for project in project_jsons_to_write.values_mut() {
        let dependencies = project.dependencies.as_mut().unwrap();
        dependencies.forklaunch_express = None;
        dependencies.forklaunch_hyper_express = None;
        dependencies.uwebsockets_js = None;

        match http_framework {
            HttpFramework::Express => {
                dependencies.forklaunch_express = Some(EXPRESS_VERSION.to_string());
            }
            HttpFramework::HyperExpress => {
                dependencies.forklaunch_hyper_express = Some(HYPER_EXPRESS_VERSION.to_string());
                dependencies.uwebsockets_js = Some(UWEBSOCKETS_VERSION.to_string());
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

    // Both runtimes execute the selected existing test framework. Bun's native
    // loader does not require discarding Jest/Vitest configuration or typings.
    // A test framework requested in the same change is the one to target, so
    // the old framework's configuration is not regenerated alongside it.
    let test_framework = if let Some(requested) = matches.get_one::<String>("test-framework") {
        Some(requested.parse::<TestFramework>()?)
    } else if let Some(existing) = &manifest_data.test_framework {
        Some(existing.parse::<TestFramework>()?)
    } else {
        let selected = prompt_field_from_selections_with_validation(
            "test-framework", None, &["test-framework"], line_editor, stdout, matches,
            "test framework", Some(&TestFramework::VARIANTS),
            |input: &str| TestFramework::VARIANTS.contains(&input),
            |_| "Invalid test framework. Please try again".to_string(),
        )?.context("Test framework required")?;
        Some(selected.parse::<TestFramework>()?)
    };

    if let Some(scripts) = application_json_to_write.scripts.as_mut() {
        if scripts.test == application_test_script(&existing_runtime, &test_framework) {
            scripts.test = application_test_script(runtime, &test_framework);
        }
    }
    for project in project_jsons_to_write.values_mut() {
        if let Some(scripts) = project.scripts.as_mut() {
            if scripts.test == project_test_script(&existing_runtime, &test_framework) {
                scripts.test = project_test_script(runtime, &test_framework);
            }
        }
    }

    // The test framework's configuration only needs regenerating when the
    // application had none (it was just prompted for). A framework requested
    // with -t is applied by the handler; the one already in place stays as it
    // is: regenerating it rewrote vitest.config.ts and reordered every
    // tsconfig's types, so bun→node did not return to the original files.
    let needs_test_framework_setup = matches.get_one::<String>("test-framework").is_none()
        && manifest_data.test_framework.is_none();
    if let Some(test_framework) = test_framework.filter(|_| needs_test_framework_setup) {
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
            if project_scripts.retention_enforce == Some(project_retention_enforce_script(&existing_runtime)) {
                project_scripts.retention_enforce = Some(project_retention_enforce_script(runtime));
            }
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
            {
                let old_up_latest = project_up_latest_script(&existing_runtime);
                let new_up_latest = project_up_latest_script(runtime);
                // Stash user customization if the existing value differs from what the old runtime generated
                if project_scripts.up_latest != old_up_latest {
                    if let Some(existing) = &project_scripts.up_latest {
                        if new_up_latest.as_ref() != Some(existing) {
                            project_scripts.additional_scripts.insert(
                                format!("up:latest:{}", existing_runtime.to_string()),
                                existing.clone(),
                            );
                        }
                    }
                }
                project_scripts.up_latest = new_up_latest;
            }
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
                    if project_scripts.start_server == Some(project_start_server_script(&existing_runtime, database)) {
                        project_scripts.start_server = Some(project_start_server_script(runtime, database));
                    }
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
            let overrides = application_json_to_write.additional_entries.entry("overrides".into()).or_insert_with(|| serde_json::json!({}));
            let overrides = overrides.as_object_mut().context("Workspace overrides must be an object")?;
            let defaults = serde_json::to_value(crate::core::pnpm_workspace::framework_release_overrides())?;
            for (name, version) in defaults.as_object().context("Framework overrides must be an object")? {
                overrides.entry(name.clone()).or_insert_with(|| version.clone());
            }
            application_json_to_write.workspaces = Some(existing_workspaces);
            if let Some(rendered) =
                crate::core::bunfig::generate_bunfig(&base_path.to_string_lossy())?
            {
                rendered_templates_cache.insert("bunfig.toml".to_string(), rendered);
            }
        }
        Runtime::Node => {
            // Undo what the Bun branch adds: the package.json workspace list
            // (pnpm-workspace.yaml holds it), the framework overrides bun reads
            // from package.json, and the generated bunfig.toml.
            application_json_to_write.workspaces = None;
            remove_framework_overrides(application_json_to_write)?;
            let bunfig_path = base_path.join("bunfig.toml");
            if read_to_string(&bunfig_path)
                .is_ok_and(|content| crate::core::bunfig::is_generated_bunfig(&content))
            {
                removal_templates.push(RemovalTemplate { path: bunfig_path });
            }
            rendered_templates_cache.insert(
                "pnpm-workspace.yaml".to_string(),
                RenderedTemplate {
                    path: base_path.join("pnpm-workspace.yaml"),
                    content: crate::core::pnpm_workspace::render_pnpm_workspace_with_packages(
                        base_path,
                        existing_workspaces,
                    )
                    .and_then(|content| {
                        // The generated production image installs injected
                        // workspace packages (see typescript_image); pnpm
                        // keeps those settings in this file, which the Bun
                        // runtime did not have.
                        let managed = base_path.ancestors().any(|root| {
                            crate::init::rust_project::managed_typescript_image(root).is_some()
                        });
                        if managed {
                            crate::core::pnpm_workspace::with_injected_workspace_packages(&content)
                        } else {
                            Ok(content)
                        }
                    })?,
                    context: None,
                },
            );
        }
    }

    let managed_image = base_path.ancestors().any(|root| {
        crate::init::rust_project::managed_typescript_image(root).is_some()
    });
    if !managed_image {
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
        log_warn!(
            stdout,
            "Warning: Dockerfile is generating from template, you may need to manually migrate changes from Dockerfile.{}",
            runtime.to_string()
        );
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

/// Drop the `overrides` entries the Bun runtime added (each framework package
/// at the default release pin), and the key when nothing else is left. An
/// entry the user changed or added stays.
fn remove_framework_overrides(application_json: &mut ApplicationPackageJson) -> Result<()> {
    let defaults = serde_json::to_value(crate::core::pnpm_workspace::framework_release_overrides())?;
    let defaults = defaults.as_object().context("Framework overrides must be an object")?;
    let Some(overrides) = application_json.additional_entries.get_mut("overrides") else {
        return Ok(());
    };
    let Some(entries) = overrides.as_object_mut() else {
        return Ok(());
    };
    entries.retain(|name, version| defaults.get(name) != Some(version));
    if entries.is_empty() {
        application_json.additional_entries.remove("overrides");
    }
    Ok(())
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

    // Update tsconfig types for the new test framework
    let project_names: Vec<&str> = project_jsons_to_write.keys().map(String::as_str).collect();
    let tsconfig_templates =
        update_tsconfig_test_framework_types(base_path, Some(test_framework), &project_names)?;
    for template in tsconfig_templates {
        let key = template.path.to_string_lossy().to_string();
        rendered_templates_cache.insert(key, template);
    }

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

    application_json_to_write.license = Some(license.to_string());

    // A new license text replaces the file in place; the file is removed only
    // when the new license has none. Removing a path also drops any write to
    // it, so doing both deleted the new LICENSE.
    if let Some(license_file_contents) = generate_license(base_path, &manifest_data)? {
        rendered_templates_cache.insert(license_path.to_string_lossy(), license_file_contents);
        return Ok(None);
    }

    Ok(exists(&license_path)?.then(|| RemovalTemplate { path: license_path.clone() }))
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
                Arg::new("cli-version")
                    .long("cli-version")
                    .value_name("X.Y.Z")
                    .value_parser(super::core::cli_version::parse_cli_version)
                    .conflicts_with_all([
                        "name", "validator", "formatter", "linter", "http-framework",
                        "runtime", "test-framework", "description", "author", "license",
                    ])
                    .help("Explicitly change only this project's CLI version pin (noninteractive)"),
            )
            .arg(
                Arg::new("framework-path")
                    .long("framework-path")
                    .value_name("FRAMEWORK_RS")
                    .conflicts_with_all(["framework-version", "cli-version"])
                    .help("Rust: build against a local framework-rs checkout in place (and its sibling blueprint-rs)"),
            )
            .arg(
                Arg::new("framework-version")
                    .long("framework-version")
                    .value_name("X.Y.Z")
                    .num_args(0..=1)
                    .default_missing_value(crate::core::cargo_toml::FRAMEWORK_VERSION)
                    .conflicts_with("cli-version")
                    .help("Rust: use the published framework crates (this exact version, default the CLI's release)"),
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
                    .value_parser(linter_value_parser()),
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
        if let Some(version) = matches.get_one::<String>("cli-version") {
            return super::core::cli_version::change_cli_version(matches, version);
        }
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let mut rendered_templates_cache = RenderedTemplatesCache::new();

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");
        if matches.contains_id("framework-path") || matches.contains_id("framework-version") {
            anyhow::ensure!(!matches.get_flag("dryrun"), "--dryrun is not supported when changing the framework source");
            let selected = crate::init::rust_project::dependencies::change(&app_root_path, matches)?;
            log_ok!(stdout, "Rust crates now use the {}", selected);
            return Ok(());
        }
        crate::init::rust_project::preflight_mixed_images(&app_root_path)?;

        let current_manifest: toml::Value = toml::from_str(&read_to_string(&manifest_path)?)?;
        let mixed = current_manifest.get("language").and_then(toml::Value::as_str) == Some("mixed");
        let previous_app_name = current_manifest.get("app_name").and_then(toml::Value::as_str).map(str::to_owned);
        // TypeScript tooling, and a rename while TypeScript packages carry the
        // application scope, go through the TypeScript change first.
        let typescript_workspace = current_manifest
            .get("modules_path")
            .and_then(toml::Value::as_str)
            .is_some_and(|modules| app_root_path.join(modules).join("package.json").is_file());
        let mixed_tooling = mixed
            && (["formatter", "linter", "test-framework", "validator", "http-framework", "runtime"].iter().any(|key| matches.get_one::<String>(key).is_some())
                || (typescript_workspace && ["name", "description", "author", "license"].iter().any(|key| matches.get_one::<String>(key).is_some())));
        if rust_project::is_rust_app_root(&app_root_path)? && !mixed_tooling {
            if matches.get_one::<String>("formatter").is_some()
                || matches.get_one::<String>("linter").is_some()
                || matches.get_one::<String>("validator").is_some()
                || matches.get_one::<String>("runtime").is_some()
                || matches.get_one::<String>("http-framework").is_some()
                || matches.get_one::<String>("test-framework").is_some()
            {
                anyhow::bail!(
                    "Rust application changes currently support name, description, author, and license"
                );
            }

            if let Some(name) = matches.get_one::<String>("name")
                && !validate_name(name)
            {
                anyhow::bail!(
                    "Application name cannot be empty or include numbers or spaces. Please try again"
                );
            }

            let changed_name = rust_project::change_rust_application(
                &app_root_path,
                matches.get_one::<String>("name").map(String::as_str),
                matches.get_one::<String>("description").map(String::as_str),
                matches.get_one::<String>("author").map(String::as_str),
                matches.get_one::<String>("license").map(String::as_str),
                None,
                matches.get_flag("dryrun"),
            )?;
            if !matches.get_flag("dryrun") {
                log_ok!(stdout, "{} changed successfully!", changed_name);
            }
            return Ok(());
        }

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
            Some(&SELECTABLE_LINTERS),
            |input| Linter::VARIANTS.contains(&input),
            |_| "Invalid linter. Please try again".to_string(),
        )?;
        if let Some(linter) = &linter {
            ensure_selectable_linter(linter)?;
        }

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
            .filter(|project| project.metadata.as_ref().and_then(|metadata| metadata.language.as_deref()) != Some("rust"))
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

        if http_framework.is_some() || runtime.is_some() {
        preflight_http_framework(
            &app_path,
            http_framework.as_deref().unwrap_or(&manifest_data.http_framework),
            runtime.as_deref().unwrap_or(&manifest_data.runtime),
        )?;
        }

        if runtime.is_some() {
            rust_project::preflight_typescript_runtime_change(&app_root_path)?;
        }
        if validator.is_some() || http_framework.is_some() || runtime.is_some() {
            crate::sdk::typescript_build::preflight_application(&app_root_path)?;
        }

        if let Some(name) = &name {
            if !dryrun {
            clean_application(
                &app_path,
                &manifest_data.runtime.parse()?,
                confirm,
                &mut stdout,
            )?;
            }

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
            if !dryrun && crate::init::rust_project::managed_typescript_image(&app_root_path).is_none() {
            clean_application(
                &app_path,
                &manifest_data.runtime.parse()?,
                confirm,
                &mut stdout,
            )?;
            }

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
        // Package keys under the application's scope follow a rename.
        let (old_scope, new_scope) = (
            format!("@{}/", existing_manifest_data.app_name),
            format!("@{}/", manifest_data.app_name),
        );
        let rename_scope = |key: &str| match key.strip_prefix(&old_scope) {
            Some(rest) => format!("{new_scope}{rest}"),
            None => key.to_string(),
        };
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
                content: crate::core::package_json::preserve_json_layout_renamed(
                    &application_package_json_data,
                    &serde_json::to_string_pretty(&application_json_to_write)?,
                    &rename_scope,
                ),
                context: None,
            },
        );
        project_jsons_to_write
            .iter()
            .for_each(|(project_name, project_json)| {
                let path = app_path.join(project_name).join("package.json");
                let rendered = serde_json::to_string_pretty(&project_json).unwrap();
                let content = match read_to_string(&path) {
                    Ok(original) => crate::core::package_json::preserve_json_layout_renamed(&original, &rendered, &rename_scope),
                    Err(_) => rendered,
                };
                rendered_templates_cache.insert(
                    path.to_string_lossy().to_string(),
                    RenderedTemplate { path, content, context: None },
                );
            });
        // A file this change removes may have been read back into the cache by
        // a later sweep over sources; writing it would resurrect it.
        let removed: HashSet<&Path> = removal_templates.iter().map(|template| template.path.as_path()).collect();
        let rendered_templates: Vec<RenderedTemplate> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .filter(|template| !removed.contains(template.path.as_path()))
            .collect();
        remove_template_files(&removal_templates, dryrun, &mut stdout)?;
        write_rendered_templates(&rendered_templates, dryrun, &mut stdout)?;
        create_symlinks(&symlink_templates, dryrun, &mut stdout)?;
        if !dryrun {
            if mixed_tooling {
                rust_project::change_rust_application(&app_root_path, name.as_deref(),
                    matches.get_one::<String>("description").map(String::as_str),
                    matches.get_one::<String>("author").map(String::as_str),
                    matches.get_one::<String>("license").map(String::as_str),
                    previous_app_name.as_deref(), false)?;
            }
            crate::init::rust_project::refresh_mixed_images(&app_root_path)?;
            log_ok!(stdout, "{} changed successfully!", &manifest_data.app_name);
            format_code(&app_path, &manifest_data.runtime.parse()?);
        } else if mixed_tooling {
            // The native half: Cargo metadata, Compose and local env files.
            rust_project::change_rust_application(&app_root_path, name.as_deref(),
                matches.get_one::<String>("description").map(String::as_str),
                matches.get_one::<String>("author").map(String::as_str),
                matches.get_one::<String>("license").map(String::as_str),
                previous_app_name.as_deref(), true)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod adapter_migration_tests {
    use super::*;
    #[test]
    fn hyperexpress_admission_uses_requested_runtime_without_mutation() {
        let root = tempfile::tempdir().unwrap();
        let package = r#"{"packageManager":"pnpm@11.28.7"}"#;
        std::fs::write(root.path().join("package.json"), package).unwrap();
        std::fs::write(root.path().join("pnpm-workspace.yaml"), "packages: []\n").unwrap();
        assert!(preflight_http_framework(root.path(), "hyper-express", "node").is_err());
        preflight_http_framework(root.path(), "hyper-express", "bun").unwrap();
        preflight_http_framework(root.path(), "express", "node").unwrap();
        assert_eq!(std::fs::read_to_string(root.path().join("package.json")).unwrap(), package);
        std::fs::remove_file(root.path().join("pnpm-workspace.yaml")).unwrap();
        std::fs::write(root.path().join("package.json"), r#"{"packageManager":"bun@1.3.14"}"#).unwrap();
        assert!(preflight_http_framework(root.path(), "hyper-express", "node").unwrap_err().to_string().contains("-F express"));
    }

    #[test]
    fn static_adapter_imports_preserve_application_text_and_generated_outputs() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("api/dist")).unwrap();
        let source = "// @forklaunch/validator/zod stays documented\nimport { string } from '@forklaunch/validator/zod';\nexport { number } from '@forklaunch/validator/zod';\nexport const label = '@forklaunch/validator/zod';\n";
        let path = root.path().join("api/schema.ts");
        std::fs::write(&path, source).unwrap();
        std::fs::write(root.path().join("api/dist/schema.ts"), source).unwrap();
        let projects = HashMap::from([("api".to_owned(), ProjectPackageJson::default())]);
        let mut cache = RenderedTemplatesCache::new();
        migrate_module_imports(root.path(), &projects, "@forklaunch/validator/zod", "@forklaunch/validator/typebox", &mut cache).unwrap();
        let staged = cache.get(&path).unwrap().unwrap().content;
        assert!(staged.contains("from \"@forklaunch/validator/typebox\""));
        assert!(staged.contains("// @forklaunch/validator/zod stays documented"));
        assert!(staged.contains("label = '@forklaunch/validator/zod'"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), source);
        assert_eq!(cache.get(root.path().join("api/dist/schema.ts")).unwrap().unwrap().content, source);
    }
}

#[cfg(test)]
mod license_tests {
    use super::*;

    #[test]
    fn rename_carries_over_to_the_license_copyright_holder() {
        let root = tempfile::tempdir().unwrap();
        let text = "MIT License\n\nCopyright (c) 2026 zebra\n\nzebra is free software\n";
        std::fs::write(root.path().join("LICENSE"), text).unwrap();
        let mut cache = RenderedTemplatesCache::new();
        rename_license_holder(root.path(), "zebra", "kestrel", &mut cache).unwrap();
        let renamed = cache.get(root.path().join("LICENSE")).unwrap().unwrap().content;
        assert_eq!(renamed, "MIT License\n\nCopyright (c) 2026 kestrel\n\nzebra is free software\n");
        // A license that does not name the application is left alone.
        std::fs::write(root.path().join("LICENSE"), "Apache License\n").unwrap();
        let mut cache = RenderedTemplatesCache::new();
        rename_license_holder(root.path(), "zebra", "kestrel", &mut cache).unwrap();
        assert_eq!(cache.get(root.path().join("LICENSE")).unwrap().unwrap().content, "Apache License\n");
    }

    #[test]
    fn changing_license_rewrites_license_file_instead_of_deleting_it() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("LICENSE"), "MIT License\n").unwrap();
        let mut manifest: ApplicationManifestData = toml::from_str(
            "id='x'\ncli_version='1.0.0'\napp_name='demo'\nmodules_path='src'\nruntime='node'\nformatter='prettier'\nlinter='oxlint'\nvalidator='zod'\nhttp_framework='express'\ntest_framework='vitest'\napp_description='d'\nauthor='a'\nlicense='MIT'\nprojects=[]\nproject_peer_topology={}\n",
        )
        .unwrap();
        let mut package = ApplicationPackageJson::default();
        let mut cache = RenderedTemplatesCache::new();
        let removal = change_license(root.path(), &License::Apache, &mut manifest, &mut package, &mut cache).unwrap();
        assert!(removal.is_none());
        let rendered = cache.get(root.path().join("LICENSE")).unwrap().unwrap();
        assert!(rendered.content.contains("Apache License"), "{}", &rendered.content[..80]);
        let removal = change_license(root.path(), &License::None, &mut manifest, &mut package, &mut cache).unwrap();
        assert_eq!(removal.unwrap().path, root.path().join("LICENSE"));
    }
}

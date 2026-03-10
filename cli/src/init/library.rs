use std::{fs::read_to_string, io::Write, path::Path};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use convert_case::{Case, Casing};
use rustyline::{Editor, history::DefaultHistory};
use serde_json::to_string_pretty;
use termcolor::{ColorChoice, StandardStream, WriteColor};
use toml::from_str;

use crate::{
    CliCommand,
    constants::{
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST,
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PACKAGE_JSON,
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE, ERROR_FAILED_TO_CREATE_GITIGNORE,
        ERROR_FAILED_TO_CREATE_LIBRARY_PACKAGE_JSON, ERROR_FAILED_TO_CREATE_SYMLINKS,
        ERROR_FAILED_TO_CREATE_TSCONFIG, ERROR_FAILED_TO_PARSE_MANIFEST,
        ERROR_FAILED_TO_READ_MANIFEST, Runtime, TestFramework,
    },
    core::{
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        format::format_code,
        gitignore::generate_gitignore,
        manifest::{
            ApplicationInitializationMetadata, InitializableManifestConfig,
            InitializableManifestConfigMetadata, ManifestData, ProjectType,
            add_project_definition_to_manifest, application::ApplicationManifestData,
            library::LibraryManifestData,
        },
        name::validate_name,
        package_json::{
            add_project_definition_to_package_json,
            package_json_constants::{
                PROJECT_BUILD_SCRIPT, PROJECT_DOCS_SCRIPT, project_clean_script,
                project_format_script, project_lint_fix_script, project_lint_script,
                project_test_script,
            },
            project_package_json::{ProjectDevDependencies, ProjectPackageJson, ProjectScripts},
        },
        pnpm_workspace::add_project_definition_to_pnpm_workspace,
        rendered_template::{RenderedTemplate, write_rendered_templates},
        symlinks::generate_symlinks,
        template::{PathIO, generate_with_template},
        tsconfig::{add_project_to_modules_tsconfig, generate_project_tsconfig},
    },
    prompt::{ArrayCompleter, prompt_with_validation, prompt_without_validation},
};

fn generate_basic_library(
    library_name: &String,
    base_path: &Path,
    manifest_path: &Path,
    manifest_data: &mut LibraryManifestData,
    stdout: &mut StandardStream,
    dryrun: bool,
) -> Result<()> {
    let output_path = base_path.join(library_name);

    let template_dir = PathIO {
        input_path: Path::new("project")
            .join("library")
            .to_string_lossy()
            .to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        module_id: None,
    };

    let ignore_files = vec![];
    let ignore_dirs = vec![];
    let preserve_files = vec![];

    let mut rendered_templates = generate_with_template(
        None,
        &template_dir,
        &ManifestData::Library(&manifest_data),
        &ignore_files,
        &ignore_dirs,
        &preserve_files,
        dryrun,
    )?;
    rendered_templates.push(generate_library_package_json(manifest_data, &output_path)?);
    rendered_templates.extend(
        generate_project_tsconfig(&output_path).with_context(|| ERROR_FAILED_TO_CREATE_TSCONFIG)?,
    );
    rendered_templates.extend(
        generate_gitignore(&output_path).with_context(|| ERROR_FAILED_TO_CREATE_GITIGNORE)?,
    );
    rendered_templates.extend(
        add_library_to_artifacts(manifest_data, base_path, manifest_path)
            .with_context(|| "Failed to add library metadata to artifacts")?,
    );

    rendered_templates.push(
        add_project_to_modules_tsconfig(base_path, &manifest_data.library_name)
            .with_context(|| "Failed to add library to modules tsconfig.json")?,
    );

    write_rendered_templates(&rendered_templates, dryrun, stdout)
        .with_context(|| "Failed to write library files")?;

    generate_symlinks(
        Some(base_path),
        &Path::new(&template_dir.output_path),
        manifest_data,
        dryrun,
    )
    .with_context(|| ERROR_FAILED_TO_CREATE_SYMLINKS)?;

    Ok(())
}

fn add_library_to_artifacts(
    manifest_data: &mut LibraryManifestData,
    base_path: &Path,
    manifest_path: &Path,
) -> Result<Vec<RenderedTemplate>> {
    let forklaunch_definition_buffer = add_project_definition_to_manifest(
        ProjectType::Library,
        manifest_data,
        None,
        None,
        None,
        None,
    )
    .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST)?;

    let runtime = manifest_data.runtime.parse()?;

    let mut package_json_buffer: Option<String> = None;
    let mut pnpm_workspace_buffer: Option<String> = None;

    match runtime {
        Runtime::Bun => {
            package_json_buffer = Some(
                add_project_definition_to_package_json(base_path, manifest_data)
                    .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PACKAGE_JSON)?,
            );
        }
        Runtime::Node => {
            pnpm_workspace_buffer = Some(
                add_project_definition_to_pnpm_workspace(base_path, manifest_data)
                    .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE)?,
            );
        }
    }

    let mut rendered_templates = Vec::new();
    rendered_templates.push(RenderedTemplate {
        path: manifest_path.to_path_buf(),
        content: forklaunch_definition_buffer,
        context: Some(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST.to_string()),
    });
    if let Some(package_json_buffer) = package_json_buffer {
        rendered_templates.push(RenderedTemplate {
            path: base_path.join("package.json"),
            content: package_json_buffer,
            context: Some(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PACKAGE_JSON.to_string()),
        });
    }
    if let Some(pnpm_workspace_buffer) = pnpm_workspace_buffer {
        rendered_templates.push(RenderedTemplate {
            path: base_path.join("pnpm-workspace.yaml"),
            content: pnpm_workspace_buffer,
            context: Some(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE.to_string()),
        });
    }

    Ok(rendered_templates)
}

fn generate_library_package_json(
    manifest_data: &LibraryManifestData,
    base_path: &Path,
) -> Result<RenderedTemplate> {
    let test_framework: Option<TestFramework> =
        if let Some(test_framework) = &manifest_data.test_framework {
            Some(test_framework.parse()?)
        } else {
            None
        };
    let package_json_buffer = ProjectPackageJson {
        name: Some(format!(
            "@{}/{}",
            manifest_data.kebab_case_app_name, manifest_data.kebab_case_name
        )),
        version: Some("0.1.0".to_string()),
        r#type: Some("module".to_string()),
        description: Some(manifest_data.description.clone()),
        keywords: Some(vec![]),
        license: Some(manifest_data.license.clone()),
        author: Some(manifest_data.author.clone()),
        types: None,
        scripts: Some(ProjectScripts {
            build: Some(PROJECT_BUILD_SCRIPT.to_string()),
            clean: Some(project_clean_script(&manifest_data.runtime.parse()?)),
            docs: Some(PROJECT_DOCS_SCRIPT.to_string()),
            format: Some(project_format_script(&manifest_data.formatter.parse()?)),
            lint: Some(project_lint_script(&manifest_data.linter.parse()?)),
            lint_fix: Some(project_lint_fix_script(&manifest_data.linter.parse()?)),
            test: project_test_script(&manifest_data.runtime.parse()?, &test_framework),
            ..Default::default()
        }),
        dev_dependencies: Some(ProjectDevDependencies {
            ..Default::default()
        }),
        ..Default::default()
    };
    Ok(RenderedTemplate {
        path: base_path.join("package.json"),
        content: to_string_pretty(&package_json_buffer)?.to_string(),
        context: Some(ERROR_FAILED_TO_CREATE_LIBRARY_PACKAGE_JSON.to_string()),
    })
}

#[derive(Debug)]
pub(super) struct LibraryCommand;

impl LibraryCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for LibraryCommand {
    fn command(&self) -> Command {
        command("library", "Initialize a new library")
            .alias("lib")
            .arg(Arg::new("name").help("The name of the library"))
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path to initialize the library in"),
            )
            .arg(
                Arg::new("description")
                    .short('D')
                    .long("description")
                    .help("The description of the service"),
            )
            .arg(
                Arg::new("dryrun")
                    .short('n')
                    .long("dryrun")
                    .help("Dry run the command")
                    .action(ArgAction::SetTrue),
            )
    }

    // pass token in from parent and perform get token above?
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let existing_manifest_data = from_str::<ApplicationManifestData>(
            &read_to_string(&manifest_path).with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let base_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Application(&existing_manifest_data),
            &None,
            &mut line_editor,
            &mut stdout,
            matches,
            0,
        )?;

        let manifest_data = existing_manifest_data.initialize(
            InitializableManifestConfigMetadata::Application(ApplicationInitializationMetadata {
                app_name: existing_manifest_data.app_name.clone(),
                database: None,
            }),
        );

        let library_name = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "name",
            matches,
            "library name",
            None,
            |input: &str| validate_name(input) && !manifest_data.app_name.contains(input),
            |_| {
                "Library name cannot be a substring of the application name, empty or include numbers or spaces. Please try again"
                    .to_string()
            },
        )?;

        let description = prompt_without_validation(
            &mut line_editor,
            &mut stdout,
            "description",
            matches,
            "library description",
            None,
        )?;

        let mut manifest_data: LibraryManifestData = LibraryManifestData {
            // Common fields from ApplicationManifestData
            id: manifest_data.id.clone(),
            app_name: manifest_data.app_name.clone(),
            modules_path: manifest_data.modules_path.clone(),
            docker_compose_path: manifest_data.docker_compose_path.clone(),
            dockerfile: manifest_data.dockerfile.clone(),
            git_repository: manifest_data.git_repository.clone(),
            camel_case_app_name: manifest_data.camel_case_app_name.clone(),
            pascal_case_app_name: manifest_data.pascal_case_app_name.clone(),
            kebab_case_app_name: manifest_data.kebab_case_app_name.clone(),
            title_case_app_name: manifest_data.title_case_app_name.clone(),
            app_description: manifest_data.app_description.clone(),
            author: manifest_data.author.clone(),
            cli_version: manifest_data.cli_version.clone(),
            formatter: manifest_data.formatter.clone(),
            linter: manifest_data.linter.clone(),
            validator: manifest_data.validator.clone(),
            runtime: manifest_data.runtime.clone(),
            test_framework: manifest_data.test_framework.clone(),
            projects: manifest_data.projects.clone(),
            http_framework: manifest_data.http_framework.clone(),
            license: manifest_data.license.clone(),
            project_peer_topology: manifest_data.project_peer_topology.clone(),
            is_biome: manifest_data.is_biome,
            is_eslint: manifest_data.is_eslint,
            is_oxlint: manifest_data.is_oxlint,
            is_prettier: manifest_data.is_prettier,
            is_express: manifest_data.is_express,
            is_hyper_express: manifest_data.is_hyper_express,
            is_zod: manifest_data.is_zod,
            is_typebox: manifest_data.is_typebox,
            is_bun: manifest_data.is_bun,
            is_node: manifest_data.is_node,
            is_vitest: manifest_data.is_vitest,
            is_jest: manifest_data.is_jest,
            platform_application_id: manifest_data.platform_application_id.clone(),
            platform_organization_id: manifest_data.platform_organization_id.clone(),

            // Library-specific fields
            library_name: library_name.clone(),
            camel_case_name: library_name.to_case(Case::Camel),
            kebab_case_name: library_name.to_case(Case::Kebab),
            title_case_name: library_name.to_case(Case::Title),
            description: description.clone(),
        };

        let dryrun = matches.get_flag("dryrun");
        generate_basic_library(
            &library_name,
            &base_path,
            &manifest_path,
            &mut manifest_data,
            &mut stdout,
            dryrun,
        )
        .with_context(|| "Failed to create library")?;

        if !dryrun {
            log_ok!(stdout, "{} initialized successfully!", library_name);
            format_code(&base_path, &manifest_data.runtime.parse()?);
        }

        Ok(())
    }
}

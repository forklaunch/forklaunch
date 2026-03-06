use std::{fs::read_to_string, io::Write, path::Path};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use convert_case::{Case, Casing};
use rustyline::{Editor, history::DefaultHistory};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::service::generate_service_package_json;
use crate::{
    CliCommand,
    constants::{
        Database, ERROR_FAILED_TO_CREATE_PACKAGE_JSON, ERROR_FAILED_TO_GENERATE_PNPM_WORKSPACE,
        ERROR_FAILED_TO_PARSE_MANIFEST, ERROR_FAILED_TO_READ_MANIFEST,
        ERROR_FAILED_TO_WRITE_DOCKER_COMPOSE, ERROR_FAILED_TO_WRITE_MANIFEST, Module, Runtime,
        get_service_module_cache, get_service_module_description, get_service_module_name,
    },
    core::{
        ast::injections::inject_into_client_sdk::ClientSdkSpecialCase,
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        client_sdk::add_project_to_client_sdk,
        command::command,
        database::{
            get_database_port, get_database_variants, get_db_driver, is_in_memory_database,
        },
        docker::add_service_definition_to_docker_compose,
        format::format_code,
        manifest::{
            ApplicationInitializationMetadata, InitializableManifestConfig,
            InitializableManifestConfigMetadata, ManifestData, ProjectType, ResourceInventory,
            add_project_definition_to_manifest, application::ApplicationManifestData,
            service::ServiceManifestData,
        },
        modules::{ModuleConfig, validate_modules},
        package_json::add_project_definition_to_package_json,
        pnpm_workspace::add_project_definition_to_pnpm_workspace,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
        symlinks::generate_symlinks,
        template::{PathIO, generate_with_template, get_routers_from_standard_package},
    },
    prompt::{ArrayCompleter, prompt_with_validation},
};

#[derive(Debug)]
pub(super) struct ModuleCommand;

impl ModuleCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for ModuleCommand {
    fn command(&self) -> Command {
        command("module", "Initialize a preconfigured module")
            .alias("mod")
            .arg(Arg::new("name").help("The name of the module"))
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path to initialize the module in"),
            )
            .arg(
                Arg::new("module")
                    .short('m')
                    .long("module")
                    .value_parser(Module::VARIANTS)
                    .help("The module to initialize"),
            )
            .arg(
                Arg::new("database")
                    .short('d')
                    .long("database")
                    .help("The database to use")
                    .value_parser(Database::VARIANTS),
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

        let existing_manifest_data = toml::from_str::<ApplicationManifestData>(
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

        let module: Module = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "module",
            matches,
            "module",
            Some(&Module::VARIANTS),
            |input| {
                let mut modules = manifest_data.projects.iter().filter_map(|project| {
                    if let Some(variant) = &project.variant {
                        return variant.parse::<Module>().ok()
                    }
                    None
                }).collect::<Vec<Module>>();
                modules.push(input.parse().unwrap());
                validate_modules(&modules, &mut ModuleConfig::default()).is_ok()
            },
            |input| format!("Conflicting module type selected. You will not be able to add this module without deleting the existing module, {}", get_service_module_name(&input.parse().unwrap())),
        )?
        .parse()?;

        let runtime = manifest_data.runtime.parse()?;
        let database_variants = get_database_variants(&runtime);

        let database: Database = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "database",
            matches,
            "database",
            Some(database_variants),
            |input| database_variants.contains(&input),
            |_| "Invalid database type. Please try again".to_string(),
        )?
        .parse()?;

        let dryrun = matches.get_flag("dryrun");

        let name = manifest_data.app_name.clone();

        let mut service_data = ServiceManifestData {
            id: manifest_data.id.clone(),
            cli_version: manifest_data.cli_version.clone(),
            app_name: name.clone(),
            modules_path: manifest_data.modules_path.clone(),
            docker_compose_path: manifest_data.docker_compose_path.clone(),
            dockerfile: manifest_data.dockerfile.clone(),
            git_repository: manifest_data.git_repository.clone(),
            camel_case_app_name: manifest_data.camel_case_app_name.clone(),
            pascal_case_app_name: manifest_data.pascal_case_app_name.clone(),
            kebab_case_app_name: manifest_data.kebab_case_app_name.clone(),
            title_case_app_name: manifest_data.title_case_app_name.clone(),
            service_name: get_service_module_name(&module),
            service_path: get_service_module_name(&module),
            camel_case_name: get_service_module_name(&module).to_case(Case::Camel),
            pascal_case_name: get_service_module_name(&module).to_case(Case::Pascal),
            kebab_case_name: get_service_module_name(&module).to_case(Case::Kebab),
            title_case_name: get_service_module_name(&module).to_case(Case::Title),
            formatter: manifest_data.formatter.clone(),
            linter: manifest_data.linter.clone(),
            validator: manifest_data.validator.clone(),
            http_framework: manifest_data.http_framework.clone(),
            runtime: manifest_data.runtime.clone(),
            test_framework: manifest_data.test_framework.clone(),
            projects: manifest_data.projects.clone(),
            project_peer_topology: manifest_data.project_peer_topology.clone(),
            author: manifest_data.author.clone(),
            app_description: manifest_data.app_description.clone(),
            license: manifest_data.license.clone(),
            description: get_service_module_description(&name, &module),

            is_eslint: manifest_data.is_eslint,
            is_biome: manifest_data.is_biome,
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

            is_postgres: database == Database::PostgreSQL,
            is_sqlite: database == Database::SQLite,
            is_mysql: database == Database::MySQL,
            is_mariadb: database == Database::MariaDB,
            is_better_sqlite: database == Database::BetterSQLite,
            is_libsql: database == Database::LibSQL,
            is_mssql: database == Database::MsSQL,
            is_mongo: database == Database::MongoDB,
            is_in_memory_database: is_in_memory_database(&database),

            database: database.to_string(),
            database_port: get_database_port(&database),
            db_driver: get_db_driver(&database),

            is_iam: module.clone() == Module::BaseIam || module.clone() == Module::BetterAuthIam,
            is_billing: module.clone() == Module::BaseBilling
                || module.clone() == Module::StripeBilling,
            is_cache_enabled: module.clone() == Module::BaseBilling
                || module.clone() == Module::StripeBilling,
            is_s3_enabled: false,
            is_database_enabled: true,
            platform_application_id: manifest_data.platform_application_id.clone(),
            platform_organization_id: manifest_data.platform_organization_id.clone(),
            release_version: manifest_data.release_version.clone(),
            release_git_commit: manifest_data.release_git_commit.clone(),
            release_git_branch: manifest_data.release_git_branch.clone(),

            is_better_auth: module.clone() == Module::BetterAuthIam,
            is_stripe: module.clone() == Module::StripeBilling,

            is_iam_configured: manifest_data.projects.iter().any(|project_entry| {
                if project_entry.name == "iam" {
                    return true;
                }
                return false;
            }),

            is_billing_configured: manifest_data.projects.iter().any(|project_entry| {
                if project_entry.name == "billing" {
                    return true;
                }
                return false;
            }),

            is_request_cache_needed: (module.clone() == Module::BaseBilling
                || module.clone() == Module::StripeBilling)
                || manifest_data.projects.iter().any(|project_entry| {
                    project_entry.name == "iam" || project_entry.name == "billing"
                }),
            is_type_needed: manifest_data.projects.iter().any(|project_entry| {
                project_entry.name == "iam" || project_entry.name == "billing"
            }),

            // Default to false for module initialization, will be set by CLI flag
            with_mappers: false,

            iam_secret: None,

            // These will be properly generated when initialized
            generated_password_encryption_secret: String::new(),
            generated_better_auth_secret: String::new(),
            generated_hmac_secret: String::new(),
        };
        let manifest_data = add_project_definition_to_manifest(
            ProjectType::Service,
            &mut service_data,
            Some(module.clone().to_string()),
            Some(ResourceInventory {
                database: Some(database.to_string()),
                cache: get_service_module_cache(&module),
                queue: None,
                object_store: None,
                redis_partition: None,
            }),
            get_routers_from_standard_package(module.clone()),
            None,
        )?;

        let template_dir = PathIO {
            input_path: Path::new("project")
                .join(&module.metadata().exclusive_files.unwrap().first().unwrap())
                .to_string_lossy()
                .to_string(),
            output_path: base_path
                .clone()
                .join(get_service_module_name(&module))
                .to_string_lossy()
                .to_string(),
            module_id: Some(module.clone()),
        };

        let mut rendered_templates = vec![];

        rendered_templates.push(RenderedTemplate {
            path: manifest_path.clone(),
            content: manifest_data,
            context: Some(ERROR_FAILED_TO_WRITE_MANIFEST.to_string()),
        });

        let docker_compose_path =
            if let Some(docker_compose_path) = &service_data.docker_compose_path {
                app_root_path.join(docker_compose_path)
            } else {
                app_root_path.join("docker-compose.yaml")
            };

        rendered_templates.push(RenderedTemplate {
            path: docker_compose_path,
            content: add_service_definition_to_docker_compose(&service_data, &app_root_path, None)?,
            context: Some(ERROR_FAILED_TO_WRITE_DOCKER_COMPOSE.to_string()),
        });

        rendered_templates.extend(generate_with_template(
            None,
            &template_dir,
            &ManifestData::Service(&service_data),
            &vec![],
            &vec![],
            &vec![],
            dryrun,
        )?);

        rendered_templates.push(generate_service_package_json(
            &service_data,
            &base_path.clone().join(get_service_module_name(&module)),
            None,
            None,
            None,
            None,
            None,
        )?);

        let mut rendered_templates_cache = RenderedTemplatesCache::new();
        for template in rendered_templates {
            rendered_templates_cache.insert(template.path.to_string_lossy().to_string(), template);
        }

        let special_case = if module == Module::BetterAuthIam {
            Some(ClientSdkSpecialCase::BetterAuth)
        } else {
            None
        };
        add_project_to_client_sdk(
            &mut rendered_templates_cache,
            &base_path,
            &service_data.app_name,
            &service_data.service_name,
            special_case,
        )?;

        match runtime {
            Runtime::Node => {
                let pnpm_workspace_path = base_path.join("pnpm-workspace.yaml");
                rendered_templates_cache.insert(
                    pnpm_workspace_path.to_string_lossy().to_string(),
                    RenderedTemplate {
                        path: pnpm_workspace_path,
                        content: add_project_definition_to_pnpm_workspace(
                            &base_path,
                            &service_data,
                        )?,
                        context: Some(ERROR_FAILED_TO_GENERATE_PNPM_WORKSPACE.to_string()),
                    },
                );
            }
            Runtime::Bun => {
                let package_json_path = base_path.join("package.json");
                rendered_templates_cache.insert(
                    package_json_path.to_string_lossy().to_string(),
                    RenderedTemplate {
                        path: package_json_path,
                        content: add_project_definition_to_package_json(&base_path, &service_data)?,
                        context: Some(ERROR_FAILED_TO_CREATE_PACKAGE_JSON.to_string()),
                    },
                );
            }
        }

        let rendered_templates: Vec<_> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .collect();

        write_rendered_templates(&rendered_templates, dryrun, &mut stdout)?;

        if !dryrun {
            generate_symlinks(
                Some(&base_path),
                &base_path.clone().join(get_service_module_name(&module)),
                &mut service_data,
                dryrun,
            )?;
            log_ok!(
                stdout,
                "{} initialized successfully!",
                get_service_module_name(&module)
            );
            format_code(&base_path, &service_data.runtime.parse()?);
        }

        Ok(())
    }
}

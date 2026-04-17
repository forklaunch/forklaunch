use std::{
    collections::{HashMap, HashSet},
    fs::read_to_string,
    io::Write,
    path::Path,
};

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
        Database, ERROR_FAILED_TO_ADD_BASE_ENTITY_TO_CORE,
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE,
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST,
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PACKAGE_JSON,
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE,
        ERROR_FAILED_TO_ADD_SERVICE_METADATA_TO_ARTIFACTS, ERROR_FAILED_TO_CREATE_GITIGNORE,
        ERROR_FAILED_TO_CREATE_PACKAGE_JSON, ERROR_FAILED_TO_CREATE_SYMLINKS,
        ERROR_FAILED_TO_CREATE_TSCONFIG, ERROR_FAILED_TO_PARSE_MANIFEST,
        ERROR_FAILED_TO_READ_MANIFEST, ERROR_FAILED_TO_WRITE_SERVICE_FILES, Runtime, TestFramework,
        WorkerType,
    },
    core::{
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        client_sdk::{add_project_to_client_sdk, regenerate_client_sdk_compliance},
        command::command,
        database::{
            add_base_entity_to_core, get_database_port, get_db_driver, is_in_memory_database,
        },
        docker::add_worker_definition_to_docker_compose,
        gitignore::generate_gitignore,
        manifest::{
            ApplicationInitializationMetadata, InitializableManifestConfig,
            InitializableManifestConfigMetadata, ManifestData, ProjectMetadata, ProjectType,
            ResourceInventory, add_project_definition_to_manifest, next_available_redis_partition,
            application::ApplicationManifestData, worker::WorkerManifestData,
        },
        name::validate_name,
        package_json::{
            add_project_definition_to_package_json,
            package_json_constants::{
                AJV_VERSION, APP_BILLING_VERSION, APP_CORE_VERSION, APP_IAM_VERSION,
                APP_MONITORING_VERSION, BETTER_SQLITE3_VERSION, BIOME_VERSION, BULLMQ_VERSION,
                COMMON_VERSION, CORE_VERSION, DOTENV_VERSION, ESLINT_VERSION, EXPRESS_VERSION,
                HYPER_EXPRESS_VERSION, INFRASTRUCTURE_REDIS_VERSION, INTERNAL_VERSION,
                IOREDIS_VERSION, MIKRO_ORM_CLI_VERSION, MIKRO_ORM_CORE_VERSION,
                MIKRO_ORM_DATABASE_VERSION, MIKRO_ORM_MIGRATIONS_VERSION,
                MIKRO_ORM_SEEDER_VERSION, OXLINT_VERSION,
                PINO_VERSION, PRETTIER_VERSION, PROJECT_BUILD_SCRIPT, PROJECT_DOCS_SCRIPT,
                PROJECT_SEED_SCRIPT, SQLITE3_VERSION, TESTING_VERSION, TSX_VERSION,
                TYPEBOX_VERSION, TYPEDOC_VERSION, TYPES_EXPRESS_SERVE_STATIC_CORE_VERSION,
                TYPES_EXPRESS_VERSION, TYPES_JEST_VERSION, TYPES_QS_VERSION, TYPES_UUID_VERSION,
                TYPESCRIPT_ESLINT_VERSION, UNIVERSAL_SDK_VERSION, UUID_VERSION, VALIDATOR_VERSION,
                WORKER_BULLMQ_VERSION, WORKER_DATABASE_VERSION, WORKER_INTERFACES_VERSION,
                WORKER_KAFKA_VERSION, WORKER_REDIS_VERSION, ZOD_VERSION, project_clean_script,
                project_dev_local_worker_script, project_dev_server_script,
                project_dev_worker_client_script, project_format_script, project_lint_fix_script,
                project_lint_script, project_migrate_script, project_start_server_script,
                project_start_worker_script, project_test_script, project_up_latest_script,
            },
            project_package_json::{
                MIKRO_ORM_CONFIG_PATHS, ProjectDependencies, ProjectDevDependencies,
                ProjectMikroOrm, ProjectPackageJson, ProjectScripts,
            },
            update_application_package_json,
        },
        pnpm_workspace::add_project_definition_to_pnpm_workspace,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
        symlinks::generate_symlinks,
        template::{PathIO, generate_with_template},
        tsconfig::{add_project_to_modules_tsconfig, generate_project_tsconfig},
        worker_type::{
            get_default_worker_options, get_worker_consumer_factory, get_worker_producer_factory,
            get_worker_type_name,
        },
    },
    prompt::{ArrayCompleter, prompt_with_validation, prompt_without_validation},
};

#[derive(Debug)]
pub(super) struct WorkerCommand;

impl WorkerCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

fn generate_basic_worker(
    worker_name: &String,
    base_path: &Path,
    app_root_path: &Path,
    manifest_data: &mut WorkerManifestData,
    stdout: &mut StandardStream,
    dryrun: bool,
) -> Result<()> {
    let output_path = base_path.join(worker_name);
    let template_dir = PathIO {
        input_path: Path::new("project")
            .join("worker")
            .to_string_lossy()
            .to_string(),
        output_path: output_path.to_string_lossy().to_string(),
        module_id: None,
    };

    let ignore_files = if !manifest_data.is_database_enabled {
        vec![
            "mikro-orm.config.ts".to_string(),
            "seeder.ts".to_string(),
            "compliance.controller.ts".to_string(),
            "compliance.routes.ts".to_string(),
            "enforce-retention.ts".to_string(),
        ]
    } else {
        vec!["consts.ts".to_string()]
    };
    let mut ignore_dirs = if !manifest_data.is_database_enabled {
        let mut dirs = vec!["seeder".to_string(), "seed.data.ts".to_string()];
        if !manifest_data.with_mappers {
            dirs.push("persistence".to_string());
        }
        dirs
    } else {
        vec![]
    };
    if !manifest_data.with_mappers {
        ignore_dirs.push("mappers".to_string());
    }
    let preserve_files = vec![];

    let mut rendered_templates = generate_with_template(
        None,
        &template_dir,
        &ManifestData::Worker(&manifest_data),
        &ignore_files,
        &ignore_dirs,
        &preserve_files,
        dryrun,
    )?;
    rendered_templates.push(generate_worker_package_json(
        manifest_data,
        &output_path,
        None,
        None,
        None,
        None,
        None,
    )?);
    rendered_templates.extend(
        generate_project_tsconfig(&output_path, Some(&["express", "qs"])).with_context(|| ERROR_FAILED_TO_CREATE_TSCONFIG)?,
    );
    rendered_templates.extend(
        generate_gitignore(&output_path).with_context(|| ERROR_FAILED_TO_CREATE_GITIGNORE)?,
    );
    rendered_templates.extend(
        add_worker_to_artifacts(manifest_data, base_path, app_root_path)
            .with_context(|| ERROR_FAILED_TO_ADD_SERVICE_METADATA_TO_ARTIFACTS)?,
    );

    if manifest_data.is_database_enabled {
        rendered_templates.extend(
            add_base_entity_to_core(&ManifestData::Worker(manifest_data), base_path)
                .with_context(|| ERROR_FAILED_TO_ADD_BASE_ENTITY_TO_CORE)?,
        );
    }

    let mut rendered_templates_cache = RenderedTemplatesCache::new();
    for template in rendered_templates {
        rendered_templates_cache.insert(template.path.to_string_lossy().to_string(), template);
    }

    add_project_to_client_sdk(
        &mut rendered_templates_cache,
        &base_path,
        &manifest_data.app_name,
        &manifest_data.worker_name,
        None,
    )?;

    regenerate_client_sdk_compliance(
        &mut rendered_templates_cache,
        &base_path,
        &manifest_data.projects,
    )?;

    let tsconfig_template = add_project_to_modules_tsconfig(base_path, &manifest_data.worker_name)
        .with_context(|| "Failed to add worker to modules tsconfig.json")?;
    rendered_templates_cache.insert(
        tsconfig_template.path.to_string_lossy().to_string(),
        tsconfig_template,
    );

    let rendered_templates: Vec<_> = rendered_templates_cache
        .drain()
        .map(|(_, template)| template)
        .collect();

    write_rendered_templates(&rendered_templates, dryrun, stdout)
        .with_context(|| ERROR_FAILED_TO_WRITE_SERVICE_FILES)?;

    generate_symlinks(
        Some(base_path),
        &Path::new(&template_dir.output_path),
        manifest_data,
        dryrun,
    )
    .with_context(|| ERROR_FAILED_TO_CREATE_SYMLINKS)?;

    Ok(())
}

fn add_worker_to_artifacts(
    manifest_data: &mut WorkerManifestData,
    base_path: &Path,
    app_root_path: &Path,
) -> Result<Vec<RenderedTemplate>> {
    let docker_compose_buffer =
        add_worker_definition_to_docker_compose(manifest_data, app_root_path, None)
            .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE)?;

    let forklaunch_manifest_buffer = add_project_definition_to_manifest(
        ProjectType::Worker,
        manifest_data,
        Some(manifest_data.worker_type.clone()),
        Some(ResourceInventory {
            database: if manifest_data.is_database_enabled {
                Some(manifest_data.database.clone().unwrap())
            } else {
                None
            },
            cache: if manifest_data.is_cache_enabled {
                Some(manifest_data.worker_type_lowercase.clone())
            } else {
                None
            },
            queue: if manifest_data.is_kafka_enabled {
                Some(manifest_data.worker_type_lowercase.clone())
            } else {
                None
            },
            object_store: None,
            redis_partition: if manifest_data.is_cache_enabled {
                Some(manifest_data.redis_partition)
            } else {
                None
            },
        }),
        Some(vec![manifest_data.worker_name.clone()]),
        Some(ProjectMetadata {
            r#type: Some(manifest_data.worker_type_lowercase.clone()),
            hosting_type: None,
        }),
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

    let docker_compose_path = if let Some(docker_compose_path) = &manifest_data.docker_compose_path
    {
        app_root_path.join(docker_compose_path)
    } else {
        app_root_path.join("docker-compose.yaml")
    };

    rendered_templates.push(RenderedTemplate {
        path: docker_compose_path,
        content: docker_compose_buffer,
        context: Some(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_DOCKER_COMPOSE.to_string()),
    });

    rendered_templates.push(RenderedTemplate {
        path: app_root_path.join(".forklaunch").join("manifest.toml"),
        content: forklaunch_manifest_buffer.clone(),
        context: Some(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST.to_string()),
    });

    rendered_templates.push(
        update_application_package_json(
            &ManifestData::Worker(manifest_data),
            base_path,
            package_json_buffer,
        )?
        .unwrap(),
    );

    if let Some(pnpm_workspace_buffer) = pnpm_workspace_buffer {
        rendered_templates.push(RenderedTemplate {
            path: base_path.join("pnpm-workspace.yaml"),
            content: pnpm_workspace_buffer,
            context: Some(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_PNPM_WORKSPACE.to_string()),
        });
    }

    Ok(rendered_templates)
}

pub(crate) fn generate_worker_package_json(
    manifest_data: &WorkerManifestData,
    base_path: &Path,
    dependencies_override: Option<ProjectDependencies>,
    dev_dependencies_override: Option<ProjectDevDependencies>,
    scripts_override: Option<ProjectScripts>,
    main_override: Option<String>,
    types_override: Option<Option<String>>,
) -> Result<RenderedTemplate> {
    let test_framework: Option<TestFramework> =
        if let Some(test_framework) = &manifest_data.test_framework {
            Some(test_framework.parse()?)
        } else {
            None
        };
    let package_json_contents = ProjectPackageJson {
        name: Some(format!(
            "@{}/{}",
            manifest_data.kebab_case_app_name, manifest_data.kebab_case_name
        )),
        version: Some("0.1.0".to_string()),
        r#type: Some("module".to_string()),
        description: Some(manifest_data.description.to_string()),
        keywords: Some(vec![]),
        license: Some(manifest_data.license.to_string()),
        author: Some(manifest_data.author.to_string()),
        main: main_override,
        types: types_override.unwrap_or(None),
        types_versions: None,
        exports: None,
        scripts: Some(if let Some(scripts) = scripts_override {
            scripts
        } else {
            ProjectScripts {
                build: Some(PROJECT_BUILD_SCRIPT.to_string()),
                clean: Some(project_clean_script(&manifest_data.runtime.parse()?)),
                dev_server: Some(project_dev_server_script(
                    &manifest_data.runtime.parse()?,
                    manifest_data
                        .database
                        .as_ref()
                        .map(|db| db.parse::<Database>().unwrap()),
                )),
                dev_worker: Some(project_dev_worker_client_script(
                    &manifest_data.runtime.parse()?,
                )),
                dev_local: Some(project_dev_local_worker_script(
                    &manifest_data.runtime.parse()?,
                    manifest_data
                        .database
                        .as_ref()
                        .map(|db| db.parse::<Database>().unwrap()),
                )),
                test: project_test_script(&manifest_data.runtime.parse()?, &test_framework),
                docs: Some(PROJECT_DOCS_SCRIPT.to_string()),
                format: Some(project_format_script(&manifest_data.formatter.parse()?)),
                lint: Some(project_lint_script(&manifest_data.linter.parse()?)),
                lint_fix: Some(project_lint_fix_script(&manifest_data.linter.parse()?)),
                migrate_create: if manifest_data.is_database_enabled {
                    Some(project_migrate_script("create").to_string())
                } else {
                    None
                },
                migrate_down: if manifest_data.is_database_enabled {
                    Some(project_migrate_script("down").to_string())
                } else {
                    None
                },
                migrate_init: if manifest_data.is_database_enabled {
                    Some(project_migrate_script("init").to_string())
                } else {
                    None
                },
                migrate_up: if manifest_data.is_database_enabled {
                    Some(project_migrate_script("up").to_string())
                } else {
                    None
                },
                seed: if manifest_data.is_database_enabled && !manifest_data.is_mongo {
                    Some(PROJECT_SEED_SCRIPT.to_string())
                } else {
                    None
                },
                start_server: Some(project_start_server_script(
                    &manifest_data.runtime.parse()?,
                    manifest_data
                        .database
                        .as_ref()
                        .map(|db| db.parse::<Database>().unwrap()),
                )),
                start_worker: Some(project_start_worker_script(
                    &manifest_data.runtime.parse()?,
                    manifest_data
                        .database
                        .as_ref()
                        .map(|db| db.parse::<Database>().unwrap()),
                )),
                up_latest: project_up_latest_script(&manifest_data.runtime.parse()?),
                ..Default::default()
            }
        }),
        dependencies: Some(if let Some(dependencies) = dependencies_override {
            dependencies
        } else {
            ProjectDependencies {
                app_name: manifest_data.app_name.to_string(),
                databases: if let Some(database) = &manifest_data.database {
                    HashSet::from([database.parse()?])
                } else {
                    HashSet::new()
                },
                app_core: Some(APP_CORE_VERSION.to_string()),
                app_monitoring: Some(APP_MONITORING_VERSION.to_string()),
                app_client_sdk: None,
                app_billing: if manifest_data.is_billing_configured {
                    Some(APP_BILLING_VERSION.to_string())
                } else {
                    None
                },
                app_iam: if manifest_data.is_iam_configured {
                    Some(APP_IAM_VERSION.to_string())
                } else {
                    None
                },
                forklaunch_better_auth_mikro_orm_fork: None,
                forklaunch_common: Some(COMMON_VERSION.to_string()),
                forklaunch_core: Some(CORE_VERSION.to_string()),
                forklaunch_express: if manifest_data.is_express {
                    Some(EXPRESS_VERSION.to_string())
                } else {
                    None
                },
                forklaunch_hyper_express: if manifest_data.is_hyper_express {
                    Some(HYPER_EXPRESS_VERSION.to_string())
                } else {
                    None
                },
                forklaunch_implementation_billing_base: None,
                forklaunch_implementation_billing_stripe: None,
                forklaunch_interfaces_billing: None,
                forklaunch_implementation_iam_base: None,
                forklaunch_interfaces_iam: None,
                forklaunch_implementation_worker_bullmq: if manifest_data
                    .worker_type_lowercase
                    .parse::<WorkerType>()?
                    == WorkerType::BullMQCache
                {
                    Some(WORKER_BULLMQ_VERSION.to_string())
                } else {
                    None
                },
                forklaunch_implementation_worker_redis: if manifest_data
                    .worker_type_lowercase
                    .parse::<WorkerType>()?
                    == WorkerType::RedisCache
                {
                    Some(WORKER_REDIS_VERSION.to_string())
                } else {
                    None
                },
                forklaunch_implementation_worker_database: if manifest_data
                    .worker_type_lowercase
                    .parse::<WorkerType>()?
                    == WorkerType::Database
                {
                    Some(WORKER_DATABASE_VERSION.to_string())
                } else {
                    None
                },
                forklaunch_implementation_worker_kafka: if manifest_data
                    .worker_type_lowercase
                    .parse::<WorkerType>()?
                    == WorkerType::Kafka
                {
                    Some(WORKER_KAFKA_VERSION.to_string())
                } else {
                    None
                },
                forklaunch_infrastructure_redis: if manifest_data.is_cache_enabled
                    || manifest_data.is_iam_configured
                    || manifest_data.is_billing_configured
                {
                    Some(INFRASTRUCTURE_REDIS_VERSION.to_string())
                } else {
                    None
                },
                forklaunch_infrastructure_s3: None,
                forklaunch_interfaces_worker: Some(WORKER_INTERFACES_VERSION.to_string()),
                forklaunch_internal: Some(INTERNAL_VERSION.to_string()),
                forklaunch_universal_sdk: Some(UNIVERSAL_SDK_VERSION.to_string()),
                forklaunch_validator: Some(VALIDATOR_VERSION.to_string()),
                mikro_orm_core: Some(MIKRO_ORM_CORE_VERSION.to_string()),
                mikro_orm_migrations: if manifest_data.is_database_enabled {
                    Some(MIKRO_ORM_MIGRATIONS_VERSION.to_string())
                } else {
                    None
                },
                mikro_orm_database: if manifest_data.is_database_enabled {
                    Some(MIKRO_ORM_DATABASE_VERSION.to_string())
                } else {
                    None
                },
                mikro_orm_reflection: None,
                mikro_orm_seeder: if manifest_data.is_database_enabled {
                    Some(MIKRO_ORM_SEEDER_VERSION.to_string())
                } else {
                    None
                },
                opentelemetry_api: None,
                types_express: Some(TYPES_EXPRESS_VERSION.to_string()),
                types_express_serve_static_core: Some(
                    TYPES_EXPRESS_SERVE_STATIC_CORE_VERSION.to_string(),
                ),
                types_qs: Some(TYPES_QS_VERSION.to_string()),
                typebox: if manifest_data.is_typebox {
                    Some(TYPEBOX_VERSION.to_string())
                } else {
                    None
                },
                ajv: Some(AJV_VERSION.to_string()),
                better_auth: None,
                bullmq: if manifest_data.worker_type_lowercase.parse::<WorkerType>()?
                    == WorkerType::BullMQCache
                {
                    Some(BULLMQ_VERSION.to_string())
                } else {
                    None
                },
                better_sqlite3: if manifest_data.is_node
                    && manifest_data.is_database_enabled
                    && manifest_data.is_better_sqlite
                {
                    Some(BETTER_SQLITE3_VERSION.to_string())
                } else {
                    None
                },
                dotenv: Some(DOTENV_VERSION.to_string()),
                jose: None,
                sqlite3: if manifest_data.is_node
                    && manifest_data.is_database_enabled
                    && manifest_data.is_sqlite
                {
                    Some(SQLITE3_VERSION.to_string())
                } else {
                    None
                },
                stripe: None,
                uuid: Some(UUID_VERSION.to_string()),
                zod: if manifest_data.is_zod {
                    Some(ZOD_VERSION.to_string())
                } else {
                    None
                },
                pino: Some(PINO_VERSION.to_string()),
                ioredis: Some(IOREDIS_VERSION.to_string()),
                additional_deps: HashMap::new(),
            }
        }),
        dev_dependencies: Some(if let Some(dev_dependencies) = dev_dependencies_override {
            dev_dependencies
        } else {
            ProjectDevDependencies {
                biome: if manifest_data.is_biome {
                    Some(BIOME_VERSION.to_string())
                } else {
                    None
                },
                eslint: if manifest_data.is_eslint {
                    Some(ESLINT_VERSION.to_string())
                } else {
                    None
                },
                testing: Some(TESTING_VERSION.to_string()),
                eslint_js: if manifest_data.is_eslint {
                    Some(ESLINT_VERSION.to_string())
                } else {
                    None
                },
                mikro_orm_cli: if manifest_data.is_database_enabled {
                    Some(MIKRO_ORM_CLI_VERSION.to_string())
                } else {
                    None
                },
                oxlint: if manifest_data.is_oxlint {
                    Some(OXLINT_VERSION.to_string())
                } else {
                    None
                },
                prettier: if manifest_data.is_prettier {
                    Some(PRETTIER_VERSION.to_string())
                } else {
                    None
                },
                tsx: Some(TSX_VERSION.to_string()),
                typedoc: Some(TYPEDOC_VERSION.to_string()),
                typescript_eslint: Some(TYPESCRIPT_ESLINT_VERSION.to_string()),
                types_jest: Some(TYPES_JEST_VERSION.to_string()),
                types_uuid: if manifest_data.is_database_enabled {
                    Some(TYPES_UUID_VERSION.to_string())
                } else {
                    None
                },
                types_pino: None,
                types_ioredis: None,
                pino: None,
                ioredis: Some(IOREDIS_VERSION.to_string()),
                additional_deps: HashMap::new(),
            }
        }),
        mikro_orm: Some(ProjectMikroOrm {
            manifest_paths: Some(
                MIKRO_ORM_CONFIG_PATHS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
        }),
        additional_entries: HashMap::new(),
    };
    Ok(RenderedTemplate {
        path: base_path.join("package.json"),
        content: to_string_pretty(&package_json_contents)?,
        context: Some(ERROR_FAILED_TO_CREATE_PACKAGE_JSON.to_string()),
    })
}

impl CliCommand for WorkerCommand {
    fn command(&self) -> Command {
        command("worker", "Initialize a new worker")
            .alias("wrk")
            .arg(Arg::new("name").help("The name of the worker"))
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path to initialize the worker in"),
            )
            .arg(
                Arg::new("type")
                    .short('t')
                    .long("type")
                    .help("The type to use")
                    .value_parser(WorkerType::VARIANTS),
            )
            .arg(
                Arg::new("database")
                    .short('d')
                    .long("database")
                    .help("The database to use")
                    .value_parser(Database::VARIANTS),
            )
            .arg(
                Arg::new("description")
                    .short('D')
                    .long("description")
                    .help("The description of the worker"),
            )
            .arg(
                Arg::new("mappers")
                    .long("mappers")
                    .help("Generate mapper files for entity/DTO transformation")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("dryrun")
                    .short('n')
                    .long("dryrun")
                    .help("Dry run the command")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let existing_manifest_data = from_str::<ApplicationManifestData>(
            &read_to_string(manifest_path).with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?,
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

        let worker_name = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "name",
            matches,
            "worker name",
            None,
            |input: &str| validate_name(input) && !manifest_data.app_name.contains(input),
            |_| {
                "Worker name cannot be a substring of the application name, empty or include numbers or spaces. Please try again"
                    .to_string()
            },
        )?;

        let r#type: WorkerType = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "type",
            matches,
            "worker type",
            Some(&WorkerType::VARIANTS),
            |input| WorkerType::VARIANTS.contains(&input),
            |_| "Invalid worker type. Please try again".to_string(),
        )?
        .parse()?;

        let mut database: Option<Database> = None;
        if r#type == WorkerType::Database {
            database = Some(
                prompt_with_validation(
                    &mut line_editor,
                    &mut stdout,
                    "database",
                    matches,
                    "worker database type",
                    Some(&Database::VARIANTS),
                    |input| Database::VARIANTS.contains(&input),
                    |_| "Invalid worker database type. Please try again".to_string(),
                )?
                .parse()?,
            );
        }

        let description = prompt_without_validation(
            &mut line_editor,
            &mut stdout,
            "description",
            matches,
            "worker description",
            None,
        )?;

        let mut manifest_data = WorkerManifestData {
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

            // Worker-specific fields
            worker_name: worker_name.clone(),
            camel_case_name: worker_name.to_case(Case::Camel),
            pascal_case_name: worker_name.to_case(Case::Pascal),
            kebab_case_name: worker_name.to_case(Case::Kebab),
            title_case_name: worker_name.to_case(Case::Title),
            description: description.clone(),
            database: if let Some(database) = &database {
                Some(database.to_string())
            } else {
                None
            },
            db_driver: if let Some(database) = &database {
                Some(get_db_driver(&database))
            } else {
                None
            },
            database_port: if let Some(database) = &database {
                get_database_port(&database)
            } else {
                None
            },
            is_worker: true,
            is_cache_enabled: r#type == WorkerType::BullMQCache || r#type == WorkerType::RedisCache,
            is_database_enabled: r#type == WorkerType::Database,
            is_kafka_enabled: r#type == WorkerType::Kafka,
            is_database_worker: r#type == WorkerType::Database,
            platform_application_id: manifest_data.platform_application_id.clone(),
            platform_organization_id: manifest_data.platform_organization_id.clone(),
            compliance: manifest_data.compliance.clone(),

            is_postgres: if let Some(database) = &database {
                database == &Database::PostgreSQL
            } else {
                false
            },
            is_mongo: if let Some(database) = &database {
                database == &Database::MongoDB
            } else {
                false
            },
            is_mysql: if let Some(database) = &database {
                database == &Database::MySQL
            } else {
                false
            },
            is_mariadb: if let Some(database) = &database {
                database == &Database::MariaDB
            } else {
                false
            },
            is_mssql: if let Some(database) = &database {
                database == &Database::MsSQL
            } else {
                false
            },
            is_sqlite: if let Some(database) = &database {
                database == &Database::SQLite
            } else {
                false
            },
            is_better_sqlite: if let Some(database) = &database {
                database == &Database::BetterSQLite
            } else {
                false
            },
            is_libsql: if let Some(database) = &database {
                database == &Database::LibSQL
            } else {
                false
            },
            is_in_memory_database: if let Some(database) = &database {
                is_in_memory_database(database)
            } else {
                false
            },

            worker_type: get_worker_type_name(&r#type),
            worker_type_lowercase: get_worker_type_name(&r#type).to_lowercase(),
            default_worker_options: get_default_worker_options(&r#type),
            worker_consumer_factory: get_worker_consumer_factory(
                &r#type,
                &worker_name.to_case(Case::Pascal),
            ),
            worker_producer_factory: get_worker_producer_factory(&r#type),

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

            is_request_cache_needed: match r#type {
                WorkerType::BullMQCache | WorkerType::RedisCache => true,
                _ => manifest_data.projects.iter().any(|project_entry| {
                    project_entry.name == "iam" || project_entry.name == "billing"
                }),
            },
            is_type_needed: true,
            with_mappers: matches.get_flag("mappers"),

            redis_partition: if r#type == WorkerType::BullMQCache || r#type == WorkerType::RedisCache {
                next_available_redis_partition(&manifest_data.projects)
            } else {
                0
            },

            // These will be properly generated when initialized
            generated_better_auth_secret: String::new(),
            generated_hmac_secret: String::new(),
            generated_encryption_key: String::new(),
            otel_token: "OtelCollector".to_string(),
        };

        let dryrun = matches.get_flag("dryrun");
        generate_basic_worker(
            &worker_name,
            &base_path,
            &app_root_path,
            &mut manifest_data,
            &mut stdout,
            dryrun,
        )
        .with_context(|| "Failed to create worker")?;

        if !dryrun {
            log_ok!(stdout, "{} initialized successfully!", worker_name);
        }

        Ok(())
    }
}

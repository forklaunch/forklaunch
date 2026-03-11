use std::{collections::HashSet, io::Write, path::Path};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, Command};
use convert_case::{Case, Casing};
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use ramhorns::Template;
use rustyline::{Editor, history::DefaultHistory};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::core::{
    change_database::{
        change_database_base_entity, change_database_env_variables,
        change_database_postinstall_script,
    },
    change_description::change_description as change_description_core,
    change_name::change_name as change_name_core,
};
use crate::{
    CliCommand,
    change::core::change_database::change_database_seed_script,
    constants::{
        Database, ERROR_FAILED_TO_PARSE_MANIFEST, ERROR_FAILED_TO_READ_DOCKER_COMPOSE,
        ERROR_FAILED_TO_READ_MANIFEST, ERROR_FAILED_TO_READ_PACKAGE_JSON, Infrastructure,
        WorkerType,
    },
    core::{
        ast::transformations::{
            transform_mikroorm_config_ts::transform_mikroorm_config_ts,
            transform_registrations_ts::transform_registrations_ts_worker_type,
            transform_test_utils_ts::{
                transform_test_utils_add_database, transform_test_utils_add_infrastructure,
                transform_test_utils_remove_database, transform_test_utils_remove_infrastructure,
            },
            transform_worker_to_service::transform_registrations_ts_worker_to_service,
        },
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        database::{get_database_variants, get_db_driver, is_in_memory_database},
        docker::{
            DockerCompose, add_database_to_docker_compose, add_kafka_to_docker_compose,
            add_redis_to_docker_compose, clean_up_unused_infrastructure_services,
            remove_service_from_docker_compose,
        },
        env::Env,
        format::format_code,
        manifest::{
            InitializableManifestConfig, InitializableManifestConfigMetadata, ManifestData,
            MutableManifestData, ProjectInitializationMetadata, ProjectType,
            worker::WorkerManifestData,
        },
        move_template::{MoveTemplate, move_template_files},
        name::validate_name,
        package_json::{
            application_package_json::ApplicationPackageJson,
            package_json_constants::{
                BULLMQ_VERSION, INFRASTRUCTURE_REDIS_VERSION, IOREDIS_VERSION,
                MIKRO_ORM_CORE_VERSION, MIKRO_ORM_DATABASE_VERSION, MIKRO_ORM_MIGRATIONS_VERSION,
                MIKRO_ORM_REFLECTION_VERSION, WORKER_BULLMQ_VERSION, WORKER_DATABASE_VERSION,
                WORKER_KAFKA_VERSION, WORKER_REDIS_VERSION,
            },
            project_package_json::ProjectPackageJson,
        },
        removal_template::{RemovalTemplate, remove_template_files},
        rendered_template::{
            RenderedTemplate, RenderedTemplatesCache, TEMPLATES_DIR, write_rendered_templates,
        },
    },
    prompt::{ArrayCompleter, prompt_field_from_selections_with_validation},
};

#[derive(Debug)]
pub(super) struct WorkerCommand;

impl WorkerCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

fn change_name(
    base_path: &Path,
    name: &str,
    confirm: bool,
    manifest_data: &mut WorkerManifestData,
    application_package_json: &mut ApplicationPackageJson,
    project_package_json: &mut ProjectPackageJson,
    docker_compose: &mut DockerCompose,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    removal_templates: &mut Vec<RemovalTemplate>,
    stdout: &mut StandardStream,
) -> Result<MoveTemplate> {
    change_name_core(
        base_path,
        name,
        confirm,
        MutableManifestData::Worker(manifest_data),
        application_package_json,
        project_package_json,
        Some(docker_compose),
        rendered_templates_cache,
        removal_templates,
        stdout,
    )
}

fn change_description(
    description: &str,
    manifest_data: &mut WorkerManifestData,
    project_package_json: &mut ProjectPackageJson,
) -> Result<()> {
    change_description_core(
        description,
        MutableManifestData::Worker(manifest_data),
        project_package_json,
    )
}

fn change_type(
    base_path: &Path,
    r#type: &WorkerType,
    database: Option<Database>,
    manifest_data: &mut WorkerManifestData,
    application_package_json: &mut ApplicationPackageJson,
    project_package_json: &mut ProjectPackageJson,
    docker_compose_data: &mut DockerCompose,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    removal_templates: &mut Vec<RemovalTemplate>,
) -> Result<()> {
    let next_redis_partition = crate::core::manifest::next_available_redis_partition(&manifest_data.projects);

    let project_entry = manifest_data
        .projects
        .iter_mut()
        .find(|project| project.name == manifest_data.worker_name)
        .unwrap();

    let existing_type = manifest_data.worker_type.parse::<WorkerType>()?;

    let existing_database = manifest_data
        .database
        .as_ref()
        .map(|db| db.parse::<Database>().unwrap());

    project_entry.metadata.as_mut().unwrap().r#type = Some(r#type.to_string());
    let resources = project_entry.resources.as_mut().unwrap();

    resources.database = None;
    resources.cache = None;
    resources.queue = None;

    rendered_templates_cache.insert(
        base_path.join("registrations.ts").to_string_lossy(),
        RenderedTemplate {
            path: base_path.join("registrations.ts").into(),
            content: transform_registrations_ts_worker_type(
                rendered_templates_cache,
                base_path,
                &manifest_data.app_name,
                &manifest_data.worker_name.to_case(Case::Pascal),
                &existing_type,
                r#type,
            )?,
            context: None,
        },
    );

    let dependencies = project_package_json.dependencies.as_mut().unwrap();

    dependencies.forklaunch_implementation_worker_bullmq = None;
    dependencies.forklaunch_implementation_worker_database = None;
    dependencies.forklaunch_implementation_worker_redis = None;
    dependencies.forklaunch_implementation_worker_kafka = None;

    let mut environment = docker_compose_data
        .services
        .get_mut(&format!("{}-worker", &manifest_data.worker_name))
        .unwrap()
        .environment
        .as_ref()
        .unwrap()
        .clone();

    let env_local_path = base_path.join(".env.local");
    let mut env_local_content: Env = serde_envfile::from_str(
        &rendered_templates_cache
            .get(&env_local_path)?
            .unwrap()
            .content,
    )?;

    env_local_content.db_name = None;
    env_local_content.db_host = None;
    env_local_content.db_user = None;
    env_local_content.db_password = None;
    env_local_content.db_port = None;
    env_local_content.redis_url = None;
    env_local_content.kafka_brokers = None;
    env_local_content.kafka_client_id = None;
    env_local_content.kafka_group_id = None;

    match r#type {
        WorkerType::BullMQCache => {
            dependencies.bullmq = Some(BULLMQ_VERSION.to_string());
            dependencies.forklaunch_implementation_worker_bullmq =
                Some(WORKER_BULLMQ_VERSION.to_string());
            dependencies.forklaunch_infrastructure_redis =
                Some(INFRASTRUCTURE_REDIS_VERSION.to_string());
            project_package_json
                .dependencies
                .as_mut()
                .unwrap()
                .ioredis = Some(IOREDIS_VERSION.to_string());
            resources.cache = Some(WorkerType::RedisCache.to_string());
            resources.redis_partition = Some(next_redis_partition);
            let _ = add_redis_to_docker_compose(
                &manifest_data.app_name,
                docker_compose_data,
                &mut environment,
                next_redis_partition,
            );
            env_local_content.redis_url = Some(format!("redis://localhost:6379/{}", next_redis_partition));
        }
        WorkerType::Database => {
            let db = database.unwrap();

            dependencies.databases = HashSet::from([db.clone()]);
            dependencies.mikro_orm_core = Some(MIKRO_ORM_CORE_VERSION.to_string());
            dependencies.mikro_orm_migrations = Some(MIKRO_ORM_MIGRATIONS_VERSION.to_string());
            dependencies.mikro_orm_reflection = Some(MIKRO_ORM_REFLECTION_VERSION.to_string());
            dependencies.mikro_orm_database = Some(MIKRO_ORM_DATABASE_VERSION.to_string());
            dependencies.forklaunch_implementation_worker_database =
                Some(WORKER_DATABASE_VERSION.to_string());

            resources.database = Some(db.to_string());

            manifest_data.database = Some(database.unwrap().to_string());
            manifest_data.db_driver = Some(get_db_driver(&db));
            manifest_data.is_mongo = db == Database::MongoDB;
            manifest_data.is_in_memory_database = is_in_memory_database(&db);

            let _ = add_database_to_docker_compose(
                &ManifestData::Worker(&manifest_data),
                docker_compose_data,
                &mut environment,
            );

            if let Some(existing_database) = existing_database {
                let removal_template = change_database_base_entity(
                    base_path,
                    &db,
                    &existing_database,
                    manifest_data.projects.clone(),
                    &manifest_data.app_name,
                    rendered_templates_cache,
                )?;

                if let Some(removal_template) = removal_template {
                    removal_templates.push(removal_template);
                }
            } else {
                let database_entity = match db {
                    Database::MongoDB => "nosql.base.entity.ts",
                    _ => "sql.base.entity.ts",
                };
                let entity_template_path = TEMPLATES_DIR
                    .get_file(
                        Path::new("project")
                            .join("core")
                            .join("persistence")
                            .join(database_entity),
                    )
                    .unwrap();
                let base_entity_ts_content =
                    Template::new(entity_template_path.contents_utf8().unwrap())?
                        .render(&manifest_data.clone());
                rendered_templates_cache.insert(
                    database_entity,
                    RenderedTemplate {
                        path: base_path
                            .parent()
                            .unwrap()
                            .join("core")
                            .join("persistence")
                            .join(database_entity),
                        content: base_entity_ts_content,
                        context: None,
                    },
                );
            }

            change_database_env_variables(
                &mut env_local_content,
                &manifest_data.app_name,
                &manifest_data.worker_name,
                &db,
            );

            change_database_postinstall_script(application_package_json, &db);
            change_database_seed_script(project_package_json, &db);

            rendered_templates_cache.insert(
                base_path.join("mikro-orm.config.ts").to_string_lossy(),
                RenderedTemplate {
                    path: base_path.join("mikro-orm.config.ts").into(),
                    content: if !base_path.join("mikro-orm.config.ts").exists() {
                        Template::new(
                            TEMPLATES_DIR
                                .get_file(
                                    Path::new("project")
                                        .join("worker")
                                        .join("mikro-orm.config.ts"),
                                )
                                .unwrap()
                                .contents_utf8()
                                .unwrap(),
                        )?
                        .render(&manifest_data.clone())
                    } else {
                        transform_mikroorm_config_ts(
                            rendered_templates_cache,
                            base_path,
                            &existing_database,
                            &db,
                        )?
                    },
                    context: None,
                },
            );
        }
        WorkerType::RedisCache => {
            dependencies.forklaunch_implementation_worker_redis =
                Some(WORKER_REDIS_VERSION.to_string());
            dependencies.forklaunch_infrastructure_redis =
                Some(INFRASTRUCTURE_REDIS_VERSION.to_string());
            project_package_json
                .dependencies
                .as_mut()
                .unwrap()
                .ioredis = Some(IOREDIS_VERSION.to_string());
            resources.cache = Some(WorkerType::RedisCache.to_string());
            resources.redis_partition = Some(next_redis_partition);
            let _ = add_redis_to_docker_compose(
                &manifest_data.app_name,
                docker_compose_data,
                &mut environment,
                next_redis_partition,
            );
            env_local_content.redis_url = Some(format!("redis://localhost:6379/{}", next_redis_partition));
        }
        WorkerType::Kafka => {
            dependencies.forklaunch_implementation_worker_kafka =
                Some(WORKER_KAFKA_VERSION.to_string());
            resources.queue = Some(WorkerType::Kafka.to_string());
            let _ = add_kafka_to_docker_compose(
                &manifest_data.app_name,
                &manifest_data.worker_name,
                docker_compose_data,
                &mut environment,
            );
            env_local_content.kafka_brokers = Some("localhost:9092".to_string());
            env_local_content.kafka_client_id = Some(format!(
                "{}-{}-client",
                manifest_data.app_name, manifest_data.worker_name
            ));
            env_local_content.kafka_group_id = Some(format!(
                "{}-{}-group",
                manifest_data.app_name, manifest_data.worker_name
            ));
        }
    }

    rendered_templates_cache.insert(
        env_local_path.clone().to_string_lossy(),
        RenderedTemplate {
            path: env_local_path,
            content: serde_envfile::to_string(&env_local_content)?,
            context: None,
        },
    );

    docker_compose_data
        .services
        .get_mut(&format!("{}-worker", &manifest_data.worker_name))
        .unwrap()
        .environment = Some(environment.clone());

    docker_compose_data
        .services
        .get_mut(&format!("{}-server", &manifest_data.worker_name))
        .unwrap()
        .environment = Some(environment.clone());

    let _ = clean_up_unused_infrastructure_services(
        docker_compose_data,
        manifest_data.projects.clone(),
    );

    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    if test_utils_path.exists() {
        let _ = transform_test_utils_remove_database(rendered_templates_cache, &base_path);
        let _ = transform_test_utils_remove_infrastructure(
            rendered_templates_cache,
            &base_path,
            &Infrastructure::Redis,
        );

        let test_content = match r#type {
            WorkerType::Database => {
                let db = database.unwrap();
                let content =
                    transform_test_utils_add_database(rendered_templates_cache, &base_path, &db)?;
                use crate::core::ast::transformations::transform_test_utils_ts::transform_test_utils_remove_kafka;
                transform_test_utils_remove_kafka(rendered_templates_cache, &base_path)
                    .unwrap_or(content)
            }
            WorkerType::RedisCache | WorkerType::BullMQCache => {
                let content = transform_test_utils_add_infrastructure(
                    rendered_templates_cache,
                    &base_path,
                    &Infrastructure::Redis,
                )?;
                use crate::core::ast::transformations::transform_test_utils_ts::transform_test_utils_remove_kafka;
                transform_test_utils_remove_kafka(rendered_templates_cache, &base_path)
                    .unwrap_or(content)
            }
            WorkerType::Kafka => {
                use crate::core::ast::transformations::transform_test_utils_ts::transform_test_utils_add_kafka;
                transform_test_utils_add_kafka(rendered_templates_cache, &base_path)?
            }
        };

        rendered_templates_cache.insert(
            test_utils_path.to_string_lossy(),
            RenderedTemplate {
                path: test_utils_path.clone(),
                content: test_content,
                context: None,
            },
        );
    }

    Ok(())
}

fn worker_to_service(
    base_path: &Path,
    manifest_data: &mut WorkerManifestData,
    project_package_json: &mut ProjectPackageJson,
    docker_compose: &mut DockerCompose,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    stdout: &mut StandardStream,
) -> Result<()> {
    let project_name = base_path.file_name().unwrap().to_string_lossy().to_string();

    let registrations_path = base_path.join("registrations.ts");
    let registrations_key = registrations_path.to_string_lossy().into_owned();
    rendered_templates_cache.insert(
        registrations_key,
        RenderedTemplate {
            path: registrations_path.clone(),
            content: transform_registrations_ts_worker_to_service(
                rendered_templates_cache,
                base_path,
            )?,
            context: None,
        },
    );

    let worker_ts_path = base_path.join("worker.ts");
    if let Some(template) = rendered_templates_cache.get(&worker_ts_path)? {
        let content = template
            .content
            .lines()
            .map(|l| format!("// {}", l))
            .collect::<Vec<_>>()
            .join("\n");
        let worker_ts_key = worker_ts_path.to_string_lossy().into_owned();
        rendered_templates_cache.insert(
            worker_ts_key,
            RenderedTemplate {
                path: worker_ts_path.clone(),
                content,
                context: None,
            },
        );
    }

    if let Some(scripts) = project_package_json.scripts.as_mut() {
        scripts.dev_worker = None;
        scripts.start_worker = None;
    }

    if let Some(deps) = project_package_json.dependencies.as_mut() {
        deps.forklaunch_interfaces_worker = None;
        deps.forklaunch_implementation_worker_bullmq = None;
        deps.forklaunch_implementation_worker_database = None;
        deps.forklaunch_implementation_worker_redis = None;
        deps.forklaunch_implementation_worker_kafka = None;
    }

    manifest_data.projects.iter_mut().for_each(|project| {
        if project.name == project_name {
            project.r#type = ProjectType::Service;
            if let Some(metadata) = project.metadata.as_mut() {
                metadata.r#type = None;
            }
        }
    });

    let worker_service_name = format!("{}-worker", project_name);
    remove_service_from_docker_compose(docker_compose, &worker_service_name)?;

    let readme_path = base_path.join("README-MIGRATION.md");
    let readme_key = readme_path.to_string_lossy().into_owned();
    let readme_content = format!(
        r#"# Worker to Service Migration

This worker has been converted back to a service.

## Changes Made

1. **registrations.ts** - Worker registrations removed.
2. **worker.ts** - File commented out.
3. **package.json** - Worker scripts and dependencies removed.
4. **manifest.toml** - Project type changed to Service.
5. **docker-compose.yaml** - Worker service removed.

## Next Steps

1. **Review server.ts** - Ensure your HTTP server and routes are active.
2. **Clean up** - You can delete `worker.ts` and `*EventRecord.entity.ts` if no longer needed.
3. **Install dependencies**:
   ```bash
   pnpm install
   ```
4. **Start the service**:
   ```bash
   pnpm dev
   ```
"#
    );
    rendered_templates_cache.insert(
        readme_key,
        RenderedTemplate {
            path: readme_path.clone(),
            content: readme_content,
            context: None,
        },
    );

    log_warn!(stdout, "Worker converted to service. See README-MIGRATION.md.");

    Ok(())
}

impl CliCommand for WorkerCommand {
    fn command(&self) -> Command {
        command("worker", "Change a forklaunch worker")
            .alias("wrk")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The service path"),
            )
            .arg(Arg::new("name").short('N').help("The name of the service"))
            .arg(
                Arg::new("type")
                    .short('t')
                    .long("type")
                    .help("The type to use"),
            )
            .arg(
                Arg::new("to")
                    .long("to")
                    .help("Convert worker to another type (service)")
                    .value_parser(["service"]),
            )
            .arg(
                Arg::new("database")
                    .short('d')
                    .long("database")
                    .help("The database to use"),
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

        let (app_root_path, project_name) = find_app_root_path(matches, RequiredLocation::Project)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let existing_manifest_data: WorkerManifestData = toml::from_str::<WorkerManifestData>(
            &rendered_templates_cache
                .get(&manifest_path)
                .with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?
                .unwrap()
                .content,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let worker_base_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Worker(&existing_manifest_data),
            &project_name,
            &mut line_editor,
            &mut stdout,
            matches,
            1,
        )?;

        let mut manifest_data = existing_manifest_data.initialize(
            InitializableManifestConfigMetadata::Project(ProjectInitializationMetadata {
                project_name: worker_base_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
                    .clone(),
                database: None,
                infrastructure: None,
                description: None,
                worker_type: None,
            }),
        );

        let runtime = manifest_data.runtime.parse()?;

        let name = matches.get_one::<String>("name");
        let to = matches.get_one::<String>("to");
        let r#type = matches.get_one::<String>("type");
        let database = matches.get_one::<String>("database");
        let description = matches.get_one::<String>("description");
        let dryrun = matches.get_flag("dryrun");
        let confirm = matches.get_flag("confirm");

        if let Some(to) = to {
            if to == "service" {
                let application_package_json_to_write =
                    serde_json::from_str::<ApplicationPackageJson>(
                        &rendered_templates_cache
                            .get(worker_base_path.parent().unwrap().join("package.json"))
                            .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?
                            .unwrap()
                            .content,
                    )?;

                let mut project_package_json_to_write = serde_json::from_str::<ProjectPackageJson>(
                    &rendered_templates_cache
                        .get(worker_base_path.join("package.json"))
                        .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?
                        .unwrap()
                        .content,
                )?;

                let docker_compose_path =
                    if let Some(docker_compose_path) = &manifest_data.docker_compose_path {
                        app_root_path.join(docker_compose_path)
                    } else {
                        app_root_path.join("docker-compose.yaml")
                    };
                let mut docker_compose_data = serde_yml::from_str::<DockerCompose>(
                    &rendered_templates_cache
                        .get(&docker_compose_path)
                        .with_context(|| ERROR_FAILED_TO_READ_DOCKER_COMPOSE)?
                        .unwrap()
                        .content,
                )?;

                worker_to_service(
                    &worker_base_path,
                    &mut manifest_data,
                    &mut project_package_json_to_write,
                    &mut docker_compose_data,
                    &mut rendered_templates_cache,
                    &mut stdout,
                )?;

                // Write modified files
                rendered_templates_cache.insert(
                    manifest_path.clone().to_string_lossy().to_string(),
                    RenderedTemplate {
                        path: manifest_path.to_path_buf(),
                        content: toml::to_string_pretty(&manifest_data)?,
                        context: None,
                    },
                );

                rendered_templates_cache.insert(
                    docker_compose_path.clone().to_string_lossy().to_string(),
                    RenderedTemplate {
                        path: docker_compose_path.into(),
                        content: serde_yml::to_string(&docker_compose_data)?,
                        context: None,
                    },
                );

                rendered_templates_cache.insert(
                    worker_base_path
                        .parent()
                        .unwrap()
                        .join("package.json")
                        .to_string_lossy()
                        .to_string(),
                    RenderedTemplate {
                        path: worker_base_path.parent().unwrap().join("package.json"),
                        content: serde_json::to_string_pretty(&application_package_json_to_write)?,
                        context: None,
                    },
                );

                rendered_templates_cache.insert(
                    worker_base_path
                        .join("package.json")
                        .to_string_lossy()
                        .to_string(),
                    RenderedTemplate {
                        path: worker_base_path.join("package.json"),
                        content: serde_json::to_string_pretty(&project_package_json_to_write)?,
                        context: None,
                    },
                );

                let rendered_templates: Vec<RenderedTemplate> = rendered_templates_cache
                    .drain()
                    .map(|(_, template)| template)
                    .collect();

                write_rendered_templates(&rendered_templates, dryrun, &mut stdout)?;

                return Ok(());
            }
        }

        let selected_options = if matches.ids().all(|id| id == "dryrun" || id == "confirm") {
            let options = vec!["name", "type", "description"];

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
            "worker name",
            None,
            |input: &str| validate_name(input) && !manifest_data.app_name.contains(input),
            |_| {
                "Worker name cannot be a substring of the application name, empty or include numbers or spaces. Please try again"
                    .to_string()
            },
        )?;

        let r#type = prompt_field_from_selections_with_validation(
            "type",
            r#type,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "type",
            Some(&WorkerType::VARIANTS),
            |_input: &str| true,
            |_| "Invalid type. Please try again".to_string(),
        )?;

        let database = if r#type
            .clone()
            .map(|r#type| r#type.parse::<WorkerType>().unwrap() == WorkerType::Database)
            .unwrap_or(false)
        {
            let database_variants = get_database_variants(&runtime);
            prompt_field_from_selections_with_validation(
                "database",
                database,
                &["database"],
                &mut line_editor,
                &mut stdout,
                matches,
                "database",
                Some(database_variants),
                |input: &str| database_variants.contains(&input),
                |_| "Invalid database. Please try again".to_string(),
            )?
            .and_then(|database| database.parse::<Database>().ok())
        } else {
            None
        };

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

        let mut removal_templates = vec![];
        let mut move_templates = vec![];

        let application_package_json_path = worker_base_path.parent().unwrap().join("package.json");
        let application_package_json_data = rendered_templates_cache
            .get(&application_package_json_path)
            .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?
            .unwrap()
            .content;

        let mut application_package_json_to_write =
            serde_json::from_str::<ApplicationPackageJson>(&application_package_json_data)?;

        let project_package_json_path = worker_base_path.join("package.json");
        let project_package_json_data = rendered_templates_cache
            .get(&project_package_json_path)
            .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?
            .unwrap()
            .content;

        let mut project_json_to_write =
            serde_json::from_str::<ProjectPackageJson>(&project_package_json_data)?;

        let docker_compose_path =
            if let Some(docker_compose_path) = &manifest_data.docker_compose_path {
                app_root_path.join(docker_compose_path)
            } else {
                app_root_path.join("docker-compose.yaml")
            };
        let mut docker_compose_data = serde_yml::from_str::<DockerCompose>(
            &rendered_templates_cache
                .get(&docker_compose_path)
                .with_context(|| ERROR_FAILED_TO_READ_DOCKER_COMPOSE)?
                .unwrap()
                .content,
        )?;

        if let Some(r#type) = r#type {
            change_type(
                &worker_base_path,
                &r#type.parse()?,
                database,
                &mut manifest_data,
                &mut application_package_json_to_write,
                &mut project_json_to_write,
                &mut docker_compose_data,
                &mut rendered_templates_cache,
                &mut removal_templates,
            )?
        }

        if let Some(description) = description {
            change_description(&description, &mut manifest_data, &mut project_json_to_write)?;
        }

        if let Some(name) = name {
            move_templates.push(change_name(
                &worker_base_path,
                &name,
                confirm,
                &mut manifest_data,
                &mut application_package_json_to_write,
                &mut project_json_to_write,
                &mut docker_compose_data,
                &mut rendered_templates_cache,
                &mut removal_templates,
                &mut stdout,
            )?);
        }

        rendered_templates_cache.insert(
            manifest_path.clone().to_string_lossy(),
            RenderedTemplate {
                path: manifest_path.to_path_buf(),
                content: toml::to_string_pretty(&manifest_data)?,
                context: None,
            },
        );

        rendered_templates_cache.insert(
            docker_compose_path.clone().to_string_lossy(),
            RenderedTemplate {
                path: docker_compose_path.into(),
                content: serde_yml::to_string(&docker_compose_data)?,
                context: None,
            },
        );

        rendered_templates_cache.insert(
            application_package_json_path.clone().to_string_lossy(),
            RenderedTemplate {
                path: application_package_json_path.into(),
                content: serde_json::to_string_pretty(&application_package_json_to_write)?,
                context: None,
            },
        );

        rendered_templates_cache.insert(
            project_package_json_path
                .clone()
                .to_string_lossy()
                .to_string(),
            RenderedTemplate {
                path: project_package_json_path.into(),
                content: serde_json::to_string_pretty(&project_json_to_write)?,
                context: None,
            },
        );

        let rendered_templates: Vec<RenderedTemplate> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .collect();

        remove_template_files(&removal_templates, dryrun, &mut stdout)?;
        write_rendered_templates(&rendered_templates, dryrun, &mut stdout)?;

        move_template_files(&move_templates, dryrun, &mut stdout)?;

        if !dryrun {
            log_ok!(stdout, "{} changed successfully!", &manifest_data.app_name);
            format_code(&worker_base_path, &manifest_data.runtime.parse()?);
        }

        Ok(())
    }
}

use std::{collections::HashSet, io::Write, path::Path};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, Command};
use convert_case::{Case, Casing};
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use indexmap::IndexMap;
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
    change::core::change_database::{change_database_retention_script, change_database_seed_script},
    constants::{
        Database, ERROR_FAILED_TO_PARSE_MANIFEST, ERROR_FAILED_TO_READ_DOCKER_COMPOSE,
        ERROR_FAILED_TO_READ_MANIFEST, ERROR_FAILED_TO_READ_PACKAGE_JSON, Infrastructure,
        WorkerType,
    },
    core::{
        ast::{
            deletions::delete_from_registrations_ts::{
                delete_from_registrations_ts_infrastructure_redis,
                delete_from_registrations_ts_infrastructure_s3,
            },
            transformations::{
                transform_mikroorm_config_ts::transform_mikroorm_config_ts,
                transform_registrations_ts::{
                    transform_registrations_ts_infrastructure_redis,
                    transform_registrations_ts_infrastructure_s3,
                },
                transform_service_to_worker::transform_registrations_ts_service_to_worker,
                transform_test_utils_ts::{
                    transform_test_utils_add_database, transform_test_utils_add_infrastructure,
                    transform_test_utils_remove_database,
                    transform_test_utils_remove_infrastructure,
                },
            },
        },
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        database::{get_database_variants, is_in_memory_database},
        docker::{
            Command as DockerCommand, DependencyCondition, DependsOn, DockerCompose,
            add_database_to_docker_compose, add_redis_to_docker_compose, add_s3_to_docker_compose,
            clean_up_unused_infrastructure_services, remove_redis_from_docker_compose,
            remove_s3_from_docker_compose, update_dockerfile_contents,
        },
        env::Env,
        format::format_code,
        manifest::{
            InitializableManifestConfig, InitializableManifestConfigMetadata, ManifestData,
            MutableManifestData, ProjectInitializationMetadata, ProjectMetadata, ProjectType,
            service::ServiceManifestData,
        },
        move_template::{MoveTemplate, move_template_files},
        name::validate_name,
        package_json::{
            application_package_json::ApplicationPackageJson,
            package_json_constants::{INFRASTRUCTURE_REDIS_VERSION, INFRASTRUCTURE_S3_VERSION, IOREDIS_VERSION},
            project_package_json::ProjectPackageJson,
        },
        removal_template::{RemovalTemplate, remove_template_files},
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
    },
    prompt::{
        ArrayCompleter, prompt_comma_separated_list_from_selections,
        prompt_field_from_selections_with_validation,
    },
};

#[derive(Debug)]
pub(super) struct ServiceCommand;

impl ServiceCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

fn change_name(
    base_path: &Path,
    name: &str,
    confirm: bool,
    manifest_data: &mut ServiceManifestData,
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
        MutableManifestData::Service(manifest_data),
        application_package_json,
        project_package_json,
        Some(docker_compose),
        rendered_templates_cache,
        removal_templates,
        stdout,
    )
}

fn change_database(
    base_path: &Path,
    database: &Database,
    manifest_data: &mut ServiceManifestData,
    application_package_json: &mut ApplicationPackageJson,
    project_package_json: &mut ProjectPackageJson,
    docker_compose_data: &mut DockerCompose,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    removal_templates: &mut Vec<RemovalTemplate>,
) -> Result<()> {
    if manifest_data.database == database.to_string() {
        return Ok(());
    }

    let existing_database = manifest_data.database.parse::<Database>()?;
    manifest_data.database = database.to_string();

    let manifest_data_clone = manifest_data.clone();
    let project = manifest_data
        .projects
        .iter_mut()
        .find(|project_entry| {
            project_entry.name == base_path.file_name().unwrap().to_string_lossy()
        })
        .unwrap();
    project.resources.as_mut().unwrap().database = Some(database.to_string());

    project_package_json
        .dependencies
        .as_mut()
        .unwrap()
        .databases = HashSet::from([database.clone()]);

    let existing_docker_service = docker_compose_data.services.get(&project.name).unwrap();
    let mut existing_docker_service_environment = existing_docker_service
        .environment
        .as_ref()
        .unwrap()
        .clone();

    add_database_to_docker_compose(
        &ManifestData::Service(&manifest_data_clone),
        docker_compose_data,
        &mut existing_docker_service_environment,
    )?;

    let dockerfile_key = base_path.parent().unwrap().join("Dockerfile");
    let dockerfile = rendered_templates_cache.get(&dockerfile_key)?;
    if let Some(dockerfile_template) = dockerfile {
        rendered_templates_cache.insert(
            dockerfile_key.to_string_lossy(),
            RenderedTemplate {
                path: dockerfile_key.clone(),
                content: update_dockerfile_contents(
                    &dockerfile_template.content,
                    &manifest_data.runtime.parse()?,
                    is_in_memory_database(database),
                )?,
                context: None,
            },
        );
    }

    let docker_service = docker_compose_data.services.get_mut(&project.name).unwrap();

    docker_service.environment = Some(existing_docker_service_environment);

    let mut index_map = IndexMap::new();
    let depends_on = docker_service.depends_on.as_mut().unwrap_or(&mut index_map);
    for database in Database::VARIANTS {
        if depends_on.contains_key(&database.to_string()) {
            depends_on.shift_remove(&database.to_string());
        }
    }
    depends_on.insert(
        database.to_string(),
        DependsOn {
            condition: DependencyCondition::ServiceStarted,
        },
    );

    let projects = manifest_data.projects.clone();
    clean_up_unused_infrastructure_services(docker_compose_data, projects)?;

    let mikro_orm_config_path = base_path.join("mikro-orm.config.ts");
    rendered_templates_cache.insert(
        mikro_orm_config_path.to_string_lossy(),
        RenderedTemplate {
            path: mikro_orm_config_path.clone(),
            content: transform_mikroorm_config_ts(
                rendered_templates_cache,
                &base_path,
                &Some(existing_database),
                database,
            )?,
            context: None,
        },
    );

    // Update registrations.ts import source for database-specific package
    let registrations_path = base_path.join("registrations.ts");
    if let Some(template) = rendered_templates_cache.get(&registrations_path)? {
        let old_source = format!(
            "@mikro-orm/{}",
            existing_database.to_string().to_lowercase()
        );
        let new_source = format!(
            "@mikro-orm/{}",
            database.to_string().to_lowercase()
        );
        let new_content = template.content.replace(&old_source, &new_source);
        rendered_templates_cache.insert(
            registrations_path.to_string_lossy(),
            RenderedTemplate {
                path: registrations_path.clone(),
                content: new_content,
                context: None,
            },
        );
    }

    let env_local_path = base_path.join(".env.local");
    let env_local_content = rendered_templates_cache
        .get(&env_local_path)?
        .unwrap()
        .content;
    let mut env_local_content = serde_envfile::from_str::<Env>(&env_local_content)?;

    change_database_env_variables(
        &mut env_local_content,
        &manifest_data.app_name,
        &manifest_data.service_name,
        database,
    );

    change_database_postinstall_script(application_package_json, database);
    change_database_seed_script(project_package_json, database);
    change_database_retention_script(
        project_package_json,
        &manifest_data.runtime.parse()?,
        true, // changing database type means we have a database
    );

    let removal_template = change_database_base_entity(
        base_path,
        database,
        &existing_database,
        manifest_data.projects.clone(),
        &manifest_data.service_name,
        rendered_templates_cache,
    )?;

    if let Some(removal_template) = removal_template {
        removal_templates.push(removal_template);
    }

    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    if test_utils_path.exists() {
        let current_content = rendered_templates_cache.get(&test_utils_path)?;
        if current_content.is_some() || std::fs::read_to_string(&test_utils_path).is_ok() {
            let new_content =
                transform_test_utils_remove_database(rendered_templates_cache, &base_path)
                    .and_then(|_| {
                        transform_test_utils_add_database(
                            rendered_templates_cache,
                            &base_path,
                            database,
                        )
                    })?;
            rendered_templates_cache.insert(
                test_utils_path.to_string_lossy(),
                RenderedTemplate {
                    path: test_utils_path.clone(),
                    content: new_content,
                    context: None,
                },
            );
        }
    }

    Ok(())
}

fn change_description(
    description: &str,
    manifest_data: &mut ServiceManifestData,
    project_package_json: &mut ProjectPackageJson,
) -> Result<()> {
    change_description_core(
        description,
        MutableManifestData::Service(manifest_data),
        project_package_json,
    )
}

fn change_infrastructure(
    base_path: &Path,
    infrastructure_to_add: Vec<Infrastructure>,
    infrastructure_to_remove: Vec<Infrastructure>,
    project_package_json: &mut ProjectPackageJson,
    manifest_data: &mut ServiceManifestData,
    docker_compose: &mut DockerCompose,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<()> {
    for infrastructure in infrastructure_to_add {
        match infrastructure {
            Infrastructure::Redis => {
                let mut environment = docker_compose
                    .services
                    .get_mut(&manifest_data.service_name)
                    .unwrap()
                    .environment
                    .as_ref()
                    .unwrap()
                    .clone();
                add_redis_to_docker_compose(
                    &manifest_data.app_name,
                    docker_compose,
                    &mut environment,
                    0,
                )?;
                docker_compose
                    .services
                    .get_mut(&manifest_data.service_name)
                    .unwrap()
                    .environment = Some(environment);

                let env_local_path = base_path.join(".env.local");
                let mut env_local_content = serde_envfile::from_str::<Env>(
                    &rendered_templates_cache
                        .get(&env_local_path)?
                        .unwrap()
                        .content,
                )?;

                env_local_content.redis_url = Some("redis://localhost:6379".to_string());

                rendered_templates_cache.insert(
                    env_local_path.to_string_lossy(),
                    RenderedTemplate {
                        path: env_local_path.clone(),
                        content: serde_envfile::to_string(&env_local_content)?,
                        context: None,
                    },
                );

                let registrations_path = base_path.join("registrations.ts");
                rendered_templates_cache.insert(
                    registrations_path.to_string_lossy(),
                    RenderedTemplate {
                        path: registrations_path.clone(),
                        content: transform_registrations_ts_infrastructure_redis(
                            rendered_templates_cache,
                            &base_path,
                        )?,
                        context: None,
                    },
                );

                let test_utils_path = base_path.join("__test__").join("test-utils.ts");
                if test_utils_path.exists() {
                    rendered_templates_cache.insert(
                        test_utils_path.to_string_lossy(),
                        RenderedTemplate {
                            path: test_utils_path.clone(),
                            content: transform_test_utils_add_infrastructure(
                                rendered_templates_cache,
                                &base_path,
                                &Infrastructure::Redis,
                            )?,
                            context: None,
                        },
                    );
                }

                project_package_json
                    .dependencies
                    .as_mut()
                    .unwrap()
                    .forklaunch_infrastructure_redis =
                    Some(INFRASTRUCTURE_REDIS_VERSION.to_string());

                project_package_json
                    .dependencies
                    .as_mut()
                    .unwrap()
                    .ioredis = Some(IOREDIS_VERSION.to_string());

                manifest_data.projects.iter_mut().for_each(|project| {
                    if project.name == manifest_data.service_name {
                        project.resources.as_mut().unwrap().cache =
                            Some(Infrastructure::Redis.to_string());
                    }
                });
            }
            Infrastructure::S3 => {
                let mut environment = docker_compose
                    .services
                    .get_mut(&manifest_data.service_name)
                    .unwrap()
                    .environment
                    .as_ref()
                    .unwrap()
                    .clone();
                add_s3_to_docker_compose(
                    &manifest_data.app_name,
                    &manifest_data.service_name,
                    docker_compose,
                    &mut environment,
                )?;
                docker_compose
                    .services
                    .get_mut(&manifest_data.service_name)
                    .unwrap()
                    .environment = Some(environment);

                let env_local_path = base_path.join(".env.local");
                let mut env_local_content = serde_envfile::from_str::<Env>(
                    &rendered_templates_cache
                        .get(&env_local_path)?
                        .unwrap()
                        .content,
                )?;

                env_local_content.s3_url = Some("http://localhost:9000".to_string());
                env_local_content.s3_bucket = Some(format!(
                    "{}-{}-dev",
                    manifest_data.app_name, manifest_data.service_name
                ));
                env_local_content.s3_region = Some("us-east-1".to_string());
                env_local_content.s3_access_key = Some("minioadmin".to_string());
                env_local_content.s3_secret_key = Some("minioadmin".to_string());

                rendered_templates_cache.insert(
                    env_local_path.to_string_lossy(),
                    RenderedTemplate {
                        path: env_local_path.clone(),
                        content: serde_envfile::to_string(&env_local_content)?,
                        context: None,
                    },
                );

                let registrations_path = base_path.join("registrations.ts");
                rendered_templates_cache.insert(
                    registrations_path.to_string_lossy(),
                    RenderedTemplate {
                        path: registrations_path.clone(),
                        content: transform_registrations_ts_infrastructure_s3(
                            rendered_templates_cache,
                            &base_path,
                        )?,
                        context: None,
                    },
                );

                let test_utils_path = base_path.join("__test__").join("test-utils.ts");
                if test_utils_path.exists() {
                    rendered_templates_cache.insert(
                        test_utils_path.to_string_lossy(),
                        RenderedTemplate {
                            path: test_utils_path.clone(),
                            content: transform_test_utils_add_infrastructure(
                                rendered_templates_cache,
                                &base_path,
                                &Infrastructure::S3,
                            )?,
                            context: None,
                        },
                    );
                }

                project_package_json
                    .dependencies
                    .as_mut()
                    .unwrap()
                    .forklaunch_infrastructure_s3 = Some(INFRASTRUCTURE_S3_VERSION.to_string());

                manifest_data.projects.iter_mut().for_each(|project| {
                    if project.name == manifest_data.service_name {
                        project.resources.as_mut().unwrap().object_store =
                            Some(Infrastructure::S3.to_string());
                    }
                });
            }
        }
    }

    for infrastructure in infrastructure_to_remove {
        match infrastructure {
            Infrastructure::Redis => {
                if !manifest_data.projects.iter_mut().any(|project| {
                    if let Some(resources) = &mut project.resources {
                        if let Some(cache) = &mut resources.cache {
                            cache == &Infrastructure::Redis.to_string()
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }) {
                    let mut environment = docker_compose
                        .services
                        .get_mut(&manifest_data.service_name)
                        .unwrap()
                        .environment
                        .as_ref()
                        .unwrap()
                        .clone();
                    remove_redis_from_docker_compose(docker_compose, &mut environment)?;
                    docker_compose
                        .services
                        .get_mut(&manifest_data.service_name)
                        .unwrap()
                        .environment = Some(environment);
                }

                let env_local_path = base_path.join(".env.local");
                let mut env_local_content = serde_envfile::from_str::<Env>(
                    &rendered_templates_cache
                        .get(&env_local_path)?
                        .unwrap()
                        .content,
                )?;

                env_local_content.redis_url = None;

                rendered_templates_cache.insert(
                    env_local_path.to_string_lossy(),
                    RenderedTemplate {
                        path: env_local_path.clone(),
                        content: serde_envfile::to_string(&env_local_content)?,
                        context: None,
                    },
                );

                let registrations_path = base_path.join("registrations.ts");
                rendered_templates_cache.insert(
                    registrations_path.to_string_lossy(),
                    RenderedTemplate {
                        path: registrations_path.clone(),
                        content: delete_from_registrations_ts_infrastructure_redis(
                            &base_path,
                            rendered_templates_cache
                                .get(&registrations_path)?
                                .unwrap()
                                .content,
                        )?,
                        context: None,
                    },
                );

                let test_utils_path = base_path.join("__test__").join("test-utils.ts");
                if test_utils_path.exists() {
                    rendered_templates_cache.insert(
                        test_utils_path.to_string_lossy(),
                        RenderedTemplate {
                            path: test_utils_path.clone(),
                            content: transform_test_utils_remove_infrastructure(
                                rendered_templates_cache,
                                &base_path,
                                &Infrastructure::Redis,
                            )?,
                            context: None,
                        },
                    );
                }

                project_package_json
                    .dependencies
                    .as_mut()
                    .unwrap()
                    .forklaunch_infrastructure_redis = None;

                project_package_json
                    .dependencies
                    .as_mut()
                    .unwrap()
                    .ioredis = None;

                manifest_data.projects.iter_mut().for_each(|project| {
                    if project.name == manifest_data.service_name {
                        project.resources.as_mut().unwrap().cache = None;
                    }
                });
            }
            Infrastructure::S3 => {
                if !manifest_data.projects.iter_mut().any(|project| {
                    if let Some(resources) = &mut project.resources {
                        if let Some(object_store) = &mut resources.object_store {
                            object_store == &Infrastructure::S3.to_string()
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }) {
                    let mut environment = docker_compose
                        .services
                        .get_mut(&manifest_data.service_name)
                        .unwrap()
                        .environment
                        .as_ref()
                        .unwrap()
                        .clone();
                    remove_s3_from_docker_compose(docker_compose, &mut environment)?;
                    docker_compose
                        .services
                        .get_mut(&manifest_data.service_name)
                        .unwrap()
                        .environment = Some(environment);
                }

                let env_local_path = base_path.join(".env.local");
                let mut env_local_content = serde_envfile::from_str::<Env>(
                    &rendered_templates_cache
                        .get(&env_local_path)?
                        .unwrap()
                        .content,
                )?;

                env_local_content.s3_bucket = None;
                env_local_content.s3_url = None;
                env_local_content.s3_region = None;
                env_local_content.s3_access_key = None;
                env_local_content.s3_secret_key = None;

                rendered_templates_cache.insert(
                    env_local_path.to_string_lossy(),
                    RenderedTemplate {
                        path: env_local_path.clone(),
                        content: serde_envfile::to_string(&env_local_content)?,
                        context: None,
                    },
                );

                let registrations_path = base_path.join("registrations.ts");
                rendered_templates_cache.insert(
                    registrations_path.to_string_lossy(),
                    RenderedTemplate {
                        path: registrations_path.clone(),
                        content: delete_from_registrations_ts_infrastructure_s3(
                            &base_path,
                            rendered_templates_cache
                                .get(&registrations_path)?
                                .unwrap()
                                .content,
                        )?,
                        context: None,
                    },
                );

                let test_utils_path = base_path.join("__test__").join("test-utils.ts");
                if test_utils_path.exists() {
                    rendered_templates_cache.insert(
                        test_utils_path.to_string_lossy(),
                        RenderedTemplate {
                            path: test_utils_path.clone(),
                            content: transform_test_utils_remove_infrastructure(
                                rendered_templates_cache,
                                &base_path,
                                &Infrastructure::S3,
                            )?,
                            context: None,
                        },
                    );
                }

                project_package_json
                    .dependencies
                    .as_mut()
                    .unwrap()
                    .forklaunch_infrastructure_s3 = None;

                manifest_data.projects.iter_mut().for_each(|project| {
                    if project.name == manifest_data.service_name {
                        project.resources.as_mut().unwrap().object_store = None;
                    }
                });
            }
        }
    }

    clean_up_unused_infrastructure_services(docker_compose, manifest_data.projects.clone())?;

    Ok(())
}

/// Convert a service to a worker
/// This adds worker.ts, event entity, updates registrations.ts, package.json, env vars, and manifest
#[allow(clippy::too_many_arguments)]
fn service_to_worker(
    base_path: &Path,
    worker_type: &WorkerType,
    runtime: &crate::constants::Runtime,
    manifest_data: &mut ServiceManifestData,
    project_package_json: &mut ProjectPackageJson,
    docker_compose: &mut DockerCompose,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    stdout: &mut StandardStream,
) -> Result<()> {
    let project_name = base_path.file_name().unwrap().to_string_lossy().to_string();
    let pascal_case_name = project_name.to_case(Case::Pascal);
    let camel_case_name = project_name.to_case(Case::Camel);

    let registrations_path = base_path.join("registrations.ts");
    rendered_templates_cache.insert(
        registrations_path.to_string_lossy(),
        RenderedTemplate {
            path: registrations_path.clone(),
            content: transform_registrations_ts_service_to_worker(
                rendered_templates_cache,
                base_path,
                &manifest_data.app_name,
                &project_name,
                worker_type,
            )?,
            context: None,
        },
    );

    // Detect naming convention from existing registrations file
    let registrations_content = std::fs::read_to_string(base_path.join("registrations.ts"))
        .unwrap_or_default();
    let otel_token = if registrations_content.contains("OtelCollector:") {
        "OtelCollector"
    } else {
        "OpenTelemetryCollector"
    };

    let worker_ts_path = base_path.join("worker.ts");
    let worker_ts_content = format!(
        r#"import {{ ci, tokens }} from './bootstrapper';
import {{ processEvents, processErrors }} from './services/{camel_case_name}.service';

/**
 * Creates an instance of OpenTelemetryCollector
 */
const openTelemetryCollector = ci.resolve(tokens.{otel_token});

/**
 * Main worker entry point
 */
(async () => {{
  openTelemetryCollector.info('Starting {pascal_case_name} Worker...');

  const workerConsumer = ci.resolve(tokens.WorkerConsumer);
  const consumer = workerConsumer(processEvents, processErrors);

  await consumer.start();

  openTelemetryCollector.info('🎉 {pascal_case_name} Worker is running! 🎉');
}})();
"#,
        camel_case_name = camel_case_name,
        pascal_case_name = pascal_case_name
    );
    rendered_templates_cache.insert(
        worker_ts_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: worker_ts_path.clone(),
            content: worker_ts_content,
            context: None,
        },
    );

    let entities_dir = base_path.join("persistence").join("entities");
    let event_entity_path = entities_dir.join(format!("{}EventRecord.entity.ts", camel_case_name));
    let is_mongo = manifest_data.database == "mongodb";
    let event_entity_content = format!(
        r#"import {{ defineEntity, p }} from '@mikro-orm/core';
import {{ {mongo_prefix}sqlBaseProperties }} from '@{app_name}/core';

export const {pascal_case_name}EventRecord = defineEntity({{
  name: '{pascal_case_name}EventRecord',
  properties: {{
    ...{mongo_prefix}sqlBaseProperties,
    message: p.string(),
    processed: p.boolean(),
    retryCount: p.integer(),
  }},
}});
"#,
        mongo_prefix = if is_mongo { "no" } else { "" },
        app_name = manifest_data.app_name,
        pascal_case_name = pascal_case_name
    );
    rendered_templates_cache.insert(
        event_entity_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: event_entity_path.clone(),
            content: event_entity_content,
            context: None,
        },
    );

    // Generate the event record interface type file
    let types_dir = base_path.join("domain").join("types");
    let event_record_types_path =
        types_dir.join(format!("{}EventRecord.types.ts", camel_case_name));
    let event_record_types_content = format!(
        r#"import type {{ WorkerEventEntity }} from '@forklaunch/interfaces-worker/types';

export interface {pascal_case_name}EventRecord extends WorkerEventEntity {{
  message: string;
  createdAt: Date;
  updatedAt: Date;
}}
"#,
        pascal_case_name = pascal_case_name
    );
    rendered_templates_cache.insert(
        event_record_types_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: event_record_types_path.clone(),
            content: event_record_types_content,
            context: None,
        },
    );

    let env_local_path = base_path.join(".env.local");
    let mut env_local_content = serde_envfile::from_str::<Env>(
        &rendered_templates_cache
            .get(&env_local_path)?
            .unwrap()
            .content,
    )?;
    env_local_content.queue_name =
        Some(format!("{}-{}-queue", manifest_data.app_name, project_name));
    rendered_templates_cache.insert(
        env_local_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: env_local_path.clone(),
            content: serde_envfile::to_string(&env_local_content)?,
            context: None,
        },
    );

    use crate::core::package_json::package_json_constants::{
        project_start_server_script, project_start_worker_script, WORKER_BULLMQ_VERSION,
        WORKER_DATABASE_VERSION, WORKER_INTERFACES_VERSION, WORKER_KAFKA_VERSION,
        WORKER_REDIS_VERSION,
    };

    let database = manifest_data.database.parse::<Database>().ok();
    let scripts = project_package_json.scripts.as_mut().unwrap();
    scripts.dev_worker = Some(match runtime {
        crate::constants::Runtime::Node => {
            "tsx watch --clear-screen=false ./worker.ts | pino-pretty".to_string()
        }
        crate::constants::Runtime::Bun => {
            "bun --watch ./worker.ts | pino-pretty".to_string()
        }
    });
    scripts.start_server = Some(project_start_server_script(runtime, database));
    scripts.start_worker = Some(project_start_worker_script(runtime, database));

    let deps = project_package_json.dependencies.as_mut().unwrap();
    deps.forklaunch_interfaces_worker = Some(WORKER_INTERFACES_VERSION.to_string());
    match worker_type {
        WorkerType::BullMQCache => {
            deps.forklaunch_implementation_worker_bullmq = Some(WORKER_BULLMQ_VERSION.to_string());
        }
        WorkerType::Database => {
            deps.forklaunch_implementation_worker_database =
                Some(WORKER_DATABASE_VERSION.to_string());
        }
        WorkerType::RedisCache => {
            deps.forklaunch_implementation_worker_redis = Some(WORKER_REDIS_VERSION.to_string());
        }
        WorkerType::Kafka => {
            deps.forklaunch_implementation_worker_kafka = Some(WORKER_KAFKA_VERSION.to_string());
        }
    }

    manifest_data.projects.iter_mut().for_each(|project| {
        if project.name == project_name {
            project.r#type = ProjectType::Worker;
            if let Some(metadata) = project.metadata.as_mut() {
                metadata.r#type = Some(worker_type.to_string());
            } else {
                project.metadata = Some(ProjectMetadata {
                    r#type: Some(worker_type.to_string()),
                    hosting_type: None,
                });
            }
        }
    });

    let server_ts_path = base_path.join("server.ts");
    if let Some(server_template) = rendered_templates_cache.get(&server_ts_path)? {
        let content = format!(
            "// TODO: Review - This service has been converted to a worker.\n// You may want to remove or modify the HTTP server and routes below.\n// The worker logic is now in worker.ts\n\n{}",
            server_template.content
        );
        rendered_templates_cache.insert(
            server_ts_path.to_string_lossy(),
            RenderedTemplate {
                path: server_ts_path.clone(),
                content,
                context: None,
            },
        );
    }

    let docker_service_name = format!("{}-worker", project_name);
    if let Some(server_service) = docker_compose.services.get(&project_name).cloned() {
        let mut worker_service = server_service.clone();
        let worker_command = match runtime {
            crate::constants::Runtime::Node => "npm run start:worker",
            crate::constants::Runtime::Bun => "bun run start:worker",
        };
        worker_service.command = Some(DockerCommand::Simple(worker_command.to_string()));
        docker_compose
            .services
            .insert(docker_service_name, worker_service);
    }

    let readme_path = base_path.join("README-MIGRATION.md");
    let readme_content = format!(
        r#"# Service to Worker Migration

This service has been converted to a worker of type `{worker_type}`.

## Changes Made

1. **worker.ts** - New worker entry point created
2. **{camel_case_name}EventRecord.entity.ts** - Event record entity for worker processing
3. **registrations.ts** - Updated with WorkerConsumer, WorkerProducer, and WorkerOptions
4. **package.json** - Added `dev:worker` and `start:worker` scripts
5. **.env.local** - Added `QUEUE_NAME` environment variable
6. **docker-compose.yaml** - Added `{project_name}-worker` service

## Next Steps

1. **Review server.ts** - Decide if you still need the HTTP server
   - If yes, keep it for API endpoints alongside the worker
   - If no, delete server.ts and remove `dev:server`/`start:server` scripts

2. **Implement worker logic** - Update `processEvents` and `processErrors` in your service

3. **Run migrations** - If using Database worker type:
   ```bash
   pnpm migrate:create
   pnpm migrate:up
   ```

4. **Install dependencies**:
   ```bash
   pnpm install
   ```

5. **Start the worker**:
   ```bash
   pnpm dev:worker
   ```
"#,
        worker_type = worker_type.to_string(),
        camel_case_name = camel_case_name,
        project_name = project_name
    );
    rendered_templates_cache.insert(
        readme_path.to_string_lossy().to_string(),
        RenderedTemplate {
            path: readme_path.clone(),
            content: readme_content,
            context: None,
        },
    );

    log_warn!(stdout, "Service converted to worker. See README-MIGRATION.md for next steps.");

    Ok(())
}

impl CliCommand for ServiceCommand {
    fn command(&self) -> Command {
        command("service", "Change a forklaunch service")
            .alias("svc")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The service path"),
            )
            .arg(Arg::new("name").short('N').help("The name of the service"))
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
                Arg::new("infrastructure")
                    .short('i')
                    .long("infrastructure")
                    .help("The infrastructure to use")
                    .value_parser(Infrastructure::VARIANTS)
                    .num_args(0..)
                    .action(ArgAction::Append),
            )
            .arg(
                Arg::new("to")
                    .long("to")
                    .help("Convert service to another type (worker)")
                    .value_parser(["worker"]),
            )
            .arg(
                Arg::new("type")
                    .short('t')
                    .long("type")
                    .help("The worker type to use (required when --to worker)")
                    .value_parser(WorkerType::VARIANTS),
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

        let existing_manifest_data = toml::from_str::<ServiceManifestData>(
            &rendered_templates_cache
                .get(&manifest_path)
                .with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?
                .unwrap()
                .content,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let service_base_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Service(&existing_manifest_data),
            &project_name,
            &mut line_editor,
            &mut stdout,
            matches,
            1,
        )?;

        let mut manifest_data = existing_manifest_data.initialize(
            InitializableManifestConfigMetadata::Project(ProjectInitializationMetadata {
                project_name: service_base_path
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

        let project = manifest_data
            .projects
            .iter_mut()
            .find(|project| project.name == manifest_data.service_name)
            .unwrap();

        let name = matches.get_one::<String>("name");
        let database = matches.get_one::<String>("database");
        let description = matches.get_one::<String>("description");
        let infrastructure = matches
            .get_many::<String>("infrastructure")
            .map(|v| v.map(|s| s.to_string()).collect());
        let to = matches.get_one::<String>("to");
        let worker_type_str = matches.get_one::<String>("type");
        let dryrun = matches.get_flag("dryrun");
        let confirm = matches.get_flag("confirm");

        if let Some(to_type) = to {
            if to_type == "worker" {
                let worker_type = if let Some(t) = worker_type_str {
                    t.parse::<WorkerType>()?
                } else {
                    let type_variants = WorkerType::VARIANTS;
                    let selected_type = prompt_field_from_selections_with_validation(
                        "type",
                        None,
                        &["type"],
                        &mut line_editor,
                        &mut stdout,
                        matches,
                        "worker type",
                        Some(type_variants.as_slice()),
                        |input: &str| type_variants.contains(&input),
                        |_| "Invalid worker type. Please try again".to_string(),
                    )?;
                    selected_type
                        .ok_or_else(|| anyhow::Error::msg("Worker type is required"))?
                        .parse::<WorkerType>()?
                };

                let mut project_package_json_to_write = serde_json::from_str::<ProjectPackageJson>(
                    &rendered_templates_cache
                        .get(&service_base_path.join("package.json"))
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

                service_to_worker(
                    &service_base_path,
                    &worker_type,
                    &runtime,
                    &mut manifest_data,
                    &mut project_package_json_to_write,
                    &mut docker_compose_data,
                    &mut rendered_templates_cache,
                    &mut stdout,
                )?;

                // Write modified files
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
                    service_base_path
                        .join("package.json")
                        .to_string_lossy()
                        .to_string(),
                    RenderedTemplate {
                        path: service_base_path.join("package.json"),
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
            let options = vec!["name", "database", "description", "infrastructure"];

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
            "service name",
            None,
            |input: &str| validate_name(input) && !manifest_data.app_name.contains(input),
            |_| {
                "Service name cannot be a substring of the application name, empty or include numbers or spaces. Please try again"
                    .to_string()
            },
        )?;

        let database_variants = get_database_variants(&runtime);
        let database = prompt_field_from_selections_with_validation(
            "database",
            database,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "database",
            Some(database_variants),
            |input: &str| database_variants.contains(&input),
            |_| "Invalid database. Please try again".to_string(),
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

        let mut active_infrastructure = vec![];
        let project_resources = project.resources.as_ref().unwrap();

        if let Some(cache) = &project_resources.cache {
            active_infrastructure.push(cache.to_string());
        }
        if let Some(queue) = &project_resources.queue {
            active_infrastructure.push(queue.to_string());
        }
        if let Some(object_store) = &project_resources.object_store {
            active_infrastructure.push(object_store.to_string());
        }

        let infrastructure = prompt_comma_separated_list_from_selections(
            "infrastructure",
            infrastructure,
            &selected_options,
            &mut line_editor,
            matches,
            "infrastructure",
            &Infrastructure::VARIANTS,
            Some(
                active_infrastructure
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>()
                    .as_slice(),
            ),
        )?;

        let mut removal_templates = vec![];
        let mut move_templates = vec![];

        let application_package_json_path =
            service_base_path.parent().unwrap().join("package.json");
        let application_package_json_data = rendered_templates_cache
            .get(&application_package_json_path)
            .with_context(|| ERROR_FAILED_TO_READ_PACKAGE_JSON)?
            .unwrap()
            .content;

        let mut application_package_json_to_write =
            serde_json::from_str::<ApplicationPackageJson>(&application_package_json_data)?;

        let project_package_json_path = service_base_path.join("package.json");
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

        if let Some(database) = database {
            change_database(
                &service_base_path,
                &database.parse()?,
                &mut manifest_data,
                &mut application_package_json_to_write,
                &mut project_json_to_write,
                &mut docker_compose_data,
                &mut rendered_templates_cache,
                &mut removal_templates,
            )?;
            log_warn!(stdout, "migrate:init or migrate:create will need to be run");
        }

        if let Some(description) = description {
            change_description(&description, &mut manifest_data, &mut project_json_to_write)?;
        }

        if let Some(infrastructure) = infrastructure {
            let infrastructure_to_add: Vec<Infrastructure> = infrastructure
                .iter()
                .filter(|s| !active_infrastructure.contains(s))
                .map(|s| s.parse::<Infrastructure>().unwrap())
                .collect();

            let infrastructure_to_remove: Vec<Infrastructure> = active_infrastructure
                .iter()
                .filter(|s| !infrastructure.contains(s))
                .map(|s| s.parse::<Infrastructure>().unwrap())
                .collect();

            change_infrastructure(
                &service_base_path,
                infrastructure_to_add,
                infrastructure_to_remove,
                &mut project_json_to_write,
                &mut manifest_data,
                &mut docker_compose_data,
                &mut rendered_templates_cache,
            )?;
        }

        if let Some(name) = name {
            move_templates.push(change_name(
                &service_base_path,
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
            log_ok!(stdout, "{} changed successfully!", &manifest_data.service_name);
            format_code(&service_base_path, &manifest_data.runtime.parse()?);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, write};

    use indexmap::IndexMap;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        constants::Infrastructure,
        core::{
            docker::DockerService,
            manifest::{
                InitializableManifestConfig, InitializableManifestConfigMetadata,
                ProjectInitializationMetadata,
            },
            package_json::{
                application_package_json::{ApplicationPackageJson, ApplicationScripts},
                project_package_json::{ProjectDependencies, ProjectPackageJson, ProjectScripts},
            },
            rendered_template::RenderedTemplatesCache,
        },
    };

    const MINIMAL_REGISTRATIONS_TS: &str = r#"import {
  number,
  schemaValidator,
  string
} from '@test-app/core';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: 'test-svc', version: '0.1.0' }
  }
});

const environmentConfig = configInjector.chain({
  HOST: { lifetime: Lifetime.Singleton, type: string, value: getEnvVar('HOST') },
  PORT: { lifetime: Lifetime.Singleton, type: number, value: Number(getEnvVar('PORT')) }
});

const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => new MikroORM(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(OTEL_SERVICE_NAME, OTEL_LEVEL || 'info')
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  }
});

const serviceDependencies = runtimeDependencies.chain({});
"#;

    const MANIFEST_TOML: &str = r#"
id = "test-id"
cli_version = "0.6.3"
app_name = "test-app"
modules_path = "src/modules"
app_description = "test"
linter = "eslint"
formatter = "prettier"
validator = "zod"
http_framework = "express"
runtime = "node"
author = "test"
license = "MIT"

[project_peer_topology]

[[projects]]
type = "Service"
name = "test-svc"
description = "test service"

[projects.resources]
database = "postgresql"
"#;

    fn setup() -> (
        TempDir,
        ServiceManifestData,
        DockerCompose,
        ProjectPackageJson,
        RenderedTemplatesCache,
    ) {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path();

        write(base.join("registrations.ts"), MINIMAL_REGISTRATIONS_TS).unwrap();
        write(base.join(".env.local"), "").unwrap();

        let raw: ServiceManifestData = toml::from_str(MANIFEST_TOML).unwrap();
        let manifest =
            raw.initialize(InitializableManifestConfigMetadata::Project(
                ProjectInitializationMetadata {
                    project_name: "test-svc".to_string(),
                    database: None,
                    infrastructure: None,
                    description: None,
                    worker_type: None,
                },
            ));

        let mut docker_compose = DockerCompose::default();
        docker_compose.services.insert(
            "test-svc".to_string(),
            DockerService {
                environment: Some(IndexMap::new()),
                ..Default::default()
            },
        );

        let pkg_json = ProjectPackageJson {
            dependencies: Some(ProjectDependencies::default()),
            ..Default::default()
        };

        let cache = RenderedTemplatesCache::new();

        (tmp, manifest, docker_compose, pkg_json, cache)
    }

    #[test]
    fn test_change_infrastructure_add_s3_updates_docker_compose_manifest_and_deps() {
        let (_tmp, mut manifest, mut docker, mut pkg, mut cache) = setup();
        let base = _tmp.path();

        change_infrastructure(
            base,
            vec![Infrastructure::S3],
            vec![],
            &mut pkg,
            &mut manifest,
            &mut docker,
            &mut cache,
        )
        .expect("change_infrastructure should succeed");

        // 1. docker-compose must have minio service
        assert!(
            docker.services.contains_key("minio"),
            "Expected 'minio' in docker-compose services, got: {:?}",
            docker.services.keys().collect::<Vec<_>>()
        );

        // 2. service environment must have S3 env vars injected
        let env = docker.services["test-svc"].environment.as_ref().unwrap();
        assert!(env.contains_key("S3_URL"), "Expected S3_URL in service environment");
        assert!(env.contains_key("S3_BUCKET"), "Expected S3_BUCKET in service environment");

        // 3. manifest object_store must be recorded as "s3"
        let project = manifest
            .projects
            .iter()
            .find(|p| p.name == "test-svc")
            .unwrap();
        assert_eq!(
            project.resources.as_ref().unwrap().object_store,
            Some("s3".to_string()),
            "Expected manifest object_store = 's3'"
        );

        // 4. package.json S3 dependency must be set
        assert!(
            pkg.dependencies
                .as_ref()
                .unwrap()
                .forklaunch_infrastructure_s3
                .is_some(),
            "Expected @forklaunch/infrastructure-s3 in package.json dependencies"
        );

        // 5. registrations.ts in cache must have S3 content injected
        let reg = cache
            .get(base.join("registrations.ts"))
            .unwrap()
            .unwrap();
        assert!(
            reg.content.contains("S3ObjectStore") || reg.content.contains("s3Url"),
            "Expected S3 import/usage in registrations.ts cache"
        );
    }

    #[test]
    fn test_change_infrastructure_remove_s3_cleans_up_docker_compose_and_manifest() {
        let (_tmp, mut manifest, mut docker, mut pkg, mut cache) = setup();
        let base = _tmp.path();

        // First add S3
        change_infrastructure(
            base,
            vec![Infrastructure::S3],
            vec![],
            &mut pkg,
            &mut manifest,
            &mut docker,
            &mut cache,
        )
        .expect("add S3 should succeed");

        assert!(
            docker.services.contains_key("minio"),
            "Pre-condition: minio must exist after add"
        );

        // Now remove S3 — reuse the same cache so it can read updated registrations.ts
        change_infrastructure(
            base,
            vec![],
            vec![Infrastructure::S3],
            &mut pkg,
            &mut manifest,
            &mut docker,
            &mut cache,
        )
        .expect("remove S3 should succeed");

        // 1. minio must be removed from docker-compose
        assert!(
            !docker.services.contains_key("minio"),
            "Expected 'minio' to be removed from docker-compose after S3 removal"
        );

        // 2. manifest object_store must be cleared
        let project = manifest
            .projects
            .iter()
            .find(|p| p.name == "test-svc")
            .unwrap();
        assert!(
            project.resources.as_ref().unwrap().object_store.is_none(),
            "Expected object_store = None after S3 removal"
        );

        // 3. package.json S3 dependency must be cleared
        assert!(
            pkg.dependencies
                .as_ref()
                .unwrap()
                .forklaunch_infrastructure_s3
                .is_none(),
            "Expected @forklaunch/infrastructure-s3 to be removed from dependencies"
        );
    }

    const POSTGRESQL_MIKRO_ORM_CONFIG: &str = r#"import { createConfigInjector, getEnvVar, Lifetime } from '@forklaunch/core/services';
import { Migrator } from '@mikro-orm/migrations';
import { number, string } from '@test-app/core';
import { defineConfig } from '@mikro-orm/postgresql';

const configInjector = createConfigInjector(schemaValidator, {
  DB_NAME: { lifetime: Lifetime.Singleton, type: string, value: getEnvVar('DB_NAME') },
  DB_HOST: { lifetime: Lifetime.Singleton, type: string, value: getEnvVar('DB_HOST') },
  DB_USER: { lifetime: Lifetime.Singleton, type: string, value: getEnvVar('DB_USER') },
  DB_PASSWORD: { lifetime: Lifetime.Singleton, type: string, value: getEnvVar('DB_PASSWORD') },
  DB_PORT: { lifetime: Lifetime.Singleton, type: number, value: Number(getEnvVar('DB_PORT')) }
});

const mikroOrmOptionsConfig = defineConfig({
  dbName: validConfigInjector.resolve('DB_NAME'),
  host: validConfigInjector.resolve('DB_HOST'),
  user: validConfigInjector.resolve('DB_USER'),
  password: validConfigInjector.resolve('DB_PASSWORD'),
  port: validConfigInjector.resolve('DB_PORT'),
  driver: PostgreSqlDriver,
  migrations: { path: 'dist/migrations-postgresql', pathTs: 'migrations-postgresql' }
});
"#;

    const MANIFEST_WITH_POSTGRESQL: &str = r#"
id = "test-id"
cli_version = "0.6.3"
app_name = "test-app"
modules_path = "src/modules"
app_description = "test"
linter = "eslint"
formatter = "prettier"
validator = "zod"
http_framework = "express"
runtime = "node"
author = "test"
license = "MIT"

[project_peer_topology]

[[projects]]
type = "Service"
name = "test-svc"
description = "test service"

[projects.resources]
database = "postgresql"
"#;

    fn setup_change_database() -> (
        TempDir,
        ServiceManifestData,
        DockerCompose,
        ApplicationPackageJson,
        ProjectPackageJson,
    ) {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("test-svc");
        create_dir_all(&base).unwrap();
        create_dir_all(base.join("persistence").join("entities")).unwrap();

        // Create the sql base properties file so transform_base_entity_ts returns None early
        // (no actual base entity transformation needed for sql→sql tests)
        let core_persistence = tmp.path().join("core").join("persistence");
        create_dir_all(&core_persistence).unwrap();
        write(
            core_persistence.join("sql.base.properties.ts"),
            "import { p } from '@mikro-orm/core';\nexport const sqlBaseProperties = { id: p.uuid().primary() };\n",
        )
        .unwrap();

        write(base.join("mikro-orm.config.ts"), POSTGRESQL_MIKRO_ORM_CONFIG).unwrap();
        write(base.join(".env.local"), "").unwrap();
        write(
            base.join("registrations.ts"),
            "import { EntityManager } from '@mikro-orm/postgresql';\n",
        )
        .unwrap();

        let raw: ServiceManifestData = toml::from_str(MANIFEST_WITH_POSTGRESQL).unwrap();
        let manifest = raw.initialize(InitializableManifestConfigMetadata::Project(
            ProjectInitializationMetadata {
                project_name: "test-svc".to_string(),
                database: None,
                infrastructure: None,
                description: None,
                worker_type: None,
            },
        ));

        let mut docker_compose = DockerCompose::default();
        docker_compose.services.insert(
            "test-svc".to_string(),
            DockerService {
                environment: Some(IndexMap::new()),
                depends_on: Some(IndexMap::new()),
                ..Default::default()
            },
        );

        let app_pkg = ApplicationPackageJson {
            scripts: Some(ApplicationScripts::default()),
            ..Default::default()
        };

        let project_pkg = ProjectPackageJson {
            dependencies: Some(ProjectDependencies::default()),
            scripts: Some(ProjectScripts::default()),
            ..Default::default()
        };

        (tmp, manifest, docker_compose, app_pkg, project_pkg)
    }

    #[test]
    fn test_change_database_postgresql_to_mysql_updates_manifest_and_mikroorm_config() {
        let (tmp, mut manifest, mut docker, mut app_pkg, mut project_pkg) =
            setup_change_database();
        let base = tmp.path().join("test-svc");
        let mut removal_templates = Vec::new();
        let mut cache = RenderedTemplatesCache::new();

        change_database(
            &base,
            &Database::MySQL,
            &mut manifest,
            &mut app_pkg,
            &mut project_pkg,
            &mut docker,
            &mut cache,
            &mut removal_templates,
        )
        .expect("change_database should succeed");

        // 1. manifest database must be updated
        assert_eq!(manifest.database, "mysql");
        let project = manifest.projects.iter().find(|p| p.name == "test-svc").unwrap();
        assert_eq!(
            project.resources.as_ref().unwrap().database,
            Some("mysql".to_string())
        );

        // 2. mikro-orm.config.ts in cache must reference mysql
        let config = cache
            .get(base.join("mikro-orm.config.ts"))
            .unwrap()
            .unwrap();
        assert!(
            config.content.contains("@mikro-orm/mysql"),
            "Expected @mikro-orm/mysql in mikro-orm.config.ts: {}",
            config.content
        );
        assert!(
            !config.content.contains("@mikro-orm/postgresql"),
            "Expected @mikro-orm/postgresql removed: {}",
            config.content
        );

        // 3. registrations.ts import source must be updated
        let reg = cache
            .get(base.join("registrations.ts"))
            .unwrap()
            .unwrap();
        assert!(
            reg.content.contains("@mikro-orm/mysql"),
            "Expected @mikro-orm/mysql in registrations.ts: {}",
            reg.content
        );
        assert!(
            !reg.content.contains("@mikro-orm/postgresql"),
            "Expected @mikro-orm/postgresql removed from registrations.ts: {}",
            reg.content
        );

        // 4. docker-compose depends_on must reference mysql
        let svc = docker.services.get("test-svc").unwrap();
        let depends_on = svc.depends_on.as_ref().unwrap();
        assert!(
            depends_on.contains_key("mysql"),
            "Expected mysql in depends_on: {:?}",
            depends_on.keys().collect::<Vec<_>>()
        );
    }
}

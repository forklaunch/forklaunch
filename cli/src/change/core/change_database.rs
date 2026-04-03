use std::{collections::HashSet, path::Path};

use anyhow::Result;
use serde_json::{from_str, to_string_pretty};
use walkdir::WalkDir;

use crate::{
    constants::Database,
    core::{
        ast::transformations::transform_base_entity_ts::transform_base_entity_ts,
        database::{get_database_port, get_postinstall_script, is_in_memory_database},
        env::Env,
        manifest::ProjectEntry,
        package_json::{
            application_package_json::ApplicationPackageJson,
            package_json_constants::{
                MIKRO_ORM_DATABASE_VERSION, PROJECT_SEED_SCRIPT,
                project_retention_enforce_script,
            },
            project_package_json::ProjectPackageJson,
        },
        removal_template::RemovalTemplate,
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, TEMPLATES_DIR},
        watermark::apply_watermark,
    },
};

pub(crate) fn change_database_base_entity(
    base_path: &Path,
    database: &Database,
    existing_database: &Database,
    projects: Vec<ProjectEntry>,
    project_name: &str,
    rendered_templates_cache: &mut RenderedTemplatesCache,
) -> Result<Option<RemovalTemplate>> {
    let import_source_from = match existing_database {
        Database::MongoDB => "nosql.base.properties.ts",
        _ => "sql.base.properties.ts",
    };
    let import_source_to = match database {
        Database::MongoDB => "nosql.base.properties.ts",
        _ => "sql.base.properties.ts",
    };

    let base_entity_from = match existing_database {
        Database::MongoDB => "nosqlBaseProperties",
        _ => "sqlBaseProperties",
    };
    let base_entity_to = match database {
        Database::MongoDB => "nosqlBaseProperties",
        _ => "sqlBaseProperties",
    };

    if let Some(base_entity_ts_content) = transform_base_entity_ts(
        rendered_templates_cache,
        &base_path.parent().unwrap(),
        database,
    )? {
        let entity_path = base_path
            .parent()
            .unwrap()
            .join("core")
            .join("persistence")
            .join(import_source_to);
        rendered_templates_cache.insert(
            entity_path.to_string_lossy(),
            RenderedTemplate {
                path: entity_path.clone(),
                content: base_entity_ts_content,
                context: None,
            },
        );
    }

    let entities_path = base_path.join("persistence").join("entities");
    for entry in WalkDir::new(&entities_path) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let path = entry.path();
            if let Some(template) = rendered_templates_cache.get(&path)? {
                let content = template.content;
                let new_content = content
                    .replace(base_entity_from, base_entity_to)
                    .replace(import_source_from, import_source_to);
                if content != new_content {
                    rendered_templates_cache.insert(
                        path.to_string_lossy(),
                        RenderedTemplate {
                            path: path.to_path_buf(),
                            content: new_content,
                            context: None,
                        },
                    );
                }
            }
        }
    }

    let core_path = base_path.parent().unwrap().join("core");
    let persistence_path = core_path.join("persistence");
    let existing_base_entity_path = persistence_path.join(import_source_from);

    let mut all_projects_scan = HashSet::new();

    all_projects_scan.insert(database.clone());

    for project in projects.iter() {
        if project.name != project_name {
            if let Some(resources) = &project.resources {
                match &resources.database {
                    Some(database) => all_projects_scan.insert(database.parse::<Database>()?),
                    None => false,
                };
            }
        }
    }

    let entities_index_path = persistence_path.join("index.ts");
    rendered_templates_cache.insert(
        entities_index_path.to_string_lossy(),
        RenderedTemplate {
            path: entities_index_path.clone(),
            content: all_projects_scan
                .iter()
                .map(|export_source| {
                    let mut database_exports = String::new();
                    let exclusive_files = export_source.metadata().exclusive_files;
                    if let Some(export_sources) = exclusive_files {
                        for export_source in export_sources {
                            database_exports.push_str(&format!(
                                "export * from './{}';",
                                export_source.replace(".ts", "").to_string()
                            ));
                        }
                    }
                    database_exports
                })
                .collect::<Vec<String>>()
                .join("\n"),
            context: None,
        },
    );

    if database == &Database::MongoDB {
        let mut core_package_json = from_str::<ProjectPackageJson>(
            &rendered_templates_cache
                .get(core_path.join("package.json"))?
                .unwrap()
                .content,
        )?;
        let mut core_package_json_dependencies = core_package_json.dependencies.unwrap();
        core_package_json_dependencies.databases = all_projects_scan.clone();
        core_package_json_dependencies.mikro_orm_database =
            Some(MIKRO_ORM_DATABASE_VERSION.to_string());
        core_package_json.dependencies = Some(core_package_json_dependencies);
        rendered_templates_cache.insert(
            core_path.join("package.json").to_string_lossy(),
            RenderedTemplate {
                path: core_path.join("package.json").to_path_buf(),
                content: to_string_pretty(&core_package_json)?,
                context: None,
            },
        );
    }

    if !all_projects_scan.contains(&existing_database)
        && apply_watermark(&RenderedTemplate {
            path: existing_base_entity_path.clone().into(),
            content: TEMPLATES_DIR
                .get_file(
                    Path::new("project")
                        .join("core")
                        .join("persistence")
                        .join(import_source_from),
                )
                .unwrap()
                .contents_utf8()
                .unwrap()
                .to_string(),
            context: None,
        })?
        .trim()
            == rendered_templates_cache
                .get(&existing_base_entity_path)?
                .unwrap()
                .content
                .trim()
    {
        return Ok(Some(RemovalTemplate {
            path: existing_base_entity_path,
        }));
    }

    Ok(None)
}

pub(crate) fn change_database_env_variables(
    env_content: &mut Env,
    app_name: &str,
    worker_name: &str,
    database: &Database,
) {
    env_content.db_name = Some(format!("{}-{}-dev", app_name, worker_name));
    if !is_in_memory_database(database) {
        env_content.db_host = Some("localhost".to_string());
        env_content.db_user = Some(format!("{}", database.to_string()));
        env_content.db_password = Some(format!("{}", database.to_string()));
        if let Some(port) = get_database_port(database) {
            env_content.db_port = Some(format!("{}", port));
        }
    }
}

pub(crate) fn change_database_postinstall_script(
    application_package_json: &mut ApplicationPackageJson,
    database: &Database,
) {
    application_package_json
        .scripts
        .as_mut()
        .unwrap()
        .postinstall = get_postinstall_script(database);
}

pub(crate) fn change_database_seed_script(
    project_package_json: &mut ProjectPackageJson,
    database: &Database,
) {
    project_package_json.scripts.as_mut().unwrap().seed = match database {
        Database::MongoDB => None,
        _ => Some(PROJECT_SEED_SCRIPT.to_string()),
    };
}

pub(crate) fn change_database_retention_script(
    project_package_json: &mut ProjectPackageJson,
    runtime: &crate::constants::Runtime,
    has_database: bool,
) {
    project_package_json
        .scripts
        .as_mut()
        .unwrap()
        .retention_enforce = if has_database {
        Some(project_retention_enforce_script(runtime))
    } else {
        None
    };
}

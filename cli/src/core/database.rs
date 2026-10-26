use std::{collections::HashSet, fs::read_to_string, path::Path};

use anyhow::{Context, Result, bail};
use serde_json::{from_str, to_string_pretty};

use super::{
    manifest::ManifestData,
    package_json::{
        package_json_constants::{
            BETTER_SQLITE_POSTINSTALL_SCRIPT, MIKRO_ORM_DATABASE_VERSION, SQLITE_POSTINSTALL_SCRIPT,
        },
        project_package_json::ProjectPackageJson,
    },
    rendered_template::{RenderedTemplate, TEMPLATES_DIR},
};
use crate::constants::{
    Database, ERROR_FAILED_TO_CREATE_DATABASE_EXPORT_INDEX_TS, ERROR_UNSUPPORTED_DATABASE, Runtime,
};

pub(crate) fn get_db_driver(database: &Database) -> String {
    match database {
        Database::MongoDB => "MongoDriver".to_string(),
        Database::PostgreSQL => "PostgreSqlDriver".to_string(),
        Database::SQLite => "SqliteDriver".to_string(),
        Database::MySQL => "MySqlDriver".to_string(),
        Database::MariaDB => "MariaDbDriver".to_string(),
        Database::BetterSQLite => "BetterSqliteDriver".to_string(),
        Database::LibSQL => "LibSqlDriver".to_string(),
        Database::MsSQL => "MsSqlDriver".to_string(),
    }
}

pub(crate) fn get_database_port(database: &Database) -> Option<String> {
    match database {
        Database::MongoDB => Some("27017".to_string()),
        Database::PostgreSQL => Some("5432".to_string()),
        Database::SQLite => None,
        Database::MySQL => Some("3306".to_string()),
        Database::MariaDB => Some("3306".to_string()),
        Database::BetterSQLite => None,
        Database::LibSQL => None,
        Database::MsSQL => Some("1433".to_string()),
    }
}

pub(crate) fn generate_index_ts_database_export(
    base_path: &Path,
    databases: Option<Vec<String>>,
    manifest_data: Option<&ManifestData>,
) -> Result<RenderedTemplate> {
    let mut export_set = HashSet::new();
    let mut database_set = HashSet::new();

    let projects = match manifest_data {
        Some(ManifestData::Service(service)) => service.projects.clone(),
        Some(ManifestData::Worker(worker)) => worker.projects.clone(),
        _ => vec![],
    };

    projects.iter().for_each(|project| {
        if let Some(resources) = &project.resources {
            if let Some(database) = &resources.database {
                database_set.insert(database.to_string());
            }
        }
    });

    database_set.extend(databases.unwrap_or_default());

    database_set
        .iter()
        .map(|database| {
            let export_string = match database.parse::<Database>()? {
                Database::MongoDB => Some("nosql.base.properties"),
                Database::PostgreSQL => Some("sql.base.properties"),
                Database::SQLite => Some("sql.base.properties"),
                Database::MySQL => Some("sql.base.properties"),
                Database::MariaDB => Some("sql.base.properties"),
                Database::MsSQL => Some("sql.base.properties"),
                Database::BetterSQLite => Some("sql.base.properties"),
                Database::LibSQL => Some("sql.base.properties"),
            };

            if let Some(export_string) = export_string {
                export_set.insert(format!("export * from './{}'", export_string));
            }

            Ok(())
        })
        .collect::<Result<Vec<()>>>()?;

    Ok(RenderedTemplate {
        path: Path::new(&base_path)
            .join("core")
            .join("persistence")
            .join("index.ts"),
        content: export_set.into_iter().collect::<Vec<String>>().join("\n"),
        context: None,
    })
}

pub(crate) fn update_core_package_json(
    manifest_data: &ManifestData,
    base_path: &Path,
) -> Result<RenderedTemplate> {
    let package_json_path = base_path.join("core").join("package.json");
    let package_json_content = read_to_string(&package_json_path)?;
    let mut full_package_json: ProjectPackageJson = from_str(&package_json_content)?;

    let database = match manifest_data {
        ManifestData::Service(service) => Some(service.database.clone()),
        ManifestData::Worker(worker) => worker.database.clone(),
        _ => bail!(ERROR_UNSUPPORTED_DATABASE),
    };

    if let Some(database) = database {
        full_package_json
            .dependencies
            .as_mut()
            .unwrap()
            .databases
            .insert(database.parse::<Database>()?);

        full_package_json
            .dependencies
            .as_mut()
            .unwrap()
            .mikro_orm_database = Some(MIKRO_ORM_DATABASE_VERSION.to_string());
    }

    Ok(RenderedTemplate {
        path: package_json_path,
        content: to_string_pretty(&full_package_json)?,
        context: None,
    })
}

pub(crate) fn get_base_entity_filename(database: &Database) -> Result<&str> {
    match database {
        Database::MongoDB => Ok("nosql.base.properties.ts"),
        Database::PostgreSQL => Ok("sql.base.properties.ts"),
        Database::SQLite => Ok("sql.base.properties.ts"),
        Database::MySQL => Ok("sql.base.properties.ts"),
        Database::MariaDB => Ok("sql.base.properties.ts"),
        Database::BetterSQLite => Ok("sql.base.properties.ts"),
        Database::LibSQL => Ok("sql.base.properties.ts"),
        Database::MsSQL => Ok("sql.base.properties.ts"),
    }
}

pub(crate) fn add_base_entity_to_core(
    manifest_data: &ManifestData,
    base_path: &Path,
) -> Result<Vec<RenderedTemplate>> {
    let database = match manifest_data {
        ManifestData::Service(service) => service.database.clone(),
        ManifestData::Worker(worker) => worker.database.clone().unwrap(),
        _ => bail!(ERROR_UNSUPPORTED_DATABASE),
    };

    let database = database.parse::<Database>()?;
    let filename = get_base_entity_filename(&database)?;

    let entity_path = base_path.join("core").join("persistence").join(filename);

    let template = TEMPLATES_DIR.get_file(
        Path::new("project")
            .join("core")
            .join("persistence")
            .join(filename),
    );

    // An existing base-entity file (init application writes it, and any
    // project on the same kind of database) stays as it is.
    let mut templates = Vec::new();
    if !entity_path.exists() {
        templates.push(RenderedTemplate {
            path: entity_path,
            content: template.unwrap().contents_utf8().unwrap().to_string(),
            context: None,
        });
    }
    templates.extend([
        generate_index_ts_database_export(
            base_path,
            Some(vec![database.to_string()]),
            Some(manifest_data),
        )
        .with_context(|| ERROR_FAILED_TO_CREATE_DATABASE_EXPORT_INDEX_TS)?,
        update_core_package_json(manifest_data, base_path)?.preserving_json_layout(),
    ]);
    Ok(templates)
}

pub(crate) fn is_in_memory_database(database: &Database) -> bool {
    match database {
        Database::SQLite => true,
        Database::BetterSQLite => true,
        Database::LibSQL => true,
        _ => false,
    }
}

pub(crate) fn get_postinstall_script(database: &Database) -> Option<String> {
    match database {
        Database::SQLite => Some(SQLITE_POSTINSTALL_SCRIPT.to_string()),
        Database::BetterSQLite => Some(BETTER_SQLITE_POSTINSTALL_SCRIPT.to_string()),
        _ => None,
    }
}

pub(crate) fn get_database_variants(runtime: &Runtime) -> &[&str] {
    match runtime {
        Runtime::Bun => &Database::VARIANTS[..Database::VARIANTS.len() - 1],
        Runtime::Node => &Database::VARIANTS[..],
    }
}

// ------------------------------------------------- shared database leftovers
//
// A TypeScript project's database also lands outside the project: the core
// library depends on its MikroORM driver, the workspace's migrate/seed scripts
// start its Compose service, and Compose runs that service. Those are shared
// by every project on the same database, so they go only when the last one
// does (on delete, or a database change); entries no project accounts for,
// such as a Compose service someone else depends on, stay.

/// The Compose services the workspace scripts start for `database`.
fn database_compose_services(database: &Database) -> Vec<String> {
    let mut services = vec![database.to_string()];
    if *database == Database::MongoDB {
        services.push("mongo-init".to_string());
    }
    services
}

/// Whether any project in `projects` (TypeScript or Rust) uses `database`.
pub(crate) fn database_in_use(
    projects: &[super::manifest::ProjectEntry],
    database: &Database,
) -> bool {
    let name = database.to_string();
    projects.iter().any(|project| {
        project
            .resources
            .as_ref()
            .and_then(|resources| resources.database.as_deref())
            == Some(name.as_str())
    })
}

/// Add (`present`) or drop `database`'s services from the compose prefix of
/// the workspace's migrate and seed scripts.
pub(crate) fn set_database_in_application_scripts(
    scripts: &mut super::package_json::application_package_json::ApplicationScripts,
    database: &Database,
    present: bool,
) {
    use super::package_json::package_json_constants::{
        compose_up_command, split_compose_up_command,
    };
    let wanted = database_compose_services(database);
    let update = |script: &mut Option<String>| {
        let Some(current) = script.as_ref() else {
            return;
        };
        let (mut services, rest) = split_compose_up_command(current);
        if services.is_empty() && !current.starts_with("docker compose up") {
            return;
        }
        let before = services.clone();
        if present {
            for service in &wanted {
                if !services.contains(service) {
                    services.push(service.clone());
                }
            }
        } else {
            services.retain(|service| !wanted.contains(service));
        }
        if services == before {
            return;
        }
        let prefix = compose_up_command(&services);
        *script = Some(if rest.is_empty() { prefix } else { format!("{prefix} && {rest}") });
    };
    update(&mut scripts.migrate_init);
    update(&mut scripts.migrate_create);
    update(&mut scripts.migrate_up);
    update(&mut scripts.migrate_down);
    update(&mut scripts.seed);
}

/// Add (`present`) or drop `database`'s MikroORM driver in the core
/// library's package.json under `workspace`, staged in `cache`.
pub(crate) fn set_core_database(
    workspace: &Path,
    database: &Database,
    present: bool,
    cache: &mut super::rendered_template::RenderedTemplatesCache,
) -> Result<()> {
    let path = workspace.join("core").join("package.json");
    let Some(staged) = cache.get(&path)? else {
        return Ok(());
    };
    let mut package: ProjectPackageJson = from_str(&staged.content)?;
    let Some(dependencies) = package.dependencies.as_mut() else {
        return Ok(());
    };
    let changed = if present {
        dependencies.mikro_orm_database.get_or_insert_with(|| MIKRO_ORM_DATABASE_VERSION.to_string());
        dependencies.databases.insert(database.clone())
    } else {
        dependencies.databases.remove(database)
    };
    if changed {
        let original = read_to_string(&path).unwrap_or_else(|_| staged.content.clone());
        cache.insert(
            path.to_string_lossy().to_string(),
            RenderedTemplate {
                content: super::package_json::preserve_json_layout(&original, &to_string_pretty(&package)?),
                path,
                context: None,
            },
        );
    }
    Ok(())
}

/// Drop `database`'s workspace leftovers when no project in `remaining`
/// still uses it: the core driver and the workspace package.json scripts
/// (both staged in `cache`), and, given `compose`, its Compose service and
/// volume unless a remaining service depends on it.
pub(crate) fn release_database(
    workspace: &Path,
    app_name: &str,
    remaining: &[super::manifest::ProjectEntry],
    database: &Database,
    deleted: &str,
    compose: Option<&mut super::docker::DockerCompose>,
    cache: &mut super::rendered_template::RenderedTemplatesCache,
) -> Result<Vec<std::path::PathBuf>> {
    let mut removals = Vec::new();
    if database_in_use(remaining, database) {
        return Ok(removals);
    }
    // The application's own database (init application -d) is in core before
    // any project uses it, and nothing records it apart from that. The last
    // database core lists is therefore kept: deleting the only service on
    // it returns the application to how init left it.
    // Compose is different: init application starts no database service, so
    // the service goes with the last project whatever core lists.
    let core_path = workspace.join("core").join("package.json");
    let application_database = match cache.get(&core_path)? {
        Some(core) => {
            let package: ProjectPackageJson = from_str(&core.content)?;
            package.dependencies.map(|d| d.databases).unwrap_or_default().len() <= 1
        }
        None => false,
    };
    if application_database {
        release_compose_database(app_name, database, deleted, compose);
        return Ok(removals);
    }
    set_core_database(workspace, database, false, cache)?;

    // The core base-entity file of its kind (SQL or document), when no
    // remaining project's database shares it and it is still as generated.
    let filename = get_base_entity_filename(database)?;
    let kind_in_use = remaining.iter().any(|project| {
        project
            .resources
            .as_ref()
            .and_then(|resources| resources.database.as_deref())
            .and_then(|name| name.parse::<Database>().ok())
            .is_some_and(|other| get_base_entity_filename(&other).ok() == Some(filename))
    });
    let entity_path = workspace.join("core").join("persistence").join(filename);
    let template = TEMPLATES_DIR
        .get_file(Path::new("project").join("core").join("persistence").join(filename))
        .and_then(|file| file.contents_utf8());
    let generated = template.map(|content| {
        super::watermark::apply_watermark(&RenderedTemplate {
            path: entity_path.clone(),
            content: content.to_string(),
            context: None,
        })
    });
    let unmodified = read_to_string(&entity_path).ok().is_some_and(|text| {
        Some(text.as_str()) == template
            || generated.as_ref().is_some_and(|generated| generated.as_ref().ok() == Some(&text))
    });
    if !kind_in_use && unmodified {
        let index_path = workspace.join("core").join("persistence").join("index.ts");
        if let Some(index) = cache.get(&index_path)? {
            let export = format!("export * from './{}'", filename.trim_end_matches(".ts"));
            let kept: Vec<&str> = index.content.split('\n').filter(|line| line.trim() != export).collect();
            let content = kept.join("\n");
            if content != index.content {
                cache.insert(
                    index_path.to_string_lossy().to_string(),
                    RenderedTemplate { path: index_path, content, context: None },
                );
                removals.push(entity_path);
            }
        }
    }

    let path = workspace.join("package.json");
    if let Some(staged) = cache.get(&path)? {
        let mut package: super::package_json::application_package_json::ApplicationPackageJson =
            from_str(&staged.content)?;
        if let Some(scripts) = package.scripts.as_mut() {
            set_database_in_application_scripts(scripts, database, false);
        }
        let rendered = to_string_pretty(&package)?;
        let laid_out = super::package_json::preserve_json_layout(&staged.content, &rendered);
        if laid_out != staged.content {
            cache.insert(
                path.to_string_lossy().to_string(),
                RenderedTemplate { path, content: laid_out, context: staged.context },
            );
        }
    }

    release_compose_database(app_name, database, deleted, compose);
    Ok(removals)
}

/// Remove `database`'s Compose service and data volume unless a service other
/// than the deleted project's own still depends on it.
fn release_compose_database(
    app_name: &str,
    database: &Database,
    deleted: &str,
    compose: Option<&mut super::docker::DockerCompose>,
) {
    if let Some(compose) = compose {
        let services = database_compose_services(database);
        // The deleted project's own processes (`<name>-migrate`, …) do not
        // count; the image reconciler removes them afterwards.
        let own = |name: &str| name == deleted || name.starts_with(&format!("{deleted}-"));
        let depended_on = compose.services.iter().any(|(name, service)| {
            !services.contains(name)
                && !own(name)
                && service
                    .depends_on
                    .as_ref()
                    .is_some_and(|depends_on| services.iter().any(|s| depends_on.contains_key(s)))
        });
        if !depended_on {
            for service in &services {
                compose.services.shift_remove(service);
            }
            compose.volumes.shift_remove(&format!("{app_name}-{}-data", database.to_string()));
        }
    }
}

#[cfg(test)]
mod release_tests {
    use super::*;
    use crate::core::{
        manifest::{ProjectEntry, ProjectType, ResourceInventory},
        package_json::application_package_json::ApplicationScripts,
        rendered_template::RenderedTemplatesCache,
    };

    fn project(name: &str, database: &str) -> ProjectEntry {
        ProjectEntry {
            r#type: ProjectType::Service,
            name: name.to_string(),
            description: String::new(),
            variant: None,
            resources: Some(ResourceInventory {
                database: Some(database.to_string()),
                cache: None,
                queue: None,
                object_store: None,
                redis_partition: None,
                capabilities: None,
            }),
            routers: None,
            metadata: None,
            serves: None,
        }
    }

    #[test]
    fn scripts_gain_and_lose_only_the_database_services() {
        let mut scripts = ApplicationScripts {
            migrate_up: Some("docker compose up -d --wait postgresql redis && pnpm -r run migrate:up".into()),
            seed: Some("pnpm -r run seed".into()),
            ..Default::default()
        };
        let original = scripts.migrate_up.clone();
        set_database_in_application_scripts(&mut scripts, &Database::MongoDB, true);
        assert_eq!(
            scripts.migrate_up.as_deref(),
            Some("docker compose up -d --wait mongodb postgresql redis && docker compose up mongo-init && pnpm -r run migrate:up")
        );
        set_database_in_application_scripts(&mut scripts, &Database::MongoDB, false);
        assert_eq!(scripts.migrate_up, original);
        // A script without a compose prefix is not the CLI's to rewrite.
        assert_eq!(scripts.seed.as_deref(), Some("pnpm -r run seed"));
    }

    #[test]
    fn core_driver_goes_with_the_last_project_on_its_database() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("core")).unwrap();
        let core = "{\n  \"name\": \"@app/core\",\n  \"dependencies\": {\n    \"@mikro-orm/core\": \"7\",\n    \"@mikro-orm/migrations\": \"7\",\n    \"@mikro-orm/postgresql\": \"7\",\n    \"zod\": \"4\"\n  }\n}";
        std::fs::write(root.path().join("core/package.json"), core).unwrap();
        std::fs::write(root.path().join("package.json"), "{\n  \"name\": \"app\",\n  \"scripts\": {\n    \"migrate:up\": \"docker compose up -d --wait postgresql && pnpm -r run migrate:up\"\n  }\n}").unwrap();

        let mut cache = RenderedTemplatesCache::new();
        set_core_database(root.path(), &Database::MySQL, true, &mut cache).unwrap();
        let added = cache.get(root.path().join("core/package.json")).unwrap().unwrap().content;
        assert_eq!(added.matches("@mikro-orm/migrations").count(), 1, "{added}");
        assert!(added.contains("\"@mikro-orm/mysql\": \"7"));
        std::fs::write(root.path().join("core/package.json"), &added).unwrap();

        let mut cache = RenderedTemplatesCache::new();
        let remaining = [project("other", "mysql")];
        release_database(root.path(), "app", &remaining, &Database::MySQL, "gone", None, &mut cache).unwrap();
        assert_eq!(cache.get(root.path().join("core/package.json")).unwrap().unwrap().content, added);

        release_database(root.path(), "app", &[], &Database::MySQL, "gone", None, &mut cache).unwrap();
        assert_eq!(cache.get(root.path().join("core/package.json")).unwrap().unwrap().content, core);
        // PostgreSQL is now core's only database (the application's own): kept.
        std::fs::write(root.path().join("core/package.json"), core).unwrap();
        let mut cache = RenderedTemplatesCache::new();
        release_database(root.path(), "app", &[], &Database::PostgreSQL, "gone", None, &mut cache).unwrap();
        assert_eq!(cache.get(root.path().join("core/package.json")).unwrap().unwrap().content, core);
    }
}

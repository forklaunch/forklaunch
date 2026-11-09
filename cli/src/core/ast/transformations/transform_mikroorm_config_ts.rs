use std::{collections::HashSet, path::Path};

use anyhow::{Context, Result};
use oxc_allocator::{Allocator, CloneIn, Vec};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey, SourceType, Statement};
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::{Database, error_failed_to_read_file},
    core::{
        ast::{
            parse_ast_program::{parse_ast_expression, parse_ast_program},
            replacements::replace_import_statment::replace_import_statment,
        },
        database::{get_db_driver, is_in_memory_database},
        rendered_template::RenderedTemplatesCache,
    },
};

pub(crate) fn transform_mikroorm_config_ts(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    existing_database: &Option<Database>,
    database: &Database,
) -> Result<String> {
    let in_memory = is_in_memory_database(database);

    let is_mongo = match database {
        Database::MongoDB => true,
        _ => false,
    };

    let allocator = Allocator::default();
    let mikro_orm_config_path = base_path.join("mikro-orm.config.ts");
    let template = rendered_templates_cache
        .get(&mikro_orm_config_path)?
        .context(error_failed_to_read_file(&mikro_orm_config_path))?;
    let mikro_orm_config_text = &template.content;
    let mikro_orm_config_type = SourceType::from_path(&mikro_orm_config_path)?;

    let mut mikro_orm_config_program =
        parse_ast_program(&allocator, &mikro_orm_config_text, mikro_orm_config_type);

    let migrator_import_text = format!(
        "import {{ Migrator }} from '@mikro-orm/migrations{}';",
        match database {
            Database::MongoDB => "-mongodb",
            _ => "",
        }
    );
    let import_source_identifier = format!(
        "@mikro-orm/migrations{}",
        match existing_database {
            Some(Database::MongoDB) => "-mongodb",
            _ => "",
        }
    );
    let _ = replace_import_statment(
        &mut mikro_orm_config_program,
        &mut parse_ast_program(&allocator, &migrator_import_text, SourceType::ts()),
        &import_source_identifier,
    )?;

    let database_import_text = format!(
        "import {{ defineConfig }} from \"@mikro-orm/{}\";",
        database.to_string().to_lowercase()
    );
    let mut database_import_program =
        parse_ast_program(&allocator, &database_import_text, SourceType::ts());
    if let Some(existing_database) = existing_database {
        let _ = replace_import_statment(
            &mut mikro_orm_config_program,
            &mut database_import_program,
            &format!(
                "@mikro-orm/{}",
                existing_database.to_string().to_lowercase()
            ),
        );
    }

    let driver_text = format!("let driver = {};", get_db_driver(database));
    let migrations_text = format!(
        "let migrations = {{
            path: 'dist/migrations-{}',
            pathTs: 'migrations-{}'
        }};",
        database.to_string().to_lowercase(),
        database.to_string().to_lowercase()
    );

    for stmt in mikro_orm_config_program.body.iter_mut() {
        let declaration = match stmt {
            Statement::VariableDeclaration(import) => import,
            _ => continue,
        };

        let call_expression = match declaration.declarations[0].init.as_mut() {
            Some(Expression::CallExpression(call_expr)) => call_expr,
            _ => continue,
        };

        if call_expression
            .callee_name()
            .is_some_and(|name| name == "createConfigInjector")
        {
            for arg in call_expression.arguments.iter_mut() {
                let expression = match arg {
                    Argument::ObjectExpression(expression) => expression,
                    _ => continue,
                };

                let mut new_properties = Vec::new_in(&&allocator);
                let mut visited_properties = HashSet::new();

                for prop in expression.properties.iter() {
                    let ObjectPropertyKind::ObjectProperty(prop) = prop else {
                        continue;
                    };

                    if let PropertyKey::StaticIdentifier(id) = &prop.key {
                        if visited_properties.contains(&id.name.as_str()) {
                            continue;
                        }

                        if ["DB_HOST", "DB_PORT", "DB_USER", "DB_PASSWORD"]
                            .contains(&id.name.as_str())
                        {
                            if in_memory || is_mongo {
                                continue;
                            }
                        }
                        visited_properties.insert(id.name.as_str());
                    }

                    new_properties.push(ObjectPropertyKind::ObjectProperty(
                        prop.clone_in(&allocator),
                    ));
                }

                let additional_object_props = match database {
                    Database::PostgreSQL
                    | Database::MariaDB
                    | Database::MySQL
                    | Database::MsSQL => {
                        let mut additional_object_props = String::new();
                        for key in ["DB_NAME", "DB_HOST", "DB_USER", "DB_PASSWORD"] {
                            if !visited_properties.contains(key) {
                                additional_object_props.push_str(&format!(
                                    "{}: {{
                                        lifetime: Lifetime.Singleton,
                                        type: string,
                                        value: getEnvVar('{}')
                                    }},\n",
                                    key, key
                                ));
                            }
                        }
                        if !visited_properties.contains("DB_PORT") {
                            additional_object_props.push_str(&format!(
                                "DB_PORT: {{
                                        lifetime: Lifetime.Singleton,
                                        type: number,
                                        value: Number(getEnvVar('DB_PORT'))
                                    }},\n",
                            ));
                        }
                        let additional_object_props = format!(
                            "const n = {{
                            {additional_object_props}
                        }};"
                        );
                        parse_ast_expression(
                            &allocator,
                            allocator.alloc_str(&additional_object_props),
                            SourceType::ts(),
                        )
                    }
                    Database::MongoDB => {
                        let mut additional_object_props = String::new();
                        for key in ["DB_HOST", "DB_USER", "DB_PASSWORD"] {
                            if !visited_properties.contains(key) {
                                additional_object_props.push_str(&format!(
                                    "{}: {{
                                        lifetime: Lifetime.Singleton,
                                        type: string,
                                        value: getEnvVar('{}')
                                    }},\n",
                                    key, key
                                ));
                            }
                        }
                        if !visited_properties.contains("DB_PORT") {
                            additional_object_props.push_str(&format!(
                                "DB_PORT: {{
                                        lifetime: Lifetime.Singleton,
                                        type: number,
                                        value: Number(getEnvVar('DB_PORT'))
                                    }},\n",
                            ));
                        }
                        let additional_object_props = format!(
                            "const n = {{
                            {additional_object_props}
                        }};"
                        );
                        parse_ast_expression(
                            &allocator,
                            allocator.alloc_str(&additional_object_props),
                            SourceType::ts(),
                        )
                    }
                    _ => None,
                };

                if let Some(additional_object_props) = additional_object_props {
                    match additional_object_props {
                        Expression::ObjectExpression(object_expression) => {
                            for prop in object_expression.properties.iter() {
                                new_properties.push(prop.clone_in(&allocator));
                            }
                        }
                        _ => {}
                    }
                }

                expression.properties = new_properties;
            }
        }

        if call_expression
            .callee_name()
            .is_some_and(|name| name == "defineConfig")
        {
            for arg in call_expression.arguments.iter_mut() {
                let expression = match arg {
                    Argument::ObjectExpression(expression) => expression,
                    _ => continue,
                };

                let mut new_properties = Vec::new_in(&&allocator);
                let mut visited_properties = HashSet::new();

                for property in expression.properties.iter_mut() {
                    let ObjectPropertyKind::ObjectProperty(prop) = property else {
                        continue;
                    };

                    if let PropertyKey::StaticIdentifier(id) = &prop.clone_in(&allocator).key {
                        if visited_properties.contains(&id.name.as_str()) {
                            continue;
                        }

                        if ["host", "port", "user", "password"].contains(&id.name.as_str()) {
                            if in_memory || is_mongo {
                                continue;
                            }
                        }

                        if id.name == "clientUrl" {
                            if !is_mongo {
                                continue;
                            }
                        }

                        if id.name == "driver" {
                            if let Some(driver_expression) =
                                parse_ast_expression(&allocator, &driver_text, SourceType::ts())
                            {
                                prop.value = driver_expression;
                            }
                        }

                        if id.name == "migrations" {
                            if let Some(migrations_expression) =
                                parse_ast_expression(&allocator, &migrations_text, SourceType::ts())
                            {
                                prop.value = migrations_expression;
                            }
                        }

                        visited_properties.insert(id.name.as_str());
                    }
                    new_properties.push(ObjectPropertyKind::ObjectProperty(
                        prop.clone_in(&allocator),
                    ));
                }

                let additional_object_props = match database {
                    Database::PostgreSQL
                    | Database::MariaDB
                    | Database::MySQL
                    | Database::MsSQL => {
                        let mut additional_object_props = String::new();
                        for (key, value) in [
                            ("dbName", "DB_NAME"),
                            ("host", "DB_HOST"),
                            ("user", "DB_USER"),
                            ("password", "DB_PASSWORD"),
                            ("port", "DB_PORT"),
                        ] {
                            if !visited_properties.contains(key) {
                                additional_object_props.push_str(&format!(
                                    "{}: validConfigInjector.resolve('{}'),\n",
                                    key, value
                                ));
                            }
                        }
                        let additional_object_props = format!(
                            "const n = {{
                            {additional_object_props}
                        }};"
                        );
                        parse_ast_expression(
                            &allocator,
                            allocator.alloc_str(&additional_object_props),
                            SourceType::ts(),
                        )
                    }
                    Database::MongoDB => {
                        // DB_URL (set by the platform for DocumentDB: tls,
                        // replicaSet, retryWrites=false) wins; the URL built
                        // from DB_HOST/DB_PORT is for local development.
                        // DB_URL names the database in its path; dbName (DB_NAME)
                        // is set too, for a DB_URL without one.
                        let mut additional_object_props = String::new();
                        if !visited_properties.contains("clientUrl") {
                            additional_object_props.push_str(
                                "clientUrl: getEnvVar('DB_URL') || `mongodb://${validConfigInjector.resolve(
                                        'DB_USER'
                                    )}:${validConfigInjector.resolve('DB_PASSWORD')}@${validConfigInjector.resolve(
                                        'DB_HOST'
                                    )}:${validConfigInjector.resolve('DB_PORT')}/${validConfigInjector.resolve(
                                        'DB_NAME'
                                    )}?authSource=admin&directConnection=true&replicaSet=rs0`,\n",
                            );
                        }
                        if !visited_properties.contains("dbName") {
                            additional_object_props
                                .push_str("dbName: validConfigInjector.resolve('DB_NAME'),\n");
                        }
                        if additional_object_props.is_empty() {
                            None
                        } else {
                            let additional_object_props =
                                format!("const n = {{\n{additional_object_props}}};");
                            parse_ast_expression(
                                &allocator,
                                allocator.alloc_str(&additional_object_props),
                                SourceType::ts(),
                            )
                        }
                    }
                    _ => None,
                };

                if let Some(additional_object_props) = additional_object_props {
                    match additional_object_props {
                        Expression::ObjectExpression(object_expression) => {
                            for prop in object_expression.properties.iter() {
                                new_properties.push(prop.clone_in(&allocator));
                            }
                        }
                        _ => {}
                    }
                }

                expression.properties = new_properties;
            }
        }
    }

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&mikro_orm_config_program)
        .code)
}

#[cfg(test)]
mod tests {
    use std::fs::write;

    use tempfile::TempDir;

    use super::*;
    use crate::core::rendered_template::RenderedTemplatesCache;

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

    fn make_cache_with_config(
        dir: &TempDir,
        content: &str,
    ) -> (RenderedTemplatesCache, std::path::PathBuf) {
        let config_path = dir.path().join("mikro-orm.config.ts");
        write(&config_path, content).unwrap();
        (RenderedTemplatesCache::new(), dir.path().to_path_buf())
    }

    #[test]
    fn test_transform_postgresql_to_mysql_updates_import_and_migrations() {
        let tmp = TempDir::new().unwrap();
        let (cache, base) = make_cache_with_config(&tmp, POSTGRESQL_MIKRO_ORM_CONFIG);

        let result = transform_mikroorm_config_ts(
            &cache,
            &base,
            &Some(Database::PostgreSQL),
            &Database::MySQL,
        )
        .unwrap();

        // Import source should be updated
        assert!(
            result.contains("@mikro-orm/mysql"),
            "Expected @mikro-orm/mysql import in: {result}"
        );
        assert!(
            !result.contains("@mikro-orm/postgresql"),
            "Expected @mikro-orm/postgresql to be removed: {result}"
        );

        // Migrator import should still be plain (not -mongodb)
        assert!(
            result.contains("@mikro-orm/migrations\""),
            "Expected plain migrations import: {result}"
        );

        // Migrations path should be updated to mysql
        assert!(
            result.contains("migrations-mysql"),
            "Expected migrations-mysql path: {result}"
        );
        assert!(
            !result.contains("migrations-postgresql"),
            "Expected migrations-postgresql to be removed: {result}"
        );
    }

    #[test]
    fn test_transform_postgresql_to_mongodb_removes_db_credentials_adds_client_url() {
        let tmp = TempDir::new().unwrap();
        let (cache, base) = make_cache_with_config(&tmp, POSTGRESQL_MIKRO_ORM_CONFIG);

        let result = transform_mikroorm_config_ts(
            &cache,
            &base,
            &Some(Database::PostgreSQL),
            &Database::MongoDB,
        )
        .unwrap();

        // Import source should be updated to mongodb
        assert!(
            result.contains("@mikro-orm/mongodb"),
            "Expected @mikro-orm/mongodb import: {result}"
        );

        // Migrator should use migrations-mongodb
        assert!(
            result.contains("@mikro-orm/migrations-mongodb"),
            "Expected migrations-mongodb import: {result}"
        );

        // defineConfig should include clientUrl for mongodb (host/user/password/port
        // stay in createConfigInjector so the clientUrl template can reference them)
        assert!(
            result.contains("clientUrl"),
            "Expected clientUrl in defineConfig for mongodb: {result}"
        );

        // defineConfig should not have the individual host/user/password/port keys
        // (they are replaced by clientUrl in the defineConfig block)
        let define_config_start = result.find("defineConfig").expect("defineConfig missing");
        let define_config_section = &result[define_config_start..];
        assert!(
            !define_config_section.contains("host:"),
            "Expected host removed from defineConfig for mongodb: {result}"
        );
        assert!(
            !define_config_section.contains("user:"),
            "Expected user removed from defineConfig for mongodb: {result}"
        );
        assert!(
            !define_config_section.contains("password:"),
            "Expected password removed from defineConfig for mongodb: {result}"
        );
        // The platform's DB_URL (DocumentDB: tls, retryWrites=false) wins over
        // the local URL, and the database still comes from DB_NAME.
        assert!(
            define_config_section.contains("getEnvVar(\"DB_URL\") ||")
                || define_config_section.contains("getEnvVar('DB_URL') ||"),
            "Expected clientUrl to prefer DB_URL: {result}"
        );
        assert!(
            define_config_section.contains("directConnection=true&replicaSet=rs0"),
            "Expected the local URL as the fallback: {result}"
        );
        assert_eq!(
            define_config_section.matches("dbName:").count(),
            1,
            "Expected exactly one dbName: {result}"
        );
    }

    #[test]
    fn test_transform_mongodb_to_postgresql_drops_client_url() {
        let tmp = TempDir::new().unwrap();
        let (cache, base) = make_cache_with_config(&tmp, POSTGRESQL_MIKRO_ORM_CONFIG);
        let mongo = transform_mikroorm_config_ts(
            &cache,
            &base,
            &Some(Database::PostgreSQL),
            &Database::MongoDB,
        )
        .unwrap();
        let (cache, base) = make_cache_with_config(&tmp, &mongo);
        let result = transform_mikroorm_config_ts(
            &cache,
            &base,
            &Some(Database::MongoDB),
            &Database::PostgreSQL,
        )
        .unwrap();
        let define_config_section = &result[result.find("defineConfig").unwrap()..];
        assert!(!define_config_section.contains("clientUrl"), "{result}");
        assert!(!define_config_section.contains("DB_URL"), "{result}");
        for key in ["dbName:", "host:", "user:", "password:", "port:"] {
            assert!(
                define_config_section.contains(key),
                "missing {key}: {result}"
            );
        }
    }

    /// The template every service, worker and blueprint module renders its
    /// mikro-orm.config.ts from (the others are symlinks to it).
    fn render_template(database: &str) -> String {
        #[derive(ramhorns::Content)]
        struct Data {
            app_name: String,
            database: String,
            snake_case_name: String,
            is_mongo: bool,
            is_postgres: bool,
            is_in_memory_database: bool,
            is_better_auth: bool,
        }
        let source = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/templates/project/service/mikro-orm.config.ts"
        ))
        .unwrap();
        ramhorns::Template::new(source.as_str())
            .unwrap()
            .render(&Data {
                app_name: "demo".to_string(),
                database: database.to_string(),
                snake_case_name: "svc".to_string(),
                is_mongo: database == "mongodb",
                is_postgres: database == "postgresql",
                is_in_memory_database: false,
                is_better_auth: false,
            })
    }

    #[test]
    fn test_template_mongodb_prefers_db_url_and_keeps_db_name() {
        let rendered = render_template("mongodb");
        let config = &rendered[rendered.find("defineConfig({").unwrap()..];
        assert!(
            config.contains("clientUrl:\n    getEnvVar('DB_URL') ||\n    `mongodb://"),
            "{config}"
        );
        // Local fallback for the docker-compose replica set.
        assert!(
            config.contains("?authSource=admin&directConnection=true&replicaSet=rs0`,"),
            "{config}"
        );
        assert!(
            config.contains("dbName: validConfigInjector.resolve(\n    tokens.DB_NAME\n  ),"),
            "{config}"
        );
        assert!(!config.contains("host:"), "{config}");
    }

    #[test]
    fn test_template_sql_engines_use_the_discrete_settings_not_db_url() {
        // MySQL, MariaDB and MSSQL (and Postgres) connect with DB_HOST, DB_PORT,
        // DB_USER, DB_PASSWORD and DB_NAME, which the platform injects for every
        // engine; DB_URL's scheme does not reach the driver.
        for database in ["postgresql", "mysql", "mariadb", "mssql"] {
            let rendered = render_template(database);
            let config = &rendered[rendered.find("defineConfig({").unwrap()..];
            assert!(!rendered.contains("DB_URL"), "{database}: {rendered}");
            assert!(!config.contains("clientUrl"), "{database}");
            for key in ["dbName:", "host:", "user:", "password:", "port:"] {
                assert!(config.contains(key), "{database}: missing {key}");
            }
            assert!(
                rendered.contains(&format!("from '@mikro-orm/{database}'")),
                "{database}"
            );
        }
    }

    #[test]
    fn test_transform_postgresql_to_sqlite_removes_host_credentials() {
        let tmp = TempDir::new().unwrap();
        let (cache, base) = make_cache_with_config(&tmp, POSTGRESQL_MIKRO_ORM_CONFIG);

        let result = transform_mikroorm_config_ts(
            &cache,
            &base,
            &Some(Database::PostgreSQL),
            &Database::LibSQL,
        )
        .unwrap();

        // Import should update to libsql
        assert!(
            result.contains("@mikro-orm/libsql"),
            "Expected @mikro-orm/libsql: {result}"
        );

        // For in-memory/libsql: host/user/password/port should be stripped from defineConfig
        assert!(
            !result.contains("host:"),
            "Expected host removed for libsql: {result}"
        );
    }

    #[test]
    fn test_transform_no_op_when_database_unchanged() {
        let tmp = TempDir::new().unwrap();
        let (cache, base) = make_cache_with_config(&tmp, POSTGRESQL_MIKRO_ORM_CONFIG);

        // Calling with same source/target — the caller (change_database) guards this,
        // but the transform itself should still produce valid output
        let result = transform_mikroorm_config_ts(
            &cache,
            &base,
            &Some(Database::PostgreSQL),
            &Database::PostgreSQL,
        )
        .unwrap();

        assert!(
            result.contains("@mikro-orm/postgresql"),
            "Import should remain postgresql: {result}"
        );
    }
}

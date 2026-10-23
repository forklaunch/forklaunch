//! Explicit native service database composition. Business SQL stays service-owned.
use super::*;

pub(super) fn templates(
    dir: &Path,
    name: &str,
    mut templates: Vec<RenderedTemplate>,
) -> Result<Vec<RenderedTemplate>> {
    let data = rust_crate_template_data(name, "PostgreSQL service", true);
    for template in &mut templates {
        let Ok(relative) = template.path.strip_prefix(dir) else { continue };
        let relative = relative.to_string_lossy();
        match relative.as_ref() {
            "Cargo.toml" => {
                let mut cargo: toml_edit::DocumentMut = template.content.parse()?;
                let mut features = cargo["dependencies"]["forklaunch-core"]["features"].as_array().cloned().unwrap_or_default();
                for feature in ["postgres", "encryption"] { if !features.iter().any(|value| value.as_str()==Some(feature)) { features.push(feature); } }
                cargo["dependencies"]["forklaunch-core"]["features"] = toml_edit::value(features);
                let mut build_features = cargo["build-dependencies"]["forklaunch-core"]["features"].as_array().cloned().unwrap_or_default();
                if !build_features.iter().any(|value| value.as_str() == Some("encryption")) { build_features.push("encryption"); }
                cargo["build-dependencies"]["forklaunch-core"]["features"] = toml_edit::value(build_features);
                cargo["build-dependencies"]["uuid"]["version"] = toml_edit::value(crate::core::cargo_toml::version("uuid").unwrap_or_default());
                let dependencies: toml_edit::DocumentMut = crate::core::cargo_toml::fill("[dependencies]\nsqlx = { default-features = false, features = [\"runtime-tokio\", \"tls-rustls\", \"postgres\", \"uuid\", \"migrate\", \"macros\"] }\nuuid = { features = [\"v4\"] }\n")?.parse()?;
                for (name, dependency) in dependencies["dependencies"].as_table().unwrap() { cargo["dependencies"][name] = dependency.clone(); }
                cargo["test"].as_array_of_tables_mut().context("native test table")?.push({ let mut test=toml_edit::Table::new(); test["name"]=toml_edit::value("persistence"); test["path"]=toml_edit::value("__test__/persistence.rs"); test });
                template.content = cargo.to_string();
            }
            "build.rs" => template.content = template.content.replace("    println!(\"cargo:rerun-if-changed=persistence/compliance_schema.rs\");", "    println!(\"cargo:rerun-if-changed=migrations\");\n    println!(\"cargo:rerun-if-changed=persistence/compliance_schema.rs\");"),
            "bootstrapper.rs" => {
                template.content = template.content.replace("pub async fn run() -> ApplicationResult {\n", "pub async fn run() -> ApplicationResult {\n    // Migrations need only MIGRATION_DATABASE_URL, not runtime settings such as RUST_LOG.\n    if env::args().skip(1).eq([\"--migrate\"]) { return Ok(crate::persistence::database::migrate().await?); }\n")
                    .replace("    if !arguments.is_empty() { return Err(\"Expected no arguments\".into()); }", "    if arguments == [\"--migrate\"] { return Ok(crate::persistence::database::migrate().await?); }\n    if !arguments.is_empty() { return Err(\"Expected no arguments or --migrate\".into()); }")
                    .replace("registrations::register(config)?", "registrations::register(config).await?")
                    .replace("server::serve(state, listener, stopping.signal()).await", "let repository = state.repository.clone();\n    forklaunch_axum::native::with_cleanup(\n        server::serve(state, listener, stopping.signal()),\n        repository.close(),\n    ).await");
            }
            "api/controllers/health.rs" => template.content = template.content.replace("Ok(state.health.report())", "state.health.report().await.map_err(|_| EndpointError::Unavailable)"),
            "persistence/mod.rs" => template.content = "pub mod entities;\npub mod compliance_schema;\npub mod database;\nmod repository;\npub use repository::PostgresRepository;\n".into(),
            "persistence/entities/mod.rs" => template.content = "pub use super::compliance_schema::StoredRecord;\n".into(),
            "Makefile" => template.content.push_str("\n.PHONY: migrate\nmigrate:\n\t$(CARGO) run --package $(PACKAGE) -- --migrate\n"),
            "__test__/http.rs" => template.content = template.content.replace("#[tokio::test]", "#[tokio::test]\n#[ignore = \"requires migrated PostgreSQL, restricted DATABASE_URL, TENANT_ID and encryption keys\"]").replace("::config::ServiceConfig::default()).unwrap()", "::config::ServiceConfig::default()).await.unwrap()"),
            _ => {}
        }
    }
    for (destination, source) in [
        ("persistence/database.rs", "database.rs"),
        ("persistence/repository.rs", "repository.rs"),
        ("persistence/compliance_schema.rs", "compliance_schema.rs"),
        ("persistence/entities/stored_record.rs", "stored_record.rs"),
        ("registrations.rs", "registrations.rs"),
        ("domain/services/health.rs", "health.rs"),
        ("migrations/0001_records.sql", "0001_records.sql"),
        ("migrations/README.md", "README.md"),
        ("__test__/persistence.rs", "persistence_test.rs"),
    ] {
        templates.retain(|template| template.path != dir.join(destination));
        templates.push(RenderedTemplate {
            path: dir.join(destination),
            content: render_rust_template(
                &format!("rust/project/postgres-service/{source}"),
                &data,
            ),
            context: None,
        });
    }
    templates.push(RenderedTemplate { path: dir.join(".env.database.example"), content: "# Separate database per service. Runtime role must not bypass row-level security.\nDATABASE_URL=postgresql://app@localhost:5432/service\nMIGRATION_DATABASE_URL=postgresql://owner@localhost:5432/service\nTENANT_ID=\nSTORAGE_ENCRYPTION_NAMESPACE=service-records\nENCRYPTION_KEY_HEX=\nENCRYPTION_KEY_ID=primary\nENCRYPTION_PREVIOUS_KEYS_JSON=\nENCRYPTION_LEGACY_KEY_HEX=disabled\nDATABASE_ALLOW_PLAINTEXT_DEVELOPMENT=false\n".into(), context: None });
    Ok(templates)
}

pub(super) fn compose(root: &Path, name: &str) -> Result<()> {
    let path = root.join("docker-compose.yaml");
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let service = compose
        .services
        .get_mut(name)
        .context("PostgreSQL service Compose entry")?;
    let env = service.environment.get_or_insert_with(IndexMap::new);
    let prefix = name.to_case(Case::UpperSnake);
    for (variable, value) in [
        ("DATABASE_URL", format!("${{{prefix}_DATABASE_URL:?Configure a restricted service PostgreSQL role}}")),
        ("TENANT_ID", format!("${{{prefix}_TENANT_ID:?Configure a trusted tenant UUID}}")),
        ("STORAGE_ENCRYPTION_NAMESPACE", format!("${{{prefix}_STORAGE_ENCRYPTION_NAMESPACE:?Configure a stable service storage namespace}}")),
        ("ENCRYPTION_KEY_HEX", "${ENCRYPTION_KEY_HEX:?Configure encryption key}".into()),
        ("ENCRYPTION_KEY_ID", "${ENCRYPTION_KEY_ID:-primary}".into()),
        ("ENCRYPTION_PREVIOUS_KEYS_JSON", "${ENCRYPTION_PREVIOUS_KEYS_JSON:-}".into()),
        ("ENCRYPTION_LEGACY_KEY_HEX", "${ENCRYPTION_LEGACY_KEY_HEX:-disabled}".into()),
        ("DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT", "${DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT:-false}".into()),
    ] { env.insert(variable.into(), value); }
    write(path, yaml_to_string(&compose)?)?;
    Ok(())
}

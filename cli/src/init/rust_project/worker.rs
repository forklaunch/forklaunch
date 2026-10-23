use super::*;
mod event_package;
mod service;
pub(super) use service::{templates as service_templates,consumer_contracts};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Backend {
    Postgres,
    Redis,
    Kafka,
}
impl Backend {
    fn variant(self) -> &'static str {
        match self {
            Self::Postgres => "worker-postgres",
            Self::Redis => "worker-redis",
            Self::Kafka => "worker-kafka",
        }
    }
    fn template(self) -> &'static str {
        match self {
            Self::Postgres => "postgres-worker",
            Self::Redis => "redis-worker",
            Self::Kafka => "kafka-worker",
        }
    }
    pub(super) fn from_options(worker_type: &str, database: &str) -> Option<Self> {
        match (worker_type, database) {
            ("database", "postgresql") => Some(Self::Postgres),
            ("redis", _) => Some(Self::Redis),
            ("kafka", _) => Some(Self::Kafka),
            // BullMQ is a TypeScript queue protocol; Rust workers use the native Redis queue.
            ("bullmq", _) => Some(Self::Redis),
            _ => None,
        }
    }
}

pub(super) fn backend(root: &Path, name: &str) -> Result<Option<Backend>> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let variant = project_table(&manifest, name)
        .and_then(|project| project.get("variant"))
        .and_then(Value::as_str);
    match variant {
        Some("worker-postgres") => return Ok(Some(Backend::Postgres)),
        Some("worker-redis") => return Ok(Some(Backend::Redis)),
        Some("worker-kafka") => return Ok(Some(Backend::Kafka)),
        // Earlier releases generated Rust BullMQ workers; they convert to native Redis.
        Some("worker-bullmq") => return Ok(Some(Backend::Redis)),
        _ => {}
    }
    let directory = root
        .join(crate_path_for_project(&manifest, name).unwrap_or_else(|| format!("crates/{name}")));
    let cargo: Value = toml::from_str(&read_to_string(directory.join("Cargo.toml"))?)?;
    if cargo.get("dependencies").and_then(|dependencies| dependencies.get("apalis-postgres")).is_some() {
        return Ok(Some(Backend::Postgres));
    }
    // Current workers keep runtime processors in consumer/, with service exports at the root.
    let native_source = ["consumer/worker.rs", "consumer/worker_hooks.rs", "worker.rs"]
        .into_iter().filter_map(|file| read_to_string(directory.join(file)).ok()).collect::<Vec<_>>().join("\n");
    for (marker, backend) in [("BullMqConsumer", Backend::Redis), ("PostgresWorkerQueue", Backend::Postgres), ("KafkaWorker", Backend::Kafka), ("RedisLeasedQueue", Backend::Redis)] {
        if native_source.contains(marker) { return Ok(Some(backend)); }
    }
    if directory.join("src/handler.rs").exists() {
        let source = read_to_string(directory.join("src/main.rs"))?;
        if source.contains("BullMqConsumer") {
            return Ok(Some(Backend::Redis));
        }
        if source.contains("PostgresWorkerQueue") {
            return Ok(Some(Backend::Postgres));
        }
        if source.contains("KafkaWorker") {
            return Ok(Some(Backend::Kafka));
        }
        if source.contains("RedisLeasedQueue") {
            return Ok(Some(Backend::Redis));
        }
    }
    Ok(None)
}

pub(super) fn templates(
    root: &Path,
    crate_dir: &Path,
    name: &str,
    description: &str,
    backend: Backend,
) -> Result<Vec<RenderedTemplate>> {
    let mut templates = worker_templates(crate_dir, name, description);
    let data = rust_crate_template_data(name, description, true);
    for template in &mut templates {
        if template.path == crate_dir.join("src/main.rs") {
            template.content =
                render_rust_template(&format!("rust/{}/main.rs", backend.template()), &data);
        } else if template.path == crate_dir.join("src/lib.rs") {
            template.content = render_rust_template("rust/leased-worker/lib.rs", &data);
        } else if template.path == crate_dir.join("tests/smoke.rs") {
            template.content = render_rust_template("rust/leased-worker/smoke.rs", &data);
        } else if template.path == crate_dir.join("Cargo.toml") {
            template.content = cargo_with_backend(&template.content, backend, root, crate_dir)?;
        }
    }
    for (output, source) in [
        ("src/handler.rs", "handler.rs"),
        (".env.example", "env.example"),
        ("README.md", "README.md"),
    ] {
        templates.push(RenderedTemplate {
            path: crate_dir.join(output),
            content: render_rust_template(&format!("rust/{}/{source}", backend.template()), &data),
            context: None,
        });
    }
    for (destination, source) in [("src/event_contract.rs", "event_contract.rs"), ("persistence/compliance_schema.rs", "compliance_schema.rs"), ("build.rs", "build.rs")] {
        templates.push(RenderedTemplate { path: crate_dir.join(destination),
            content: render_rust_template(&format!("rust/leased-worker/{source}"), &data), context: None });
    }
    templates.push(RenderedTemplate {
        path: crate_dir.join("src/events.rs"),
        content: render_rust_template("rust/leased-worker/events.rs", &data),
        context: None,
    });
    templates.push(RenderedTemplate {
        path: crate_dir.join("src/signals.rs"),
        content: render_rust_template(
            if backend == Backend::Kafka {
                "rust/kafka-worker/signals.rs"
            } else {
                "rust/leased-worker/signals.rs"
            },
            &data,
        ),
        context: None,
    });
    templates.extend(event_package::templates(crate_dir, name)?);
    Ok(templates)
}

fn cargo_with_backend(
    content: &str,
    backend: Backend,
    root: &Path,
    directory: &Path,
) -> Result<String> {
    let mut cargo: toml_edit::DocumentMut = content.parse()?;
    for (name, path, features) in [
        (
            "forklaunch-worker-database",
            "../../.forklaunch/blueprint-rs/crates/implementations/worker/worker-database",
            if backend == Backend::Postgres {
                vec!["postgres", "http-control"]
            } else {
                vec!["http-control"]
            },
        ),
        (
            "forklaunch-worker-redis",
            "../../.forklaunch/blueprint-rs/crates/implementations/worker/worker-redis",
            vec!["redis-client"],
        ),
        (
            "forklaunch-worker-interface",
            "../../.forklaunch/blueprint-rs/crates/interfaces/worker-interface",
            vec![],
        ),
    ] {
        let mut dependency = toml_edit::InlineTable::new();
        let absolute = root
            .join(".forklaunch")
            .join(path.trim_start_matches("../../.forklaunch/"));
        let relative = crate::core::relative_path::get_relative_path(directory, &absolute);
        dependency.insert("path", relative.to_string_lossy().as_ref().into());
        if !features.is_empty() {
            dependency.insert("features", toml_edit::Array::from_iter(features).into());
        }
        cargo["dependencies"][name] = toml_edit::value(dependency);
    }
    // Conversion rewrites the executable, so enable the lifecycle it now uses
    // even when the original crate predates generated OTLP startup.
    if cargo["dependencies"]["forklaunch-core"]["features"].is_none() {
        cargo["dependencies"]["forklaunch-core"]["features"] =
            toml_edit::value(toml_edit::Array::new());
    }
    let core_features = cargo["dependencies"]["forklaunch-core"]["features"]
        .as_array_mut()
        .context("Expected forklaunch-core feature array")?;
    if !core_features
        .iter()
        .any(|feature| feature.as_str() == Some("otlp"))
    {
        core_features.push("otlp");
    }
    if !core_features.iter().any(|feature| feature.as_str() == Some("encryption")) { core_features.push("encryption"); }
    // Shared module types (errors, paging, audit context) are framework `domain`.
    if !core_features.iter().any(|feature| feature.as_str() == Some("domain")) { core_features.push("domain"); }
    cargo["dependencies"]["serde_json"] = toml_edit::value(toml_edit::InlineTable::new());
    cargo["package"]["build"] = toml_edit::value("build.rs");
    cargo["dependencies"]["forklaunch-worker-interface"]["features"] = toml_edit::value(toml_edit::Array::from_iter(["schema"]));
    let mut schema = toml_edit::InlineTable::new();
    schema.insert("features", toml_edit::Array::from_iter(["uuid1"]).into());
    cargo["dependencies"]["schemars"] = toml_edit::value(schema);
    cargo["dependencies"]["zeroize"] = toml_edit::value(toml_edit::InlineTable::new());
    for (name, features) in [("serde", vec!["derive"]), ("uuid", vec!["serde", "v4"])] {
        let mut dependency = toml_edit::InlineTable::new();
        dependency.insert("features", toml_edit::Array::from_iter(features).into());
        cargo["dependencies"][name] = toml_edit::value(dependency);
    }

    for name in ["forklaunch-core", "forklaunch-worker-interface", "serde", "serde_json", "uuid", "zeroize", "schemars"] {
        cargo["build-dependencies"][name] = cargo["dependencies"][name].clone();
    }
    cargo["dependencies"]["forklaunch-worker-interface"]["features"] = toml_edit::value(toml_edit::Array::from_iter(["schema", "telemetry"]));
    cargo["build-dependencies"]["forklaunch-core"]["features"] = toml_edit::value(toml_edit::Array::from_iter(["compliance-artifact", "encryption", "build"]));

    if backend == Backend::Kafka {
        let mut dependency = toml_edit::InlineTable::new();
        let target =
            root.join(".forklaunch/blueprint-rs/crates/implementations/worker/worker-kafka");
        let relative = crate::core::relative_path::get_relative_path(directory, &target);
        dependency.insert("path", relative.to_string_lossy().as_ref().into());
        dependency.insert(
            "features",
            toml_edit::Array::from_iter(["kafka-client"]).into(),
        );
        cargo["dependencies"]["forklaunch-worker-kafka"] = toml_edit::value(dependency);
    }
    if backend == Backend::Postgres {
        let mut sqlx = toml_edit::InlineTable::new();
        sqlx.insert("default-features", false.into());
        sqlx.insert(
            "features",
            toml_edit::Array::from_iter(["runtime-tokio", "tls-rustls", "postgres"]).into(),
        );
        cargo["dependencies"]["sqlx"] = toml_edit::value(sqlx);
    }
    crate::core::cargo_toml::fill(&cargo.to_string())
}

pub(super) fn convert(root: &Path, name: &str, backend: Backend) -> Result<()> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let directory = root
        .join(crate_path_for_project(&manifest, name).unwrap_or_else(|| format!("crates/{name}")));
    if directory.join("lib.rs").exists() {
        let description = cargo_description(&directory.join("Cargo.toml"))?.unwrap_or_else(|| "Durable worker".into());
        let templates = templates(root, &directory, name, &description, backend)?;
        let templates =
            native_layout::flatten(&directory, name, RustProjectKind::Worker, templates);
        let templates = if directory.join("domain/schemas/event_contract.rs").is_file()
            && contracts_layout::directory(&directory).is_some()
        {
            consumer_contracts(&directory, name, templates)?
        } else { templates };
        let templates = native_mappers::templates(&directory, false, templates)?;
        let templates = native_layout::framework_paths(root, templates);
        native_layout::apply_conversion(&directory, RustProjectKind::Worker, templates)?;
        mark(root, name, backend)?;
        return compose(root, name, backend);
    }
    let cargo = directory.join("Cargo.toml");
    write(
        &cargo,
        cargo_with_backend(&read_to_string(&cargo)?, backend, root, &directory)?,
    )?;
    let data = rust_crate_template_data(name, "Durable worker", true);
    create_dir_all(directory.join("persistence"))?;
    if !directory.join("persistence/compliance_schema.rs").exists() {
        write(directory.join("persistence/compliance_schema.rs"), render_rust_template("rust/leased-worker/compliance_schema.rs", &data))?;
    }
    write(directory.join("src/event_contract.rs"), render_rust_template("rust/leased-worker/event_contract.rs", &data))?;
    write(directory.join("build.rs"), render_rust_template("rust/leased-worker/build.rs", &data).replace("domain/schemas/event_contract.rs", "src/event_contract.rs"))?;
    write(directory.join("src/events.rs"), render_rust_template("rust/leased-worker/events.rs", &data))?;
    write(
        directory.join("src/main.rs"),
        render_rust_template(&format!("rust/{}/main.rs", backend.template()), &data),
    )?;
    write(
        directory.join("src/lib.rs"),
        render_rust_template("rust/leased-worker/lib.rs", &data),
    )?;
    write(
        directory.join("tests/smoke.rs"),
        render_rust_template("rust/leased-worker/smoke.rs", &data),
    )?;
    write(
        directory.join("src/signals.rs"),
        render_rust_template(
            if backend == Backend::Kafka {
                "rust/kafka-worker/signals.rs"
            } else {
                "rust/leased-worker/signals.rs"
            },
            &data,
        ),
    )?;
    // Handler code and user documentation survive backend changes.
    for (output, template) in [
        ("src/handler.rs", "handler.rs"),
        (".env.example", "env.example"),
        ("README.md", "README.md"),
    ] {
        let path = directory.join(output);
        if !path.exists() {
            write(
                path,
                render_rust_template(&format!("rust/{}/{template}", backend.template()), &data),
            )?;
        }
    }
    mark(root, name, backend)?;
    compose(root, name, backend)
}

pub(super) fn mark(root: &Path, name: &str, backend: Backend) -> Result<()> {
    let path = root.join(".forklaunch/manifest.toml");
    let mut manifest = read_manifest_value(&path)?;
    let directory = root
        .join(crate_path_for_project(&manifest, name).unwrap_or_else(|| format!("crates/{name}")));
    // Sync preserves application-owned sources. Do not advertise a new role for
    // an older/custom main that has not adopted the generated entry point.
    let runtime = if directory.join("lib.rs").exists() {
        directory.join("worker.rs")
    } else {
        directory.join("src/main.rs")
    };
    let signals = if directory.join("lib.rs").exists() {
        directory.join("signals.rs")
    } else {
        directory.join("src/signals.rs")
    };
    let supports_control = read_to_string(runtime).is_ok_and(|source| {
        source.contains("if args == [\"--control-service\"]")
            && source.contains("signals::serve_management")
    }) && read_to_string(signals)
        .is_ok_and(|source| source.contains("pub async fn serve_management"));
    let project = project_table_mut(&mut manifest, name).context("Expected worker project")?;
    project.insert("variant".into(), Value::String(backend.variant().into()));
    let metadata = project
        .entry("metadata")
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .context("Expected worker metadata")?;
    if supports_control {
        metadata.insert("workerControlService".into(), Value::Boolean(true));
    } else {
        metadata.remove("workerControlService");
    }
    metadata.insert(
        "workerType".into(),
        Value::String(
            match backend {
                Backend::Postgres => "database",
                Backend::Redis => "redis",
                Backend::Kafka => "kafka",
            }
            .into(),
        ),
    );
    let resources = project
        .entry("resources")
        .or_insert_with(|| Value::Table(Map::new()))
        .as_table_mut()
        .context("Expected worker resources")?;
    for key in ["database", "cache", "queue"] {
        resources.remove(key);
    }
    let (resource, value) = match backend {
        Backend::Postgres => ("database", "postgresql"),
        Backend::Redis => ("cache", "redis"),
        Backend::Kafka => ("queue", "kafka"),
    };
    resources.insert(resource.into(), Value::String(value.into()));
    write_manifest_value(&path, &manifest)
}

pub(super) fn compose(root: &Path, name: &str, backend: Backend) -> Result<()> {
    ensure_rust_docker_compose_project(root, RustProjectKind::Worker, name, false)?;
    let path = root.join("docker-compose.yaml");
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let producer_port = next_rust_docker_port(&compose);
    let service = compose
        .services
        .get_mut(name)
        .context("Expected worker Compose service")?;
    let env = service.environment.get_or_insert_with(IndexMap::new);
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    // Settings are named per worker so two workers never share a database or queue.
    let prefix = name.to_case(Case::UpperSnake);
    // Remove obsolete generated requirements when switching brokers, including the
    // earlier shared `WORKER_` names. Explicit application values remain untouched.
    for owner in [prefix.as_str(), "WORKER"] {
        for (key, expected, keep) in [
            ("DATABASE_URL", format!("${{{owner}_DATABASE_URL:?Configure a restricted PostgreSQL role}}"), backend == Backend::Postgres),
            ("REDIS_URL", format!("${{{owner}_REDIS_URL:?Configure Redis}}"), backend == Backend::Redis),
            ("KAFKA_BROKERS", format!("${{{owner}_KAFKA_BROKERS:?Configure Kafka brokers}}"), backend == Backend::Kafka),
            ("KAFKA_TRANSACTIONAL_ID", format!("${{{owner}_KAFKA_TRANSACTIONAL_ID:?Configure a stable unique identity for {name}}}"), backend == Backend::Kafka),
            ("KAFKA_PUBLISHER_ID", format!("${{{owner}_KAFKA_PUBLISHER_ID:?Configure a stable publisher identity for {name}}}"), backend == Backend::Kafka),
            ("REDIS_KEY_PREFIX", format!("${{{owner}_REDIS_KEY_PREFIX:?Configure the application Redis ACL namespace}}"), false),
        ] {
            if !keep && env.get(key) == Some(&expected) { env.shift_remove(key); }
        }
    }
    let native_apalis = project_table(&manifest, name)
        .and_then(|project| project.get("metadata"))
        .and_then(|metadata| metadata.get("workerRuntime"))
        .and_then(Value::as_str) == Some("apalis");
    if native_apalis {
        env.entry("DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT".into())
            .or_insert_with(|| "${DATABASE_ALLOW_PLAINTEXT_DEVELOPMENT:-false}".into());
    }
    match backend {
        Backend::Postgres => {
            env.entry("DATABASE_URL".into()).or_insert_with(|| {
                format!("${{{prefix}_DATABASE_URL:?Configure a restricted PostgreSQL role}}")
            });
        }
        Backend::Kafka => {
            env.entry("KAFKA_BROKERS".into())
                .or_insert_with(|| format!("${{{prefix}_KAFKA_BROKERS:?Configure Kafka brokers}}"));
            env.entry("WORKER_NAMESPACE".into())
                .or_insert_with(|| format!("${{{prefix}_NAMESPACE:?Configure a worker namespace}}"));
            env.entry("KAFKA_TRANSACTIONAL_ID".into()).or_insert_with(|| format!("${{{prefix}_KAFKA_TRANSACTIONAL_ID:?Configure a stable unique identity for {name}}}"));
            // The producer (`POST /jobs`, `--enqueue`) publishes under its own identity.
            env.entry("KAFKA_PUBLISHER_ID".into()).or_insert_with(|| format!("${{{prefix}_KAFKA_PUBLISHER_ID:?Configure a stable publisher identity for {name}}}"));
        }
        Backend::Redis => {
            env.entry("REDIS_URL".into())
                .or_insert_with(|| format!("${{{prefix}_REDIS_URL:?Configure Redis}}"));
            if backend == Backend::Redis {
                env.entry("WORKER_NAMESPACE".into())
                    .or_insert_with(|| format!("${{{prefix}_NAMESPACE:?Configure a worker namespace}}"));
            }
        }
    }
    env.entry("TENANT_ID".into())
        .or_insert_with(|| format!("${{{prefix}_TENANT_ID:?Configure a trusted tenant UUID}}"));
    // Runner settings are required at startup; these match `.env.example`.
    let runner: &[(&str, &str)] = match backend {
        _ if native_apalis => &[],
        Backend::Postgres | Backend::Redis => &[
            ("WORKER_MAX_ATTEMPTS", "3"), ("WORKER_BACKOFF_SECONDS", "30"), ("WORKER_CONCURRENCY", "1"),
            ("WORKER_BATCH_SIZE", "1"), ("WORKER_POLL_MS", "250"), ("WORKER_LEASE_MS", "30000"),
            ("WORKER_RENEW_MS", "10000"), ("WORKER_DRAIN_MS", "30000"),
        ],
        Backend::Kafka => &[
            ("KAFKA_GROUP_ID", "workers"), ("KAFKA_PARTITIONS", "3"), ("KAFKA_REPLICATION_FACTOR", "1"),
            ("KAFKA_OPERATION_TIMEOUT_MS", "5000"), ("KAFKA_SESSION_TIMEOUT_MS", "10000"),
            ("WORKER_MAX_PAYLOAD_BYTES", "1048576"), ("WORKER_MAX_ATTEMPTS", "3"),
            ("WORKER_BACKOFF_SECONDS", "30"), ("WORKER_HANDLER_TIMEOUT_MS", "300000"), ("WORKER_DRAIN_MS", "30000"),
        ],
    };
    if !native_apalis {
        env.entry("WORKER_QUEUE".into()).or_insert_with(|| name.into());
    }
    for (key, value) in runner {
        env.entry((*key).into()).or_insert_with(|| (*value).into());
    }
    for (name, value) in [
        ("WORKER_ALLOW_PLAINTEXT_DEVELOPMENT", "${WORKER_ALLOW_PLAINTEXT_DEVELOPMENT:-false}"),
        ("ENCRYPTION_KEY_HEX", "${ENCRYPTION_KEY_HEX:?Configure a 32-byte worker encryption key}"),
        ("ENCRYPTION_KEY_ID", "${ENCRYPTION_KEY_ID:-primary}"),
        ("ENCRYPTION_PREVIOUS_KEYS_JSON", "${ENCRYPTION_PREVIOUS_KEYS_JSON:-}"),
        ("ENCRYPTION_LEGACY_KEY_HEX", "${ENCRYPTION_LEGACY_KEY_HEX:-disabled}"),
    ] { env.entry(name.into()).or_insert_with(|| value.into()); }
    let has_service = project_table(&manifest, name).and_then(|project|project.get("metadata")).and_then(|metadata|metadata.get("workerService")).and_then(Value::as_bool) == Some(true);
    if has_service {
        service.command = Some(DockerCommand::Multiple(vec![format!("./target/release/{name}-worker")]));
        let mut producer = service.clone();
        producer.command = Some(DockerCommand::Multiple(vec![format!("./target/release/{name}")]));
        producer.hostname = Some(format!("{name}-server"));
        producer.container_name = None;
        producer.additional_properties.insert("x-forklaunch-worker-for".into(), serde_yml::Value::String(name.into()));
        producer.ports = Some(vec![format!("127.0.0.1:{producer_port}:3000")]);
        let environment = producer.environment.get_or_insert_with(IndexMap::new);
        environment.insert("HOST".into(), "0.0.0.0".into());
        environment.insert("PORT".into(), "3000".into());
        environment.insert("MCP_ENABLED".into(), "true".into());
        environment.entry("PUBLIC_ORIGIN".into()).or_insert_with(|| format!("http://localhost:{producer_port}"));
        // Internal callers sign producer requests with the shared service key.
        environment.entry("INTERNAL_HMAC_SECRET_KEY".into()).or_insert_with(|| "${INTERNAL_HMAC_SECRET_KEY:?Configure the shared internal service HMAC key}".into());
        let consumer = compose.services[name].clone();
        let producer = compose.services.entry(format!("{name}-server")).or_insert(producer);
        // An existing producer gains the queue settings the consumer gained
        // (a backend change); its own listener settings stay.
        let environment = producer.environment.get_or_insert_with(IndexMap::new);
        for (key, value) in consumer.environment.iter().flatten() {
            if !matches!(key.as_str(), "HOST" | "PORT" | "MCP_ENABLED" | "PUBLIC_ORIGIN") {
                environment.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        for (key, condition) in consumer.depends_on.iter().flatten() {
            producer.depends_on.get_or_insert_with(IndexMap::new).entry(key.clone()).or_insert_with(|| condition.clone());
        }
        repair_legacy_producer_origin(producer, name);
    }
    write(path, yaml_to_string(&compose)?)?;
    language_images::reconcile(root)
}

pub(super) fn rename_sources(root: &Path, old: &str, new: &str) -> Result<()> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let directory = root
        .join(crate_path_for_project(&manifest, new).unwrap_or_else(|| format!("crates/{new}")));
    if directory.join("consumer/lib.rs").is_file() {
        let cargo_path=directory.join("Cargo.toml");
        let mut cargo:toml_edit::DocumentMut=read_to_string(&cargo_path)?.parse()?;
        if let Some(binaries)=cargo.get_mut("bin").and_then(toml_edit::Item::as_array_of_tables_mut) {
            for binary in binaries.iter_mut() {
                if binary.get("path").and_then(toml_edit::Item::as_str)==Some("worker.rs") { binary["name"]=toml_edit::value(format!("{new}-worker")); }
            }
        }
        write(cargo_path,cargo.to_string())?;
    }
    for relative in ["src/main.rs", "tests/smoke.rs"] {
        let path = directory.join(relative);
        if path.exists() {
            let source = read_to_string(&path)?.replace(
                &format!("use {}::", old.to_case(Case::Snake)),
                &format!("use {}::", new.to_case(Case::Snake)),
            );
            write(path, source)?;
        }
    }
    for relative in ["src/lib.rs", "src/config/mod.rs"] {
        let path = directory.join(relative);
        if path.exists() {
            write(
                &path,
                read_to_string(&path)?.replace(&format!("\"{old}\""), &format!("\"{new}\"")),
            )?;
        }
    }
    // The generated queue name follows the worker; explicit names stay.
    let example = directory.join(".env.example");
    if example.is_file() {
        let source = read_to_string(&example)?;
        let renamed = source.replace(&format!("\nWORKER_QUEUE={old}\n"), &format!("\nWORKER_QUEUE={new}\n"));
        if renamed != source {
            write(&example, renamed)?;
        }
    }
    let path = root.join("docker-compose.yaml");
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    for service in [old.to_owned(), format!("{old}-server")] {
        if let Some(queue) = compose.services.get_mut(&service)
            .and_then(|service| service.environment.as_mut())
            .and_then(|env| env.get_mut("WORKER_QUEUE"))
            .filter(|queue| queue.as_str() == old)
        {
            *queue = new.to_owned();
        }
    }
    if let Some(mut producer) = compose.services.shift_remove(&format!("{old}-server")) {
        if producer_owner(&producer) == Some(old) {
            producer.hostname = Some(format!("{new}-server"));
            repair_legacy_producer_origin(&mut producer, old);
            producer.command = Some(DockerCommand::Multiple(vec![format!("./target/release/{new}")]));
            producer.additional_properties.insert("x-forklaunch-worker-for".into(), serde_yml::Value::String(new.into()));
            compose.services.insert(format!("{new}-server"), producer);
        } else { compose.services.insert(format!("{old}-server"), producer); }
    }
    if let Some(mut service) = compose.services.shift_remove(old) {
        service.hostname = Some(new.to_owned());
        let consumer = directory.join("consumer/lib.rs").is_file();
        let binary = if consumer { format!("{new}-worker") } else { new.to_owned() };
        service.command = Some(DockerCommand::Multiple(vec![format!("./target/release/{binary}")]));
        compose.services.insert(new.to_owned(), service);
    }
    write(path, yaml_to_string(&compose)?)?;
    Ok(())
}

/// An ordinary native Apalis profile with protected lifecycle/build entrypoints
/// and explicit application-owned backend and asset hooks.
/// The application-owned apalis processor. `JobState` is built once at worker start and
/// injected into every job, so processors reuse pools and SDK clients.
const APALIS_HANDLER: &str = r#"//! Replace this fail-closed hook with an authorized audited domain processor.
use apalis_core::error::BoxDynError;
use forklaunch_worker_interface::{ProcessorError, SealedEvent, apalis, native::RuntimeError};

/// Resources shared by every job, built once when the worker starts and injected
/// with `Data<JobState>`: the runtime pool, plus the SDK clients, keys or caches the
/// processor needs (for example a generated client with `with_hmac_secret`).
#[derive(Clone)]
pub struct JobState {
    pub pool: sqlx::PgPool,
}
impl JobState {
    pub async fn new(pool: sqlx::PgPool) -> Result<Self, RuntimeError> {
        Ok(Self { pool })
    }
}

pub async fn process_job(_event: SealedEvent, _state: ::apalis::prelude::Data<JobState>) -> Result<(), BoxDynError> {
    Err(apalis::queue_error(ProcessorError::Configuration))
}
"#;

pub(super) fn apalis_templates(root: &Path, dir: &Path, name: &str, description: &str) -> Result<Vec<RenderedTemplate>> {
    let data=rust_crate_template_data(name,description,true);
    let relative=crate::core::relative_path::get_relative_path(dir,&root.join(".forklaunch/framework-rs")).to_string_lossy().replace('\\',"/");
    let worker_path=crate::core::relative_path::get_relative_path(dir,&root.join(".forklaunch/blueprint-rs/crates/interfaces/worker-interface")).to_string_lossy().replace('\\',"/");
    let mut files=Vec::new();
    for file in ["main.rs","lib.rs","bootstrapper.rs","worker.rs","worker_hooks.rs","registrations.rs","build.rs"] {
        files.push(RenderedTemplate{path:dir.join(file),content:render_rust_template(&format!("rust/apalis-worker/{file}"),&data),context:None});
    }
    let cargo=format!(r#"[package]
name = {name:?}
version = "0.1.0"
edition = "2024"
[lib]
path = "lib.rs"
[[bin]]
name = {name:?}
path = "main.rs"
[dependencies]
forklaunch-worker-interface = {{ package = "forklaunch-worker-interface", path = "{worker_path}", features = ["native", "apalis", "telemetry", "schema"] }}
forklaunch-core = {{ package = "forklaunch-core", path = "{relative}/crates/core", features = ["postgres", "otlp", "encryption"] }}
apalis = {{}}
apalis-core = {{}}
apalis-postgres = {{}}
sqlx = {{ default-features = false, features = ["runtime-tokio", "tls-rustls", "postgres", "uuid"] }}
tokio = {{ features = ["macros", "rt-multi-thread", "signal", "time"] }}
serde = {{ features = ["derive"] }}
serde_json = {{}}
uuid = {{ features = ["v4", "serde"] }}
schemars = {{ features = ["uuid1"] }}
zeroize = {{}}
[build-dependencies]
forklaunch-core = {{ package = "forklaunch-core", path = "{relative}/crates/core", features = ["encryption", "build"] }}
forklaunch-worker-interface = {{ package = "forklaunch-worker-interface", path = "{worker_path}", features = ["schema"] }}
serde = {{ features = ["derive"] }}
serde_json = {{}}
uuid = {{ features = ["serde"] }}
schemars = {{ features = ["uuid1"] }}
zeroize = {{}}
"#);
    let cargo = crate::core::cargo_toml::fill(&cargo)?;
    let event=render_rust_template("rust/leased-worker/event_contract.rs",&data);
    for (path,content) in [
        ("Cargo.toml",cargo),
        ("domain/mod.rs","pub mod services;\npub mod schemas;\npub use schemas::event_contract::{WorkInput,WorkEvent};\n".into()),
        ("domain/services/mod.rs","pub mod handler;\n".into()),
        ("domain/services/handler.rs",APALIS_HANDLER.into()),
        ("domain/schemas/mod.rs","pub mod event_contract;\n".into()),
        ("domain/schemas/event_contract.rs",event),
        ("persistence/mod.rs","//! Application-owned persistence extensions.\n".into()),
        ("persistence/compliance_schema.rs",render_rust_template("rust/leased-worker/compliance_schema.rs",&data).replace("#[compliance(entity =", "#[compliance(crate_path = \"forklaunch_core\", entity =").replace("job.payload_json", "apalis.jobs.job")),
        ("migrations/mod.rs","use apalis_postgres::PostgresStorage;\nuse forklaunch_worker_interface::native::RuntimeError;\nuse sqlx::PgPool;\n\npub async fn run(pool:&PgPool)->Result<(),RuntimeError> { PostgresStorage::setup(pool).await?; Ok(()) }\n".into()),
        ("build_assets.rs","#[allow(dead_code)]\n#[path=\"domain/schemas/event_contract.rs\"]\nmod event_contract;\nuse std::error::Error;\nuse event_contract::{WorkEvent, WorkInput};\nuse forklaunch_core::build;\n\npub fn generate()->Result<(),Box<dyn Error>> {\n println!(\"cargo:rerun-if-changed=domain/schemas/event_contract.rs\");\n let input=serde_json::to_value(schemars::schema_for!(WorkInput))?;\n let event=serde_json::to_value(schemars::schema_for!(WorkEvent))?;\n build::worker_events(env!(\"CARGO_PKG_NAME\"), &input, &event, include_str!(\"domain/schemas/event_contract.rs\"), \"events\")?; Ok(())\n}\n".into()),
        (".env.example","DATABASE_URL=postgres://worker:change-me@localhost:5432/worker?sslmode=require\n".into()),
        ("README.md","# Native PostgreSQL worker\n\nApalis owns queue leasing and schema. Run `cargo run -- --migrate` with a migration-owner URL before starting the runtime role. `worker_hooks.rs` assembles the typed native backend and migrations; `build_assets.rs` exports application event schemas. Lifecycle/build entrypoints remain generated. The default domain handler fails closed until application authorization, audited decryption and processing are implemented. Runtime plaintext database transport requires the explicitly named local development override.\n".into()),
        ("Makefile",native_layout::makefile(name,true)),
    ] {files.push(RenderedTemplate{path:dir.join(path),content,context:None});}
    files.extend(event_package::templates(dir, name)?);
    Ok(files)
}

/// Ownership marker prevents deleting unrelated similarly named Compose services.
pub(super) fn producer_owner(service: &DockerService) -> Option<&str> {
    service.additional_properties.get("x-forklaunch-worker-for")?.as_str()
}

// Only replace the obsolete generated internal hostname. Application origins and
// custom/unresolved port bindings are authoritative and must not be guessed.
fn repair_legacy_producer_origin(service: &mut DockerService, name: &str) {
    let legacy = format!("http://{name}-server:3000");
    if producer_owner(service) != Some(name)
        || service
            .environment
            .as_ref()
            .and_then(|env| env.get("PUBLIC_ORIGIN"))
            != Some(&legacy)
    {
        return;
    }
    let Some(origin) = producer_loopback_origin(service) else {
        return;
    };
    service
        .environment
        .as_mut()
        .unwrap()
        .insert("PUBLIC_ORIGIN".into(), origin);
}

fn producer_loopback_origin(service: &DockerService) -> Option<String> {
    let target = service
        .environment
        .as_ref()
        .and_then(|env| env.get("PORT"))
        .map(String::as_str)
        .unwrap_or("3000")
        .parse::<u16>()
        .ok()?;
    let mut published = service.ports.as_ref()?.iter().filter_map(|binding| {
        let (binding, protocol) = binding.split_once('/').unwrap_or((binding, "tcp"));
        if protocol != "tcp" {
            return None;
        }
        let (host, port) = binding.rsplit_once(':')?;
        if port.parse::<u16>().ok()? != target {
            return None;
        }
        let (address, port) = host.rsplit_once(':').unwrap_or(("", host));
        if !matches!(
            address,
            "" | "127.0.0.1" | "0.0.0.0" | "[::1]" | "[::]" | "::1" | "::"
        ) {
            return None;
        }
        port.parse::<u16>().ok().filter(|port| *port != 0)
    });
    let port = published.next()?;
    if published.any(|other| other != port) {
        return None;
    }
    Some(format!("http://localhost:{port}"))
}

/// A producer worker's generated files for `backend`, as `init worker` writes them.
pub(super) fn producer_templates(root: &Path, dir: &Path, name: &str, description: &str, backend: Backend) -> Result<Vec<RenderedTemplate>> {
    let worker = templates(root, dir, name, description, backend)?;
    let worker = native_layout::flatten(dir, name, RustProjectKind::Worker, worker);
    let files = service_templates(dir, name, description, worker, false, backend)?;
    let files = native_mappers::templates(dir, dir.join("domain/mappers/health.rs").is_file(), files)?;
    Ok(native_layout::framework_paths(root, files))
}

/// Move a producer worker (HTTP/MCP producer plus consumer) to another queue
/// backend. Only files still exactly as generated for the current backend are
/// replaced, application-added Cargo dependencies are kept, and Compose and
/// `.env` follow. Anything customized stops the change before a write.
pub(super) fn change_producer_backend(root: &Path, name: &str, from: Backend, to: Backend, dryrun: bool) -> Result<Vec<String>> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let dir = root.join(crate_path_for_project(&manifest, name).context("Worker crate path missing")?);
    anyhow::ensure!(dir.join("consumer/lib.rs").is_file(), "Worker {name} is not a producer worker");
    let description = cargo_description(&dir.join("Cargo.toml"))?.unwrap_or_default();
    let current = producer_templates(root, &dir, name, &description, from)?;
    let target = producer_templates(root, &dir, name, &description, to)?;
    let planned = super::kind_change::replace_generated(root, &dir, current, target, dryrun)?;
    if dryrun {
        return Ok(planned);
    }
    mark(root, name, to)?;
    switch_compose_backend(root, name, from, to)?;
    compose(root, name, to)?;
    local_runtime::prune_project_env(root, name, &[
        "DATABASE_URL", "MIGRATION_DATABASE_URL", "SQLX_OFFLINE", "REDIS_URL", "WORKER_NAMESPACE",
        "KAFKA_BROKERS", "KAFKA_TRANSACTIONAL_ID", "KAFKA_PUBLISHER_ID", "KAFKA_GROUP_ID", "KAFKA_PARTITIONS",
        "KAFKA_REPLICATION_FACTOR", "KAFKA_OPERATION_TIMEOUT_MS", "KAFKA_SESSION_TIMEOUT_MS",
        "WORKER_MAX_PAYLOAD_BYTES", "WORKER_HANDLER_TIMEOUT_MS", "WORKER_BATCH_SIZE", "WORKER_POLL_MS",
        "WORKER_LEASE_MS", "WORKER_RENEW_MS", "WORKER_CONCURRENCY",
    ])?;
    Ok(planned)
}

/// Settings and one-shot jobs only the previous backend used. Values that are
/// not the generated ones stay.
fn switch_compose_backend(root: &Path, name: &str, from: Backend, to: Backend) -> Result<()> {
    let path = root.join("docker-compose.yaml");
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let prefix = name.to_case(Case::UpperSnake);
    let generated: &[(&str, String)] = &match from {
        Backend::Postgres => vec![("DATABASE_URL", format!("${{{prefix}_DATABASE_URL:?Configure a restricted PostgreSQL role}}"))],
        Backend::Redis => vec![
            ("REDIS_URL", format!("${{{prefix}_REDIS_URL:?Configure Redis}}")),
            ("WORKER_NAMESPACE", format!("${{{prefix}_NAMESPACE:?Configure a worker namespace}}")),
        ],
        Backend::Kafka => vec![
            ("KAFKA_BROKERS", format!("${{{prefix}_KAFKA_BROKERS:?Configure Kafka brokers}}")),
            ("WORKER_NAMESPACE", format!("${{{prefix}_NAMESPACE:?Configure a worker namespace}}")),
            ("KAFKA_TRANSACTIONAL_ID", format!("${{{prefix}_KAFKA_TRANSACTIONAL_ID:?Configure a stable unique identity for {name}}}")),
            ("KAFKA_PUBLISHER_ID", format!("${{{prefix}_KAFKA_PUBLISHER_ID:?Configure a stable publisher identity for {name}}}")),
        ],
    };
    let leased = |backend: Backend| backend != Backend::Kafka;
    let runner: &[(&str, &str)] = match (leased(from), leased(to)) {
        (true, false) => &[("WORKER_BATCH_SIZE", "1"), ("WORKER_POLL_MS", "250"), ("WORKER_LEASE_MS", "30000"), ("WORKER_RENEW_MS", "10000"), ("WORKER_CONCURRENCY", "1")],
        (false, true) => &[("KAFKA_GROUP_ID", "workers"), ("KAFKA_PARTITIONS", "3"), ("KAFKA_REPLICATION_FACTOR", "1"), ("KAFKA_OPERATION_TIMEOUT_MS", "5000"), ("KAFKA_SESSION_TIMEOUT_MS", "10000"), ("WORKER_MAX_PAYLOAD_BYTES", "1048576"), ("WORKER_HANDLER_TIMEOUT_MS", "300000")],
        _ => &[],
    };
    // Redis and Kafka both name their worker namespace.
    let generated: Vec<(&str, String)> = generated.iter().filter(|(key, _)| !(*key == "WORKER_NAMESPACE" && to != Backend::Postgres)).cloned().collect();
    let generated = generated.as_slice();
    let processes: Vec<String> = compose.services.iter()
        .filter(|(key, service)| *key == name || producer_owner(service) == Some(name))
        .map(|(key, _)| key.clone())
        .collect();
    for key in &processes {
        let service = compose.services.get_mut(key).unwrap();
        if let Some(env) = service.environment.as_mut() {
            env.retain(|variable, value| {
                !generated.iter().any(|(generated, expected)| generated == variable && expected == value)
                    && !runner.iter().any(|(generated, expected)| generated == variable && expected == value)
            });
        }
        // The broker the process no longer reads.
        let env = service.environment.clone().unwrap_or_default();
        if let Some(depends) = service.depends_on.as_mut() {
            if from == Backend::Redis && !env.contains_key("REDIS_URL") { depends.shift_remove("redis"); }
            if from == Backend::Kafka && !env.contains_key("KAFKA_BROKERS") { depends.shift_remove("kafka"); }
            if depends.is_empty() { service.depends_on = None; }
        }
    }
    // The previous backend's migration job (tables or topics) is regenerated
    // for the new backend when it needs one.
    let migrate = format!("{name}-migrate");
    compose.services.shift_remove(&migrate);
    for service in compose.services.values_mut() {
        if let Some(depends) = service.depends_on.as_mut() {
            depends.shift_remove(&migrate);
            if depends.is_empty() { service.depends_on = None; }
        }
    }
    write(&path, yaml_to_string(&compose)?)?;
    // The producer runs with the consumer's settings.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        register_worker(root.path(), name);
        root
    }

    fn register_worker(root: &Path, name: &str) {
        write(root.join(".forklaunch/manifest.toml"), format!(
            "app_name='demo'\nlanguage='rust'\nruntime='tokio'\nprojects=[{{name='{name}',type='Worker',metadata={{language='rust',workerService=true}}}}]\n"
        )).unwrap();
    }

    fn producer(root: &Path, name: &str) -> DockerService {
        read_or_create_rust_docker_compose(&root.join("docker-compose.yaml"))
            .unwrap()
            .services
            .shift_remove(&format!("{name}-server"))
            .unwrap()
    }

    fn update_producer(root: &Path, name: &str, origin: &str, binding: &str) {
        let path = root.join("docker-compose.yaml");
        let mut compose = read_or_create_rust_docker_compose(&path).unwrap();
        let producer = compose.services.get_mut(&format!("{name}-server")).unwrap();
        producer
            .environment
            .as_mut()
            .unwrap()
            .insert("PUBLIC_ORIGIN".into(), origin.into());
        producer.ports = Some(vec![binding.into()]);
        write(path, yaml_to_string(&compose).unwrap()).unwrap();
    }

    #[test]
    fn worker_producer_origin_uses_published_loopback_for_every_backend_and_reinit() {
        for backend in [
            Backend::Postgres,
            Backend::Redis,
            Backend::Kafka,
        ] {
            let root = fixture("jobs");
            compose(root.path(), "jobs", backend).unwrap();
            let initial = producer(root.path(), "jobs");
            assert_eq!(initial.ports.as_ref().unwrap(), &["127.0.0.1:8000:3000"]);
            assert_eq!(
                initial.environment.as_ref().unwrap()["PUBLIC_ORIGIN"],
                "http://localhost:8000"
            );
            compose(root.path(), "jobs", backend).unwrap();
            let repeated = producer(root.path(), "jobs");
            assert_eq!(repeated.ports, initial.ports);
            assert_eq!(repeated.environment, initial.environment);
        }
    }

    #[test]
    fn leased_workers_and_producers_get_explicit_runner_settings() {
        for (backend, setting) in [
            (Backend::Postgres, "WORKER_LEASE_MS"),
            (Backend::Redis, "WORKER_POLL_MS"),
            (Backend::Kafka, "KAFKA_GROUP_ID"),
        ] {
            let root = fixture("jobs");
            compose(root.path(), "jobs", backend).unwrap();
            let compose = read_or_create_rust_docker_compose(&root.path().join("docker-compose.yaml")).unwrap();
            for service in ["jobs", "jobs-server"] {
                let env = compose.services[service].environment.as_ref().unwrap();
                assert_eq!(env["WORKER_QUEUE"], "jobs", "{service}");
                assert!(env.contains_key(setting), "{service} {setting}");
            }
        }
    }

    #[test]
    fn worker_producer_origin_repairs_owned_legacy_during_sync_and_rename() {
        let root = fixture("jobs");
        compose(root.path(), "jobs", Backend::Postgres).unwrap();
        update_producer(
            root.path(),
            "jobs",
            "http://jobs-server:3000",
            "127.0.0.1:8137:3000/tcp",
        );
        compose(root.path(), "jobs", Backend::Postgres).unwrap();
        assert_eq!(
            producer(root.path(), "jobs").environment.unwrap()["PUBLIC_ORIGIN"],
            "http://localhost:8137"
        );
        update_producer(
            root.path(),
            "jobs",
            "http://jobs-server:3000",
            "127.0.0.1:8137:3000/tcp",
        );
        register_worker(root.path(), "tasks");
        rename_sources(root.path(), "jobs", "tasks").unwrap();
        compose(root.path(), "tasks", Backend::Postgres).unwrap();
        let renamed = producer(root.path(), "tasks");
        assert_eq!(
            renamed.environment.unwrap()["PUBLIC_ORIGIN"],
            "http://localhost:8137"
        );
        assert_eq!(renamed.ports.unwrap(), ["127.0.0.1:8137:3000/tcp"]);
        update_producer(
            root.path(),
            "tasks",
            "https://jobs.example.test",
            "127.0.0.1:8137:3000/tcp",
        );
        compose(root.path(), "tasks", Backend::Postgres).unwrap();
        register_worker(root.path(), "dispatch");
        rename_sources(root.path(), "tasks", "dispatch").unwrap();
        compose(root.path(), "dispatch", Backend::Postgres).unwrap();
        assert_eq!(
            producer(root.path(), "dispatch").environment.unwrap()["PUBLIC_ORIGIN"],
            "https://jobs.example.test"
        );
    }

    #[test]
    fn worker_producer_origin_does_not_guess_custom_or_unpublished_bindings() {
        let root = fixture("jobs");
        compose(root.path(), "jobs", Backend::Redis).unwrap();
        for binding in [
            "3000",
            "${HTTP_PORT}:3000",
            "10.0.0.2:8137:3000",
            "8137:3000/udp",
        ] {
            update_producer(root.path(), "jobs", "http://jobs-server:3000", binding);
            compose(root.path(), "jobs", Backend::Redis).unwrap();
            assert_eq!(
                producer(root.path(), "jobs").environment.unwrap()["PUBLIC_ORIGIN"],
                "http://jobs-server:3000"
            );
        }
        let mut unowned = producer(root.path(), "jobs");
        unowned.ports = Some(vec!["8137:3000".into()]);
        unowned
            .additional_properties
            .shift_remove("x-forklaunch-worker-for");
        repair_legacy_producer_origin(&mut unowned, "jobs");
        assert_eq!(
            unowned.environment.unwrap()["PUBLIC_ORIGIN"],
            "http://jobs-server:3000"
        );
    }
}

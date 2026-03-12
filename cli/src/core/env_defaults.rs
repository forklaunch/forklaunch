use std::collections::HashMap;
use std::path::Path;

use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::core::{
    env::load_env_file,
    env_scope::parse_inter_service_url_var,
    manifest::application::ApplicationManifestData,
};

/// Context for resolving env var defaults.
pub(crate) enum EnvContext<'a> {
    /// .env.local context: services are on localhost
    EnvLocal {
        project_name: &'a str,
    },
    /// docker-compose context: services are referenced by container name
    #[allow(dead_code)]
    DockerCompose {
        service_key: &'a str,
        project_name: &'a str,
    },
}

/// Vars where the majority-value heuristic applies. These are shared/common vars
/// where all services typically have the same value, so if most services already
/// use a particular value, a new service should adopt it too.
const MAJORITY_ELIGIBLE_VARS: &[&str] = &[
    "NODE_ENV",
    "HOST",
    "PROTOCOL",
    "PORT",
    "WS_PORT",
    "VERSION",
    "DOCS_PATH",
    "CORS_ORIGINS",
    "DB_HOST",
    "DB_PORT",
    "DB_USER",
    "DB_PASSWORD",
    "DB_SSL",
    "PGSSLMODE",
    "REDIS_TLS",
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "OTEL_LEVEL",
    "S3_URL",
    "S3_REGION",
    "S3_ACCESS_KEY_ID",
    "S3_SECRET_ACCESS_KEY",
    "KAFKA_BROKERS",
    "BETTER_AUTH_BASE_PATH",
    "STRIPE_API_KEY",
    "STRIPE_WEBHOOK_SECRET",
];

/// Check if a var is eligible for majority-value resolution.
fn is_majority_eligible(var_name: &str) -> bool {
    let upper = var_name.to_ascii_uppercase();
    MAJORITY_ELIGIBLE_VARS.iter().any(|&v| v == upper)
}

/// Collected existing env var values across the workspace.
/// Maps var_name -> list of non-empty values found across all .env files.
pub(crate) struct ExistingEnvValues {
    values: HashMap<String, Vec<String>>,
}

impl ExistingEnvValues {
    /// Scan all .env files in the workspace (root + per-project) and collect values.
    pub(crate) fn collect(modules_path: &Path) -> Self {
        let mut values: HashMap<String, Vec<String>> = HashMap::new();

        // Scan root .env files
        if let Some(app_root) = modules_path.parent() {
            for env_file in &[".env.local", ".env", ".env.development"] {
                if let Ok(vars) = load_env_file(&app_root.join(env_file)) {
                    for (key, val) in vars {
                        if !val.is_empty() {
                            values.entry(key).or_default().push(val);
                        }
                    }
                }
            }
        }

        // Scan each project's .env files
        if let Ok(entries) = std::fs::read_dir(modules_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    for env_file in &[".env.local", ".env", ".env.development"] {
                        if let Ok(vars) = load_env_file(&path.join(env_file)) {
                            for (key, val) in vars {
                                if !val.is_empty() {
                                    values.entry(key).or_default().push(val);
                                }
                            }
                        }
                    }
                }
            }
        }

        Self { values }
    }

    /// Find the majority value for a var (most common non-empty value).
    /// Returns `None` if no values exist.
    pub(crate) fn majority_value(&self, var_name: &str) -> Option<String> {
        let vals = self.values.get(var_name)?;
        if vals.is_empty() {
            return None;
        }

        // Count occurrences of each value
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for val in vals {
            *counts.entry(val.as_str()).or_insert(0) += 1;
        }

        // Find the value with the highest count
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(val, _)| val.to_string())
    }

    /// Empty instance (for testing or when scanning isn't needed).
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    /// Create from a pre-built map (for testing).
    #[cfg(test)]
    pub(crate) fn from_map(values: HashMap<String, Vec<String>>) -> Self {
        Self { values }
    }
}

/// Resolve a default value for a known env var pattern.
/// Returns `None` if the var is not a recognized pattern.
///
/// Resolution order for majority-eligible vars:
/// 1. Check existing values across the workspace for a majority value
/// 2. Fall back to hardcoded defaults
pub(crate) fn resolve_env_var_default(
    var_name: &str,
    manifest: &ApplicationManifestData,
    context: &EnvContext,
    existing_hmac_secret: Option<&str>,
    existing_values: &ExistingEnvValues,
) -> Option<String> {
    let upper = var_name.to_ascii_uppercase();
    let project_names: Vec<String> = manifest.projects.iter().map(|p| p.name.clone()).collect();

    // HMAC_SECRET_KEY: consistent random b64 string
    if upper == "HMAC_SECRET_KEY" {
        if let Some(existing) = existing_hmac_secret {
            return Some(existing.to_string());
        }
        return Some(generate_random_b64_secret(32));
    }

    // For majority-eligible vars, check if there's a prevailing value across
    // existing .env files before falling back to hardcoded defaults.
    if is_majority_eligible(var_name) {
        if let Some(majority) = existing_values.majority_value(var_name) {
            return Some(majority);
        }
    }

    // REDIS_URL: per-service with partition
    if upper == "REDIS_URL" {
        let project_name = match context {
            EnvContext::EnvLocal { project_name } => *project_name,
            EnvContext::DockerCompose { project_name, .. } => *project_name,
        };
        let partition = manifest
            .projects
            .iter()
            .find(|p| p.name == project_name)
            .and_then(|p| p.resources.as_ref())
            .and_then(|r| r.redis_partition)
            .unwrap_or(0);

        return match context {
            EnvContext::EnvLocal { .. } => {
                Some(format!("redis://localhost:6379/{}", partition))
            }
            EnvContext::DockerCompose { .. } => {
                Some(format!("redis://redis:6379/{}", partition))
            }
        };
    }

    // Inter-service URL vars: <SERVICE_NAME>_URL
    if let Some((target_service, transport, _port_env_var)) =
        parse_inter_service_url_var(var_name, &project_names)
    {
        return match context {
            EnvContext::EnvLocal { .. } => {
                let protocol = match transport.as_str() {
                    "ws" => "ws",
                    "grpc" => "http",
                    _ => "http",
                };
                let port = match transport.as_str() {
                    "ws" => "11000",
                    _ => "8000",
                };
                Some(format!("{}://localhost:{}", protocol, port))
            }
            EnvContext::DockerCompose { .. } => {
                let protocol = match transport.as_str() {
                    "ws" => "ws",
                    "grpc" => "http",
                    _ => "http",
                };
                let port = match transport.as_str() {
                    "ws" => "11000",
                    _ => "8000",
                };
                Some(format!("{}://{}:{}", protocol, target_service, port))
            }
        };
    }

    // JWKS_PUBLIC_KEY_URL
    if upper == "JWKS_PUBLIC_KEY_URL" {
        let iam_project = manifest.projects.iter().find(|p| p.name == "iam");
        if let Some(iam) = iam_project {
            if let Some(variant) = &iam.variant {
                if variant.contains("better-auth") {
                    return match context {
                        EnvContext::EnvLocal { .. } => {
                            Some("http://localhost:8000/api/auth/jwks".to_string())
                        }
                        EnvContext::DockerCompose { .. } => {
                            Some("http://iam:8000/api/auth/jwks".to_string())
                        }
                    };
                }
            }
        }
    }

    // BETTER_AUTH_BASE_URL
    if upper == "BETTER_AUTH_BASE_URL" {
        return match context {
            EnvContext::EnvLocal { .. } => Some("http://localhost:8000".to_string()),
            EnvContext::DockerCompose { .. } => Some("http://iam:8000".to_string()),
        };
    }

    // Common application vars with well-known defaults
    match upper.as_str() {
        "NODE_ENV" => Some("development".to_string()),
        "HOST" => match context {
            EnvContext::EnvLocal { .. } => Some("localhost".to_string()),
            EnvContext::DockerCompose { .. } => Some("0.0.0.0".to_string()),
        },
        "PROTOCOL" => Some("http".to_string()),
        "PORT" => Some("8000".to_string()),
        "WS_PORT" => Some("11000".to_string()),
        "VERSION" => Some("v1".to_string()),
        "DOCS_PATH" => Some("/docs".to_string()),
        "OTEL_EXPORTER_OTLP_ENDPOINT" => match context {
            EnvContext::EnvLocal { .. } => Some("http://localhost:4318".to_string()),
            EnvContext::DockerCompose { .. } => Some("http://otel-collector:4318".to_string()),
        },
        "OTEL_SERVICE_NAME" => {
            let project_name = match context {
                EnvContext::EnvLocal { project_name } => *project_name,
                EnvContext::DockerCompose { project_name, .. } => *project_name,
            };
            Some(format!("{}-{}-dev", manifest.app_name, project_name))
        }
        "QUEUE_NAME" => {
            let project_name = match context {
                EnvContext::EnvLocal { project_name } => *project_name,
                EnvContext::DockerCompose { project_name, .. } => *project_name,
            };
            Some(format!("{}-{}-queue", manifest.app_name, project_name))
        }
        "CORS_ORIGINS" => Some("http://localhost:3001".to_string()),
        "BETTER_AUTH_BASE_PATH" => Some("/api/auth".to_string()),
        "BETTER_AUTH_SECRET" => Some(generate_random_b64_secret(32)),
        "DB_HOST" => match context {
            EnvContext::EnvLocal { .. } => Some("localhost".to_string()),
            EnvContext::DockerCompose { .. } => {
                let db_name = &manifest.database;
                let host = match db_name.as_str() {
                    "postgresql" => "postgres",
                    "mongodb" => "mongodb",
                    "mysql" => "mysql",
                    "mariadb" => "mariadb",
                    "mssql" => "mssql",
                    _ => "localhost",
                };
                Some(host.to_string())
            }
        },
        "DB_USER" => Some(manifest.database.clone()),
        "DB_PASSWORD" => Some(manifest.database.clone()),
        "DB_PORT" => {
            let port = match manifest.database.as_str() {
                "postgresql" => "5432",
                "mongodb" => "27017",
                "mysql" | "mariadb" => "3306",
                "mssql" => "1433",
                _ => return None,
            };
            Some(port.to_string())
        }
        "DB_NAME" => {
            let project_name = match context {
                EnvContext::EnvLocal { project_name } => *project_name,
                EnvContext::DockerCompose { project_name, .. } => *project_name,
            };
            Some(format!("{}-{}-dev", manifest.app_name, project_name))
        }
        "S3_URL" => match context {
            EnvContext::EnvLocal { .. } => Some("http://localhost:9000".to_string()),
            EnvContext::DockerCompose { .. } => Some("http://minio:9000".to_string()),
        },
        "S3_REGION" => Some("us-east-1".to_string()),
        "S3_ACCESS_KEY_ID" => Some("minioadmin".to_string()),
        "S3_SECRET_ACCESS_KEY" => Some("minioadmin".to_string()),
        "S3_BUCKET" => {
            let project_name = match context {
                EnvContext::EnvLocal { project_name } => *project_name,
                EnvContext::DockerCompose { project_name, .. } => *project_name,
            };
            Some(format!("{}-{}-dev", manifest.app_name, project_name))
        }
        "KAFKA_BROKERS" => match context {
            EnvContext::EnvLocal { .. } => Some("localhost:9092".to_string()),
            EnvContext::DockerCompose { .. } => Some("kafka:9092".to_string()),
        },
        "KAFKA_CLIENT_ID" => {
            let project_name = match context {
                EnvContext::EnvLocal { project_name } => *project_name,
                EnvContext::DockerCompose { project_name, .. } => *project_name,
            };
            Some(format!("{}-{}-client", manifest.app_name, project_name))
        }
        "KAFKA_GROUP_ID" => {
            let project_name = match context {
                EnvContext::EnvLocal { project_name } => *project_name,
                EnvContext::DockerCompose { project_name, .. } => *project_name,
            };
            Some(format!("{}-{}-group", manifest.app_name, project_name))
        }
        "STRIPE_API_KEY" => Some("replace-with-stripe-api-key".to_string()),
        "STRIPE_WEBHOOK_SECRET" => Some("replace-with-stripe-webhook-secret".to_string()),
        _ => None,
    }
}

/// Find an existing HMAC_SECRET_KEY value from any .env file in the workspace.
/// This ensures consistency across all services.
pub(crate) fn find_existing_hmac_secret(
    modules_path: &Path,
) -> Option<String> {
    // Check root .env.local first
    if let Some(app_root) = modules_path.parent() {
        if let Ok(vars) = load_env_file(&app_root.join(".env.local")) {
            if let Some(val) = vars.get("HMAC_SECRET_KEY") {
                if !val.is_empty() {
                    return Some(val.clone());
                }
            }
        }
        if let Ok(vars) = load_env_file(&app_root.join(".env")) {
            if let Some(val) = vars.get("HMAC_SECRET_KEY") {
                if !val.is_empty() {
                    return Some(val.clone());
                }
            }
        }
    }

    // Check each project's .env.local
    if let Ok(entries) = std::fs::read_dir(modules_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(vars) = load_env_file(&path.join(".env.local")) {
                    if let Some(val) = vars.get("HMAC_SECRET_KEY") {
                        if !val.is_empty() {
                            return Some(val.clone());
                        }
                    }
                }
            }
        }
    }

    None
}

fn generate_random_b64_secret(byte_length: usize) -> String {
    let mut bytes = vec![0u8; byte_length];
    getrandom::getrandom(&mut bytes).expect("Failed to generate random bytes");
    STANDARD.encode(&bytes)
}

/// Resolve defaults for a batch of missing env vars.
/// Returns a HashMap with values filled in where possible.
#[allow(dead_code)]
pub(crate) fn resolve_env_var_defaults(
    missing_vars: &[String],
    manifest: &ApplicationManifestData,
    context: &EnvContext,
    existing_hmac_secret: Option<&str>,
    existing_values: &ExistingEnvValues,
) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for var_name in missing_vars {
        let value = resolve_env_var_default(var_name, manifest, context, existing_hmac_secret, existing_values)
            .unwrap_or_default();
        result.insert(var_name.clone(), value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{
        ProjectEntry, ProjectType, ResourceInventory, application::ApplicationManifestData,
    };

    fn make_manifest(
        app_name: &str,
        database: &str,
        projects: Vec<(&str, ProjectType, Option<ResourceInventory>, Option<String>)>,
    ) -> ApplicationManifestData {
        ApplicationManifestData {
            id: "test-id".to_string(),
            cli_version: "1.0.0".to_string(),
            app_name: app_name.to_string(),
            camel_case_app_name: app_name.to_string(),
            pascal_case_app_name: app_name.to_string(),
            kebab_case_app_name: app_name.to_string(),
            title_case_app_name: app_name.to_string(),
            modules_path: "src/modules".to_string(),
            docker_compose_path: None,
            dockerfile: None,
            git_repository: None,
            runtime: "node".to_string(),
            formatter: "prettier".to_string(),
            linter: "eslint".to_string(),
            validator: "zod".to_string(),
            http_framework: "express".to_string(),
            test_framework: None,
            app_description: "Test app".to_string(),
            author: "Test".to_string(),
            license: "MIT".to_string(),
            projects: projects
                .into_iter()
                .map(|(name, r#type, resources, variant)| ProjectEntry {
                    name: name.to_string(),
                    r#type,
                    description: format!("Test {}", name),
                    variant,
                    resources: Some(resources.unwrap_or(ResourceInventory {
                        database: None,
                        cache: None,
                        queue: None,
                        object_store: None,
                        redis_partition: None,
                    })),
                    routers: None,
                    metadata: None,
                })
                .collect(),
            project_peer_topology: HashMap::new(),
            database: database.to_string(),
            is_postgres: database == "postgresql",
            is_sqlite: database == "sqlite",
            is_mysql: database == "mysql",
            is_mariadb: database == "mariadb",
            is_better_sqlite: database == "better-sqlite",
            is_libsql: database == "libsql",
            is_mssql: database == "mssql",
            is_mongo: database == "mongodb",
            is_in_memory_database: false,
            is_eslint: true,
            is_biome: false,
            is_oxlint: false,
            is_prettier: true,
            is_express: true,
            is_hyper_express: false,
            is_zod: true,
            is_typebox: false,
            is_bun: false,
            is_node: true,
            is_vitest: false,
            is_jest: false,
            platform_application_id: None,
            platform_organization_id: None,
        }
    }

    fn simple_manifest() -> ApplicationManifestData {
        make_manifest(
            "myapp",
            "postgresql",
            vec![
                ("iam", ProjectType::Service, None, Some("better-auth".to_string())),
                ("billing", ProjectType::Service, Some(ResourceInventory {
                    database: Some("postgresql".to_string()),
                    cache: Some("redis".to_string()),
                    queue: None,
                    object_store: None,
                    redis_partition: Some(0),
                }), None),
                ("notifications", ProjectType::Worker, Some(ResourceInventory {
                    database: None,
                    cache: Some("redis".to_string()),
                    queue: None,
                    object_store: None,
                    redis_partition: Some(1),
                }), None),
            ],
        )
    }

    fn ev() -> ExistingEnvValues { ExistingEnvValues::empty() }

    // --- HMAC_SECRET_KEY ---

    #[test]
    fn test_hmac_secret_key_uses_existing() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        let result = resolve_env_var_default("HMAC_SECRET_KEY", &manifest, &ctx, Some("existing-secret"), &ev());
        assert_eq!(result, Some("existing-secret".to_string()));
    }

    #[test]
    fn test_hmac_secret_key_generates_when_none() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        let result = resolve_env_var_default("HMAC_SECRET_KEY", &manifest, &ctx, None, &ev());
        assert!(result.is_some());
        let secret = result.unwrap();
        assert!(!secret.is_empty());
        assert!(base64::Engine::decode(&STANDARD, &secret).is_ok());
    }

    #[test]
    fn test_hmac_secret_key_consistent_across_contexts() {
        let manifest = simple_manifest();
        let ctx1 = EnvContext::EnvLocal { project_name: "billing" };
        let ctx2 = EnvContext::DockerCompose { service_key: "billing", project_name: "billing" };
        let r1 = resolve_env_var_default("HMAC_SECRET_KEY", &manifest, &ctx1, Some("shared-secret"), &ev());
        let r2 = resolve_env_var_default("HMAC_SECRET_KEY", &manifest, &ctx2, Some("shared-secret"), &ev());
        assert_eq!(r1, r2);
    }

    // --- REDIS_URL ---

    #[test]
    fn test_redis_url_env_local_with_partition() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("REDIS_URL", &manifest, &ctx, None, &ev()), Some("redis://localhost:6379/0".to_string()));
    }

    #[test]
    fn test_redis_url_env_local_partition_1() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "notifications" };
        assert_eq!(resolve_env_var_default("REDIS_URL", &manifest, &ctx, None, &ev()), Some("redis://localhost:6379/1".to_string()));
    }

    #[test]
    fn test_redis_url_docker_compose() {
        let manifest = simple_manifest();
        let ctx = EnvContext::DockerCompose { service_key: "billing", project_name: "billing" };
        assert_eq!(resolve_env_var_default("REDIS_URL", &manifest, &ctx, None, &ev()), Some("redis://redis:6379/0".to_string()));
    }

    // --- Inter-service URLs ---

    #[test]
    fn test_inter_service_url_env_local() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("IAM_URL", &manifest, &ctx, None, &ev()), Some("http://localhost:8000".to_string()));
    }

    #[test]
    fn test_inter_service_url_docker_compose() {
        let manifest = simple_manifest();
        let ctx = EnvContext::DockerCompose { service_key: "billing", project_name: "billing" };
        assert_eq!(resolve_env_var_default("IAM_URL", &manifest, &ctx, None, &ev()), Some("http://iam:8000".to_string()));
    }

    #[test]
    fn test_inter_service_ws_url() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("IAM_WS_URL", &manifest, &ctx, None, &ev()), Some("ws://localhost:11000".to_string()));
    }

    // --- Common application vars ---

    #[test]
    fn test_node_env() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("NODE_ENV", &manifest, &ctx, None, &ev()), Some("development".to_string()));
    }

    #[test]
    fn test_host_env_local_vs_docker() {
        let manifest = simple_manifest();
        assert_eq!(
            resolve_env_var_default("HOST", &manifest, &EnvContext::EnvLocal { project_name: "billing" }, None, &ev()),
            Some("localhost".to_string())
        );
        assert_eq!(
            resolve_env_var_default("HOST", &manifest, &EnvContext::DockerCompose { service_key: "billing", project_name: "billing" }, None, &ev()),
            Some("0.0.0.0".to_string())
        );
    }

    #[test]
    fn test_port_and_ws_port() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("PORT", &manifest, &ctx, None, &ev()), Some("8000".to_string()));
        assert_eq!(resolve_env_var_default("WS_PORT", &manifest, &ctx, None, &ev()), Some("11000".to_string()));
    }

    #[test]
    fn test_otel_service_name() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("OTEL_SERVICE_NAME", &manifest, &ctx, None, &ev()), Some("myapp-billing-dev".to_string()));
    }

    #[test]
    fn test_queue_name() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "notifications" };
        assert_eq!(resolve_env_var_default("QUEUE_NAME", &manifest, &ctx, None, &ev()), Some("myapp-notifications-queue".to_string()));
    }

    // --- Database vars ---

    #[test]
    fn test_db_host_env_local_vs_docker() {
        let manifest = simple_manifest();
        assert_eq!(
            resolve_env_var_default("DB_HOST", &manifest, &EnvContext::EnvLocal { project_name: "billing" }, None, &ev()),
            Some("localhost".to_string())
        );
        assert_eq!(
            resolve_env_var_default("DB_HOST", &manifest, &EnvContext::DockerCompose { service_key: "billing", project_name: "billing" }, None, &ev()),
            Some("postgres".to_string())
        );
    }

    #[test]
    fn test_db_port_postgresql() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("DB_PORT", &manifest, &ctx, None, &ev()), Some("5432".to_string()));
    }

    #[test]
    fn test_db_name() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("DB_NAME", &manifest, &ctx, None, &ev()), Some("myapp-billing-dev".to_string()));
    }

    // --- S3 vars ---

    #[test]
    fn test_s3_url_env_local_vs_docker() {
        let manifest = simple_manifest();
        assert_eq!(
            resolve_env_var_default("S3_URL", &manifest, &EnvContext::EnvLocal { project_name: "billing" }, None, &ev()),
            Some("http://localhost:9000".to_string())
        );
        assert_eq!(
            resolve_env_var_default("S3_URL", &manifest, &EnvContext::DockerCompose { service_key: "billing", project_name: "billing" }, None, &ev()),
            Some("http://minio:9000".to_string())
        );
    }

    // --- Kafka vars ---

    #[test]
    fn test_kafka_brokers_env_local_vs_docker() {
        let manifest = simple_manifest();
        assert_eq!(
            resolve_env_var_default("KAFKA_BROKERS", &manifest, &EnvContext::EnvLocal { project_name: "billing" }, None, &ev()),
            Some("localhost:9092".to_string())
        );
        assert_eq!(
            resolve_env_var_default("KAFKA_BROKERS", &manifest, &EnvContext::DockerCompose { service_key: "billing", project_name: "billing" }, None, &ev()),
            Some("kafka:9092".to_string())
        );
    }

    // --- Auth vars ---

    #[test]
    fn test_jwks_public_key_url_with_better_auth() {
        let manifest = simple_manifest();
        assert_eq!(
            resolve_env_var_default("JWKS_PUBLIC_KEY_URL", &manifest, &EnvContext::EnvLocal { project_name: "billing" }, None, &ev()),
            Some("http://localhost:8000/api/auth/jwks".to_string())
        );
        assert_eq!(
            resolve_env_var_default("JWKS_PUBLIC_KEY_URL", &manifest, &EnvContext::DockerCompose { service_key: "billing", project_name: "billing" }, None, &ev()),
            Some("http://iam:8000/api/auth/jwks".to_string())
        );
    }

    // --- Unknown var ---

    #[test]
    fn test_unknown_var_returns_none() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        assert_eq!(resolve_env_var_default("CUSTOM_UNKNOWN_VAR", &manifest, &ctx, None, &ev()), None);
    }

    // --- Majority value tests ---

    #[test]
    fn test_majority_value_overrides_default_for_port() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        let existing = ExistingEnvValues::from_map(HashMap::from([
            ("PORT".to_string(), vec!["8001".to_string(), "8001".to_string(), "8000".to_string()]),
        ]));
        assert_eq!(
            resolve_env_var_default("PORT", &manifest, &ctx, None, &existing),
            Some("8001".to_string())
        );
    }

    #[test]
    fn test_majority_value_overrides_default_for_version() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        let existing = ExistingEnvValues::from_map(HashMap::from([
            ("VERSION".to_string(), vec!["v2".to_string(), "v2".to_string()]),
        ]));
        assert_eq!(
            resolve_env_var_default("VERSION", &manifest, &ctx, None, &existing),
            Some("v2".to_string())
        );
    }

    #[test]
    fn test_majority_value_for_db_host() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        let existing = ExistingEnvValues::from_map(HashMap::from([
            ("DB_HOST".to_string(), vec!["db.example.com".to_string(), "db.example.com".to_string()]),
        ]));
        assert_eq!(
            resolve_env_var_default("DB_HOST", &manifest, &ctx, None, &existing),
            Some("db.example.com".to_string())
        );
    }

    #[test]
    fn test_majority_value_does_not_apply_to_per_service_vars() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        // DB_NAME is per-service, not majority-eligible
        let existing = ExistingEnvValues::from_map(HashMap::from([
            ("DB_NAME".to_string(), vec!["other-db".to_string(), "other-db".to_string()]),
        ]));
        assert_eq!(
            resolve_env_var_default("DB_NAME", &manifest, &ctx, None, &existing),
            Some("myapp-billing-dev".to_string())
        );
    }

    #[test]
    fn test_majority_value_does_not_apply_to_redis_url() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        // REDIS_URL is per-service (never application scoped), not majority-eligible
        let existing = ExistingEnvValues::from_map(HashMap::from([
            ("REDIS_URL".to_string(), vec!["redis://custom:6379/0".to_string()]),
        ]));
        assert_eq!(
            resolve_env_var_default("REDIS_URL", &manifest, &ctx, None, &existing),
            Some("redis://localhost:6379/0".to_string())
        );
    }

    #[test]
    fn test_majority_value_empty_falls_back_to_default() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        // No existing values -> fall back to hardcoded default
        assert_eq!(
            resolve_env_var_default("PORT", &manifest, &ctx, None, &ev()),
            Some("8000".to_string())
        );
    }

    #[test]
    fn test_majority_value_single_value() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        let existing = ExistingEnvValues::from_map(HashMap::from([
            ("PORT".to_string(), vec!["9000".to_string()]),
        ]));
        assert_eq!(
            resolve_env_var_default("PORT", &manifest, &ctx, None, &existing),
            Some("9000".to_string())
        );
    }

    #[test]
    fn test_majority_value_struct_methods() {
        let existing = ExistingEnvValues::from_map(HashMap::from([
            ("A".to_string(), vec!["x".to_string(), "y".to_string(), "x".to_string()]),
            ("B".to_string(), vec!["z".to_string()]),
        ]));
        assert_eq!(existing.majority_value("A"), Some("x".to_string()));
        assert_eq!(existing.majority_value("B"), Some("z".to_string()));
        assert_eq!(existing.majority_value("C"), None);
    }

    #[test]
    fn test_resolve_env_var_defaults_batch() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        let vars = vec![
            "NODE_ENV".to_string(),
            "PORT".to_string(),
            "UNKNOWN_VAR".to_string(),
        ];
        let result = resolve_env_var_defaults(&vars, &manifest, &ctx, None, &ev());
        assert_eq!(result.get("NODE_ENV"), Some(&"development".to_string()));
        assert_eq!(result.get("PORT"), Some(&"8000".to_string()));
        assert_eq!(result.get("UNKNOWN_VAR"), Some(&String::new()));
    }

    #[test]
    fn test_resolve_env_var_defaults_batch_with_majority() {
        let manifest = simple_manifest();
        let ctx = EnvContext::EnvLocal { project_name: "billing" };
        let existing = ExistingEnvValues::from_map(HashMap::from([
            ("PORT".to_string(), vec!["9090".to_string(), "9090".to_string()]),
        ]));
        let vars = vec!["PORT".to_string(), "VERSION".to_string()];
        let result = resolve_env_var_defaults(&vars, &manifest, &ctx, None, &existing);
        assert_eq!(result.get("PORT"), Some(&"9090".to_string()));
        assert_eq!(result.get("VERSION"), Some(&"v1".to_string())); // no majority, falls back
    }
}

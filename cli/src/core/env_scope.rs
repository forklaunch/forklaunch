use std::collections::HashMap;

use anyhow::Result;

use crate::core::{
    ast::infrastructure::env::EnvVarUsage,
    manifest::{ProjectType, application::ApplicationManifestData},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EnvironmentVariableScope {
    Application,
    Service,
    Worker,
}

#[derive(Debug, Clone)]
pub(crate) struct ScopedEnvVar {
    pub name: String,
    pub scope: EnvironmentVariableScope,
    pub scope_id: Option<String>, // service/worker name if scoped
    pub used_by: Vec<String>,     // List of projects using this variable
    pub value: Option<String>,    // Captured value from docker-compose (if available)
}

impl EnvironmentVariableScope {
    #[allow(dead_code)]
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            EnvironmentVariableScope::Application => "application",
            EnvironmentVariableScope::Service => "service",
            EnvironmentVariableScope::Worker => "worker",
        }
    }
}

/// Determine the scope for each environment variable based on usage patterns
pub(crate) fn determine_env_var_scopes(
    project_env_vars: &HashMap<String, Vec<EnvVarUsage>>,
    manifest: &ApplicationManifestData,
) -> Result<Vec<ScopedEnvVar>> {
    let mut var_usage: HashMap<String, Vec<String>> = HashMap::new();

    for (project_name, env_vars) in project_env_vars {
        for env_var in env_vars {
            var_usage
                .entry(env_var.var_name.clone())
                .or_insert_with(Vec::new)
                .push(project_name.clone());
        }
    }

    let mut scoped_vars = Vec::new();

    for (var_name, projects) in var_usage {
        let mut unique_projects: Vec<String> = projects
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        unique_projects.sort();

        let (scope, scope_id) = if unique_projects.len() > 1
            || unique_projects
                .iter()
                .any(|p| p == "core" || p == "monitoring")
        {
            (EnvironmentVariableScope::Application, None)
        } else if let Some(project_name) = unique_projects.first() {
            let project_type = manifest
                .projects
                .iter()
                .find(|p| &p.name == project_name)
                .map(|p| &p.r#type);

            match project_type {
                Some(ProjectType::Service) => (
                    EnvironmentVariableScope::Service,
                    Some(project_name.clone()),
                ),
                Some(ProjectType::Worker) => {
                    (EnvironmentVariableScope::Worker, Some(format!("{}-worker", project_name)))
                }
                _ => (EnvironmentVariableScope::Application, None),
            }
        } else {
            (EnvironmentVariableScope::Application, None)
        };

        scoped_vars.push(ScopedEnvVar {
            name: var_name,
            scope,
            scope_id,
            used_by: unique_projects,
            value: None,
        });
    }

    // Collect project names for inter-service URL detection
    let project_names: Vec<String> = manifest.projects.iter().map(|p| p.name.clone()).collect();

    // Post-process: promote observability/monitoring vars and inter-service URL vars to Application scope
    for var in &mut scoped_vars {
        // Promote OTEL_, LOKI_, TEMPO_, PROMETHEUS_ vars to Application scope
        if is_observability_var(&var.name) {
            var.scope = EnvironmentVariableScope::Application;
            var.scope_id = None;
            continue;
        }

        // Promote inter-service URL vars to Application scope
        if is_inter_service_url_var(&var.name, &project_names) {
            var.scope = EnvironmentVariableScope::Application;
            var.scope_id = None;
            continue;
        }

        // Promote common infrastructure/application vars to Application scope
        if is_common_var(&var.name) {
            var.scope = EnvironmentVariableScope::Application;
            var.scope_id = None;
        }
    }

    scoped_vars.sort_by(|a, b| match a.scope.cmp(&b.scope) {
        std::cmp::Ordering::Equal => a.name.cmp(&b.name),
        other => other,
    });

    Ok(scoped_vars)
}

/// Vars that are inherently per-component and must never be promoted to application scope.
/// Each service/worker has its own distinct value for these.
const NEVER_APPLICATION_SCOPED: &[&str] = &["QUEUE_NAME", "OTEL_SERVICE_NAME", "REDIS_URL"];

/// Check if a var is inherently per-component and must never be application-scoped.
pub(crate) fn is_never_application_scoped(var_name: &str) -> bool {
    let upper = var_name.to_ascii_uppercase();
    NEVER_APPLICATION_SCOPED.iter().any(|&v| v == upper)
}

/// Check if a variable should always be application-scoped (never service/worker-scoped).
/// This covers observability vars, inter-service URLs, HMAC keys, and JWKS keys.
pub(crate) fn is_application_scoped_var(var_name: &str, project_names: &[String]) -> bool {
    is_observability_var(var_name)
        || is_inter_service_url_var(var_name, project_names)
        || is_shared_key_var(var_name)
        || is_common_var(var_name)
}

/// Check if a var name is a shared key variable (HMAC, JWKS) that should be application-scoped.
fn is_shared_key_var(var_name: &str) -> bool {
    let upper = var_name.to_ascii_uppercase();
    if upper.contains("HMAC") {
        return true;
    }
    if upper.contains("JWKS") && upper.contains("PUBLIC") && upper.contains("KEY") {
        return true;
    }
    false
}

/// Common infrastructure and application vars that are always application-scoped.
/// These are shared by all services/workers with the same value.
/// DB_NAME is excluded because it differs per component.
const COMMON_APPLICATION_VARS: &[&str] = &[
    "NODE_ENV",
    "HOST",
    "PORT",
    "PROTOCOL",
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
    "IAM_DB_NAME",
    "OTEL_LEVEL",
];

/// Check if a var name is a common application/infrastructure variable.
fn is_common_var(var_name: &str) -> bool {
    let upper = var_name.to_ascii_uppercase();
    COMMON_APPLICATION_VARS.iter().any(|&v| v == upper)
}

/// Check if a var name is an observability/monitoring variable that should be application-scoped.
/// Per-component vars like OTEL_SERVICE_NAME are excluded since they differ per service.
fn is_observability_var(var_name: &str) -> bool {
    let upper = var_name.to_ascii_uppercase();

    // These are per-component, not application-scoped
    const COMPONENT_SCOPED: &[&str] = &["OTEL_SERVICE_NAME", "OTEL_RESOURCE_ATTRIBUTES"];

    if COMPONENT_SCOPED.iter().any(|&v| v == upper) {
        return false;
    }

    upper.starts_with("OTEL_")
        || upper.starts_with("LOKI_")
        || upper.starts_with("TEMPO_")
        || upper.starts_with("PROMETHEUS_")
}

/// Auth/infrastructure URL vars that Pulumi computes at deploy time.
const PULUMI_INJECTED_URL_VARS: &[&str] = &[
    "JWKS_PUBLIC_KEY_URL",
    "BETTER_AUTH_BASE_URL",
];

/// Check if a var is a Pulumi-injected URL var (auth/infrastructure URLs).
pub(crate) fn is_pulumi_injected_url(var_name: &str) -> bool {
    let upper = var_name.to_ascii_uppercase();
    PULUMI_INJECTED_URL_VARS.iter().any(|&v| v == upper)
}

/// Public wrapper: check if a var name is an inter-service URL variable.
/// These are Pulumi-injected at deploy time and should not appear in .env.template files.
pub(crate) fn is_inter_service_url(var_name: &str, project_names: &[String]) -> bool {
    is_inter_service_url_var(var_name, project_names)
}

/// Check if a var is Pulumi-injected at deploy time (inter-service URLs or auth URLs).
pub(crate) fn is_pulumi_injected(var_name: &str, project_names: &[String]) -> bool {
    is_inter_service_url_var(var_name, project_names) || is_pulumi_injected_url(var_name)
}

/// Known infixes for inter-service URL vars mapped to normalized transport.
/// "http", "api", "service" all resolve to "http". "ws" resolves to "ws".
const KNOWN_URL_INFIXES: &[(&str, &str)] = &[
    ("HTTP", "http"),
    ("WS", "ws"),
    ("API", "http"),
    ("SERVICE", "http"),
    ("GRPC", "grpc"),
];

/// Check if a var name matches the pattern `{SERVICE_NAME}[_INFIX]_{URL|URI|FQDN|HOST}`
/// where SERVICE_NAME corresponds to a known project (converted from kebab-case to SCREAMING_SNAKE_CASE)
/// and INFIX is an optional segment like `SERVICE`, `API`, etc.
fn is_inter_service_url_var(var_name: &str, project_names: &[String]) -> bool {
    parse_inter_service_url_var(var_name, project_names).is_some()
}

/// Parse an inter-service URL var name into (target_service, transport, port_env_var).
/// Transport is normalized: "api"/"service"/"http" → "http", "ws" → "ws".
/// port_env_var indicates which env var on the target service provides the port
/// (e.g. "PORT" for http, "WS_PORT" for ws).
/// Returns `None` if the var doesn't match the pattern.
pub(crate) fn parse_inter_service_url_var(
    var_name: &str,
    project_names: &[String],
) -> Option<(String, String, String)> {
    let upper = var_name.to_ascii_uppercase();

    const URL_SUFFIXES: &[&str] = &["_URL", "_URI", "_FQDN", "_HOST"];

    for suffix in URL_SUFFIXES {
        if let Some(prefix) = upper.strip_suffix(suffix) {
            for project_name in project_names {
                let screaming = project_name.to_ascii_uppercase().replace('-', "_");
                if prefix == screaming {
                    // Exact match: e.g. BILLING_URL → http transport, PORT
                    return Some((project_name.clone(), "http".to_string(), "PORT".to_string()));
                }
                if let Some(rest) = prefix.strip_prefix(&screaming) {
                    if rest.starts_with('_') {
                        let infix = &rest[1..]; // strip leading '_'
                        let transport = KNOWN_URL_INFIXES
                            .iter()
                            .find(|(k, _)| *k == infix)
                            .map(|(_, v)| *v)
                            .unwrap_or("http");
                        let port_env_var = match transport {
                            "ws" => "WS_PORT",
                            "grpc" => "GRPC_PORT",
                            _ => "PORT",
                        };
                        return Some((
                            project_name.clone(),
                            transport.to_string(),
                            port_env_var.to_string(),
                        ));
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{
        ProjectEntry, ProjectType, ResourceInventory, application::ApplicationManifestData,
    };

    fn make_manifest(projects: Vec<(&str, ProjectType)>) -> ApplicationManifestData {
        ApplicationManifestData {
            id: "test".to_string(),
            cli_version: "1.0.0".to_string(),
            app_name: "test".to_string(),
            camel_case_app_name: "test".to_string(),
            pascal_case_app_name: "Test".to_string(),
            kebab_case_app_name: "test".to_string(),
            title_case_app_name: "Test".to_string(),
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
            app_description: "Test".to_string(),
            author: "Test".to_string(),
            license: "MIT".to_string(),
            projects: projects
                .into_iter()
                .map(|(name, r#type)| ProjectEntry {
                    name: name.to_string(),
                    r#type,
                    description: format!("Test {}", name),
                    variant: None,
                    resources: Some(ResourceInventory {
                        database: None,
                        cache: None,
                        queue: None,
                        object_store: None,
                        redis_partition: None,
                    }),
                    routers: None,
                    metadata: None,
                })
                .collect(),
            project_peer_topology: HashMap::new(),
            database: "postgresql".to_string(),
            is_postgres: true,
            is_sqlite: false,
            is_mysql: false,
            is_mariadb: false,
            is_better_sqlite: false,
            is_libsql: false,
            is_mssql: false,
            is_mongo: false,
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
            release_version: None,
        }
    }

    #[test]
    fn test_observability_vars_promoted_to_application() {
        let mut project_vars = HashMap::new();
        project_vars.insert(
            "billing".to_string(),
            vec![
                EnvVarUsage {
                    var_name: "OTEL_EXPORTER_OTLP_ENDPOINT".to_string(),
                },
                EnvVarUsage {
                    var_name: "PORT".to_string(),
                },
            ],
        );

        let manifest = make_manifest(vec![("billing", ProjectType::Service)]);
        let scoped = determine_env_var_scopes(&project_vars, &manifest).unwrap();

        let otel_var = scoped
            .iter()
            .find(|v| v.name == "OTEL_EXPORTER_OTLP_ENDPOINT")
            .unwrap();
        assert_eq!(otel_var.scope, EnvironmentVariableScope::Application);
        assert_eq!(otel_var.scope_id, None);

        let port_var = scoped.iter().find(|v| v.name == "PORT").unwrap();
        assert_eq!(port_var.scope, EnvironmentVariableScope::Application);
    }

    #[test]
    fn test_loki_tempo_prometheus_promoted() {
        assert!(is_observability_var("LOKI_URL"));
        assert!(is_observability_var("TEMPO_ENDPOINT"));
        assert!(is_observability_var("PROMETHEUS_PUSH_GATEWAY"));
        assert!(is_observability_var("OTEL_EXPORTER_OTLP_ENDPOINT"));
        assert!(!is_observability_var("DB_HOST"));
        // Per-component OTEL vars stay at service scope
        assert!(!is_observability_var("OTEL_SERVICE_NAME"));
        assert!(!is_observability_var("OTEL_RESOURCE_ATTRIBUTES"));
    }

    #[test]
    fn test_inter_service_url_promoted_to_application() {
        let mut project_vars = HashMap::new();
        project_vars.insert(
            "billing".to_string(),
            vec![EnvVarUsage {
                var_name: "PLATFORM_MANAGEMENT_URL".to_string(),
            }],
        );

        let manifest = make_manifest(vec![
            ("billing", ProjectType::Service),
            ("platform-management", ProjectType::Service),
        ]);

        let scoped = determine_env_var_scopes(&project_vars, &manifest).unwrap();

        let url_var = scoped
            .iter()
            .find(|v| v.name == "PLATFORM_MANAGEMENT_URL")
            .unwrap();
        assert_eq!(url_var.scope, EnvironmentVariableScope::Application);
        assert_eq!(url_var.scope_id, None);
    }

    #[test]
    fn test_inter_service_url_pattern_matching() {
        let projects = vec![
            "billing".to_string(),
            "platform-management".to_string(),
            "auth".to_string(),
        ];

        assert!(is_inter_service_url_var("BILLING_URL", &projects));
        assert!(is_inter_service_url_var("PLATFORM_MANAGEMENT_URL", &projects));
        assert!(is_inter_service_url_var("AUTH_URI", &projects));
        assert!(is_inter_service_url_var("BILLING_FQDN", &projects));
        assert!(is_inter_service_url_var("AUTH_HOST", &projects));
        // Infix variants (e.g. _SERVICE_, _API_)
        assert!(is_inter_service_url_var("BILLING_SERVICE_URL", &projects));
        assert!(is_inter_service_url_var("BILLING_API_URL", &projects));
        assert!(is_inter_service_url_var("AUTH_SERVICE_URI", &projects));
        assert!(is_inter_service_url_var("PLATFORM_MANAGEMENT_API_URL", &projects));
        // Non-matches
        assert!(!is_inter_service_url_var("UNKNOWN_URL", &projects));
        assert!(!is_inter_service_url_var("BILLING_PORT", &projects));
    }
}

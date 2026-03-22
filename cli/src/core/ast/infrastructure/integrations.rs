use std::{collections::HashMap, fs, path::Path};

use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_ast::ast::{ObjectExpression, ObjectProperty, PropertyKey, VariableDeclaration};
use oxc_ast_visit::Visit;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;
use serde_json::Value;

use crate::core::{
    rendered_template::RenderedTemplatesCache,
    sync::detection::detect_database_from_mikro_orm_config,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Integration {
    pub id: String,
    pub integration_type: String,
    pub technology: Option<String>,
    pub config: HashMap<String, Value>,
}

pub struct IntegrationsVisitor {
    pub integrations: Vec<Integration>,
    in_runtime_deps: bool,
    depth: usize,
}

impl IntegrationsVisitor {
    pub fn new() -> Self {
        Self {
            integrations: Vec::new(),
            in_runtime_deps: false,
            depth: 0,
        }
    }

    fn map_dependency_to_integration_type(dep_name: &str) -> Option<String> {
        match dep_name {
            // Database (Orm is the new token name, MikroORM is legacy)
            "Orm" | "MikroORM" => Some("database".to_string()),

            // EntityMgr/EntityManager is ignored - it's a scoped dependency of Orm
            "EntityMgr" | "EntityManager" => None,

            // Cache
            "RedisClient" | "Redis" | "RedisCache" | "TtlCache" => Some("cache".to_string()),

            // Object Store (ObjectStore is the new token name, S3ObjectStore is legacy)
            "ObjectStore" | "S3ObjectStore" | "S3Client" | "S3" => {
                Some("objectstore".to_string())
            }

            // Message Queue
            "KafkaClient" | "Kafka" | "QueueClient" => Some("messagequeue".to_string()),
            "BullMQ" => Some("messagequeue".to_string()),

            // Observability (OtelCollector is the new token name, OpenTelemetryCollector is legacy)
            "OtelCollector" | "OpenTelemetryCollector" | "PrometheusClient" => {
                Some("observability".to_string())
            }

            // Skip these - they're not integrations
            "SchemaValidator" | "Metrics" | "WorkerOptions" | "WorkerConsumer"
            | "WorkerProducer" => None,

            // Unknown dependencies become ThirdParty
            _ => Some("thirdparty".to_string()),
        }
    }

    fn map_dependency_to_technology(dep_name: &str) -> Option<String> {
        match dep_name {
            // Database - technology determined from mikro-orm.config.ts
            "Orm" | "MikroORM" => None,

            // Cache technologies
            "RedisClient" | "Redis" | "RedisCache" | "TtlCache" => Some("redis".to_string()),

            // Object Store technologies
            "ObjectStore" | "S3ObjectStore" | "S3Client" | "S3" => Some("s3".to_string()),

            // Message Queue technologies
            "BullMQ" => Some("bullmq".to_string()),
            "KafkaClient" | "Kafka" => Some("kafka".to_string()),

            // No technology for others
            _ => None,
        }
    }

    fn get_integration_id(dep_name: &str) -> String {
        if dep_name == "TtlCache" {
            return "Redis Cache".to_string();
        }

        Self::to_title_case_with_spaces(dep_name)
    }

    fn to_title_case_with_spaces(name: &str) -> String {
        if name.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        let chars: Vec<char> = name.chars().collect();

        for (i, &ch) in chars.iter().enumerate() {
            if i > 0 {
                let prev = chars[i - 1];
                let next = chars.get(i + 1).copied();

                if ch.is_uppercase() {
                    // Add space before uppercase letters if:
                    // 1. Previous char was lowercase, OR
                    // 2. Previous char was a digit, OR
                    // 3. Previous char was uppercase AND next char is lowercase (e.g., "XMLHttp" -> "XML Http")
                    if prev.is_lowercase()
                        || prev.is_ascii_digit()
                        || (prev.is_uppercase() && next.map_or(false, |c| c.is_lowercase()))
                    {
                        result.push(' ');
                    }
                }
            }

            // Capitalize first letter, keep rest as-is
            if i == 0 {
                result.push(ch.to_uppercase().next().unwrap_or(ch));
            } else {
                result.push(ch);
            }
        }

        result
    }
}

impl<'a> Visit<'a> for IntegrationsVisitor {
    fn visit_variable_declaration(&mut self, decl: &VariableDeclaration<'a>) {
        // Look for: const runtimeDependencies = ...
        for declarator in &decl.declarations {
            if let oxc_ast::ast::BindingPatternKind::BindingIdentifier(ident) = &declarator.id.kind
            {
                if ident.name == "runtimeDependencies" {
                    self.in_runtime_deps = true;
                    self.depth = 0;

                    // Visit the initializer to extract dependencies
                    if let Some(init) = &declarator.init {
                        self.visit_expression(init);
                    }

                    self.in_runtime_deps = false;
                    self.depth = 0;
                }
            }
        }

        oxc_ast_visit::walk::walk_variable_declaration(self, decl);
    }

    fn visit_object_expression(&mut self, expr: &ObjectExpression<'a>) {
        if self.in_runtime_deps {
            self.depth += 1;
        }

        oxc_ast_visit::walk::walk_object_expression(self, expr);

        if self.in_runtime_deps {
            self.depth -= 1;
        }
    }

    fn visit_object_property(&mut self, prop: &ObjectProperty<'a>) {
        if self.in_runtime_deps {
            // Only process top-level properties (depth 1, inside the chain() object literal)
            // The structure is: runtimeDependencies = environmentConfig.chain({ ... })
            // So we want depth 1 (inside the chain() object literal)
            // Depth 0 = outside, Depth 1 = inside runtimeDependencies object, Depth 2+ = nested objects
            if self.depth == 1 {
                if let PropertyKey::StaticIdentifier(ident) = &prop.key {
                    let dep_name = ident.name.to_string();

                    if let Some(integration_type) =
                        Self::map_dependency_to_integration_type(&dep_name)
                    {
                        let id = Self::get_integration_id(&dep_name);
                        let technology = Self::map_dependency_to_technology(&dep_name);
                        let config = HashMap::new();

                        self.integrations.push(Integration {
                            id,
                            integration_type,
                            technology,
                            config,
                        });
                    }
                }
            }
        }

        oxc_ast_visit::walk::walk_object_property(self, prop);
    }
}

pub fn extract_integrations_from_file(
    file_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<Vec<Integration>> {
    let source_code = rendered_templates_cache.get(file_path)?.unwrap().content;

    extract_integrations_from_source(&source_code)
}

pub fn extract_integrations_from_source(source_code: &str) -> Result<Vec<Integration>> {
    let allocator = Allocator::default();

    let ParserReturn {
        program, errors, ..
    } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    if !errors.is_empty() {
        log::debug!(
            "TypeScript parse errors during integrations scan: {:?}",
            errors
        );
    }

    let mut visitor = IntegrationsVisitor::new();
    visitor.visit_program(&program);

    Ok(visitor.integrations)
}

pub fn find_all_integrations(
    modules_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<HashMap<String, Vec<Integration>>> {
    let mut all_integrations = HashMap::new();

    if !modules_path.exists() {
        return Ok(all_integrations);
    }

    for entry in fs::read_dir(modules_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let registrations_path = path.join("registrations.ts");
            if registrations_path.exists() {
                let project_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let mut integrations =
                    extract_integrations_from_file(&registrations_path, rendered_templates_cache)?;

                // For MikroORM integrations, replace the ID with the database type
                for integration in &mut integrations {
                    if integration.id.to_ascii_lowercase().contains("mikro")
                        && integration.id.to_ascii_lowercase().contains("orm")
                    {
                        // Extract database type from mikro-orm.config.ts
                        if let Ok(Some(database)) = detect_database_from_mikro_orm_config(&path) {
                            // Convert database enum to display name and technology
                            let (db_name, db_technology) = match database {
                                crate::constants::Database::PostgreSQL => {
                                    ("PostgreSQL", "postgresql")
                                }
                                crate::constants::Database::MySQL => ("MySQL", "mysql"),
                                crate::constants::Database::MariaDB => ("MariaDB", "mariadb"),
                                crate::constants::Database::MsSQL => ("MsSQL", "mssql"),
                                crate::constants::Database::MongoDB => ("MongoDB", "mongodb"),
                                crate::constants::Database::SQLite => ("SQLite", "sqlite"),
                                crate::constants::Database::BetterSQLite => {
                                    ("Better SQLite", "better-sqlite")
                                }
                                crate::constants::Database::LibSQL => ("LibSQL", "libsql"),
                            };
                            integration.id = db_name.to_string();
                            integration.technology = Some(db_technology.to_string());
                        }
                    }
                }

                if !integrations.is_empty() {
                    all_integrations.insert(project_name, integrations);
                }
            }
        }
    }

    Ok(all_integrations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_integrations() {
        let source = r#"
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => new MikroORM(config)
  },
  RedisClient: {
    lifetime: Lifetime.Singleton,
    type: RedisClient,
    factory: () => new RedisClient()
  },
  S3ObjectStore: {
    lifetime: Lifetime.Singleton,
    type: S3ObjectStore,
    factory: () => new S3ObjectStore()
  },
  KafkaClient: {
    lifetime: Lifetime.Singleton,
    type: KafkaClient,
    factory: () => new KafkaClient()
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: () => new OpenTelemetryCollector()
  },
  SchemaValidator: {
    lifetime: Lifetime.Singleton,
    type: SchemaValidator,
    factory: () => SchemaValidator()
  },
  UnknownService: {
    lifetime: Lifetime.Singleton,
    type: UnknownService,
    factory: () => new UnknownService()
  }
});
        "#;

        let integrations = extract_integrations_from_source(source).unwrap();

        // Should have 6 integrations: 5 known types + 1 ThirdParty (UnknownService)
        // SchemaValidator should be filtered out
        assert_eq!(integrations.len(), 6);
        assert!(
            integrations
                .iter()
                .any(|i| i.id == "Mikro ORM" && i.integration_type == "database")
        );
        assert!(integrations.iter().any(|i| i.id == "Redis Client"
            && i.integration_type == "cache"
            && i.technology == Some("redis".to_string())));
        assert!(
            integrations
                .iter()
                .any(|i| i.id == "S3 Object Store" && i.integration_type == "objectstore")
        );
        assert!(
            integrations
                .iter()
                .any(|i| i.id == "Kafka Client" && i.integration_type == "messagequeue")
        );
        assert!(
            integrations.iter().any(
                |i| i.id == "Open Telemetry Collector" && i.integration_type == "observability"
            )
        );
        assert!(
            integrations
                .iter()
                .any(|i| i.id == "Unknown Service" && i.integration_type == "thirdparty")
        );
    }

    #[test]
    fn test_ttl_cache_becomes_redis_cache() {
        let source = r#"
const runtimeDependencies = environmentConfig.chain({
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: () => new RedisTtlCache()
  }
});
        "#;

        let integrations = extract_integrations_from_source(source).unwrap();

        assert_eq!(integrations.len(), 1);
        assert!(integrations.iter().any(|i| i.id == "Redis Cache"
            && i.integration_type == "cache"
            && i.technology == Some("redis".to_string())));
    }
}

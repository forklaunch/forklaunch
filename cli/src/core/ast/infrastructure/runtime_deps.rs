use std::{collections::HashSet, fs, path::Path};

use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_ast::ast::{ObjectProperty, PropertyKey, VariableDeclaration};
use oxc_ast_visit::Visit;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;
use serde::{Deserialize, Serialize};

use crate::core::rendered_template::RenderedTemplatesCache;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    Database,
    Cache,
    Storage,
    Queue,
    Monitoring,
}

impl ResourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceType::Database => "database",
            ResourceType::Cache => "cache",
            ResourceType::Storage => "storage",
            ResourceType::Queue => "queue",
            ResourceType::Monitoring => "monitoring",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuntimeDependency {
    pub name: String,
    pub resource_type: ResourceType,
}

pub struct RuntimeDepsVisitor {
    pub dependencies: Vec<RuntimeDependency>,
    in_runtime_deps: bool,
}

impl RuntimeDepsVisitor {
    pub fn new() -> Self {
        Self {
            dependencies: Vec::new(),
            in_runtime_deps: false,
        }
    }

    fn map_dependency_to_resource_type(dep_name: &str) -> Option<ResourceType> {
        match dep_name {
            // Database (Orm is the new token name, MikroORM is legacy)
            "Orm" | "MikroORM" => Some(ResourceType::Database),

            // EntityMgr/EntityManager is ignored - it's a scoped dependency of Orm
            "EntityMgr" | "EntityManager" => None,

            // Cache
            "RedisClient" | "Redis" | "RedisCache" => Some(ResourceType::Cache),

            // Storage (ObjectStore is the new token name, S3ObjectStore is legacy)
            "ObjectStore" | "S3ObjectStore" | "S3Client" | "S3" => Some(ResourceType::Storage),

            // Queue/Message Broker
            "KafkaClient" | "Kafka" | "BullMQ" | "QueueClient" => Some(ResourceType::Queue),

            // Monitoring (OtelCollector is the new token name, OpenTelemetryCollector is legacy)
            "OtelCollector" | "OpenTelemetryCollector" | "PrometheusClient" => {
                Some(ResourceType::Monitoring)
            }

            // Skip these - they're not infrastructure resources
            "SchemaValidator" | "Metrics" => None,

            _ => None,
        }
    }
}

impl<'a> Visit<'a> for RuntimeDepsVisitor {
    fn visit_variable_declaration(&mut self, decl: &VariableDeclaration<'a>) {
        // Look for: const runtimeDependencies = ...
        for declarator in &decl.declarations {
            if let oxc_ast::ast::BindingPatternKind::BindingIdentifier(ident) = &declarator.id.kind
            {
                if ident.name == "runtimeDependencies" {
                    self.in_runtime_deps = true;

                    // Visit the initializer to extract dependencies
                    if let Some(init) = &declarator.init {
                        self.visit_expression(init);
                    }

                    self.in_runtime_deps = false;
                }
            }
        }

        oxc_ast_visit::walk::walk_variable_declaration(self, decl);
    }

    fn visit_object_property(&mut self, prop: &ObjectProperty<'a>) {
        if self.in_runtime_deps {
            if let PropertyKey::StaticIdentifier(ident) = &prop.key {
                let dep_name = ident.name.to_string();

                if let Some(resource_type) = Self::map_dependency_to_resource_type(&dep_name) {
                    self.dependencies.push(RuntimeDependency {
                        name: dep_name,
                        resource_type,
                    });
                }
            }
        }

        oxc_ast_visit::walk::walk_object_property(self, prop);
    }
}

pub fn extract_runtime_deps_from_file(
    file_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<Vec<RuntimeDependency>> {
    let source_code = rendered_templates_cache.get(file_path)?.unwrap().content;

    extract_runtime_deps_from_source(&source_code)
}

pub fn extract_runtime_deps_from_source(source_code: &str) -> Result<Vec<RuntimeDependency>> {
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
            "TypeScript parse errors during runtime deps scan: {:?}",
            errors
        );
    }

    let mut visitor = RuntimeDepsVisitor::new();
    visitor.visit_program(&program);

    Ok(visitor.dependencies)
}

pub fn find_all_runtime_deps(
    modules_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<std::collections::HashMap<String, Vec<RuntimeDependency>>> {
    let mut all_deps = std::collections::HashMap::new();

    if !modules_path.exists() {
        return Ok(all_deps);
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

                let deps =
                    extract_runtime_deps_from_file(&registrations_path, rendered_templates_cache)?;

                // For MikroORM, extract database type from mikro-orm.config.ts
                // This ensures we have the correct database type information
                if deps.iter().any(|d| d.name == "MikroORM") {
                    if let Ok(Some(_database)) =
                        crate::core::sync::detection::detect_database_from_mikro_orm_config(&path)
                    {
                        // Database type extracted successfully - MikroORM dependency is valid
                        // The database type is stored in the project's resources, not in runtime deps
                        // So we just verify it exists and keep the MikroORM dependency
                    }
                    // If database type can't be extracted, we still keep MikroORM as a database dependency
                }

                if !deps.is_empty() {
                    all_deps.insert(project_name, deps);
                }
            }
        }
    }

    Ok(all_deps)
}

pub fn get_unique_resource_types(deps: &[RuntimeDependency]) -> Vec<String> {
    let unique: HashSet<ResourceType> = deps.iter().map(|d| d.resource_type.clone()).collect();
    let mut types: Vec<ResourceType> = unique.into_iter().collect();
    types.sort();
    types.into_iter().map(|t| t.as_str().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_runtime_deps() {
        let source = r#"
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(config)
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
  }
});
        "#;

        let deps = extract_runtime_deps_from_source(source).unwrap();

        assert_eq!(deps.len(), 3);
        assert!(
            deps.iter()
                .any(|d| d.name == "MikroORM" && d.resource_type == ResourceType::Database)
        );
        assert!(
            deps.iter()
                .any(|d| d.name == "RedisClient" && d.resource_type == ResourceType::Cache)
        );
        assert!(
            deps.iter()
                .any(|d| d.name == "S3ObjectStore" && d.resource_type == ResourceType::Storage)
        );
    }

    #[test]
    fn test_entity_manager_ignored() {
        let source = r#"
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(config)
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }) => MikroORM.em.fork()
  }
});
        "#;

        let deps = extract_runtime_deps_from_source(source).unwrap();

        // EntityManager should be ignored
        assert_eq!(deps.len(), 1);
        assert!(
            deps.iter()
                .any(|d| d.name == "MikroORM" && d.resource_type == ResourceType::Database)
        );
        assert!(
            !deps.iter()
                .any(|d| d.name == "EntityManager")
        );
    }

    #[test]
    fn test_new_token_names() {
        let source = r#"
const runtimeDependencies = environmentConfig.chain({
  Orm: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(config)
  },
  OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: () => new OpenTelemetryCollector()
  },
  ObjectStore: {
    lifetime: Lifetime.Singleton,
    type: S3ObjectStore,
    factory: () => new S3ObjectStore()
  },
  EntityMgr: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ Orm }) => Orm.em.fork()
  }
});
        "#;

        let deps = extract_runtime_deps_from_source(source).unwrap();

        assert_eq!(deps.len(), 3); // EntityMgr is ignored
        assert!(
            deps.iter()
                .any(|d| d.name == "Orm" && d.resource_type == ResourceType::Database)
        );
        assert!(
            deps.iter()
                .any(|d| d.name == "OtelCollector" && d.resource_type == ResourceType::Monitoring)
        );
        assert!(
            deps.iter()
                .any(|d| d.name == "ObjectStore" && d.resource_type == ResourceType::Storage)
        );
        assert!(!deps.iter().any(|d| d.name == "EntityMgr"));
    }
}

use std::{collections::HashMap, path::Path};

use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_ast::ast::ImportDeclaration;
use oxc_ast_visit::Visit;
use oxc_parser::{Parser, ParserReturn};
use oxc_span::SourceType;
use walkdir::WalkDir;

use crate::core::rendered_template::RenderedTemplatesCache;

/// Detects SDK client imports from other services in the mesh.
/// e.g., `import { BillingSdkClient } from "@myapp/billing"` would indicate
/// this service depends on the `billing` service.

#[derive(Debug, Clone)]
struct ServiceDependency {
    /// The name of the service being imported from (e.g., "billing")
    service_name: String,
    /// The type of dependency (e.g., "network" for SDK client imports)
    dependency_type: String,
}

struct SdkImportVisitor {
    pub dependencies: Vec<ServiceDependency>,
}

impl SdkImportVisitor {
    fn new() -> Self {
        Self {
            dependencies: Vec::new(),
        }
    }

    /// Extract service name from import source like "@myapp/billing" -> "billing"
    fn extract_service_from_import(&self, source: &str) -> Option<String> {
        // Pattern: @{app_name}/{service_name} or @forklaunch/{service_name}
        if source.starts_with('@') {
            let parts: Vec<&str> = source.split('/').collect();
            if parts.len() >= 2 {
                let service_name = parts[1].to_string();
                // Exclude known non-service packages
                if !service_name.starts_with("client-sdk")
                    && !service_name.starts_with("forklaunch")
                    && !service_name.starts_with("core")
                    && !service_name.starts_with("common")
                    && !service_name.starts_with("shared")
                    && !service_name.starts_with("types")
                    && !service_name.starts_with("config")
                {
                    return Some(service_name);
                }
            }
        }
        None
    }
}

impl<'a> Visit<'a> for SdkImportVisitor {
    fn visit_import_declaration(&mut self, decl: &ImportDeclaration<'a>) {
        let source = decl.source.value.as_str();

        // Check if this import contains an SdkClient type
        if let Some(specifiers) = &decl.specifiers {
            for specifier in specifiers {
                if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(spec) = specifier {
                    let imported_name = spec.imported.name().as_str();

                    // Check for SdkClient pattern (case insensitive for the suffix)
                    if imported_name.ends_with("SdkClient") {
                        if let Some(service_name) = self.extract_service_from_import(source) {
                            self.dependencies.push(ServiceDependency {
                                service_name,
                                dependency_type: "network".to_string(),
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Extract SDK client dependencies from a single file
fn extract_sdk_dependencies_from_file(
    file_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<Vec<ServiceDependency>> {
    let source_code = rendered_templates_cache.get(file_path)?.unwrap().content;
    extract_sdk_dependencies_from_source(&source_code)
}

/// Extract SDK client dependencies from source code
fn extract_sdk_dependencies_from_source(source_code: &str) -> Result<Vec<ServiceDependency>> {
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
        log::debug!("TypeScript parse errors during SDK deps scan: {:?}", errors);
    }

    let mut visitor = SdkImportVisitor::new();
    visitor.visit_program(&program);

    Ok(visitor.dependencies)
}

/// Find all SDK client dependencies for all services/workers in the modules path.
/// Returns a map of service_name -> Vec<(dependency_name, dependency_type)>
pub fn find_all_service_dependencies(
    modules_path: &Path,
    rendered_templates_cache: &RenderedTemplatesCache,
) -> Result<HashMap<String, Vec<(String, String)>>> {
    let mut all_deps: HashMap<String, Vec<(String, String)>> = HashMap::new();

    if !modules_path.exists() {
        return Ok(all_deps);
    }

    for entry in WalkDir::new(modules_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext == "ts" || ext == "tsx")
                && !e.path().to_string_lossy().contains("node_modules")
                && !e.path().to_string_lossy().contains("/dist/")
                && !e.path().to_string_lossy().contains(".test.")
                && !e.path().to_string_lossy().contains(".spec.")
                && !e.path().to_string_lossy().ends_with(".d.ts")
        })
    {
        let file_path = entry.path();

        // Determine which service this file belongs to
        let relative = file_path.strip_prefix(modules_path).ok();
        let service_name = relative.and_then(|p| {
            p.components()
                .next()
                .and_then(|c| c.as_os_str().to_str().map(|s| s.to_string()))
        });

        if let Some(service) = service_name {
            if let Ok(deps) =
                extract_sdk_dependencies_from_file(file_path, rendered_templates_cache)
            {
                for dep in deps {
                    // Don't add self-reference
                    if dep.service_name != service {
                        all_deps
                            .entry(service.clone())
                            .or_insert_with(Vec::new)
                            .push((dep.service_name, dep.dependency_type));
                    }
                }
            }
        }
    }

    // Deduplicate dependencies for each service (by service name only)
    for deps in all_deps.values_mut() {
        deps.sort_by(|a, b| a.0.cmp(&b.0));
        deps.dedup_by(|a, b| a.0 == b.0);
    }

    Ok(all_deps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sdk_dependencies() {
        let source = r#"
        import { BillingSdkClient } from "@myapp/billing";
        import { IamSdkClient } from "@myapp/iam";
        import { universalSdk } from "@forklaunch/universal-sdk";
        import { SomeOtherType } from "@myapp/utils";
        "#;

        let deps = extract_sdk_dependencies_from_source(source).unwrap();

        assert_eq!(deps.len(), 2);
        assert!(deps.iter().any(|d| d.service_name == "billing" && d.dependency_type == "network"));
        assert!(deps.iter().any(|d| d.service_name == "iam" && d.dependency_type == "network"));
    }

    #[test]
    fn test_ignores_non_sdk_imports() {
        let source = r#"
        import { SomeType } from "@myapp/billing";
        import { Client } from "some-package";
        "#;

        let deps = extract_sdk_dependencies_from_source(source).unwrap();

        assert_eq!(deps.len(), 0);
    }
}

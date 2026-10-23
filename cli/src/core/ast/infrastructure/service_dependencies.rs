use std::{
    collections::{BTreeMap, HashMap},
    fs::read_to_string,
    path::{Component, Path, PathBuf},
};

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
        let source = decl.source.value.as_str().unwrap_or_default();

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
        program,
        diagnostics: errors,
        ..
    } = Parser::new(
        &allocator,
        source_code,
        SourceType::default().with_typescript(true),
    )
    .parse();

    if errors.has_errors() {
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

#[derive(Debug, Clone)]
struct RustProjectDependencyInfo {
    name: String,
    crate_path: PathBuf,
    is_network_target: bool,
}

/// Find service dependencies for Rust services/workers from Cargo path dependencies and Rust SDK
/// crate imports. Returns service_name -> Vec<(dependency_name, dependency_type)>.
pub fn find_all_rust_service_dependencies(
    app_root: &Path,
) -> Result<HashMap<String, Vec<(String, String)>>> {
    let manifest_path = app_root.join(".forklaunch").join("manifest.toml");
    if !manifest_path.exists() {
        return Ok(HashMap::new());
    }

    let manifest_content = read_to_string(&manifest_path)?;
    let manifest = toml::from_str::<toml::Value>(&manifest_content)?;
    let rust_projects = rust_project_dependency_info(app_root, &manifest);
    let mut all_deps = HashMap::new();

    for project in rust_projects
        .iter()
        .filter(|project| project.is_network_target)
    {
        let project_dir = app_root.join(&project.crate_path);
        let mut deps = BTreeMap::<String, String>::new();
        detect_rust_cargo_dependencies(&project_dir, &rust_projects, &mut deps)?;
        detect_rust_source_dependencies(&project_dir, &project.name, &rust_projects, &mut deps)?;
        if !deps.is_empty() {
            all_deps.insert(project.name.clone(), deps.into_iter().collect());
        }
    }

    Ok(all_deps)
}

fn rust_project_dependency_info(
    app_root: &Path,
    manifest: &toml::Value,
) -> Vec<RustProjectDependencyInfo> {
    manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let name = project.get("name").and_then(toml::Value::as_str)?;
            let project_type = project.get("type").and_then(toml::Value::as_str)?;
            let metadata = project.get("metadata").and_then(toml::Value::as_table)?;
            if metadata.get("language").and_then(toml::Value::as_str) != Some("rust") {
                return None;
            }
            let crate_path = metadata
                .get("cratePath")
                .and_then(toml::Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    let native = PathBuf::from("src").join(name);
                    if app_root.join(&native).join("Cargo.toml").is_file() {
                        native
                    } else {
                        PathBuf::from("crates").join(name)
                    }
                });
            Some(RustProjectDependencyInfo {
                name: name.to_string(),
                crate_path: clean_path(&app_root.join(crate_path)),
                is_network_target: matches!(project_type, "Service" | "Worker"),
            })
        })
        .collect()
}

fn detect_rust_cargo_dependencies(
    project_dir: &Path,
    rust_projects: &[RustProjectDependencyInfo],
    deps: &mut BTreeMap<String, String>,
) -> Result<()> {
    let cargo_path = project_dir.join("Cargo.toml");
    if !cargo_path.exists() {
        return Ok(());
    }

    let cargo = toml::from_str::<toml::Value>(&read_to_string(&cargo_path)?)?;
    let Some(dependencies) = cargo.get("dependencies").and_then(toml::Value::as_table) else {
        return Ok(());
    };

    for (dependency_name, dependency) in dependencies {
        let dependency_package = dependency
            .get("package")
            .and_then(toml::Value::as_str)
            .unwrap_or(dependency_name);

        if let Some(path) = dependency.get("path").and_then(toml::Value::as_str) {
            let dependency_path = clean_path(&project_dir.join(path));
            if let Some((name, dependency_type)) =
                classify_rust_path_dependency(&dependency_path, rust_projects)
            {
                insert_rust_dependency(deps, name, dependency_type);
                continue;
            }
        }

        if let Some(service_name) = rust_sdk_service_name(dependency_name, rust_projects)
            .or_else(|| rust_sdk_service_name(dependency_package, rust_projects))
        {
            insert_rust_dependency(deps, service_name, "network");
        }
    }

    Ok(())
}

fn detect_rust_source_dependencies(
    project_dir: &Path,
    current_project: &str,
    rust_projects: &[RustProjectDependencyInfo],
    deps: &mut BTreeMap<String, String>,
) -> Result<()> {
    if !project_dir.exists() {
        return Ok(());
    }

    for entry in WalkDir::new(project_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension().is_some_and(|ext| ext == "rs")
                && !entry.path().to_string_lossy().contains("/target/")
        })
    {
        let Ok(source) = read_to_string(entry.path()) else {
            continue;
        };
        for dependency in rust_projects {
            if dependency.name == current_project {
                continue;
            }
            let crate_ident = dependency.name.replace('-', "_");
            let sdk_ident = format!("{crate_ident}_sdk");
            if dependency.is_network_target && contains_rust_crate_reference(&source, &sdk_ident) {
                insert_rust_dependency(deps, dependency.name.clone(), "network");
            } else if contains_rust_crate_reference(&source, &crate_ident) {
                insert_rust_dependency(deps, dependency.name.clone(), "local");
            }
        }
    }

    Ok(())
}

fn classify_rust_path_dependency(
    dependency_path: &Path,
    rust_projects: &[RustProjectDependencyInfo],
) -> Option<(String, &'static str)> {
    for project in rust_projects {
        if dependency_path == project.crate_path {
            return Some((project.name.clone(), "local"));
        }

        let sdk_path = Path::new("sdks")
            .join("rust")
            .join(format!("{}-sdk", project.name));
        let native_sdk_path = Path::new("src/client-sdk/rust").join(&project.name);
        let native_named_sdk_path =
            Path::new("src/client-sdk/rust").join(format!("{}-sdk", project.name));
        let local_sdk_path = Path::new(&project.crate_path).join("sdk/rust");
        if project.is_network_target
            && (dependency_path.ends_with(sdk_path)
                || dependency_path.ends_with(native_sdk_path)
                || dependency_path.ends_with(native_named_sdk_path)
                || dependency_path.ends_with(local_sdk_path))
        {
            return Some((project.name.clone(), "network"));
        }
    }

    dependency_path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .and_then(|file_name| rust_sdk_service_name(file_name, rust_projects))
        .map(|service_name| (service_name, "network"))
}

fn rust_sdk_service_name(
    crate_name: &str,
    rust_projects: &[RustProjectDependencyInfo],
) -> Option<String> {
    let service_candidate = crate_name
        .strip_suffix("-sdk")
        .or_else(|| crate_name.strip_suffix("_sdk"))?;
    rust_projects
        .iter()
        .find(|project| project.is_network_target && project.name == service_candidate)
        .map(|project| project.name.clone())
}

fn insert_rust_dependency(
    deps: &mut BTreeMap<String, String>,
    dependency_name: String,
    dependency_type: &'static str,
) {
    match deps.get(&dependency_name).map(String::as_str) {
        Some("network") => {}
        Some(_) if dependency_type == "network" => {
            deps.insert(dependency_name, dependency_type.to_string());
        }
        Some(_) => {}
        None => {
            deps.insert(dependency_name, dependency_type.to_string());
        }
    }
}

fn contains_rust_crate_reference(source: &str, crate_ident: &str) -> bool {
    source.contains(&format!("use {crate_ident}::"))
        || source.contains(&format!("use {crate_ident} as "))
        || source.contains(&format!("extern crate {crate_ident}"))
        || source.contains(&format!("{crate_ident}::"))
}

fn clean_path(path: &Path) -> PathBuf {
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                cleaned.pop();
            }
            _ => cleaned.push(component.as_os_str()),
        }
    }
    cleaned
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
        assert!(
            deps.iter()
                .any(|d| d.service_name == "billing" && d.dependency_type == "network")
        );
        assert!(
            deps.iter()
                .any(|d| d.service_name == "iam" && d.dependency_type == "network")
        );
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

    #[test]
    fn detects_flat_native_projects_shared_core_and_service_sdk_paths() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for path in [
            ".forklaunch",
            "src/notes/api/routes",
            "src/insights",
            "src/core/security",
            "src/client-sdk/rust/notes",
        ] {
            std::fs::create_dir_all(root.join(path)).unwrap();
        }
        std::fs::write(root.join(".forklaunch/manifest.toml"), r#"
projects = [
  { type = "Service", name = "notes", metadata = { language = "rust" } },
  { type = "Worker", name = "insights", metadata = { language = "rust", cratePath = "src/insights" } },
  { type = "Library", name = "security", metadata = { language = "rust", cratePath = "src/core/security" } },
]
"#).unwrap();
        std::fs::write(
            root.join("src/notes/Cargo.toml"),
            r#"
[package]
name = "notes"
version = "0.1.0"
[lib]
path = "lib.rs"
[dependencies]
security = { path = "../core/security" }
"#,
        )
        .unwrap();
        std::fs::write(root.join("src/notes/lib.rs"), "pub mod api;\n").unwrap();
        std::fs::write(
            root.join("src/notes/api/routes/notes.rs"),
            "use security::Principal;\n",
        )
        .unwrap();
        std::fs::write(
            root.join("src/insights/Cargo.toml"),
            r#"
[package]
name = "insights"
version = "0.1.0"
[dependencies]
backend = { path = "../client-sdk/rust/notes" }
"#,
        )
        .unwrap();
        let dependencies = find_all_rust_service_dependencies(root).unwrap();
        assert_eq!(
            dependencies["notes"],
            vec![("security".into(), "local".into())]
        );
        assert_eq!(
            dependencies["insights"],
            vec![("notes".into(), "network".into())]
        );
    }

    #[test]
    fn service_local_sdk_is_a_network_dependency() {
        let projects = vec![RustProjectDependencyInfo {
            name: "notes".into(),
            crate_path: PathBuf::from("src/notes"),
            is_network_target: true,
        }];
        assert_eq!(
            classify_rust_path_dependency(Path::new("src/notes/sdk/rust"), &projects),
            Some(("notes".into(), "network")),
        );
        assert_eq!(
            classify_rust_path_dependency(Path::new("src/notes"), &projects),
            Some(("notes".into(), "local")),
        );
    }

    #[test]
    fn test_detects_rust_cargo_path_and_sdk_dependencies() {
        let temp_dir = tempfile::tempdir().unwrap();
        let app_root = temp_dir.path();
        std::fs::create_dir_all(app_root.join(".forklaunch")).unwrap();
        std::fs::create_dir_all(app_root.join("crates/accounts/src")).unwrap();
        std::fs::create_dir_all(app_root.join("crates/billing/src")).unwrap();
        std::fs::create_dir_all(app_root.join("crates/common-kernel/src")).unwrap();
        std::fs::create_dir_all(app_root.join("sdks/rust/billing-sdk")).unwrap();

        std::fs::write(
            app_root.join(".forklaunch/manifest.toml"),
            r#"
app_name = "rust-app"
modules_path = "crates"
projects = [
  { type = "Service", name = "accounts", metadata = { language = "rust", cratePath = "crates/accounts" } },
  { type = "Service", name = "billing", metadata = { language = "rust", cratePath = "crates/billing" } },
  { type = "Library", name = "common-kernel", metadata = { language = "rust", cratePath = "crates/common-kernel" } },
]
"#,
        )
        .unwrap();
        std::fs::write(
            app_root.join("crates/accounts/Cargo.toml"),
            r#"
[package]
name = "accounts"
version = "0.1.0"
edition = "2024"

[dependencies]
common-kernel = { path = "../common-kernel" }
billing-sdk = { path = "../../sdks/rust/billing-sdk" }
"#,
        )
        .unwrap();
        std::fs::write(
            app_root.join("crates/accounts/src/lib.rs"),
            "use billing_sdk::BillingClient;\nuse common_kernel::Repository;\n",
        )
        .unwrap();
        std::fs::write(
            app_root.join("crates/billing/Cargo.toml"),
            r#"
[package]
name = "billing"
version = "0.1.0"
edition = "2024"
"#,
        )
        .unwrap();
        std::fs::write(
            app_root.join("crates/common-kernel/Cargo.toml"),
            r#"
[package]
name = "common-kernel"
version = "0.1.0"
edition = "2024"
"#,
        )
        .unwrap();

        let deps = find_all_rust_service_dependencies(app_root).unwrap();
        let accounts_deps = deps.get("accounts").unwrap();
        assert!(
            accounts_deps
                .iter()
                .any(|(name, dep_type)| name == "billing" && dep_type == "network")
        );
        assert!(
            accounts_deps
                .iter()
                .any(|(name, dep_type)| name == "common-kernel" && dep_type == "local")
        );
        assert!(!deps.contains_key("billing"));
    }
}

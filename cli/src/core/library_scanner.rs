use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LibraryDefinition {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CodeNode {
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String, // "local" | "npm" | "api-call" | "dev-dependency"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>, // for npm
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<CodeNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>, // absolute or relative path for local files
    #[serde(rename = "targetService", skip_serializing_if = "Option::is_none")]
    pub target_service: Option<String>, // for api-call type: target service name
}

pub fn scan_project_libraries(package_json_path: &Path) -> Result<Vec<LibraryDefinition>> {
    let content = fs::read_to_string(package_json_path).context(format!(
        "Failed to read package.json at {:?}",
        package_json_path
    ))?;

    let json: serde_json::Value = serde_json::from_str(&content)?;

    let mut libraries = Vec::new();

    if let Some(dependencies) = json.get("dependencies").and_then(|d| d.as_object()) {
        for (name, version) in dependencies {
            // Check for @forklaunch or other "top level" libraries
            // The user said "stored in the manifest locally", implying workspace libs?
            // For now, let's include everything starting with @forklaunch
            if name.starts_with("@forklaunch") {
                libraries.push(LibraryDefinition {
                    name: name.clone(),
                    version: version.as_str().unwrap_or("unknown").to_string(),
                });
            }
        }
    }

    Ok(libraries)
}

#[allow(dead_code)]
pub fn scan_route_topology(
    file_path: &Path,
    modules_root: &Path,
    package_json_path: &Path,
) -> Result<CodeNode> {
    let mut scanner = ImportScanner::new(modules_root, package_json_path);
    scanner.scan(file_path)
}

/// Reusable import scanner that caches results across multiple scan() calls.
/// Create one per manifest generation and call scan() for each controller file
/// to benefit from shared result_cache across all scans.
pub struct ImportScanner {
    modules_root: PathBuf,
    #[allow(dead_code)]
    app_root: PathBuf,                                             // upper bound for lockfile search
    package_json_cache: HashMap<PathBuf, HashMap<String, String>>, // path → deps (specifiers)
    lockfile_importers: HashMap<String, HashMap<String, String>>,  // relative_path → { pkg → exact_version }
    lockfile_root: Option<PathBuf>,                                // directory containing pnpm-lock.yaml
    visited: HashSet<PathBuf>,
    result_cache: HashMap<PathBuf, CodeNode>,
    scope_prefix: String,        // e.g., "@forklaunch-platform/"
    #[allow(dead_code)]
    app_name: String,            // e.g., "forklaunch-platform"
    node_version: Option<String>, // Node.js version from Dockerfile (e.g., "25")
}

impl ImportScanner {
    pub fn new(modules_root: &Path, package_json_path: &Path) -> Self {
        // No explicit app_root — walk up unbounded to find lockfile
        Self::with_app_root(modules_root, package_json_path, "", Path::new("/"))
    }

    #[allow(dead_code)]
    pub fn new_with_app_name(modules_root: &Path, package_json_path: &Path, app_name: &str) -> Self {
        Self::with_app_root(modules_root, package_json_path, app_name, Path::new("/"))
    }

    pub fn with_app_root(modules_root: &Path, package_json_path: &Path, app_name: &str, app_root: &Path) -> Self {
        let mut package_json_cache = HashMap::new();
        if let Ok(deps) = Self::load_dependencies(package_json_path) {
            package_json_cache.insert(
                package_json_path.canonicalize().unwrap_or_else(|_| package_json_path.to_path_buf()),
                deps,
            );
        }

        let canonical_app_root = app_root.canonicalize().unwrap_or_else(|_| app_root.to_path_buf());

        // Find and parse pnpm-lock.yaml walking up from modules_root, bounded by app_root
        let (lockfile_importers, lockfile_root) = Self::load_lockfile(
            &modules_root.canonicalize().unwrap_or_else(|_| modules_root.to_path_buf()),
            &canonical_app_root,
        );

        // Parse Node.js version from Dockerfile
        let node_version = Self::parse_node_version_from_dockerfile(modules_root);

        Self {
            modules_root: modules_root.canonicalize().unwrap_or_else(|_| modules_root.to_path_buf()),
            app_root: canonical_app_root,
            package_json_cache,
            lockfile_importers,
            lockfile_root,
            visited: HashSet::new(),
            result_cache: HashMap::new(),
            scope_prefix: if app_name.is_empty() {
                String::new()
            } else {
                format!("@{}/", app_name)
            },
            app_name: app_name.to_string(),
            node_version,
        }
    }

    /// Normalize "@scope/name/subpath" to "@scope/name" for package.json lookup
    fn normalize_scoped_package(import_path: &str) -> String {
        if import_path.starts_with('@') {
            let parts: Vec<&str> = import_path.split('/').collect();
            if parts.len() >= 2 {
                return format!("{}/{}", parts[0], parts[1]);
            }
        }
        import_path.to_string()
    }

    fn load_dependencies(package_json_path: &Path) -> Result<HashMap<String, String>> {
        let content = fs::read_to_string(package_json_path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        let mut deps = HashMap::new();
        if let Some(d) = json.get("dependencies").and_then(|v| v.as_object()) {
            for (k, v) in d {
                deps.insert(k.clone(), v.as_str().unwrap_or("").to_string());
            }
        }
        // Also check devDependencies just in case
        if let Some(d) = json.get("devDependencies").and_then(|v| v.as_object()) {
            for (k, v) in d {
                deps.insert(k.clone(), v.as_str().unwrap_or("").to_string());
            }
        }
        Ok(deps)
    }

    /// Walk up from `start_dir` to find pnpm-lock.yaml, stopping at `stop_dir` (inclusive).
    fn load_lockfile(start_dir: &Path, stop_dir: &Path) -> (HashMap<String, HashMap<String, String>>, Option<PathBuf>) {
        let mut dir = start_dir.to_path_buf();
        loop {
            let candidate = dir.join("pnpm-lock.yaml");
            if candidate.exists() {
                if let Ok(importers) = Self::parse_lockfile(&candidate) {
                    return (importers, Some(dir));
                }
            }
            // Stop at app_root — don't walk above it
            if dir == stop_dir || !dir.starts_with(stop_dir) {
                break;
            }
            match dir.parent() {
                Some(parent) if parent != dir => dir = parent.to_path_buf(),
                _ => break,
            }
        }
        (HashMap::new(), None)
    }

    /// Parse pnpm-lock.yaml and extract importer → { pkg → exact_version } map.
    fn parse_lockfile(lockfile_path: &Path) -> Result<HashMap<String, HashMap<String, String>>> {
        let content = fs::read_to_string(lockfile_path)?;
        let yaml: serde_yml::Value = serde_yml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse lockfile: {}", e))?;

        let mut result = HashMap::new();

        let importers = yaml.get("importers")
            .and_then(|v| v.as_mapping());
        let importers = match importers {
            Some(m) => m,
            None => return Ok(result),
        };

        for (importer_key, importer_val) in importers {
            let importer_path = match importer_key.as_str() {
                Some(s) => s.to_string(),
                None => continue,
            };

            let mut deps_map = HashMap::new();

            // Process both dependencies and devDependencies
            for section in &["dependencies", "devDependencies"] {
                if let Some(deps) = importer_val.get(*section).and_then(|v| v.as_mapping()) {
                    for (name_val, info_val) in deps {
                        let name = match name_val.as_str() {
                            Some(s) => s,
                            None => continue,
                        };
                        let version = info_val.get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        // Skip workspace links (link:../foo)
                        if version.starts_with("link:") {
                            continue;
                        }

                        // Strip peer dep suffixes: "1.2.3(react@19.0.0)" → "1.2.3"
                        let exact = if let Some(idx) = version.find('(') {
                            &version[..idx]
                        } else {
                            version
                        };

                        if !exact.is_empty() {
                            deps_map.insert(name.to_string(), exact.to_string());
                        }
                    }
                }
            }

            if !deps_map.is_empty() {
                result.insert(importer_path, deps_map);
            }
        }

        Ok(result)
    }

    pub fn scan(&mut self, file_path: &Path) -> Result<CodeNode> {
        // Canonicalize to ensure cache/visited checks work regardless of ../. in paths
        let file_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
        let file_path = file_path.as_path();

        let file_name = file_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        // Return cached result if this file was already fully scanned (DAG dedup)
        if let Some(cached) = self.result_cache.get(file_path) {
            return Ok(cached.clone());
        }

        // Circular dependency check (file is currently being scanned in this call stack)
        if self.visited.contains(file_path) {
            return Ok(CodeNode {
                name: format!("{} (Cycle)", file_name),
                node_type: "local".to_string(),
                version: None,
                children: None,
                path: Some(file_path.to_string_lossy().to_string()),
                target_service: None,
            });
        }
        self.visited.insert(file_path.to_path_buf());

        let content = fs::read_to_string(file_path).unwrap_or_default();
        let imports = self.extract_imports(&content);
        let sdk_client_info = self.extract_sdk_client_info(&content);
        let file_dir = file_path.parent().unwrap();

        let mut children = Vec::new();

        for (import, is_type_only) in imports {
            if import.starts_with(".") {
                // Local import
                let resolved = self.resolve_local_import(file_dir, &import);
                if let Some(resolved_path) = resolved {
                    // Only scan if it's inside the modules root to avoid escaping
                    if resolved_path.starts_with(&self.modules_root) {
                        if let Ok(child_node) = self.scan(&resolved_path) {
                            children.push(child_node);
                        }
                    }
                }
            } else {
                // NPM / Library import — normalize scoped subpaths and capture ALL non-relative imports
                let pkg_name = Self::normalize_scoped_package(&import);
                let version = if import.starts_with("node:") {
                    self.node_version.clone()
                } else {
                    self.lookup_version(&pkg_name, file_dir)
                        .or_else(|| self.lookup_version(&import, file_dir))
                };

                // Classify the import:
                //   1. api-call: only SdkClient from scoped package
                //   2. npm (with target_service): mixed SdkClient + other imports from scoped package
                //   3. dev-dependency: type-only import (import type { ... })
                //   4. npm: regular import
                let is_scoped = !self.scope_prefix.is_empty()
                    && import.starts_with(&self.scope_prefix);
                let sdk_status = sdk_client_info.get(&import);

                let (node_type, target_service) = match (sdk_status, is_scoped) {
                    (Some(true), true) => {
                        // Only SdkClient imports → api-call
                        let svc = import[self.scope_prefix.len()..].split('/').next().unwrap_or("").to_string();
                        ("api-call", Some(svc))
                    }
                    (Some(false), true) => {
                        // Mixed SdkClient + other imports → npm with target_service for edge
                        let svc = import[self.scope_prefix.len()..].split('/').next().unwrap_or("").to_string();
                        ("npm", Some(svc))
                    }
                    _ if is_type_only => {
                        // Type-only import → dev-dependency
                        ("dev-dependency", None)
                    }
                    _ => {
                        // Regular import → npm
                        ("npm", None)
                    }
                };

                children.push(CodeNode {
                    name: pkg_name,
                    node_type: node_type.to_string(),
                    version: version.or_else(|| Some("unknown".to_string())),
                    children: None,
                    path: None,
                    target_service,
                });
            }
        }

        // Deduplicate non-local children by name
        let mut seen = HashSet::new();
        let children: Vec<CodeNode> = children
            .into_iter()
            .filter(|c| {
                if c.node_type != "local" {
                    seen.insert(c.name.clone())
                } else {
                    true
                }
            })
            .collect();

        self.visited.remove(file_path);

        let node = CodeNode {
            name: file_name,
            node_type: "local".to_string(),
            version: None,
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
            path: Some(
                file_path
                    .strip_prefix(&self.modules_root)
                    .unwrap_or(file_path)
                    .to_string_lossy()
                    .to_string(),
            ),
            target_service: None,
        };

        // Cache the result so re-visits return instantly
        self.result_cache.insert(file_path.to_path_buf(), node.clone());

        Ok(node)
    }

    fn extract_imports(&self, content: &str) -> Vec<(String, bool)> {
        let re = Regex::new(r#"import\s+(type\s+)?(?:[\w\s{},*]+from\s+)?['"]([^'"]+)['"]"#).unwrap();
        re.captures_iter(content)
            .filter_map(|cap| {
                cap.get(2).map(|m| {
                    let is_type_only = cap.get(1).is_some();
                    (m.as_str().to_string(), is_type_only)
                })
            })
            .collect()
    }

    /// Identify import paths that contain SdkClient types from app-scoped packages.
    /// Returns path → is_only_sdk:
    ///   true = ALL imported names are SdkClient (→ api-call type)
    ///   false = SOME imported names are SdkClient (→ npm type with target_service for edge)
    fn extract_sdk_client_info(&self, content: &str) -> HashMap<String, bool> {
        let mut result = HashMap::new();
        if self.scope_prefix.is_empty() {
            return result;
        }
        let re = Regex::new(r#"import\s+(?:type\s+)?\{([^}]*)\}\s+from\s+['"]([^'"]+)['"]"#).unwrap();
        for cap in re.captures_iter(content) {
            let names_str = cap.get(1).map_or("", |m| m.as_str());
            let module_path = cap.get(2).map_or("", |m| m.as_str());

            if !module_path.starts_with(&self.scope_prefix) {
                continue;
            }

            let names: Vec<&str> = names_str.split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                // Strip leading `type ` prefix from individual named imports
                .map(|s| s.strip_prefix("type ").unwrap_or(s).trim())
                .collect();

            if names.is_empty() {
                continue;
            }

            let has_sdk_client = names.iter().any(|name| name.contains("SdkClient"));
            if has_sdk_client {
                let all_sdk = names.iter().all(|name| name.contains("SdkClient"));
                result.insert(module_path.to_string(), all_sdk);
            }
        }
        result
    }

    /// Walk up from `dir` toward `modules_root` to find the nearest package.json.
    fn find_nearest_package_json(&self, dir: &Path) -> Option<PathBuf> {
        let mut current = dir.to_path_buf();
        loop {
            let candidate = current.join("package.json");
            if candidate.exists() {
                return candidate.canonicalize().ok();
            }
            // Stop at modules_root (don't go above it)
            if current == self.modules_root || !current.starts_with(&self.modules_root) {
                break;
            }
            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => break,
            }
        }
        None
    }

    /// Look up a package version: lockfile (exact) → nearest package.json (specifier) → any cached.
    fn lookup_version(&mut self, pkg_name: &str, file_dir: &Path) -> Option<String> {
        // 1. Try pnpm-lock.yaml for exact resolved version
        if let Some(ref lockfile_root) = self.lockfile_root.clone() {
            // Find the importer whose path best matches file_dir relative to lockfile root
            let canonical_dir = file_dir.canonicalize().unwrap_or_else(|_| file_dir.to_path_buf());
            let canonical_root = lockfile_root.canonicalize().unwrap_or_else(|_| lockfile_root.clone());
            if let Ok(rel) = canonical_dir.strip_prefix(&canonical_root) {
                let rel_str = rel.to_string_lossy().to_string();
                // Walk up from file_dir's relative path to find the matching importer
                let mut search = rel_str.as_str();
                loop {
                    if let Some(deps) = self.lockfile_importers.get(search) {
                        if let Some(version) = deps.get(pkg_name) {
                            return Some(version.clone());
                        }
                    }
                    // Walk up one directory
                    match search.rfind('/') {
                        Some(idx) => search = &search[..idx],
                        None => {
                            // Try root importer "."
                            if let Some(deps) = self.lockfile_importers.get(".") {
                                if let Some(version) = deps.get(pkg_name) {
                                    return Some(version.clone());
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }

        // 2. Try nearest package.json (specifier/range)
        if let Some(pj_path) = self.find_nearest_package_json(file_dir) {
            if !self.package_json_cache.contains_key(&pj_path) {
                if let Ok(deps) = Self::load_dependencies(&pj_path) {
                    self.package_json_cache.insert(pj_path.clone(), deps);
                }
            }
            if let Some(deps) = self.package_json_cache.get(&pj_path) {
                if let Some(version) = deps.get(pkg_name) {
                    return Some(version.clone());
                }
            }
        }

        // 3. Fall back to any cached package.json
        for deps in self.package_json_cache.values() {
            if let Some(version) = deps.get(pkg_name) {
                return Some(version.clone());
            }
        }

        None
    }

    /// Extract Node.js major version from the Dockerfile's FROM directive.
    /// Looks for patterns like `FROM node:25-trixie-slim` and extracts "25".
    fn parse_node_version_from_dockerfile(modules_root: &Path) -> Option<String> {
        let dockerfile_path = modules_root.join("Dockerfile");
        let content = fs::read_to_string(&dockerfile_path).ok()?;
        let from_re = Regex::new(r"(?i)^FROM\s+node:(\d+)").ok()?;
        for line in content.lines() {
            if let Some(cap) = from_re.captures(line.trim()) {
                return cap.get(1).map(|m| m.as_str().to_string());
            }
        }
        None
    }

    fn resolve_local_import(&self, param_dir: &Path, import_path: &str) -> Option<PathBuf> {
        let base = param_dir.join(import_path);

        // Try exact match
        if base.exists() && base.is_file() {
            return base.canonicalize().ok();
        }

        // Try appending .ts (not replacing extension, since filenames like "plan.controller"
        // have dots that aren't file extensions)
        let mut ts_name = base.as_os_str().to_os_string();
        ts_name.push(".ts");
        let ts = PathBuf::from(ts_name);
        if ts.exists() {
            return ts.canonicalize().ok();
        }

        // Try appending .tsx
        let mut tsx_name = base.as_os_str().to_os_string();
        tsx_name.push(".tsx");
        let tsx = PathBuf::from(tsx_name);
        if tsx.exists() {
            return tsx.canonicalize().ok();
        }

        // Try /index.ts
        let index = base.join("index.ts");
        if index.exists() {
            return index.canonicalize().ok();
        }

        None
    }
}

/// Parsed route extracted from a .routes.ts file
#[derive(Debug, Clone)]
pub struct ParsedRoute {
    pub method: String,
    pub path: String,
    pub handler: String,
}

/// Parse a .routes.ts file to extract route definitions and map handlers to source files.
///
/// Returns (routes, handler_source_map) where:
/// - routes: Vec of (method, path, handler_name)
/// - handler_source_map: HashMap of handler_name → resolved source file path
pub fn parse_route_file(
    route_file_path: &Path,
    modules_root: &Path,
) -> Result<(Vec<ParsedRoute>, HashMap<String, PathBuf>)> {
    let content = fs::read_to_string(route_file_path)
        .with_context(|| format!("Failed to read route file: {:?}", route_file_path))?;

    let parent_dir = route_file_path.parent().unwrap_or(Path::new("."));

    // 1. Extract imports: `import { handler1, handler2 } from '../controllers/foo.controller'`
    //    Use (?s) dotall flag so [^}]+ matches across newlines for multi-line imports
    let import_re = Regex::new(
        r#"(?s)import\s*\{([^}]+)\}\s*from\s*['"]([^'"]+)['"]"#
    ).unwrap();

    // Build map: handler_name → import source path (relative)
    let mut handler_to_import_path: HashMap<String, String> = HashMap::new();
    for cap in import_re.captures_iter(&content) {
        let names_str = cap.get(1).map_or("", |m| m.as_str());
        let import_path = cap.get(2).map_or("", |m| m.as_str());

        // Skip non-controller imports (e.g., forklaunchRouter, schemaValidator, bootstrapper)
        if !import_path.contains("controller") {
            continue;
        }

        for name in names_str.split(',') {
            let name = name.trim();
            // Skip `type` imports like `type SomeType`
            if name.is_empty() || name.starts_with("type ") {
                continue;
            }
            handler_to_import_path.insert(name.to_string(), import_path.to_string());
        }
    }

    // Resolve import paths to actual file paths
    let mut handler_source_map: HashMap<String, PathBuf> = HashMap::new();
    let scanner = ImportScanner::new(modules_root, &modules_root.join("package.json"));
    for (handler, import_path) in &handler_to_import_path {
        if let Some(resolved) = scanner.resolve_local_import(parent_dir, import_path) {
            handler_source_map.insert(handler.clone(), resolved);
        }
    }

    // 2. Extract route registrations:
    //    `router.get('/:id', getUser)` or `routerName.post('/', createUser)`
    //    Handles both single-line and multi-line formats
    let route_re = Regex::new(
        r#"\.\s*(get|post|put|delete|patch|head|options)\s*\(\s*['"]([^'"]+)['"]\s*,\s*(\w+)"#
    ).unwrap();

    let mut routes: Vec<ParsedRoute> = Vec::new();
    for cap in route_re.captures_iter(&content) {
        let method = cap.get(1).map_or("", |m| m.as_str()).to_uppercase();
        let path = cap.get(2).map_or("", |m| m.as_str()).to_string();
        let handler = cap.get(3).map_or("", |m| m.as_str()).to_string();

        routes.push(ParsedRoute {
            method,
            path,
            handler,
        });
    }

    Ok((routes, handler_source_map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_worker_controller_topology() {
        // Test with actual sample-worker controller
        let controller_path = Path::new("../blueprint/sample-worker/api/controllers/sampleWorker.controller.ts");
        let modules_root = Path::new("../blueprint/sample-worker");
        let package_json = Path::new("../blueprint/sample-worker/package.json");

        // Skip test if files don't exist (e.g., in CI without blueprint)
        if !controller_path.exists() {
            eprintln!("Skipping test - controller not found at: {:?}", controller_path);
            return;
        }

        let result = scan_route_topology(controller_path, modules_root, package_json);

        assert!(result.is_ok(), "Topology scan should succeed");

        let topology = result.unwrap();

        // Serialize to JSON first (to avoid ownership issues)
        let json = serde_json::to_string_pretty(&topology);
        assert!(json.is_ok(), "Should serialize to JSON");

        // Verify root node
        assert_eq!(topology.name, "sampleWorker.controller");
        assert_eq!(topology.node_type, "local");
        assert!(topology.path.is_some());

        // Verify it found dependencies
        assert!(topology.children.is_some(), "Should have dependencies");
        let children = topology.children.as_ref().unwrap();
        assert!(!children.is_empty(), "Should have at least one dependency");

        // Print topology for debugging
        println!("=== TOPOLOGY SCAN RESULTS ===");
        println!("Root: {} (type: {})", topology.name, topology.node_type);
        println!("Dependencies found: {}", children.len());

        for child in children {
            println!("  - {} (type: {}, version: {:?})",
                child.name,
                child.node_type,
                child.version
            );
        }

        // Verify we found both npm and local dependencies
        let has_npm = children.iter().any(|c| c.node_type == "npm");
        let has_local = children.iter().any(|c| c.node_type == "local");

        assert!(has_npm, "Should find npm dependencies");
        assert!(has_local, "Should find local dependencies");

        println!("\n=== JSON OUTPUT ===");
        println!("{}", json.unwrap());
    }

    #[test]
    fn test_scan_service_controller_topology() {
        // Test with actual billing-stripe service controller
        let controller_path = Path::new("../blueprint/billing-stripe/api/controllers/plan.controller.ts");
        let modules_root = Path::new("../blueprint/billing-stripe");
        let package_json = Path::new("../blueprint/billing-stripe/package.json");

        // Skip test if files don't exist
        if !controller_path.exists() {
            eprintln!("Skipping test - controller not found at: {:?}", controller_path);
            return;
        }

        let result = scan_route_topology(controller_path, modules_root, package_json);

        assert!(result.is_ok(), "Topology scan should succeed for service");

        let topology = result.unwrap();

        // Verify root node
        assert_eq!(topology.name, "plan.controller");
        assert_eq!(topology.node_type, "local");
        assert!(topology.path.is_some());

        // Verify it found dependencies
        assert!(topology.children.is_some(), "Service should have dependencies");
        let children = topology.children.as_ref().unwrap();
        assert!(!children.is_empty(), "Service should have at least one dependency");

        println!("=== SERVICE TOPOLOGY SCAN ===");
        println!("Root: {} (type: {})", topology.name, topology.node_type);
        println!("Dependencies found: {}", children.len());

        for child in children {
            println!("  - {} (type: {}, version: {:?})",
                child.name,
                child.node_type,
                child.version
            );
        }

        // Services should have npm dependencies like @forklaunch packages
        let has_forklaunch = children.iter().any(|c|
            c.name.starts_with("@forklaunch")
        );
        assert!(has_forklaunch, "Service should use @forklaunch packages");
    }

    #[test]
    fn test_extract_imports() {
        let scanner = ImportScanner::new(Path::new("."), Path::new("package.json"));

        let code = r#"
            import { handlers } from '@forklaunch/blueprint-core';
            import { ci, tokens } from '../../bootstrapper';
            import type { SomeType } from './types';
            import { something } from '@forklaunch-platform/iam/utils';
            import Stripe from 'stripe';
        "#;

        let imports = scanner.extract_imports(code);
        let paths: Vec<&str> = imports.iter().map(|(p, _)| p.as_str()).collect();

        assert_eq!(imports.len(), 5);
        assert!(paths.contains(&"@forklaunch/blueprint-core"));
        assert!(paths.contains(&"../../bootstrapper"));
        assert!(paths.contains(&"./types"));
        assert!(paths.contains(&"@forklaunch-platform/iam/utils"));
        assert!(paths.contains(&"stripe"));

        // Verify type-only detection
        let type_only: Vec<&str> = imports.iter()
            .filter(|(_, is_type)| *is_type)
            .map(|(p, _)| p.as_str())
            .collect();
        assert_eq!(type_only.len(), 1);
        assert!(type_only.contains(&"./types"));
    }

    #[test]
    fn test_normalize_scoped_package() {
        // Scoped with subpath
        assert_eq!(
            ImportScanner::normalize_scoped_package("@forklaunch-platform/iam/utils"),
            "@forklaunch-platform/iam"
        );
        // Scoped without subpath
        assert_eq!(
            ImportScanner::normalize_scoped_package("@forklaunch/core"),
            "@forklaunch/core"
        );
        // Non-scoped package
        assert_eq!(
            ImportScanner::normalize_scoped_package("stripe"),
            "stripe"
        );
        // Non-scoped with subpath (shouldn't happen in practice but handled)
        assert_eq!(
            ImportScanner::normalize_scoped_package("express"),
            "express"
        );
    }

    #[test]
    fn test_parse_route_file_plan() {
        let route_file = Path::new("../blueprint/billing-stripe/api/routes/plan.routes.ts");
        let modules_root = Path::new("../blueprint/billing-stripe");

        if !route_file.exists() {
            eprintln!("Skipping test - route file not found at: {:?}", route_file);
            return;
        }

        let result = parse_route_file(route_file, modules_root);
        assert!(result.is_ok(), "Should parse route file");

        let (routes, handler_sources) = result.unwrap();

        // Should find 5 routes: createPlan, listPlans, getPlan, updatePlan, deletePlan
        assert_eq!(routes.len(), 5, "Should find 5 routes");

        // Verify methods
        let methods: Vec<&str> = routes.iter().map(|r| r.method.as_str()).collect();
        assert!(methods.contains(&"POST"), "Should have POST route");
        assert!(methods.contains(&"GET"), "Should have GET routes");
        assert!(methods.contains(&"PUT"), "Should have PUT route");
        assert!(methods.contains(&"DELETE"), "Should have DELETE route");

        // Verify handlers
        let handlers: Vec<&str> = routes.iter().map(|r| r.handler.as_str()).collect();
        assert!(handlers.contains(&"createPlan"), "Should have createPlan handler");
        assert!(handlers.contains(&"getPlan"), "Should have getPlan handler");

        // Verify handler sources point to controller file
        assert!(!handler_sources.is_empty(), "Should have handler source mappings");
        for (_handler, source_path) in &handler_sources {
            assert!(source_path.exists(), "Source path should exist: {:?}", source_path);
        }

        println!("=== PARSED ROUTES ===");
        for route in &routes {
            println!("  {} {} -> {}", route.method, route.path, route.handler);
        }
        println!("=== HANDLER SOURCES ===");
        for (handler, source) in &handler_sources {
            println!("  {} -> {:?}", handler, source);
        }
    }

    #[test]
    fn test_parse_route_file_sample_worker() {
        let route_file = Path::new("../blueprint/sample-worker/api/routes/sampleWorker.routes.ts");
        let modules_root = Path::new("../blueprint/sample-worker");

        if !route_file.exists() {
            eprintln!("Skipping test - route file not found at: {:?}", route_file);
            return;
        }

        let result = parse_route_file(route_file, modules_root);
        assert!(result.is_ok(), "Should parse worker route file");

        let (routes, handler_sources) = result.unwrap();

        // Should find 2 routes: sampleWorkerGet, sampleWorkerPost
        assert_eq!(routes.len(), 2, "Should find 2 routes");

        let handlers: Vec<&str> = routes.iter().map(|r| r.handler.as_str()).collect();
        assert!(handlers.contains(&"sampleWorkerGet"));
        assert!(handlers.contains(&"sampleWorkerPost"));

        assert!(!handler_sources.is_empty(), "Should resolve handler sources");
    }

    #[test]
    fn test_route_topology_has_npm_versions() {
        // Test that per-route topology scanning resolves npm package versions from package.json
        let controller_path = Path::new("../blueprint/billing-stripe/api/controllers/plan.controller.ts");
        let modules_root = Path::new("../blueprint/billing-stripe");
        let package_json = Path::new("../blueprint/billing-stripe/package.json");

        if !controller_path.exists() {
            eprintln!("Skipping test - controller not found at: {:?}", controller_path);
            return;
        }

        let result = scan_route_topology(controller_path, modules_root, package_json);
        assert!(result.is_ok(), "Topology scan should succeed");

        let topology = result.unwrap();

        // Root should be local
        assert_eq!(topology.node_type, "local");
        assert!(topology.children.is_some(), "Should have children");

        let children = topology.children.as_ref().unwrap();

        // Find npm children and verify they have versions
        let npm_children: Vec<&CodeNode> = children.iter()
            .filter(|c| c.node_type == "npm")
            .collect();

        assert!(!npm_children.is_empty(), "Should have npm dependencies");

        println!("=== ROUTE TOPOLOGY NPM VERSIONS ===");
        for npm_dep in &npm_children {
            println!("  {} -> version: {:?}", npm_dep.name, npm_dep.version);
            // Every npm dep should have a version (from package.json)
            assert!(
                npm_dep.version.is_some(),
                "npm dep '{}' should have a version",
                npm_dep.name
            );
            // Version should not be empty
            let version = npm_dep.version.as_ref().unwrap();
            assert!(
                !version.is_empty(),
                "npm dep '{}' version should not be empty",
                npm_dep.name
            );
        }

        // Also check nested local children for npm deps with versions
        let local_children: Vec<&CodeNode> = children.iter()
            .filter(|c| c.node_type == "local")
            .collect();

        for local_child in &local_children {
            if let Some(nested) = &local_child.children {
                let nested_npm: Vec<&CodeNode> = nested.iter()
                    .filter(|c| c.node_type == "npm")
                    .collect();
                for npm_dep in &nested_npm {
                    println!("  (nested in {}) {} -> version: {:?}",
                        local_child.name, npm_dep.name, npm_dep.version);
                    assert!(
                        npm_dep.version.is_some(),
                        "Nested npm dep '{}' in '{}' should have a version",
                        npm_dep.name, local_child.name
                    );
                }
            }
        }
    }

    #[test]
    fn test_route_handler_topology_per_route() {
        // Test that parse_route_file + scan_route_topology produces per-handler topology
        let route_file = Path::new("../blueprint/billing-stripe/api/routes/plan.routes.ts");
        let modules_root = Path::new("../blueprint/billing-stripe");
        let package_json = Path::new("../blueprint/billing-stripe/package.json");

        if !route_file.exists() {
            eprintln!("Skipping test - route file not found at: {:?}", route_file);
            return;
        }

        let (routes, handler_sources) = parse_route_file(route_file, modules_root)
            .expect("Should parse route file");

        assert!(!routes.is_empty(), "Should find routes");
        assert!(!handler_sources.is_empty(), "Should resolve handler sources");

        println!("=== PER-ROUTE TOPOLOGY ===");
        for route in &routes {
            println!("\n  Route: {} {}", route.method, route.path);

            if let Some(source_path) = handler_sources.get(&route.handler) {
                let topology = scan_route_topology(source_path, modules_root, package_json);
                assert!(
                    topology.is_ok(),
                    "Topology scan should succeed for handler '{}'",
                    route.handler
                );

                let topo = topology.unwrap();
                println!("    Root: {} (type: {})", topo.name, topo.node_type);

                if let Some(children) = &topo.children {
                    for child in children {
                        match child.node_type.as_str() {
                            "npm" => println!("    npm: {} @ {}", child.name,
                                child.version.as_deref().unwrap_or("?")),
                            "local" => println!("    local: {} ({})", child.name,
                                child.path.as_deref().unwrap_or("?")),
                            _ => {}
                        }
                    }

                    // Verify npm deps have versions
                    let npm_deps: Vec<&CodeNode> = children.iter()
                        .filter(|c| c.node_type == "npm")
                        .collect();
                    for dep in &npm_deps {
                        assert!(
                            dep.version.is_some() && !dep.version.as_ref().unwrap().is_empty(),
                            "Route {} {} handler '{}': npm dep '{}' should have version",
                            route.method, route.path, route.handler, dep.name
                        );
                    }
                }
            } else {
                println!("    (no source mapping for handler '{}')", route.handler);
            }
        }
    }
}

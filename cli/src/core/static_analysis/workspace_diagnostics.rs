//! Read-only structural evidence. No build scripts, compilers or application code execute.
use crate::core::manifest::{ProjectEntry, ProjectType};
use serde::Serialize;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Diagnostic {
    pub code: &'static str,
    pub severity: &'static str,
    pub project: String,
    pub path: String,
    pub message: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectCoverage {
    pub project: String,
    pub language: String,
    pub directory: String,
    pub status: &'static str,
    pub rust_files: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub delegated_surfaces: Vec<serde_json::Value>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Analysis {
    pub schema_version: u32,
    pub scope: &'static str,
    pub module_filter: Option<String>,
    pub status: &'static str,
    pub projects: Vec<ProjectCoverage>,
    pub diagnostics: Vec<Diagnostic>,
    pub not_checked: Vec<&'static str>,
}
impl Analysis {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == "error")
    }
    fn issue(
        &mut self,
        code: &'static str,
        severity: &'static str,
        project: &str,
        path: &Path,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            code,
            severity,
            project: project.into(),
            path: path.to_string_lossy().into(),
            message: message.into(),
        });
    }
}
pub(crate) fn project_dir(root: &Path, modules: &str, project: &ProjectEntry) -> Option<PathBuf> {
    let relative = project
        .metadata
        .as_ref()
        .and_then(|m| m.crate_path.as_deref().or(m.static_path.as_deref()))
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(modules).join(&project.name));
    if relative.is_absolute()
        || relative.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let directory = root.join(relative);
    if directory.exists()
        && !directory
            .canonicalize()
            .ok()?
            .starts_with(root.canonicalize().ok()?)
    {
        return None;
    }
    Some(directory)
}
fn source_files(
    dir: &Path,
    root: &Path,
    out: &mut Vec<String>,
    budget: &mut usize,
    skipped_symlinks: &mut bool,
) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries {
        let Ok(entry) = entry else { return false };
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        let Ok(kind) = entry.file_type() else {
            return false;
        };
        if matches!(
            entry.file_name().to_str(),
            Some("target" | "node_modules" | ".git" | "vendor")
        ) && (kind.is_dir() || kind.is_symlink())
        {
            continue;
        }
        if kind.is_symlink() {
            *skipped_symlinks = true;
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if !source_files(&path, root, out, budget, skipped_symlinks) {
                return false;
            }
        } else if path.extension().is_some_and(|e| e == "rs")
            || path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".entity.ts") || n.ends_with(".schema.ts"))
        {
            out.push(
                path.strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into(),
            );
        }
    }
    true
}
fn bounded(path: &Path) -> Result<String, String> {
    let metadata = fs::metadata(path).map_err(|_| "Cannot read file metadata")?;
    if !metadata.is_file() || metadata.len() > 2_097_152 {
        return Err("Expected a regular file no larger than 2 MiB".into());
    }
    fs::read_to_string(path).map_err(|_| "Cannot read UTF-8 file".into())
}
fn target_exists(root: &Path, dir: &Path, path: &str) -> bool {
    let target = dir.join(path);
    target.is_file()
        && target
            .canonicalize()
            .ok()
            .is_some_and(|p| root.canonicalize().ok().is_some_and(|r| p.starts_with(r)))
}
fn cargo(report: &mut Analysis, root: &Path, dir: &Path, p: &ProjectEntry) {
    let path = dir.join("Cargo.toml");
    if !path.is_file() {
        report.issue(
            "FLR-LAYOUT-001",
            "error",
            &p.name,
            &path,
            "Rust project is missing its own Cargo.toml",
        );
        return;
    }
    let value = bounded(&path).and_then(|s| {
        toml::from_str::<toml::Value>(&s).map_err(|_| "Cannot parse Cargo.toml".into())
    });
    let value = match value {
        Ok(v) => v,
        Err(e) => {
            report.issue("FLR-ANALYSIS-001", "error", &p.name, &path, e);
            return;
        }
    };
    let library = matches!(p.r#type, ProjectType::Library);
    if value.get("package").is_none() {
        if !library || value.get("workspace").is_none() {
            report.issue(
                "FLR-LAYOUT-001",
                "error",
                &p.name,
                &path,
                "Project requires a Cargo package; only libraries may be virtual workspaces",
            );
        }
        return;
    }
    if !value.get("package").is_some_and(toml::Value::is_table) {
        report.issue(
            "FLR-ANALYSIS-001",
            "error",
            &p.name,
            &path,
            "Cargo package metadata must be a table",
        );
        return;
    }
    let mut targets = Vec::new();
    if let Some(bins) = value.get("bin").and_then(toml::Value::as_array) {
        for bin in bins {
            if let Some(path) = bin.get("path").and_then(toml::Value::as_str) {
                targets.push(path.to_owned());
            } else if target_exists(root, dir, "src/main.rs") {
                targets.push("src/main.rs".into());
            } else if let Some(name) = bin.get("name").and_then(toml::Value::as_str) {
                let file = format!("src/bin/{name}.rs");
                let folder = format!("src/bin/{name}/main.rs");
                targets.push(if target_exists(root, dir, &file) {
                    file
                } else {
                    folder
                });
            }
        }
    }
    if library {
        if let Some(lib) = value.get("lib") {
            targets.push(
                lib.get("path")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("src/lib.rs")
                    .into(),
            );
        } else if value["package"]
            .get("autolib")
            .and_then(toml::Value::as_bool)
            != Some(false)
        {
            targets.push("src/lib.rs".into());
        }
    } else if targets.is_empty() {
        if value["package"]
            .get("autobins")
            .and_then(toml::Value::as_bool)
            != Some(false)
        {
            if target_exists(root, dir, "src/main.rs") {
                targets.push("src/main.rs".into());
            }
            if let Ok(entries) = fs::read_dir(dir.join("src/bin")) {
                for e in entries.flatten() {
                    let file = e.path();
                    if file.extension().is_some_and(|e| e == "rs") {
                        targets.push(format!("src/bin/{}", e.file_name().to_string_lossy()));
                    } else if file.join("main.rs").is_file() {
                        targets.push(format!(
                            "src/bin/{}/main.rs",
                            e.file_name().to_string_lossy()
                        ));
                    }
                }
            }
        }
        if targets.is_empty() {
            report.issue(
                "FLR-LAYOUT-002",
                "error",
                &p.name,
                &path,
                "Service or worker has no discoverable Cargo binary target",
            );
        }
    }
    for target in targets {
        if !target_exists(root, dir, &target) {
            report.issue(
                "FLR-LAYOUT-002",
                "error",
                &p.name,
                &dir.join(target),
                "Declared Cargo target source is missing or escapes the application",
            );
        }
    }
}
pub(crate) fn inspect(
    root: &Path,
    modules: &str,
    projects: &[ProjectEntry],
    application_language: Option<&str>,
    filter: Option<&str>,
) -> Analysis {
    let mut report = Analysis {
        schema_version: 1,
        scope: if filter.is_some() {
            "module"
        } else {
            "workspace"
        },
        module_filter: filter.map(str::to_owned),
        status: "partial",
        projects: vec![],
        diagnostics: vec![],
        not_checked: vec![
            "rust_macro_expansion_and_typechecking",
            "typescript_typechecking",
            "canonical_contract_semantics",
            "generated_artifact_freshness_and_digests",
            "runtime_auth_tenancy_encryption_and_audit",
            "retention_and_migration_policy",
            "docker_execution_and_shell_commands",
        ],
    };
    if let Some(language) = application_language {
        if !matches!(language, "rust" | "typescript" | "mixed") {
            report.issue(
                "FLR-LANGUAGE-001",
                "error",
                "",
                Path::new(".forklaunch/manifest.toml"),
                "Unsupported application language",
            );
        }
    }
    for p in projects
        .iter()
        .filter(|p| filter.is_none_or(|name| p.name == name))
    {
        let Some(dir) = project_dir(root, modules, p) else {
            report.issue(
                "FLR-LAYOUT-001",
                "error",
                &p.name,
                Path::new(".forklaunch/manifest.toml"),
                "Project directory escapes application or cannot be resolved",
            );
            continue;
        };
        let relative = dir.strip_prefix(root).unwrap_or(&dir);
        let declared = p.metadata.as_ref().and_then(|m| m.language.as_deref());
        let language = declared
            .or(application_language.filter(|l| *l != "mixed"))
            .unwrap_or_else(|| {
                if dir.join("Cargo.toml").is_file() {
                    "rust"
                } else if dir.join("package.json").is_file() {
                    "typescript"
                } else {
                    "unknown"
                }
            });
        let mut coverage = ProjectCoverage {
            project: p.name.clone(),
            language: language.into(),
            directory: relative.to_string_lossy().into(),
            status: "structural_only",
            rust_files: vec![],
            delegated_surfaces: vec![],
        };
        if !dir.is_dir() {
            coverage.status = "missing";
            report.issue(
                "FLR-LAYOUT-001",
                "error",
                &p.name,
                relative,
                "Manifest project directory is missing",
            );
        } else {
            let mut skipped_symlinks = false;
            if !source_files(
                &dir,
                root,
                &mut coverage.rust_files,
                &mut 20000,
                &mut skipped_symlinks,
            ) {
                if !report.not_checked.contains(&"complete_source_inventory") {
                    report.not_checked.push("complete_source_inventory");
                }
                report.issue(
                    "FLR-ANALYSIS-001",
                    "warning",
                    &p.name,
                    relative,
                    "Source traversal incomplete: unreadable directory or file budget exceeded",
                );
                coverage.status = "incomplete";
            }
            if skipped_symlinks && !report.not_checked.contains(&"symlinked_source_inventory") {
                report.not_checked.push("symlinked_source_inventory");
            }
            for source in &coverage.rust_files {
                let path = root.join(source);
                let parsed = if source.ends_with(".entity.ts") {
                    bounded(&path).and_then(|_| {
                        super::EntityAnalyzer::parse_entity_file(&path)
                            .map(|_| ())
                            .map_err(|_| "Entity source could not be parsed".into())
                    })
                } else if source.ends_with(".schema.ts") {
                    bounded(&path).and_then(|_| {
                        super::SchemaAnalyzer::parse_schema_file(&path)
                            .map(|_| ())
                            .map_err(|_| "Schema source could not be parsed".into())
                    })
                } else if source.ends_with(".rs") {
                    bounded(&path).and_then(|source| {
                        syn::parse_file(&source)
                            .map(|_| ())
                            .map_err(|error| format!("Rust source could not be parsed: {error}"))
                    })
                } else {
                    continue;
                };
                if let Err(error) = parsed {
                    report.issue(
                        "FLR-ANALYSIS-001",
                        "error",
                        &p.name,
                        Path::new(source),
                        error,
                    );
                    coverage.status = "incomplete";
                }
            }
            coverage.rust_files.retain(|path| path.ends_with(".rs"));
            coverage.rust_files.sort();
            match language {
                "rust" => cargo(&mut report, root, &dir, p),
                "typescript" => {
                    let package = dir.join("package.json");
                    match bounded(&package).and_then(|s| {
                        serde_json::from_str::<serde_json::Value>(&s)
                            .map_err(|_| "Cannot parse package.json".into())
                    }) {
                        Ok(v) if v.is_object() => {}
                        _ => report.issue(
                            "FLR-LAYOUT-003",
                            "error",
                            &p.name,
                            &package,
                            "TypeScript project requires a valid package.json object",
                        ),
                    }
                }
                _ => report.issue(
                    "FLR-LANGUAGE-001",
                    "error",
                    &p.name,
                    relative,
                    "Project language is unknown or unsupported; declare metadata.language",
                ),
            }
            if declared.is_some()
                && application_language
                    .is_some_and(|l| matches!(l, "rust" | "typescript") && l != language)
            {
                report.issue("FLR-LANGUAGE-002","error",&p.name,relative,"Project language contradicts application language; declare a mixed application");
            }
            if language == "typescript"
                && dir.join("Cargo.toml").is_file()
                && !dir.join("package.json").is_file()
            {
                report.issue(
                    "FLR-LANGUAGE-002",
                    "error",
                    &p.name,
                    relative,
                    "Declared TypeScript project has Cargo metadata but no package.json",
                );
            }
        }
        let artifact = PathBuf::from(crate::core::artifact_paths::path(
            &p.name,
            p.metadata
                .as_ref()
                .and_then(|metadata| toml::Table::try_from(metadata).ok())
                .as_ref(),
            crate::core::artifact_paths::Artifact::Contract,
        ));
        let artifact_file = root.join(&artifact);
        if !artifact.is_absolute()
            && !artifact
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            && artifact_file
                .canonicalize()
                .is_ok_and(|path| path.starts_with(root.canonicalize().unwrap_or_default()))
            && let Ok(bytes) = bounded(&artifact_file)
        {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&bytes) {
                if let Some(surfaces) = value
                    .get("delegatedSurfaces")
                    .and_then(serde_json::Value::as_array)
                {
                    coverage.delegated_surfaces = surfaces.clone();
                    if !surfaces.is_empty() {
                        report.issue("FLR-CONTRACT-001", "warning", &p.name, &artifact,
                            "Provider-owned delegated endpoints are excluded from generated endpoint SDKs; use the declared provider client. Artifact metadata is structural evidence, not provider contract verification.");
                        if !report.not_checked.contains(&"delegated_provider_endpoints") {
                            report.not_checked.push("delegated_provider_endpoints");
                        }
                    }
                }
            }
        }
        report.projects.push(coverage);
    }
    if filter.is_some() && report.projects.is_empty() && !report.has_errors() {
        report.issue(
            "FLR-ANALYSIS-002",
            "error",
            filter.unwrap(),
            Path::new(".forklaunch/manifest.toml"),
            "Module filter matches no manifest project",
        );
    }
    compose(&mut report, root, filter);
    report.projects.sort_by(|a, b| a.project.cmp(&b.project));
    report
        .diagnostics
        .sort_by(|a, b| (&a.project, a.code, &a.path).cmp(&(&b.project, b.code, &b.path)));
    if report.has_errors() {
        report.status = "errors";
    }
    report
}

fn compose(report: &mut Analysis, root: &Path, filter: Option<&str>) {
    let mut files = vec![
        "compose.yaml".to_owned(),
        "compose.yml".into(),
        "docker-compose.yaml".into(),
        "docker-compose.yml".into(),
    ];
    if let Ok(source) = bounded(&root.join(".forklaunch/manifest.toml")) {
        if let Ok(manifest) = toml::from_str::<toml::Value>(&source) {
            if let Some(path) = manifest
                .get("docker_compose_path")
                .and_then(toml::Value::as_str)
            {
                files.push(path.into());
            }
        }
    }
    files.sort();
    files.dedup();
    // Generated Dockerfiles carry a fingerprint; their inventories are derived.
    let rust_image = crate::init::rust_project::rust_image_summary(root);
    let generated_image = rust_image
        .as_ref()
        .filter(|(_, _, unchanged)| *unchanged)
        .map(|(image, binaries, _)| serde_json::json!({"image": image, "binaries": binaries}));
    let generated_image = generated_image.as_ref();
    let typescript = crate::init::rust_project::typescript_image_summary(root);
    let typescript_receipt = typescript.as_ref().map(|(receipt, _, _)| receipt);
    let typescript_plan = typescript.as_ref().and_then(|(_, plan, _)| plan.clone());
    let managed_typescript = typescript
        .as_ref()
        .filter(|(_, _, unchanged)| *unchanged)
        .map(|(receipt, _, _)| receipt);
    if typescript_receipt.is_some() && managed_typescript.is_none() {
        report.issue("FLR-ANALYSIS-004", "warning", "", &root.join("Dockerfile.typescript"),
            "Custom or missing TypeScript Dockerfile: managed process commands and migration ordering were not checked");
    }
    for filename in files {
        let file = root.join(filename);
        if !file.is_file() {
            continue;
        }
        let parsed = bounded(&file).and_then(|s| {
            serde_yml::from_str::<serde_json::Value>(&s)
                .map_err(|_| "Cannot parse Compose YAML".into())
        });
        let doc = match parsed {
            Ok(value) => value,
            Err(error) => {
                report.issue("FLR-ANALYSIS-001", "error", "", &file, error);
                continue;
            }
        };
        let Some(services) = doc.get("services").and_then(serde_json::Value::as_object) else {
            report.issue(
                "FLR-ANALYSIS-001",
                "error",
                "",
                &file,
                "Compose services must be an object",
            );
            continue;
        };
        for (name, service) in services {
            if filter.is_some_and(|f| {
                f != name
                    && !managed_typescript.is_some_and(|receipt| {
                        receipt
                            .get("processes")
                            .and_then(|processes| processes.get(name))
                            .and_then(|process| process.get("directory"))
                            .and_then(serde_json::Value::as_str)
                            == Some(f)
                    })
            }) {
                continue;
            }
            if let Some(receipt) = managed_typescript {
                inspect_typescript_process(
                    report,
                    &file,
                    name,
                    service,
                    services,
                    receipt,
                    typescript_plan.as_ref(),
                );
            }
            if let Some(receipt) = generated_image {
                let expected_image = receipt
                    .get("image")
                    .and_then(serde_json::Value::as_str);
                let is_managed = service
                    .get("image")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|image| Some(image) == expected_image)
                    || service
                        .get("build")
                        .and_then(|build| build.get("dockerfile"))
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|path| {
                            matches!(path, "Dockerfile.rust" | "./Dockerfile.rust")
                        });
                if is_managed {
                    let binary = service
                        .get("command")
                        .and_then(serde_json::Value::as_array)
                        .and_then(|command| command.first())
                        .and_then(serde_json::Value::as_str)
                        .and_then(|command| command.strip_prefix("/app/bin/"))
                        .filter(|binary| !binary.contains('/'));
                    let known = binary.is_some_and(|binary| {
                        receipt["binaries"]
                            .as_array()
                            .is_some_and(|bins| bins.iter().any(|b| b.as_str() == Some(binary)))
                    });
                    if !known {
                        report.issue("FLR-LAYOUT-005","error",name,&file,"Managed shared Rust image requires an exec-form command from its declared binary inventory");
                    }
                }
            }
            let Some(build) = service.get("build") else {
                continue;
            };
            let context = build
                .as_str()
                .or_else(|| build.get("context").and_then(serde_json::Value::as_str))
                .unwrap_or(".");
            let dockerfile = build
                .get("dockerfile")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Dockerfile");
            if build.get("dockerfile_inline").is_some() {
                continue;
            }
            if context.contains('$')
                || context.contains("://")
                || dockerfile.contains('$')
                || Path::new(context).is_absolute()
                || Path::new(dockerfile).is_absolute()
            {
                report.issue(
                    "FLR-ANALYSIS-003",
                    "warning",
                    name,
                    &file,
                    "Dynamic, remote or absolute Docker build paths were not checked",
                );
                continue;
            }
            let directory = file.parent().unwrap_or(root).join(context);
            let target = directory.join(dockerfile);
            if !directory.is_dir() || !target.is_file() {
                report.issue(
                    "FLR-LAYOUT-004",
                    "error",
                    name,
                    &file,
                    "Compose build context or selected Dockerfile is missing",
                );
            }
        }
    }
}

/// Compare only declarative managed-image commands; never execute application code.
fn inspect_typescript_process(
    report: &mut Analysis,
    file: &Path,
    name: &str,
    service: &serde_json::Value,
    services: &serde_json::Map<String, serde_json::Value>,
    receipt: &serde_json::Value,
    plan: Option<&serde_json::Value>,
) {
    let selected = service
        .get("image")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|image| {
            receipt.get("image").and_then(serde_json::Value::as_str) == Some(image)
        })
        || service
            .pointer("/build/dockerfile")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|path| {
                let managed = receipt
                    .get("dockerfilePath")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Dockerfile.typescript");
                matches!(managed, "Dockerfile" | "Dockerfile.typescript")
                    && path.trim_start_matches("./") == managed
            });
    if !selected {
        return;
    }
    let process = receipt
        .get("processes")
        .and_then(|processes| processes.get(name));
    let migration_directory = name.strip_suffix("-migrate");
    let directory = process
        .and_then(|p| p.get("directory"))
        .and_then(serde_json::Value::as_str)
        .or(migration_directory);
    let entry = process
        .and_then(|p| p.get("entry"))
        .and_then(serde_json::Value::as_str)
        .or(migration_directory.map(|_| "migrate"));
    let build_process = directory.and_then(|directory| {
        plan?
            .get("processes")?
            .as_array()?
            .iter()
            .find(|p| p.get("directory").and_then(serde_json::Value::as_str) == Some(directory))
    });
    let database = build_process
        .and_then(|p| p.get("database"))
        .and_then(serde_json::Value::as_str);
    let runtime = receipt
        .get("runtime")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("node");
    if !matches!(runtime, "node" | "bun") {
        report.issue(
            "FLR-LAYOUT-006",
            "error",
            name,
            file,
            "Managed TypeScript image has an unsupported runtime",
        );
        return;
    }
    let expected = directory.zip(entry).map(|(directory, entry)| {
        serde_json::json!([
            runtime,
            "--enable-source-maps",
            format!("/app/{directory}/production/{entry}.mjs")
        ])
    });
    let known = process.is_some()
        || (migration_directory.is_some() && database.is_some_and(|db| db != "none"));
    if !known || expected.as_ref() != service.get("command") {
        report.issue("FLR-LAYOUT-006", "error", name, file,
            "Managed TypeScript image command must match its declared process or migration entrypoint");
    }
    if process.is_some() && database.is_some_and(|db| db != "none") {
        let migration = format!("{}-migrate", directory.unwrap());
        let ordered = service
            .get("depends_on")
            .and_then(|deps| deps.get(&migration))
            .and_then(|dep| dep.get("condition"))
            .and_then(serde_json::Value::as_str)
            == Some("service_completed_successfully");
        let expected_migration = serde_json::json!([
            runtime,
            "--enable-source-maps",
            format!("/app/{}/production/migrate.mjs", directory.unwrap())
        ]);
        let valid_migration = services.get(&migration).is_some_and(|migration_service| {
            migration_service.get("command") == Some(&expected_migration)
                && migration_service.get("image") == receipt.get("image")
        });
        if !ordered || !valid_migration {
            report.issue("FLR-LAYOUT-007", "error", name, file,
                "Managed TypeScript database process must depend on successful completion of its migration service");
        }
    }
    if plan.is_none() {
        report.issue("FLR-ANALYSIS-004", "warning", name, file,
            "TypeScript build plan missing or invalid: migration inventory and ordering were not checked");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegated_inventory_uses_declared_local_artifact_and_retains_legacy() {
        let root = tempfile::tempdir().unwrap();
        for declared in [false, true] {
            let mut service = project("iam", "typescript", "Service");
            let artifact = if declared {
                "src/iam/sdk/contract.json"
            } else {
                ".forklaunch/contracts/iam/contract.json"
            };
            if declared {
                service.metadata.as_mut().unwrap().contract_artifact_path = Some(artifact.into());
            }
            put(root.path(), "src/iam/package.json", "{}");
            put(
                root.path(),
                artifact,
                r#"{"delegatedSurfaces":[{"id":"better-auth"}]}"#,
            );
            let report = inspect(root.path(), "src", &[service], Some("typescript"), None);
            assert!(report.not_checked.contains(&"delegated_provider_endpoints"));
            assert_eq!(
                report.projects[0].delegated_surfaces[0]["id"],
                "better-auth"
            );
            fs::remove_file(root.path().join(artifact)).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn source_symlink_coverage_is_reported_only_when_source_was_skipped() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("src");
        std::fs::create_dir_all(&source).unwrap();
        std::os::unix::fs::symlink(
            root.path().join("absent-cache"),
            source.join("node_modules"),
        )
        .unwrap();
        let mut files = Vec::new();
        let mut skipped = false;
        assert!(source_files(
            &source,
            root.path(),
            &mut files,
            &mut 100,
            &mut skipped
        ));
        assert!(!skipped);
        std::os::unix::fs::symlink(
            root.path().join("absent-contract"),
            source.join("contract.rs"),
        )
        .unwrap();
        assert!(source_files(
            &source,
            root.path(),
            &mut files,
            &mut 100,
            &mut skipped
        ));
        assert!(skipped);
        assert!(files.is_empty());
    }
    fn project(name: &str, language: &str, kind: &str) -> ProjectEntry {
        serde_json::from_value(serde_json::json!({"name":name,"description":"","type":kind,"metadata":{"language":language}})).unwrap()
    }
    fn put(root: &Path, path: &str, content: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    #[test]
    fn rust_syntax_is_checked_without_evaluating_build_scripts_or_macros() {
        let root = tempfile::tempdir().unwrap();
        put(
            root.path(),
            "src/notes/Cargo.toml",
            "[package]\nname='notes'\nversion='0.1.0'\n[lib]\npath='lib.rs'\n",
        );
        put(
            root.path(),
            "src/notes/lib.rs",
            "compile_error!(\"must not execute\");",
        );
        put(
            root.path(),
            "src/notes/build.rs",
            "fn main() { panic!(\"must not run\"); }",
        );
        let projects = vec![project("notes", "rust", "Library")];
        assert!(!inspect(root.path(), "src", &projects, Some("rust"), None).has_errors());
        put(
            root.path(),
            "src/notes/api/routes/notes.rs",
            "pub fn broken(",
        );
        let report = inspect(root.path(), "src", &projects, Some("rust"), None);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "FLR-ANALYSIS-001"
                && diagnostic.path.ends_with("api/routes/notes.rs")
        }));
        assert!(
            report
                .not_checked
                .contains(&"rust_macro_expansion_and_typechecking")
        );
    }
    #[test]
    fn mixed_projects_include_builtins_missing_dirs_and_relocated_rust_sources() {
        let root = tempfile::tempdir().unwrap();
        let mut rust = project("notes", "rust", "Service");
        rust.metadata.as_mut().unwrap().crate_path = Some("src/team/notes".into());
        put(
            root.path(),
            "src/team/notes/Cargo.toml",
            "[package]\nname='notes'\nversion='0.1.0'\n[[bin]]\nname='notes'\npath='server.rs'\n",
        );
        put(root.path(), "src/team/notes/server.rs", "fn main() {}");
        put(root.path(), "src/auth/package.json", "{}");
        let projects = vec![
            rust,
            project("auth", "typescript", "Service"),
            project("core", "rust", "Library"),
        ];
        let report = inspect(root.path(), "src", &projects, Some("mixed"), None);
        assert_eq!(report.projects.len(), 3);
        assert_eq!(
            report.projects[2].rust_files,
            vec!["src/team/notes/server.rs"]
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.project == "core" && d.code == "FLR-LAYOUT-001")
        );
        let filtered = inspect(root.path(), "src", &projects, Some("mixed"), Some("notes"));
        assert!(!filtered.has_errors());
        assert_eq!(filtered.scope, "module");
        assert_eq!(filtered.status, "partial");
        assert!(!filtered.not_checked.is_empty());
    }
    #[test]
    fn invalid_cargo_missing_entrypoint_language_conflicts_and_unknown_filters_fail() {
        let root = tempfile::tempdir().unwrap();
        let projects = vec![
            project("broken", "rust", "Worker"),
            project("empty", "rust", "Service"),
        ];
        put(root.path(), "src/broken/Cargo.toml", "[package");
        put(
            root.path(),
            "src/empty/Cargo.toml",
            "[package]\nname='empty'\nversion='0.1.0'\n[[bin]]\nname='empty'\npath='server.rs'\n",
        );
        let report = inspect(root.path(), "src", &projects, Some("typescript"), None);
        for code in ["FLR-ANALYSIS-001", "FLR-LAYOUT-002", "FLR-LANGUAGE-002"] {
            assert!(report.diagnostics.iter().any(|d| d.code == code), "{code}");
        }
        assert!(inspect(root.path(), "src", &projects, Some("mixed"), Some("absent")).has_errors());
    }
    #[test]
    fn compose_reports_missing_selected_dockerfile_without_interpreting_shell_commands() {
        let root = tempfile::tempdir().unwrap();
        put(
            root.path(),
            "compose.yaml",
            "services:\n  notes:\n    build:\n      context: .\n      dockerfile: Dockerfile.rust\n    command: 'custom arbitrary shell code'\n  remote:\n    build: https://example.test/repo.git\n",
        );
        let report = inspect(root.path(), "src", &[], Some("mixed"), None);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "FLR-LAYOUT-004" && d.project == "notes")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "FLR-ANALYSIS-003" && d.project == "remote")
        );
        assert!(
            report
                .not_checked
                .contains(&"docker_execution_and_shell_commands")
        );
        put(root.path(), "Dockerfile.rust", "FROM scratch\n");
        assert!(!inspect(root.path(), "src", &[], Some("mixed"), None).has_errors());
    }

    #[test]
    fn generated_shared_image_commands_are_checked_but_custom_images_are_not_adopted() {
        let root = tempfile::tempdir().unwrap();
        put(root.path(), ".forklaunch/manifest.toml", "app_name = \"demo\"\nruntime = \"node\"\n");
        put(root.path(), "Dockerfile.rust", &crate::core::forklaunch_state::stamp("FROM scratch\nCOPY --from=builder /app/target/release/notes /app/bin/notes\n"));
        put(
            root.path(),
            "compose.yaml",
            "services:\n  notes:\n    image: demo-rust:local\n    command: ['/app/missing']\n",
        );
        assert!(
            inspect(root.path(), "src", &[], Some("mixed"), None)
                .diagnostics
                .iter()
                .any(|d| d.code == "FLR-LAYOUT-005")
        );
        put(
            root.path(),
            "compose.yaml",
            "services:\n  notes:\n    image: demo-rust:local\n    command: ['/app/bin/notes', 'migrate']\n",
        );
        assert!(!inspect(root.path(), "src", &[], Some("mixed"), None).has_errors());
        put(root.path(), "Dockerfile.rust", "FROM custom\nCOPY --from=builder /app/target/release/notes /app/bin/notes\n");
        put(
            root.path(),
            "compose.yaml",
            "services:\n  notes:\n    image: demo-rust:local\n    command: custom-entrypoint\n",
        );
        assert!(!inspect(root.path(), "src", &[], Some("mixed"), None).has_errors());
    }

    #[test]
    fn typescript_managed_processes_require_commands_and_successful_migrations() {
        let root = tempfile::tempdir().unwrap();
        put(root.path(), ".forklaunch/manifest.toml", "app_name = \"demo\"\nruntime = \"node\"\n");
        let plan = serde_json::json!({
            "processes":[{"directory":"notes","entries":["server"],"database":"postgresql"}],
            "services":{"notes":{"directory":"notes","entry":"server"}}
        });
        put(
            root.path(),
            "Dockerfile.typescript",
            &crate::core::forklaunch_state::stamp(&format!(
                "FROM node:22\nCOPY <<\"FORKLAUNCH_EOF\" /forklaunch/production.json\n{plan}\nFORKLAUNCH_EOF\n"
            )),
        );
        let mut compose = serde_json::json!({"services":{
            "notes":{"image":"demo-typescript:local", "command":["node","--enable-source-maps","/app/notes/production/server.mjs"],
                "depends_on":{"notes-migrate":{"condition":"service_completed_successfully"}}},
            "notes-migrate":{"image":"demo-typescript:local", "command":["node","--enable-source-maps","/app/notes/production/migrate.mjs"]}
        }});
        put(root.path(), "compose.yaml", &compose.to_string());
        assert!(!inspect(root.path(), "src", &[], Some("mixed"), None).has_errors());
        compose["services"]["notes"]["command"] = serde_json::json!(["node", "missing.js"]);
        compose["services"]["notes"]["depends_on"]["notes-migrate"]["condition"] =
            "service_started".into();
        put(root.path(), "compose.yaml", &compose.to_string());
        let report = inspect(root.path(), "src", &[], Some("mixed"), None);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "FLR-LAYOUT-006")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "FLR-LAYOUT-007")
        );
        // An edited generated Dockerfile keeps its stamp but no longer matches it.
        let edited = std::fs::read_to_string(root.path().join("Dockerfile.typescript")).unwrap().replace("FROM node:22", "FROM custom");
        put(root.path(), "Dockerfile.typescript", &edited);
        let report = inspect(root.path(), "src", &[], Some("mixed"), None);
        assert!(!report.has_errors());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|d| d.code == "FLR-ANALYSIS-004")
        );
    }

    #[test]
    fn library_only_and_virtual_workspace_are_allowed_but_escape_is_not() {
        let root = tempfile::tempdir().unwrap();
        let mut project = project("core", "rust", "Library");
        put(
            root.path(),
            "src/core/Cargo.toml",
            "[workspace]\nmembers=[]\n",
        );
        assert!(!inspect(root.path(), "src", &[project.clone()], Some("rust"), None).has_errors());
        project.metadata.as_mut().unwrap().crate_path = Some("../outside".into());
        assert!(inspect(root.path(), "src", &[project], Some("rust"), None).has_errors());
    }
}

//! `forklaunch analyze` — read-only workspace inspection that emits a structured JSON
//! description of the application (modules, entities, schemas, routers, services, workers).
//! A versioned analysis envelope reports language-aware structural diagnostics and
//! unchecked capabilities. Confirmed errors return nonzero after emitting JSON;
//! ordinary inspection never runs Cargo, build scripts, TypeScript or application code.
//! Explicit `--verify` also invokes compilers and checks generated SDKs.
//!
//! Designed to be consumed by tooling that needs a canonical, parser-stable view of the
//! workspace — primarily the studio orchestrator's planner. By reusing the existing
//! `core/static_analysis` modules (the same ones `change/router.rs` and `change/service.rs`
//! call), we guarantee the orchestrator and the CLI agree on what the workspace looks
//! like, eliminating an entire class of parser-drift bugs.
//!
//! Output goes to stdout; progress + warnings go to stderr. Stdout is parseable JSON.

use std::{
    fs::read_dir,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::Serialize;

use crate::{
    CliCommand,
    core::{
        command::command,
        manifest::ProjectType,
        static_analysis::{
            EntityAnalyzer, SchemaAnalyzer,
            entity_analyzer::{EntityProperty, RelationType},
            workspace_diagnostics,
        },
        validate::require_manifest,
    },
};

mod verification;

pub(crate) struct AnalyzeCommand;

impl AnalyzeCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for AnalyzeCommand {
    fn command(&self) -> Command {
        command(
            "analyze",
            "Inspect workspace inventory and diagnostics as JSON. Read-only by default; --verify explicitly runs compilers.",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("Application root path (defaults to the current directory's manifest)"),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .help("Output format")
                .value_parser(["json"])
                .default_value("json"),
        )
        .arg(
            Arg::new("module")
                .short('m')
                .long("module")
                .help("Filter to a single module by name"),
        )
        .arg(
            Arg::new("verify")
                .long("verify")
                .help("Run compilers (including workspace build scripts/proc macros) and SDK checks; may update build caches and generated artifacts")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("strict")
                .long("strict")
                .help("Also fail when required static/compiler verification coverage is incomplete")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("pretty")
                .long("pretty")
                .help("Pretty-print JSON output")
                .action(clap::ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let (app_root, manifest) = require_manifest(matches)?;
        let (manifest, normalization_error) =
            match crate::core::manifest::language::normalized(&app_root, &manifest) {
                Ok(normalized) => (normalized, None),
                Err(error) => (manifest, Some(format!("{error:#}"))),
            };
        let module_filter = matches.get_one::<String>("module").cloned();
        let pretty = matches.get_flag("pretty");

        let modules = collect_modules(
            &app_root,
            &manifest.modules_path,
            &manifest.projects,
            module_filter.as_deref(),
        )?;

        let mut analysis = workspace_diagnostics::inspect(
            &app_root,
            &manifest.modules_path,
            &manifest.projects,
            manifest
                .application_language
                .as_deref()
                .or(manifest.language.as_deref()),
            module_filter.as_deref(),
        );
        if let Some(message) = normalization_error {
            analysis.diagnostics.push(workspace_diagnostics::Diagnostic {
                code: "FLR-LANGUAGE-003",
                severity: "error",
                project: String::new(),
                path: ".forklaunch/manifest.toml".into(),
                message,
            });
            analysis.status = "errors";
        }
        let verification = verification::inspect(
            &app_root,
            &manifest.modules_path,
            &manifest.projects,
            module_filter.as_deref(),
            matches.get_flag("verify"),
            matches.get_flag("strict"),
            &mut analysis,
        );
        let doc = AnalyzeDocument {
            analysis,
            verification,
            app_name: manifest.app_name.clone(),
            modules_path: manifest.modules_path.clone(),
            modules,
        };

        emit_document(&doc, pretty, &mut std::io::stdout().lock())
    }
}

fn emit_document(
    doc: &AnalyzeDocument,
    pretty: bool,
    writer: &mut impl std::io::Write,
) -> Result<()> {
    let serialized = if pretty {
        serde_json::to_string_pretty(doc)?
    } else {
        serde_json::to_string(doc)?
    };
    writeln!(writer, "{serialized}")?;
    if doc.analysis.has_errors() {
        bail!("Workspace analysis found confirmed structural errors; see JSON diagnostics");
    }
    Ok(())
}

// ─── Output DTOs ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct AnalyzeDocument {
    analysis: workspace_diagnostics::Analysis,
    #[serde(skip_serializing_if = "Option::is_none")]
    verification: Option<verification::Verification>,
    #[serde(rename = "appName")]
    app_name: String,
    #[serde(rename = "modulesPath")]
    modules_path: String,
    modules: Vec<ModuleEntry>,
}

#[derive(Serialize)]
struct ModuleEntry {
    name: String,
    kind: String,
    description: Option<String>,
    entities: Vec<EntityEntry>,
    schemas: Vec<SchemaEntry>,
    routers: Vec<String>,
    services: Vec<String>,
    workers: Vec<String>,
}

#[derive(Serialize)]
struct EntityEntry {
    name: String,
    fields: Vec<FieldEntry>,
    relations: Vec<RelationEntry>,
}

#[derive(Serialize)]
struct FieldEntry {
    name: String,
    #[serde(rename = "typeName")]
    type_name: String,
    nullable: bool,
    collection: bool,
}

#[derive(Serialize)]
struct RelationEntry {
    field: String,
    #[serde(rename = "toEntity")]
    to_entity: String,
    cardinality: String,
}

#[derive(Serialize)]
struct SchemaEntry {
    #[serde(rename = "exportName")]
    export_name: String,
    fields: Vec<String>,
}

// ─── Module collection ─────────────────────────────────────────────────────────

fn collect_modules(
    app_root: &Path,
    modules_path: &str,
    projects: &[crate::core::manifest::ProjectEntry],
    filter: Option<&str>,
) -> Result<Vec<ModuleEntry>> {
    let mut entries = Vec::new();

    for project in projects {
        if let Some(name) = filter {
            if project.name != name {
                continue;
            }
        }
        // Skip built-in modules that the orchestrator never plans against.
        if is_builtin_module(&project.name) {
            continue;
        }
        let Some(module_dir) = workspace_diagnostics::project_dir(app_root, modules_path, project)
        else {
            continue;
        };
        if !module_dir.is_dir() {
            // Project named in the manifest but the directory isn't on disk yet — skip.
            continue;
        }

        let kind = match project.r#type {
            ProjectType::Service => "service",
            ProjectType::Worker => "worker",
            ProjectType::Library => "library",
        };

        let entities = collect_entities(&module_dir).unwrap_or_else(|err| {
            eprintln!(
                "warning: failed to analyze entities for module '{}': {}",
                project.name, err
            );
            Vec::new()
        });
        let schemas = collect_schemas(&module_dir).unwrap_or_else(|err| {
            eprintln!(
                "warning: failed to analyze schemas for module '{}': {}",
                project.name, err
            );
            Vec::new()
        });
        let routers = project
            .routers
            .clone()
            .unwrap_or_else(|| list_basenames(&module_dir.join("api/routes"), ".routes.ts"));
        let mut services = list_basenames(&module_dir.join("domain/services"), ".service.ts");
        let mut routers = routers;
        let mut workers = list_workers(&module_dir);
        if module_dir.join("Cargo.toml").is_file() {
            if project.routers.is_none() {
                routers.extend(
                    list_basenames(&module_dir.join("api/routes"), ".rs")
                        .into_iter()
                        .filter(|name| name != "mod"),
                );
            }
            services.extend(
                list_basenames(&module_dir.join("domain/services"), ".rs")
                    .into_iter()
                    .filter(|name| name != "mod"),
            );
            if matches!(project.r#type, ProjectType::Worker) {
                workers.push(project.name.clone());
            }
        }

        entries.push(ModuleEntry {
            name: project.name.clone(),
            kind: kind.to_string(),
            description: Some(project.description.clone()).filter(|s| !s.is_empty()),
            entities,
            schemas,
            routers,
            services,
            workers,
        });
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

fn is_builtin_module(name: &str) -> bool {
    matches!(
        name,
        "core" | "monitoring" | "client-sdk" | "iam" | "billing" | "messaging"
    )
}

fn collect_entities(module_dir: &Path) -> Result<Vec<EntityEntry>> {
    let entities_dir = module_dir.join("persistence/entities");
    let mut out = Vec::new();
    if !entities_dir.is_dir() {
        return Ok(out);
    }
    for entry in
        read_dir(&entities_dir).with_context(|| format!("read_dir {}", entities_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.ends_with(".entity.ts"))
            .unwrap_or(false)
        {
            continue;
        }
        let defs = EntityAnalyzer::parse_entity_file(&path).unwrap_or_else(|err| {
            eprintln!("warning: failed to parse {}: {}", path.display(), err);
            Vec::new()
        });
        for def in defs {
            let mut fields = Vec::new();
            let mut relations = Vec::new();
            for property in &def.properties {
                match &property.relation_type {
                    Some(rel) => relations.push(RelationEntry {
                        field: property.name.clone(),
                        to_entity: property.type_name.clone(),
                        cardinality: cardinality_name(rel),
                    }),
                    None => fields.push(field_entry(property)),
                }
            }
            out.push(EntityEntry {
                name: def.name,
                fields,
                relations,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn field_entry(property: &EntityProperty) -> FieldEntry {
    FieldEntry {
        name: property.name.clone(),
        type_name: property.type_name.clone(),
        nullable: property.is_nullable,
        collection: property.is_collection,
    }
}

fn cardinality_name(rel: &RelationType) -> String {
    match rel {
        RelationType::ManyToOne => "N:1".to_string(),
        RelationType::OneToMany => "1:N".to_string(),
        RelationType::ManyToMany => "M:N".to_string(),
        RelationType::OneToOne => "1:1".to_string(),
    }
}

fn collect_schemas(module_dir: &Path) -> Result<Vec<SchemaEntry>> {
    let schemas_dir = module_dir.join("domain/schemas");
    let mut out = Vec::new();
    if !schemas_dir.is_dir() {
        return Ok(out);
    }
    if module_dir.join("Cargo.toml").is_file() {
        // Syntax inventory only: macros, cfg and re-exports require the compiler.
        for entry in walkdir::WalkDir::new(&schemas_dir)
            .follow_links(false)
            .max_depth(16)
            .into_iter()
            .take(10_001)
            .enumerate()
        {
            let (index, entry) = entry;
            if index == 10_000 {
                bail!("Rust schema inventory exceeds the 10,000 entry analysis limit");
            }
            let entry = entry?;
            if !entry.file_type().is_file()
                || entry
                    .path()
                    .extension()
                    .is_none_or(|extension| extension != "rs")
            {
                continue;
            }
            if entry.metadata()?.len() > 2_097_152 {
                bail!("Rust schema source exceeds the 2 MiB analysis limit");
            }
            let source = std::fs::read_to_string(entry.path())?;
            let parsed = syn::parse_file(&source)?;
            collect_rust_schema_items(&parsed.items, &mut out);
        }
        out.sort_by(|a, b| a.export_name.cmp(&b.export_name));
        return Ok(out);
    }
    for entry in
        read_dir(&schemas_dir).with_context(|| format!("read_dir {}", schemas_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.ends_with(".schema.ts"))
            .unwrap_or(false)
        {
            continue;
        }
        let defs = SchemaAnalyzer::parse_schema_file(&path).unwrap_or_else(|err| {
            eprintln!("warning: failed to parse {}: {}", path.display(), err);
            Vec::new()
        });
        for def in defs {
            out.push(SchemaEntry {
                export_name: def.name,
                fields: def.properties.iter().map(|p| p.name.clone()).collect(),
            });
        }
    }
    out.sort_by(|a, b| a.export_name.cmp(&b.export_name));
    Ok(out)
}

fn collect_rust_schema_items(items: &[syn::Item], out: &mut Vec<SchemaEntry>) {
    for item in items {
        match item {
            syn::Item::Struct(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                out.push(SchemaEntry {
                    export_name: item.ident.to_string(),
                    fields: item
                        .fields
                        .iter()
                        .filter_map(|field| field.ident.as_ref().map(ToString::to_string))
                        .collect(),
                });
            }
            syn::Item::Enum(item) if matches!(item.vis, syn::Visibility::Public(_)) => {
                out.push(SchemaEntry {
                    export_name: item.ident.to_string(),
                    fields: Vec::new(),
                });
            }
            syn::Item::Mod(item) => {
                if let Some((_, items)) = &item.content {
                    collect_rust_schema_items(items, out);
                }
            }
            _ => {}
        }
    }
}

fn list_basenames(dir: &Path, suffix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(read) = read_dir(dir) else {
        return out;
    };
    for entry in read.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !name.ends_with(suffix) {
            continue;
        }
        out.push(name.trim_end_matches(suffix).to_string());
    }
    out.sort();
    out
}

fn list_workers(module_dir: &Path) -> Vec<String> {
    // Workers are detected by either a `worker.ts` file at the module root or any
    // `*-worker.ts` file. Best-effort — matches the heuristic the studio uses today.
    let mut out = Vec::new();
    let Ok(read) = read_dir(module_dir) else {
        return out;
    };
    for entry in read.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if name == "worker.ts" {
            out.push("worker".to_string());
            continue;
        }
        if name.ends_with("-worker.ts") {
            out.push(name.trim_end_matches(".ts").to_string());
        }
    }
    out.sort();
    out
}

// Convenience for tests / callers that want a typed handle on the document instead of JSON.
#[allow(dead_code)]
pub(crate) fn analyze_workspace(
    app_root: &Path,
    modules_path: &str,
    projects: &[crate::core::manifest::ProjectEntry],
    module_filter: Option<&str>,
) -> Result<serde_json::Value> {
    let modules = collect_modules(app_root, modules_path, projects, module_filter)?;
    let doc = AnalyzeDocument {
        verification: None,
        analysis: workspace_diagnostics::inspect(
            app_root,
            modules_path,
            projects,
            None,
            module_filter,
        ),
        app_name: String::new(),
        modules_path: modules_path.to_string(),
        modules,
    };
    Ok(serde_json::to_value(&doc)?)
}

// Path utility so callers don't have to recompute.
#[allow(dead_code)]
pub(crate) fn module_dir(app_root: &Path, modules_path: &str, module: &str) -> PathBuf {
    app_root.join(modules_path).join(module)
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn rust_schema_inventory_handles_framework_attributes_and_inline_modules() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='notes'\nversion='0.1.0'\n",
        )
        .unwrap();
        let schemas = root.path().join("domain/schemas");
        std::fs::create_dir_all(&schemas).unwrap();
        std::fs::write(
            schemas.join("notes.rs"),
            r#"
            #[forklaunch_core::schema(rename_all = "camelCase")]
            pub struct Note { pub title: String, pub revision: i32 }
            struct Internal { secret: String }
            mod nested { pub enum State { Ready, Pending } }
        "#,
        )
        .unwrap();
        let entries = collect_schemas(root.path()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].export_name, "Note");
        assert_eq!(entries[0].fields, ["title", "revision"]);
        assert_eq!(entries[1].export_name, "State");
    }
    #[test]
    fn json_inventory_is_emitted_before_confirmed_errors_and_builtins_are_diagnosed() {
        let root = tempfile::tempdir().unwrap();
        let project:crate::core::manifest::ProjectEntry=serde_json::from_value(serde_json::json!({"name":"core","type":"Library","description":"","metadata":{"language":"rust"}})).unwrap();
        let projects = vec![project];
        let document = AnalyzeDocument {
            verification: None,
            app_name: "sample".into(),
            modules_path: "src".into(),
            modules: collect_modules(root.path(), "src", &projects, None).unwrap(),
            analysis: workspace_diagnostics::inspect(
                root.path(),
                "src",
                &projects,
                Some("rust"),
                None,
            ),
        };
        let mut output = Vec::new();
        assert!(emit_document(&document, false, &mut output).is_err());
        let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(json["appName"], "sample");
        assert_eq!(json["modulesPath"], "src");
        assert_eq!(json["modules"], serde_json::json!([]));
        assert_eq!(json["analysis"]["schemaVersion"], 1);
        assert_eq!(json["analysis"]["status"], "errors");
        assert_eq!(json["analysis"]["projects"][0]["project"], "core");
    }
}

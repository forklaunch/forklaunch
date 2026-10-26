//! Change a native project's generated shape in place: a producer worker's
//! queue backend, or a service into a producer worker and back.
//!
//! The current and target shapes are rendered exactly as `init` would write
//! them. Files that differ are replaced only while they are still as
//! generated; application files outside the generated set are never touched,
//! application Cargo dependencies are kept, and anything customized stops the
//! change before the first write.
use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// A file as `init` leaves it on disk, normalized for comparison.
fn generated_on_disk(template: &RenderedTemplate) -> Result<[String; 2]> {
    let content = dependencies::render_for_path(&template.path, &template.content)?;
    let content = native_layout::format_source(&template.path, &content);
    let marked = crate::core::watermark::apply_watermark(&RenderedTemplate {
        content: content.clone(),
        ..template.clone()
    })?;
    Ok([
        native_layout::conversion_source(&template.path, &content),
        native_layout::conversion_source(&template.path, &marked),
    ])
}

/// Cargo dependency entries the application added to a generated manifest.
fn application_dependencies(actual: &Value, generated: &Value) -> Map<String, Value> {
    let mut extra = Map::new();
    for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
        let known = generated.get(section).and_then(Value::as_table);
        for (key, value) in actual.get(section).and_then(Value::as_table).into_iter().flatten() {
            if !known.is_some_and(|known| known.contains_key(key)) {
                extra
                    .entry(section.to_owned())
                    .or_insert_with(|| Value::Table(Map::new()))
                    .as_table_mut()
                    .expect("dependency table")
                    .insert(key.clone(), value.clone());
            }
        }
    }
    extra
}

fn without_package_notes(mut value: Value) -> Value {
    if let Some(package) = value.get_mut("package").and_then(Value::as_table_mut) {
        package.remove("metadata");
        package.remove("description");
    }
    value
}

/// The target Cargo manifest with the application's own dependencies and
/// package notes carried over, after checking the generated part is unchanged.
fn next_cargo(path: &Path, before: &RenderedTemplate, after: &RenderedTemplate) -> Result<String> {
    let generated: Value = toml::from_str(&dependencies::render_for_path(path, &before.content)?)?;
    let actual: Value = toml::from_str(&read_to_string(path)?)?;
    let extra = application_dependencies(&actual, &generated);
    let mut actual_generated = actual.clone();
    for (section, entries) in &extra {
        if let Some(table) = actual_generated.get_mut(section.as_str()).and_then(Value::as_table_mut) {
            for key in entries.as_table().expect("dependency table").keys() {
                table.remove(key);
            }
        }
    }
    anyhow::ensure!(
        without_package_notes(actual_generated) == without_package_notes(generated),
        "This change would overwrite customized {}; migrate it explicitly first",
        path.display()
    );
    // The target as `init` writes it, plus the application's entries.
    let mut next: toml_edit::DocumentMut = dependencies::render_for_path(path, &after.content)?.parse()?;
    for (section, entries) in extra {
        if !next.contains_key(&section) {
            next[&section] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        let table = next[&section].as_table_like_mut().context("Cargo dependency table")?;
        for (key, value) in entries.as_table().expect("dependency table") {
            if table.get(key).is_none() {
                let item: toml_edit::DocumentMut = format!("value = {value}").parse()?;
                table.insert(key, item["value"].clone());
            }
        }
    }
    if let Some(description) = actual.get("package").and_then(|package| package.get("description")).and_then(Value::as_str) {
        next["package"]["description"] = toml_edit::value(description);
    }
    if let Some(metadata) = actual.get("package").and_then(|package| package.get("metadata")) {
        let item: toml_edit::DocumentMut = format!("[package.metadata]\n{}", toml::to_string(metadata)?).parse()?;
        next["package"]["metadata"] = item["package"]["metadata"].clone();
    }
    Ok(next.to_string())
}

/// Replace the generated `current` shape of `dir` with `target`; returns the
/// planned paths. Nothing is written on a dry run or when a check fails.
pub(super) fn replace_generated(
    root: &Path,
    dir: &Path,
    current: Vec<RenderedTemplate>,
    target: Vec<RenderedTemplate>,
    dryrun: bool,
) -> Result<Vec<String>> {
    let current: BTreeMap<PathBuf, RenderedTemplate> = current.into_iter().map(|t| (t.path.clone(), t)).collect();
    let target: BTreeMap<PathBuf, RenderedTemplate> = target.into_iter().map(|t| (t.path.clone(), t)).collect();
    let mut writes = Vec::new();
    let mut removals = Vec::new();
    let mut cargo = Vec::new();
    for path in current.keys().chain(target.keys()).collect::<BTreeSet<_>>() {
        let (before, after) = (current.get(path), target.get(path));
        if before.map(|t| &t.content) == after.map(|t| &t.content) {
            continue;
        }
        let is_cargo = path.file_name().is_some_and(|file| file == "Cargo.toml");
        if is_cargo && path.exists() && let (Some(before), Some(after)) = (before, after) {
            cargo.push(RenderedTemplate { content: next_cargo(path, before, after)?, ..after.clone() });
            continue;
        }
        if path.exists() {
            let expected = before.map(generated_on_disk).transpose()?;
            let actual = native_layout::conversion_source(path, &read_to_string(path)?);
            let unchanged = expected.as_ref().is_some_and(|expected| expected.contains(&actual));
            // Documentation the application edited stays as written.
            if !unchanged && path.file_name().is_some_and(|file| file == "README.md" || file == ".env.example") {
                continue;
            }
            anyhow::ensure!(
                expected.is_some_and(|expected| expected.contains(&actual)),
                "This change would overwrite customized {}; migrate it explicitly first",
                path.display()
            );
        }
        match after {
            Some(template) => writes.push(template.clone()),
            None if path.exists() => removals.push(path.clone()),
            None => {}
        }
    }
    let relative = |path: &Path| path.strip_prefix(root).unwrap_or(path).display().to_string();
    let mut planned: Vec<String> = writes.iter().map(|t| relative(&t.path)).collect();
    planned.extend(cargo.iter().map(|template| relative(&template.path)));
    planned.extend(removals.iter().map(|path| format!("{} (removed)", relative(path))));
    if dryrun {
        return Ok(planned);
    }
    for template in &mut writes {
        template.content = dependencies::render_for_path(&template.path, &template.content)?;
        template.content = native_layout::format_source(&template.path, &template.content);
    }
    write_rendered_templates(&writes, false, &mut StandardStream::stdout(ColorChoice::Always))?;
    write_rendered_templates(&cargo, false, &mut StandardStream::stdout(ColorChoice::Always))?;
    for path in removals {
        remove_file(&path)?;
        remove_empty_parents(&path, dir);
    }
    Ok(planned)
}

fn remove_empty_parents(path: &Path, stop: &Path) {
    let mut parent = path.parent();
    while let Some(directory) = parent {
        if directory == stop || std::fs::remove_dir(directory).is_err() {
            break;
        }
        parent = directory.parent();
    }
}

/// A native service's generated files, as `init service` writes them.
pub(super) fn service_templates(root: &Path, dir: &Path, name: &str, description: &str, postgres: bool) -> Result<Vec<RenderedTemplate>> {
    let files = native_layout::service_templates(dir, name, description);
    let files = if postgres { service_database::templates(dir, name, files)? } else { files };
    let files = native_mappers::templates(dir, dir.join("domain/mappers/health.rs").is_file(), files)?;
    Ok(native_layout::framework_paths(root, files))
}

/// Settings a project's processes read, from Compose.
fn process_settings(root: &Path, name: &str) -> Result<BTreeSet<String>> {
    let compose = read_or_create_rust_docker_compose(&root.join("docker-compose.yaml"))?;
    Ok(compose
        .services
        .iter()
        .filter(|(key, service)| *key == name || worker::producer_owner(service) == Some(name))
        .flat_map(|(_, service)| service.environment.iter().flatten().map(|(key, _)| key.clone()))
        .chain(["MIGRATION_DATABASE_URL".to_owned(), "SQLX_OFFLINE".to_owned(), "PUBLIC_ORIGIN".to_owned()])
        .collect())
}

/// Build outputs of a worker's typed event package.
const WORKER_EVENT_OUTPUTS: &[&str] = &[
    "events/event-contract.json",
    "events/rust/events.rs",
    "events/typescript/encrypted-channel.d.mts",
    "events/typescript/encrypted-channel.mjs",
    "events/typescript/encrypted-worker.d.mts",
    "events/typescript/encrypted-worker.mjs",
    "events/typescript/events.ts",
    "events/typescript/package.json",
    "events/typescript/tsconfig.json",
];

/// Turn a native service into a producer worker on `backend`, or (with
/// `backend` None) a producer worker into a service.
pub(super) fn change_kind(root: &Path, name: &str, backend: Option<worker::Backend>, dryrun: bool) -> Result<Vec<String>> {
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let manifest = read_manifest_value(&manifest_path)?;
    let project = project_table(&manifest, name).context("Project missing from the manifest")?;
    let dir = root.join(crate_path_for_project(&manifest, name).context("Project crate path missing")?);
    let description = cargo_description(&dir.join("Cargo.toml"))?.unwrap_or_default();
    let routers: Vec<String> = project
        .get("routers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    anyhow::ensure!(
        routers.is_empty(),
        "{name} serves routers ({}); a converted project keeps only its generated routes. Delete or move the routers first (forklaunch delete router)",
        routers.join(", ")
    );
    let (current, target) = match backend {
        Some(backend) => {
            let postgres = project.get("metadata").and_then(|m| m.get("persistence")).and_then(Value::as_str) == Some("postgresql");
            (service_templates(root, &dir, name, &description, postgres)?, worker::producer_templates(root, &dir, name, &description, backend)?)
        }
        None => {
            let backend = worker::backend(root, name)?.context("Worker backend could not be determined")?;
            (worker::producer_templates(root, &dir, name, &description, backend)?, service_templates(root, &dir, name, &description, false)?)
        }
    };
    let mut planned = replace_generated(root, &dir, current, target, dryrun)?;
    planned.extend([".forklaunch/manifest.toml".into(), "docker-compose.yaml".into(), ".env".into(), "package.json (scripts)".into()]);
    if dryrun {
        return Ok(planned);
    }
    let previous_settings = process_settings(root, name)?;
    remove_rust_docker_compose_project(root, name)?;
    remove_rust_package_scripts(root, name)?;
    let mut manifest = read_manifest_value(&manifest_path)?;
    {
        let project = project_table_mut(&mut manifest, name).context("Project missing from the manifest")?;
        let metadata = project.entry("metadata").or_insert_with(|| Value::Table(Map::new())).as_table_mut().context("Project metadata")?;
        match backend {
            Some(_) => {
                metadata.remove("persistence");
                metadata.insert("workerService".into(), Value::Boolean(true));
                metadata.insert("contractExport".into(), Value::Boolean(true));
                metadata.insert("workerRuntime".into(), Value::String("leased".into()));
            }
            None => {
                for key in ["workerService", "workerRuntime", "workerType", "workerControlService", "contractExport"] {
                    metadata.remove(key);
                }
                metadata.insert("persistence".into(), Value::String("memory".into()));
            }
        }
        project.insert("type".into(), Value::String(if backend.is_some() { "Worker" } else { "Service" }.into()));
        if backend.is_none() {
            project.remove("variant");
            if let Some(resources) = project.get_mut("resources").and_then(Value::as_table_mut) {
                for key in ["database", "cache", "queue"] {
                    resources.remove(key);
                }
            }
        }
    }
    write_manifest_value(&manifest_path, &manifest)?;
    if backend.is_none() {
        for output in WORKER_EVENT_OUTPUTS {
            let path = dir.join(output);
            if path.is_file() {
                remove_file(&path)?;
                remove_empty_parents(&path, &dir);
            }
        }
    }
    let kind = if backend.is_some() { RustProjectKind::Worker } else { RustProjectKind::Service };
    ensure_rust_docker_compose_project(root, kind, name, false)?;
    ensure_rust_package_scripts(root, kind, name, false)?;
    // Listener settings follow the new process layout.
    let local = dir.join(".env.local");
    if let Ok(existing) = read_to_string(&local) {
        let kept: String = existing
            .split_inclusive('\n')
            .filter(|line| !matches!(line.split('=').next().map(str::trim), Some("PORT" | "PUBLIC_ORIGIN")))
            .collect();
        write(&local, kept)?;
    }
    if let Some(backend) = backend {
        worker::mark(root, name, backend)?;
        worker::compose(root, name, backend)?;
    } else {
        language_images::reconcile(root)?;
    }
    ensure_rust_local_env(root, &dir, name)?;
    let previous: Vec<&str> = previous_settings.iter().map(String::as_str).collect();
    local_runtime::prune_project_env(root, name, &previous)?;
    Ok(planned)
}

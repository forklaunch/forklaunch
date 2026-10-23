//! Discover native projects in the same module tree as TypeScript projects.
use super::*;

pub(super) fn roots(manifest: &Value) -> Vec<PathBuf> {
    let modules = PathBuf::from(
        manifest
            .get("modules_path")
            .and_then(Value::as_str)
            .unwrap_or("src"),
    );
    let mut roots = vec![
        modules.clone(),
        modules.join("core"),
        PathBuf::from("crates"),
    ];
    roots.sort();
    roots.dedup();
    roots
}

pub(super) fn path(root: &Path, manifest: &Value, name: &str) -> Result<Option<PathBuf>> {
    // Registered paths remain authoritative, including missing projects that sync removes.
    if let Some(path) = crate_path_for_project(manifest, name) {
        return Ok(Some(PathBuf::from(path)));
    }
    let candidates: Vec<_> = roots(manifest)
        .into_iter()
        .map(|directory| directory.join(name))
        .filter(|path| root.join(path).join("Cargo.toml").is_file())
        .collect();
    anyhow::ensure!(
        candidates.len() <= 1,
        "Multiple Rust projects named {name} were found; register the intended metadata.cratePath before syncing"
    );
    Ok(candidates.into_iter().next())
}

/// Persist the actual location and source-supported roles for a newly discovered project.
/// Existing registrations retain all application-owned metadata and resources.
pub(super) fn register(
    root: &Path,
    kind: RustProjectKind,
    name: &str,
    path: &Path,
    description: &str,
) -> Result<()> {
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let manifest = read_manifest_value(&manifest_path)?;
    if project_table(&manifest, name).is_some() {
        return Ok(());
    }
    let directory = root.join(path);
    let cargo: Value = toml::from_str(&read_to_string(directory.join("Cargo.toml"))?)?;
    let dependencies = cargo.get("dependencies");
    add_project_to_manifest(root, kind, name, description, &ArgMatches::default(), false)?;
    let mut manifest = read_manifest_value(&manifest_path)?;
    let project = project_table_mut(&mut manifest, name).context("Missing discovered project")?;
    let metadata = project
        .get_mut("metadata")
        .and_then(Value::as_table_mut)
        .context("Missing metadata")?;
    metadata.insert(
        "cratePath".into(),
        path.to_string_lossy().into_owned().into(),
    );
    let service = contracts_layout::is_native_service(&directory);
    if directory.join("sdk/rust/Cargo.toml").is_file() {
        metadata.insert("sdkLayout".into(), "project-local-v1".into());
    }
    if service || contracts_layout::has_local_sdk(&directory) {
        contracts_layout::set_artifact_paths(metadata, name);
    }
    if kind == RustProjectKind::Worker {
        let apalis = dependencies
            .and_then(|value| value.get("apalis-postgres"))
            .is_some();
        metadata.insert(
            "workerRuntime".into(),
            if apalis { "apalis" } else { "leased" }.into(),
        );
        metadata.insert("workerService".into(), service.into());
        metadata.insert("contractExport".into(), service.into());
    }
    if kind == RustProjectKind::Service {
        let postgres = dependencies.and_then(|value| value.get("sqlx")).is_some();
        metadata.insert(
            "persistence".into(),
            if postgres { "postgresql" } else { "memory" }.into(),
        );
        if !postgres {
            if let Some(resources) = project.get_mut("resources").and_then(Value::as_table_mut) {
                resources.remove("database");
            }
        }
    }
    write_manifest_value(&manifest_path, &manifest)
}

pub(super) fn routers(root: &Path, name: &str, directory: &Path) -> Result<()> {
    let routes = if directory.join("api/routes").is_dir() {
        service_routes_directory(directory)
    } else {
        directory.join("src/routes")
    };
    if !routes.is_dir() {
        return Ok(());
    }
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let project = project_table_mut(&mut manifest, name).context("Missing synced project")?;
    let previous: Vec<_> = project
        .get("routers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    // Generated routes are not routers: every project's health check, and a
    // worker's producer endpoint (`enqueue`), which init does not record and
    // a conversion to a service would otherwise refuse to drop.
    let worker = project.get("type").and_then(Value::as_str) == Some("Worker");
    let mut names: Vec<_> = rust_route_modules(&routes)?
        .into_iter()
        .filter(|name| name != "health" && !(worker && name == "enqueue"))
        .map(|module| {
            previous
                .iter()
                .find(|name| name.to_case(Case::Snake) == module)
                .cloned()
                .unwrap_or_else(|| module.to_case(Case::Kebab))
        })
        .collect();
    names.sort();
    names.dedup();
    project.insert(
        "routers".into(),
        Value::Array(names.into_iter().map(Value::String).collect()),
    );
    write_manifest_value(&manifest_path, &manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_custom_and_legacy_roots_respect_registered_paths_and_ambiguity() {
        let root = tempfile::tempdir().unwrap();
        let mut manifest: Value = toml::from_str("modules_path = 'services/team'\n").unwrap();
        let library = root.path().join("services/team/core/shared");
        create_dir_all(&library).unwrap();
        write(library.join("Cargo.toml"), "[package]\nname='shared'\n").unwrap();
        assert_eq!(
            path(root.path(), &manifest, "shared").unwrap(),
            Some(PathBuf::from("services/team/core/shared"))
        );
        let duplicate = root.path().join("crates/shared");
        create_dir_all(&duplicate).unwrap();
        write(duplicate.join("Cargo.toml"), "[package]\nname='shared'\n").unwrap();
        assert!(path(root.path(), &manifest, "shared").is_err());
        let projects = toml::from_str::<Value>(
            "[[projects]]\nname='shared'\n[projects.metadata]\ncratePath='registered/missing'\n",
        )
        .unwrap()["projects"]
            .clone();
        manifest
            .as_table_mut()
            .unwrap()
            .insert("projects".into(), projects);
        assert_eq!(
            path(root.path(), &manifest, "shared").unwrap(),
            Some(PathBuf::from("registered/missing"))
        );
    }
}

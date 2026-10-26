//! Admit lifecycle replacement only when its source still matches generated defaults.
use super::*;

const LIFECYCLE_FILES: &[&str] = &[
    "lib.rs", "main.rs", "build.rs", "build_assets.rs", "application.rs", "config.rs", "error.rs", "domain/mod.rs",
    "domain/types/mod.rs", "domain/types/error.rs", "domain/services/mod.rs",
    "persistence/mod.rs", "persistence/compliance_schema.rs", "bootstrapper.rs",
    "registrations.rs", "server.rs", "worker.rs", "signals.rs", "tests/smoke.rs",
    "__test__/http.rs", "Makefile", ".env.example", "domain/Cargo.toml",
];

pub(super) fn replaces(relative: &str) -> bool { LIFECYCLE_FILES.contains(&relative) }

pub(super) fn preflight(root: &Path, name: &str, kind: RustProjectKind) -> Result<()> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let project = project_table(&manifest, name).context("Conversion project missing")?;
    let directory = root.join(crate_path_for_project(&manifest, name).context("Conversion crate path missing")?);
    anyhow::ensure!(directory.join("lib.rs").is_file(), "Legacy Rust project conversion requires an explicit lifecycle migration before changing kind/backend");
    let description = cargo_description(&directory.join("Cargo.toml"))?.unwrap_or_default();
    let templates = match kind {
        RustProjectKind::Service => {
            let templates = native_layout::service_templates(&directory, name, &description);
            if project.get("metadata").and_then(|m| m.get("persistence")).and_then(Value::as_str) == Some("postgresql") {
                service_database::templates(&directory, name, templates)?
            } else { templates }
        }
        RustProjectKind::Worker if directory.join("worker_hooks.rs").exists() || directory.join("consumer/worker_hooks.rs").exists() => {
            bail!("Apalis runtime changes require an explicit queue/lifecycle migration; existing application hooks are preserved");
        }
        RustProjectKind::Worker => {
            anyhow::ensure!(project.get("metadata").and_then(|m| m.get("workerService")).and_then(Value::as_bool) != Some(true),
                "Worker producer changes require an explicit queue/process migration; existing HTTP contracts are preserved");
            let templates = if let Some(backend) = worker::backend(root, name)? {
                worker::templates(root, &directory, name, &description, backend)?
            } else { rust_crate_templates(&directory, name, &description, kind) };
            let templates = native_layout::flatten(&directory, name, kind, templates);
            if contracts_layout::directory(&directory).is_some() {
                worker::consumer_contracts(&directory, name, templates)?
            } else { templates }
        }
        RustProjectKind::Library => native_layout::flatten(&directory, name, kind, rust_crate_templates(&directory, name, &description, kind)),
    };
    let templates = native_mappers::templates(&directory, directory.join("domain/mappers/health.rs").is_file(), templates)?;
    let templates = native_layout::framework_paths(root, templates);
    let cargo_template = templates.iter().find(|template| template.path == directory.join("Cargo.toml"))
        .context("Conversion Cargo template missing")?;
    let mut expected_cargo: Value = toml::from_str(&dependencies::render_for_path(&cargo_template.path, &cargo_template.content)?)?;
    let mut actual_cargo: Value = toml::from_str(&read_to_string(&cargo_template.path)?)?;
    for cargo in [&mut expected_cargo, &mut actual_cargo] {
        if let Some(package) = cargo.get_mut("package").and_then(Value::as_table_mut) { package.remove("metadata"); }
    }
    // Extra runtime dependencies (including automatically linked local libraries)
    // are explicitly retained by apply_conversion. Other customized manifest
    // sections need a deliberate migration rather than being silently replaced.
    if let Some(dependencies) = actual_cargo.get_mut("dependencies").and_then(Value::as_table_mut) {
        dependencies.retain(|name, _| expected_cargo.get("dependencies").and_then(Value::as_table).is_some_and(|known| known.contains_key(name)));
    }
    anyhow::ensure!(actual_cargo == expected_cargo,
        "Conversion would overwrite customized lifecycle manifest {}; migrate it explicitly before changing kind/backend", cargo_template.path.display());
    for relative in LIFECYCLE_FILES {
        let path = directory.join(relative);
        anyhow::ensure!(!path.exists() || templates.iter().any(|template| template.path == path),
            "Conversion would overwrite unrecognized lifecycle file {}; migrate it explicitly before changing kind/backend", path.display());
    }
    for template in templates {
        let Ok(relative) = template.path.strip_prefix(&directory) else { continue; };
        let relative = relative.to_string_lossy();
        if !replaces(&relative) || !template.path.exists() { continue; }
        anyhow::ensure!(!template.path.is_symlink(), "Conversion would replace symlinked lifecycle file {}; migrate it explicitly", template.path.display());
        let expected = dependencies::render_for_path(&template.path, &template.content)?;
        let marked = crate::core::watermark::apply_watermark(&RenderedTemplate { content: expected.clone(), ..template.clone() })?;
        let actual = read_to_string(&template.path)?;
        let normalized = native_layout::conversion_source(&template.path, &actual);
        anyhow::ensure!(
            [expected, marked].iter().any(|source| native_layout::conversion_source(&template.path, source) == normalized),
            "Conversion would overwrite customized lifecycle file {}; migrate it explicitly before changing kind/backend", template.path.display()
        );
    }
    Ok(())
}

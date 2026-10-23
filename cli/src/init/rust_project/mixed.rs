//! Add the native workspace to an existing Node/Bun application without replacing
//! its TypeScript runtime, tooling, package workspace or Docker build.
use toml_edit::{Array, DocumentMut, Item, Table, value};

use super::*;
use crate::core::{application_output::output_path, docker::DockerCompose};

fn table<'a>(parent: &'a mut Table, name: &str) -> Result<&'a mut Table> {
    parent
        .entry(name)
        .or_insert(Item::Table(Table::new()))
        .as_table_mut()
        .with_context(|| format!("Cargo {name} must be a table"))
}

pub(super) fn dockerfile(root: &Path) -> Result<&'static str> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    Ok(
        if matches!(
            manifest.get("runtime").and_then(Value::as_str),
            Some("node" | "bun")
        ) {
            "./Dockerfile.rust"
        } else {
            "./Dockerfile"
        },
    )
}

#[derive(Content)]
pub(super) struct TypeScriptDockerTemplate<'a> {
    pub(super) app_name: &'a str,
    pub(super) is_node: bool,
    pub(super) is_bun: bool,
    pub(super) is_in_memory_database: bool,
}

pub(super) fn recognized_typescript_default(manifest: &Value, actual: &str) -> Result<bool> {
    let runtime = manifest
        .get("runtime")
        .and_then(Value::as_str)
        .unwrap_or("node");
    let app_name = manifest
        .get("app_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    for memory in [false, true] {
        let rendered = crate::core::template::render_embedded_template(
            "application/Dockerfile",
            &TypeScriptDockerTemplate {
                app_name,
                is_node: runtime == "node",
                is_bun: runtime == "bun",
                is_in_memory_database: memory,
            },
        )?;
        for file in ["Dockerfile", "Dockerfile.typescript"] {
            let marked = crate::core::watermark::apply_watermark(&RenderedTemplate {
                path: PathBuf::from(file),
                content: rendered.clone(),
                context: None,
            })?;
            if actual.trim_end() == rendered.trim_end() || actual.trim_end() == marked.trim_end() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn plan_typescript_sibling(
    root: &Path,
    manifest: &Value,
) -> Result<Option<(PathBuf, PathBuf, String)>> {
    if manifest
        .get("dockerfile")
        .and_then(Value::as_str)
        .is_some_and(|path| !matches!(path, "Dockerfile" | "./Dockerfile"))
    {
        return Ok(None);
    }
    let modules = manifest
        .get("modules_path")
        .and_then(Value::as_str)
        .unwrap_or("modules");
    let runtime = manifest
        .get("runtime")
        .and_then(Value::as_str)
        .unwrap_or("node");
    let app_name = manifest
        .get("app_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let destination = output_path(root, Path::new("Dockerfile.typescript"))?;
    for candidate in [
        Path::new(modules).join("Dockerfile"),
        PathBuf::from("Dockerfile"),
    ] {
        let source = output_path(root, &candidate)?;
        if !source.is_file() {
            continue;
        }
        let actual = read_to_string(&source)?;
        let stamp = crate::core::forklaunch_state::stamped(&actual);
        if stamp.is_some() {
            // A generated image that was edited is refused here, not adopted.
            super::typescript_image::preflight(root)?;
        }
        let mut recognized = stamp == Some(true);
        for memory in [false, true] {
            let rendered = crate::core::template::render_embedded_template(
                "application/Dockerfile",
                &TypeScriptDockerTemplate {
                    app_name,
                    is_node: runtime == "node",
                    is_bun: runtime == "bun",
                    is_in_memory_database: memory,
                },
            )?;
            let marked = crate::core::watermark::apply_watermark(&RenderedTemplate {
                path: source.clone(),
                content: rendered.clone(),
                context: None,
            })?;
            recognized |= [rendered, marked]
                .iter()
                .any(|expected| actual.trim_end() == expected.trim_end());
        }
        if !recognized {
            continue;
        }
        if destination.exists() && read_to_string(&destination)?.trim_end() != actual.trim_end() {
            bail!("TypeScript sibling Dockerfile conflicts with the generated source; reconcile it before adding projects");
        }
        return Ok(Some((source, destination, actual)));
    }
    Ok(None)
}

pub(super) fn ensure_workspace(root: &Path, dryrun: bool) -> Result<()> {
    let manifest_path = output_path(root, Path::new(".forklaunch/manifest.toml"))?;
    let mut manifest = read_manifest_value(&manifest_path)?;
    if !matches!(
        manifest.get("runtime").and_then(Value::as_str),
        Some("node" | "bun")
    ) {
        return Ok(());
    }
    // Project manifest views accept both spellings; emit only one canonical key.
    let application = manifest
        .as_table_mut()
        .context("Application manifest must be a table")?;
    for (canonical, alias) in [
        ("applicationLanguage", "application_language"),
        ("cargoWorkspace", "cargo_workspace"),
        ("sdkTargets", "sdk_targets"),
    ] {
        if let Some(aliased) = application.remove(alias) {
            if let Some(existing) = application.get(canonical) {
                if existing != &aliased {
                    bail!("Conflicting application metadata: {canonical} and {alias}");
                }
            } else {
                application.insert(canonical.into(), aliased);
            }
        }
    }
    if manifest
        .get("cargoWorkspace")
        .and_then(Value::as_str)
        .is_some_and(|path| path != "Cargo.toml")
    {
        bail!("Native generation requires cargoWorkspace = \"Cargo.toml\" at the application root");
    }
    let cargo_path = output_path(root, Path::new("Cargo.toml"))?;
    let mut cargo: DocumentMut = if cargo_path.exists() {
        read_to_string(&cargo_path)?
            .parse()
            .context("Failed to parse existing Cargo workspace")?
    } else {
        DocumentMut::new()
    };
    let workspace = table(cargo.as_table_mut(), "workspace")?;
    workspace.entry("resolver").or_insert(value("3"));
    if !workspace
        .entry("members")
        .or_insert(value(Array::new()))
        .is_array()
    {
        bail!("Cargo workspace.members must be an array");
    }
    let package = table(workspace, "package")?;
    package.entry("version").or_insert(value("0.1.0"));
    let edition = package.entry("edition").or_insert(value("2024"));
    if edition.as_str() != Some("2024") {
        bail!("Generated native crates require Cargo workspace.package.edition = \"2024\"");
    }
    package.entry("license").or_insert(value(
        manifest
            .get("license")
            .and_then(Value::as_str)
            .unwrap_or("MIT"),
    ));
    let mut authors = Array::new();
    if let Some(author) = manifest
        .get("author")
        .and_then(Value::as_str)
        .filter(|author| !author.is_empty())
    {
        authors.push(author);
    }
    package.entry("authors").or_insert(value(authors));
    let typescript = plan_typescript_sibling(root, &manifest)?;
    let mut outputs = Vec::new();
    let mut compose_migration = None;
    if let Some((source, _, _)) = &typescript {
        let compose_relative = manifest
            .get("docker_compose_path")
            .and_then(Value::as_str)
            .unwrap_or("docker-compose.yaml");
        let compose_path = output_path(root, Path::new(compose_relative))?;
        if compose_path.is_file() {
            compose_migration = Some((
                compose_path.clone(),
                yaml_from_str::<DockerCompose>(&read_to_string(compose_path)?)?,
            ));
        }
        let specific_ignore = source.with_file_name(format!("{}.dockerignore", source.file_name().unwrap().to_string_lossy()));
        let ignore = if specific_ignore.is_file() { specific_ignore } else { source.parent().unwrap().join(".dockerignore") };
        let target = output_path(root, Path::new("Dockerfile.typescript.dockerignore"))?;
        if ignore.is_file() {
            let content = read_to_string(&ignore)?;
            if target.exists() {
                anyhow::ensure!(read_to_string(&target)? == content, "TypeScript sibling ignore rules conflict with the owned source");
            } else {
                outputs.push((target, content));
            }
        }
    }
    for (destination, template) in [
        ("Dockerfile.rust", "rust/application/Dockerfile"),
        (
            "Dockerfile.rust.dockerignore",
            "rust/application/dockerignore",
        ),
        ("rust-toolchain.toml", "rust/application/rust-toolchain.toml"),
        ("rustfmt.toml", "rust/application/rustfmt.toml"),
        ("clippy.toml", "rust/application/clippy.toml"),
    ] {
        let destination = output_path(root, Path::new(destination))?;
        if !destination.exists() {
            outputs.push((destination, render_static_rust_template(template)));
        } else if !destination.is_file() {
            bail!(
                "Native workspace output must be a file: {}",
                destination.display()
            );
        }
    }
    let application = manifest
        .as_table_mut()
        .context("Application manifest must be a table")?;
    application.insert("language".into(), Value::String("mixed".into()));
    application.insert("applicationLanguage".into(), Value::String("mixed".into()));
    application
        .entry("cargoWorkspace")
        .or_insert(Value::String("Cargo.toml".into()));
    application.entry("sdkTargets").or_insert(Value::Array(
        default_sdk_targets()
            .into_iter()
            .map(Value::String)
            .collect(),
    ));
    if dryrun {
        return Ok(());
    }
    write(cargo_path, cargo.to_string())?;
    for (destination, source) in outputs {
        write(destination, source)?;
    }
    ensure_root_ignore_rules(root)?;
    if let Some((source, destination, content)) = typescript {
        // A generated image keeps its stamp; its ignore file moves with it.
        let generated = crate::core::forklaunch_state::stamped(&content) == Some(true);
        write(&destination, content)?;
        if generated {
            let old_ignore = source.with_file_name(format!("{}.dockerignore", source.file_name().unwrap().to_string_lossy()));
            if old_ignore.is_file() { remove_file(old_ignore)?; }
        }
        remove_file(source)?;
        if let Some((compose_path, mut compose)) = compose_migration {
            crate::core::docker::route_typescript_dockerfiles(root, &compose_path, &mut compose);
            write(compose_path, yaml_to_string(&compose)?)?;
        }
        if matches!(
            manifest.get("dockerfile").and_then(Value::as_str),
            Some("Dockerfile" | "./Dockerfile")
        ) {
            manifest.as_table_mut().unwrap().remove("dockerfile");
        }
    }
    write_manifest_value(&manifest_path, &manifest)?;
    crate::sdk::typescript_build::prepare_existing_services(root)
}

/// The Cargo workspace root owns build outputs that the TypeScript workspace's
/// own ignore file cannot reach. Missing rules are appended; existing ones stay.
pub(super) fn ensure_root_ignore_rules(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let rules = render_static_rust_template("rust/application/gitignore");
    let Ok(existing) = read_to_string(&path) else {
        return Ok(write(path, rules)?);
    };
    let present: std::collections::HashSet<&str> = existing.lines().map(str::trim).collect();
    let missing: Vec<&str> = rules
        .lines()
        .filter(|rule| !rule.trim().is_empty() && !rule.starts_with('#') && !present.contains(rule.trim()))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let separator = if existing.is_empty() || existing.ends_with('\n') { "" } else { "\n" };
    write(path, format!("{existing}{separator}\n# ForkLaunch build outputs\n{}\n", missing.join("\n")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_ignore_rules_cover_build_outputs_and_preserve_user_rules() {
        let root = tempfile::tempdir().unwrap();
        ensure_root_ignore_rules(root.path()).unwrap();
        let generated = read_to_string(root.path().join(".gitignore")).unwrap();
        assert!(generated.lines().any(|rule| rule == "**/target/"));
        assert!(generated.lines().any(|rule| rule == "**/.forklaunch/compliance.json"));
        write(root.path().join(".gitignore"), "**/target/\ncoverage/").unwrap();
        ensure_root_ignore_rules(root.path()).unwrap();
        let merged = read_to_string(root.path().join(".gitignore")).unwrap();
        assert!(merged.starts_with("**/target/\ncoverage/\n\n# ForkLaunch build outputs\n"));
        assert_eq!(merged.matches("**/target/").count(), 1);
        ensure_root_ignore_rules(root.path()).unwrap();
        assert_eq!(read_to_string(root.path().join(".gitignore")).unwrap(), merged, "Reconciliation is idempotent");
    }

    #[test]
    fn owned_contract_image_moves_to_language_sibling() {
        for runtime in ["node", "bun"] {
            let root = fixture();
            write(root.path().join(".forklaunch/manifest.toml"), format!("runtime='{runtime}'\napp_name='demo'\nmodules_path='modules'\n")).unwrap();
            let generated = crate::core::forklaunch_state::stamp("# generated compiler image\n");
            write(root.path().join("Dockerfile"), &generated).unwrap();
            write(root.path().join("Dockerfile.dockerignore"), crate::core::forklaunch_state::stamp("node_modules\n")).unwrap();
            let manifest_before = read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap();
            write(root.path().join("Dockerfile"), generated.replace("compiler image", "user customization")).unwrap();
            assert!(ensure_workspace(root.path(), false).is_err());
            assert_eq!(read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap(), manifest_before);
            assert!(!root.path().join("Dockerfile.typescript").exists());
            write(root.path().join("Dockerfile"), &generated).unwrap();
            write(root.path().join("Dockerfile.typescript"), "# custom sibling\n").unwrap();
            assert!(ensure_workspace(root.path(), false).is_err());
            assert_eq!(read_to_string(root.path().join("Dockerfile.typescript")).unwrap(), "# custom sibling\n");
            remove_file(root.path().join("Dockerfile.typescript")).unwrap();
            ensure_workspace(root.path(), false).unwrap();
            assert!(!root.path().join("Dockerfile").exists());
            assert!(!root.path().join("Dockerfile.dockerignore").exists());
            assert_eq!(read_to_string(root.path().join("Dockerfile.typescript")).unwrap(), generated);
            super::super::typescript_image::preflight(root.path()).unwrap();
        }
    }

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(root.path().join(".forklaunch/manifest.toml"), "runtime = \"node\"\nmodules_path = \"modules\"\nauthor = \"Example\"\nlicense = \"MIT\"\n").unwrap();
        root
    }

    #[test]
    fn recognized_typescript_default_moves_beside_rust_and_routes_existing_workers() {
        for runtime in ["node", "bun"] {
            let root = fixture();
            write(
                root.path().join(".forklaunch/manifest.toml"),
                format!("runtime='{runtime}'\napp_name='demo'\nmodules_path='modules'\n"),
            )
            .unwrap();
            create_dir_all(root.path().join("modules")).unwrap();
            let content = crate::core::template::render_embedded_template(
                "application/Dockerfile",
                &TypeScriptDockerTemplate {
                    app_name: "demo",
                    is_node: runtime == "node",
                    is_bun: runtime == "bun",
                    is_in_memory_database: false,
                },
            )
            .unwrap();
            write(root.path().join("modules/Dockerfile"), &content).unwrap();
            write(
                root.path().join("modules/.dockerignore"),
                "node_modules\n.env\n",
            )
            .unwrap();
            write(root.path().join("docker-compose.yaml"),"services:\n  api:\n    build: {context: ./modules, dockerfile: ./Dockerfile}\n  worker:\n    build: {context: ./modules, dockerfile: ./Dockerfile}\n  custom:\n    build: {context: ./modules, dockerfile: Custom.Dockerfile}\n").unwrap();
            ensure_workspace(root.path(), true).unwrap();
            assert!(root.path().join("modules/Dockerfile").exists());
            assert!(!root.path().join("Dockerfile.typescript").exists());
            ensure_workspace(root.path(), false).unwrap();
            assert!(!root.path().join("modules/Dockerfile").exists());
            assert_eq!(
                read_to_string(root.path().join("Dockerfile.typescript")).unwrap(),
                content
            );
            assert!(root.path().join("Dockerfile.rust").is_file());
            assert_eq!(
                read_to_string(root.path().join("Dockerfile.typescript.dockerignore")).unwrap(),
                "node_modules\n.env\n"
            );
            let compose: DockerCompose =
                yaml_from_str(&read_to_string(root.path().join("docker-compose.yaml")).unwrap())
                    .unwrap();
            for name in ["api", "worker"] {
                assert_eq!(
                    compose.services[name].build.as_ref().unwrap().dockerfile,
                    "../Dockerfile.typescript"
                );
            }
            assert_eq!(
                compose.services["custom"]
                    .build
                    .as_ref()
                    .unwrap()
                    .dockerfile,
                "Custom.Dockerfile"
            );
            ensure_workspace(root.path(), false).unwrap();
        }
    }

    #[test]
    fn adds_native_workspace_without_changing_typescript_tooling() {
        let root = fixture();
        write(root.path().join("Dockerfile"), "FROM node:22\n").unwrap();
        let original = read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap();
        ensure_workspace(root.path(), true).unwrap();
        assert!(!root.path().join("Cargo.toml").exists());
        assert!(!root.path().join("Dockerfile.rust").exists());
        assert_eq!(
            read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap(),
            original
        );
        ensure_workspace(root.path(), false).unwrap();
        assert_eq!(
            read_to_string(root.path().join("Dockerfile")).unwrap(),
            "FROM node:22\n"
        );
        let manifest = read_manifest_value(&root.path().join(".forklaunch/manifest.toml")).unwrap();
        assert_eq!(manifest["runtime"].as_str(), Some("node"));
        assert_eq!(manifest["modules_path"].as_str(), Some("modules"));
        assert_eq!(manifest["language"].as_str(), Some("mixed"));
        let cargo: Value =
            toml::from_str(&read_to_string(root.path().join("Cargo.toml")).unwrap()).unwrap();
        assert_eq!(
            cargo["workspace"]["package"]["authors"][0].as_str(),
            Some("Example")
        );
        let source = read_to_string(root.path().join("Cargo.toml")).unwrap();
        ensure_workspace(root.path(), false).unwrap();
        assert_eq!(
            read_to_string(root.path().join("Cargo.toml")).unwrap(),
            source
        );
        assert_eq!(dockerfile(root.path()).unwrap(), "./Dockerfile.rust");
    }

    #[test]
    fn preserves_existing_cargo_members_comments_and_custom_native_files() {
        let root = fixture();
        write(root.path().join("Cargo.toml"), "# user-owned workspace\n[workspace]\nmembers = [\"tools\"]\n[workspace.package]\nversion = \"2.0.0\"\nedition = \"2024\"\n[workspace.metadata]\nowner = \"custom\"\n").unwrap();
        write(
            root.path().join("Dockerfile.rust"),
            "# custom native image\n",
        )
        .unwrap();
        ensure_workspace(root.path(), false).unwrap();
        let source = read_to_string(root.path().join("Cargo.toml")).unwrap();
        assert!(source.starts_with("# user-owned workspace"));
        let cargo: Value = toml::from_str(&source).unwrap();
        assert_eq!(cargo["workspace"]["members"][0].as_str(), Some("tools"));
        assert_eq!(
            cargo["workspace"]["package"]["version"].as_str(),
            Some("2.0.0")
        );
        assert_eq!(
            cargo["workspace"]["metadata"]["owner"].as_str(),
            Some("custom")
        );
        assert_eq!(
            read_to_string(root.path().join("Dockerfile.rust")).unwrap(),
            "# custom native image\n"
        );
    }

    #[test]
    fn rejects_incompatible_cargo_before_mutating_application() {
        let root = fixture();
        let original = read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap();
        let source = "[workspace]\nmembers = []\n[workspace.package]\nedition = \"2021\"\n";
        write(root.path().join("Cargo.toml"), source).unwrap();
        assert!(
            ensure_workspace(root.path(), false)
                .unwrap_err()
                .to_string()
                .contains("edition")
        );
        assert_eq!(
            read_to_string(root.path().join("Cargo.toml")).unwrap(),
            source
        );
        assert_eq!(
            read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap(),
            original
        );
        assert!(!root.path().join("Dockerfile.rust").exists());
    }
    #[test]
    fn preserves_aliased_sdk_settings_without_duplicate_manifest_fields() {
        let root = fixture();
        let path = root.path().join(".forklaunch/manifest.toml");
        let original = read_to_string(&path).unwrap();
        write(&path, format!("{original}sdk_targets = [\"rust\"]\ncargo_workspace = \"Cargo.toml\"\napplication_language = \"typescript\"\n")).unwrap();
        ensure_workspace(root.path(), false).unwrap();
        let manifest = read_manifest_value(&path).unwrap();
        assert_eq!(
            manifest["sdkTargets"],
            Value::Array(vec![Value::String("rust".into())])
        );
        assert_eq!(manifest["applicationLanguage"].as_str(), Some("mixed"));
        assert!(manifest.get("sdk_targets").is_none());
        assert!(manifest.get("cargo_workspace").is_none());
        assert!(manifest.get("application_language").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_escaping_output_before_creating_a_native_workspace() {
        let root = fixture();
        let external = tempfile::tempdir().unwrap();
        let path = root.path().join(".forklaunch/manifest.toml");
        let original = read_to_string(&path).unwrap();
        write(external.path().join("Dockerfile"), "external file").unwrap();
        std::os::unix::fs::symlink(
            external.path().join("Dockerfile"),
            root.path().join("Dockerfile.rust"),
        )
        .unwrap();
        assert!(
            ensure_workspace(root.path(), false)
                .unwrap_err()
                .to_string()
                .contains("inside")
        );
        assert!(!root.path().join("Cargo.toml").exists());
        assert_eq!(read_to_string(&path).unwrap(), original);
        assert_eq!(
            read_to_string(external.path().join("Dockerfile")).unwrap(),
            "external file"
        );
    }
}

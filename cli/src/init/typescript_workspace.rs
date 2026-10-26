//! Stage a TypeScript workspace and the requested project before changing a native app.
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use clap::ArgMatches;
use serde_json::Value as Json;
use toml::Value;
use walkdir::WalkDir;

use crate::core::{
    application_output::output_path,
    base_path::{RequiredLocation, find_app_root_path},
};

fn run(binary: &Path, args: &[OsString], directory: &Path) -> Result<()> {
    let result = Command::new(binary)
        .args(args)
        .current_dir(directory)
        .env("FORKLAUNCH_SKIP_FORMAT", "1")
        .output()?;
    if !result.status.success() {
        bail!(
            "TypeScript workspace initialization failed:\n{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    Ok(())
}

fn files(root: &Path) -> Result<Vec<PathBuf>> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            !matches!(
                entry.file_name().to_str(),
                Some("target" | "node_modules" | ".git")
            )
        })
        .filter_map(|entry| match entry {
            Ok(entry) if entry.file_type().is_dir() => None,
            Ok(entry) => Some(Ok(entry.path().strip_prefix(root).unwrap().to_path_buf())),
            Err(error) => Some(Err(error.into())),
        })
        .collect()
}

#[derive(PartialEq)]
enum Contents {
    File(Vec<u8>),
    Link(PathBuf),
}

fn rebase_link(link: PathBuf, from_root: &Path, to_root: &Path) -> PathBuf {
    let canonical = from_root.canonicalize().ok();
    link.strip_prefix(from_root)
        .ok()
        .or_else(|| {
            canonical
                .as_ref()
                .and_then(|root| link.strip_prefix(root).ok())
        })
        .map(|relative| to_root.join(relative))
        .unwrap_or(link)
}

fn contents(path: &Path, from_root: &Path, to_root: &Path) -> Result<Option<Contents>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        let link = fs::read_link(path)?;
        let link = rebase_link(link, from_root, to_root);
        Ok(Some(Contents::Link(link)))
    } else {
        Ok(Some(Contents::File(fs::read(path)?)))
    }
}

fn copy_file(source: &Path, destination: &Path, from_root: &Path, to_root: &Path) -> Result<()> {
    fs::create_dir_all(destination.parent().context("Missing destination parent")?)?;
    if fs::symlink_metadata(source)?.file_type().is_symlink() {
        if !source
            .canonicalize()?
            .starts_with(from_root.canonicalize()?)
        {
            bail!(
                "Cannot stage an external application symlink: {}",
                source.display()
            );
        }
        let link = fs::read_link(source)?;
        let link = rebase_link(link, from_root, to_root);
        #[cfg(unix)]
        std::os::unix::fs::symlink(link, destination)?;
        #[cfg(not(unix))]
        bail!("TypeScript workspace migration of symbolic links requires Unix");
    } else {
        let temporary = tempfile::NamedTempFile::new_in(destination.parent().unwrap())?;
        fs::copy(source, temporary.path())?;
        temporary
            .persist(destination)
            .with_context(|| format!("Failed to write {}", destination.display()))?;
    }
    Ok(())
}

fn merge_map_missing(original: &mut Json, additional: &Json, field: &str) -> Result<()> {
    let existing = original
        .as_object_mut()
        .context("package.json must be an object")?
        .entry(field)
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .with_context(|| format!("package.json {field} must be an object"))?;
    if let Some(values) = additional.get(field).and_then(Json::as_object) {
        for (key, value) in values {
            existing.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    Ok(())
}

fn merge_workspace(stage: &Path, template: &Path, original: &Value) -> Result<()> {
    let manifest_path = stage.join(".forklaunch/manifest.toml");
    let mut manifest = original.clone();
    let generated: Value = toml::from_str(&fs::read_to_string(
        template.join(".forklaunch/manifest.toml"),
    )?)?;
    let modules = generated.get("modules_path").and_then(Value::as_str).context("Missing source root")?;
    let table = manifest
        .as_table_mut()
        .context("Manifest must be a table")?;
    for key in [
        "modules_path",
        "runtime",
        "http_framework",
        "validator",
        "formatter",
        "linter",
        "test_framework",
    ] {
        table.insert(
            key.to_owned(),
            generated
                .get(key)
                .context("Missing TypeScript workspace setting")?
                .clone(),
        );
    }
    table.insert("language".into(), Value::String("mixed".into()));
    table.insert("applicationLanguage".into(), Value::String("mixed".into()));
    table.remove("application_language");
    let projects = table
        .entry("projects")
        .or_insert_with(|| Value::Array(vec![]))
        .as_array_mut()
        .context("Projects must be an array")?;
    // Existing static captures keep their path when the JavaScript modules root changes.
    for project in projects
        .iter_mut()
        .filter(|project| project.get("variant").and_then(Value::as_str) == Some("storefront"))
    {
        let name = project
            .get("name")
            .and_then(Value::as_str)
            .context("Storefront name missing")?
            .to_owned();
        let metadata = project
            .as_table_mut()
            .context("Project must be a table")?
            .entry("metadata")
            .or_insert_with(|| Value::Table(Default::default()))
            .as_table_mut()
            .context("Project metadata must be a table")?;
        if !metadata.contains_key("staticPath") && !metadata.contains_key("static_path") {
            let modules = original
                .get("modules_path")
                .and_then(Value::as_str)
                .unwrap_or("crates");
            metadata.insert(
                "staticPath".into(),
                Value::String(Path::new(modules).join(name).to_string_lossy().into_owned()),
            );
        }
    }
    for project in generated
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if projects
            .iter()
            .any(|existing| existing.get("name") == project.get("name"))
        {
            bail!(
                "TypeScript workspace project name is already registered: {}",
                project["name"]
            );
        }
        projects.push(project.clone());
    }
    let topology = table
        .entry("project_peer_topology")
        .or_insert_with(|| Value::Table(Default::default()))
        .as_table_mut()
        .context("Project topology must be a table")?;
    if let Some(additional) = generated
        .get("project_peer_topology")
        .and_then(Value::as_table)
    {
        for (key, value) in additional {
            let members = topology
                .entry(key.clone())
                .or_insert_with(|| Value::Array(vec![]))
                .as_array_mut()
                .context("Project topology entries must be arrays")?;
            for name in value
                .as_array()
                .context("Generated topology must be an array")?
            {
                if !members.contains(name) {
                    members.push(name.clone());
                }
            }
        }
    }
    fs::write(manifest_path, toml::to_string_pretty(&manifest)?)?;

    let native_relative = original.get("dockerfile").and_then(Value::as_str).unwrap_or("Dockerfile");
    let native_dockerfile = output_path(stage, Path::new(native_relative))?;
    let native_default = super::rust_project::render_static_rust_template("rust/application/Dockerfile");
    let marked = crate::core::watermark::apply_watermark(&crate::core::rendered_template::RenderedTemplate {
        path: native_dockerfile.clone(), content: native_default.clone(), context: None,
    })?;
    let owned_native = fs::read_to_string(&native_dockerfile).is_ok_and(|actual| {
        actual.trim_end() == native_default.trim_end() || actual.trim_end() == marked.trim_end()
    });
    let compose_relative = original
        .get("docker_compose_path")
        .and_then(Value::as_str)
        .unwrap_or("docker-compose.yaml");
    let compose_path = output_path(stage, Path::new(compose_relative))?;
    let mut compose: serde_yml::Value = if compose_path.exists() {
        serde_yml::from_str(&fs::read_to_string(&compose_path)?)?
    } else {
        serde_yml::Value::Mapping(Default::default())
    };
    let generated_compose: serde_yml::Value =
        serde_yml::from_str(&fs::read_to_string(template.join("docker-compose.yaml"))?)?;
    for field in ["services", "networks", "volumes", "configs", "secrets"] {
        if let Some(additional) = generated_compose
            .get(field)
            .and_then(serde_yml::Value::as_mapping)
        {
            let existing = compose
                .as_mapping_mut()
                .context("Compose must be a mapping")?
                .entry(serde_yml::Value::String(field.into()))
                .or_insert_with(|| serde_yml::Value::Mapping(Default::default()))
                .as_mapping_mut()
                .context("Compose entries must be mappings")?;
            for (name, value) in additional {
                existing
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
        }
    }
    if owned_native {
        if let Some(services) = compose.get_mut("services").and_then(serde_yml::Value::as_mapping_mut) {
            for service in services.values_mut() {
                if let Some(build) = service.get_mut("build").and_then(serde_yml::Value::as_mapping_mut) {
                    if build.get("dockerfile").and_then(serde_yml::Value::as_str).is_some_and(|path| path.trim_start_matches("./") == native_relative.trim_start_matches("./")) {
                        build.insert(serde_yml::Value::String("dockerfile".into()), serde_yml::Value::String("Dockerfile.rust".into()));
                    }
                }
            }
        }
    }
    fs::write(compose_path, serde_yml::to_string(&compose)?)?;

    // Both package trees participate in the root workspace. Keep existing SDK
    // scripts and custom commands, adding only missing TypeScript tooling.
    let package_path = stage.join("package.json");
    let mut generated_package: Json =
        serde_json::from_str(&fs::read_to_string(template.join(modules).join("package.json"))?)?;
    // Tool configuration lives with the TypeScript modules. Root build includes
    // the native workspace and shared SDK; other TS commands retain that cwd.
    if let Some(scripts) = generated_package
        .get_mut("scripts")
        .and_then(Json::as_object_mut)
    {
        for (name, command) in scripts.iter_mut() {
            *command = Json::String(if name == "build" {
                "cargo build --workspace && pnpm -r --no-bail run build".into()
            } else if name == "dev" {
                "forklaunch dev".into()
            } else {
                format!("pnpm --dir {modules} run {name}")
            });
        }
        for (name, command) in [
            ("rust:build", "cargo build --workspace"),
            ("rust:test", "cargo test --workspace"),
            ("rust:fmt", "cargo fmt --all"),
            ("rust:lint", "cargo clippy --workspace --all-targets"),
        ] {
            scripts.insert(name.into(), Json::String(command.into()));
        }
        // The TypeScript image installs from the nested modules workspace and
        // copies its lockfile, so a root install must produce that one too.
        scripts.insert("postinstall".into(), Json::String(format!("pnpm --dir {modules} install")));
    }
    let mut package: Json = if package_path.exists() {
        serde_json::from_str(&fs::read_to_string(&package_path)?)?
    } else {
        serde_json::json!({"name": original["app_name"].as_str(), "private": true, "type": "module", "packageManager": "pnpm@11.28.7"})
    };
    for field in ["scripts", "devDependencies"] {
        merge_map_missing(&mut package, &generated_package, field)?;
    }
    fs::write(
        package_path,
        format!("{}\n", serde_json::to_string_pretty(&package)?),
    )?;
    let workspace_path = stage.join("pnpm-workspace.yaml");
    let mut workspace: serde_yml::Value = if workspace_path.exists() {
        serde_yml::from_str(&fs::read_to_string(&workspace_path)?)?
    } else {
        serde_yml::Value::Mapping(Default::default())
    };
    let packages = workspace
        .as_mapping_mut()
        .context("PNPM workspace must be a mapping")?
        .entry(serde_yml::Value::String("packages".into()))
        .or_insert_with(|| serde_yml::Value::Sequence(vec![]))
        .as_sequence_mut()
        .context("PNPM packages must be a list")?;
    let workspace_members = serde_yml::Value::String(format!("{modules}/*"));
    if !packages.contains(&workspace_members) {
        packages.push(workspace_members);
    }
    let generated_workspace: serde_yml::Value = serde_yml::from_str(&fs::read_to_string(
        template.join(modules).join("pnpm-workspace.yaml"),
    )?)?;
    if let Some(additional) = generated_workspace.as_mapping() {
        for (name, value) in additional {
            if name.as_str() != Some("packages") {
                workspace
                    .as_mapping_mut()
                    .unwrap()
                    .entry(name.clone())
                    .or_insert_with(|| value.clone());
            }
        }
    }
    fs::write(workspace_path, serde_yml::to_string(&workspace)?)?;
    if owned_native {
        let destination = stage.join("Dockerfile.rust");
        if destination.exists() && fs::read_to_string(&destination)?.trim_end() != native_default.trim_end() {
            bail!("Owned Rust Dockerfile conflicts with existing shared image");
        }
        fs::write(&destination, native_default)?;
        if native_dockerfile != destination { fs::remove_file(&native_dockerfile)?; }
        manifest.as_table_mut().unwrap().remove("dockerfile");
        fs::write(stage.join(".forklaunch/manifest.toml"), toml::to_string_pretty(&manifest)?)?;
    }
    if stage.join(".dockerignore").is_file() && !stage.join("Dockerfile.rust.dockerignore").exists()
    {
        fs::copy(
            stage.join(".dockerignore"),
            stage.join("Dockerfile.rust.dockerignore"),
        )?;
    }
    // Register the native services' TypeScript SDK packages in both the root and
    // the nested modules workspace. The nested workspace is the install and image
    // unit, so TypeScript projects depending on them resolve workspace links,
    // not the registry.
    crate::sdk::include_typescript_sdk(stage)?;
    super::rust_project::prepare_mixed_workspace_images(stage)?;
    Ok(())
}

fn project_args(arguments: impl IntoIterator<Item = OsString>, stage: &Path) -> Vec<OsString> {
    let mut result = vec![];
    let mut arguments = arguments.into_iter();
    while let Some(argument) = arguments.next() {
        let text = argument.to_string_lossy();
        if matches!(text.as_ref(), "--path" | "-p") {
            arguments.next();
            continue;
        }
        if text.starts_with("--path=")
            || (text.starts_with("-p") && !text.starts_with("--"))
            || matches!(text.as_ref(), "--dryrun" | "-n")
        {
            continue;
        }
        if let Some(rest) = text.strip_prefix("-n") {
            result.push(format!("-{rest}").into());
        } else {
            result.push(argument);
        }
    }
    result.push("--path".into());
    result.push(stage.as_os_str().to_owned());
    result
}

pub(super) fn attach_if_needed(matches: &ArgMatches) -> Result<bool> {
    let Some((kind, selected)) = matches.subcommand() else {
        return Ok(false);
    };
    if kind == "module" && selected.subcommand_name() == Some("storefront") {
        return Ok(false);
    }
    if !matches!(kind, "service" | "worker" | "library" | "module")
        || super::rust_project::uses_rust_generation(selected)
    {
        return Ok(false);
    }
    let (root, _) = find_app_root_path(selected, RequiredLocation::Application)?;
    let original: Value =
        toml::from_str(&fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?;
    if original.get("runtime").and_then(Value::as_str) != Some("tokio") {
        return Ok(false);
    }
    let modules = if original.get("modules_path").and_then(Value::as_str) == Some("src") { "src" } else { "modules" };
    if root.join(modules).join("package.json").exists() {
        bail!("Cannot initialize the TypeScript workspace: source root already has package.json");
    }
    let temporary = tempfile::tempdir()?;
    let template = temporary.path().join("template");
    let stage = temporary.path().join("app");
    fs::create_dir_all(&stage)?;
    let binary = std::env::current_exe()?;
    let mut arguments: Vec<OsString> = [
        "init",
        "application",
        original["app_name"]
            .as_str()
            .context("Missing application name")?,
        "--language",
        "typescript",
        "--modules-path",
        modules,
        "--database",
        "postgresql",
        "--validator",
        "zod",
        "--formatter",
        "prettier",
        "--linter",
        "oxlint",
        "--http-framework",
        "express",
        "--runtime",
        "node",
        "--test-framework",
        "vitest",
        "-D",
        original
            .get("app_description")
            .and_then(Value::as_str)
            .unwrap_or("Mixed application"),
        "-A",
        original
            .get("author")
            .and_then(Value::as_str)
            .unwrap_or("ForkLaunch"),
        "-L",
        original
            .get("license")
            .and_then(Value::as_str)
            .unwrap_or("MIT"),
        "--path",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    arguments.push(template.as_os_str().to_owned());
    run(&binary, &arguments, &std::env::current_dir()?)?;
    let mut baseline = BTreeMap::new();
    for relative in files(&root)? {
        baseline.insert(
            relative.clone(),
            contents(&root.join(&relative), &root, &root)?,
        );
        copy_file(&root.join(&relative), &stage.join(relative), &root, &stage)?;
    }
    for relative in files(&template)? {
        if relative == Path::new(".forklaunch/manifest.toml")
            || relative == Path::new("docker-compose.yaml")
        {
            continue;
        }
        let output = match relative.to_str() {
            Some("Dockerfile") => PathBuf::from("Dockerfile.typescript"),
            Some("Dockerfile.dockerignore") => PathBuf::from("Dockerfile.typescript.dockerignore"),
            _ => relative.clone(),
        };
        let destination = stage.join(&output);
        if fs::symlink_metadata(&destination).is_err() {
            copy_file(&template.join(&relative), &destination, &template, &stage)?;
        }
    }
    merge_workspace(&stage, &template, &original)?;
    run(
        &binary,
        &project_args(std::env::args_os().skip(1), &stage),
        temporary.path(),
    )?;

    let mut changed = BTreeMap::new();
    for relative in files(&stage)? {
        let source = stage.join(&relative);
        let destination = root.join(&relative);
        let staged = contents(&source, &stage, &root)?;
        let previous = baseline.get(&relative).unwrap_or(&None);
        if &staged != previous {
            if &contents(&destination, &root, &root)? != previous {
                bail!(
                    "Application changed during initialization: {}",
                    destination.display()
                );
            }
            let destination = output_path(&root, &relative)?;
            if fs::symlink_metadata(&source)?.file_type().is_symlink()
                && !source.canonicalize()?.starts_with(stage.canonicalize()?)
            {
                bail!(
                    "Generated symlink must stay inside the application: {}",
                    relative.display()
                );
            }
            changed.insert(destination, source);
        }
    }

    let mut removed = Vec::new();
    for (relative, previous) in &baseline {
        if fs::symlink_metadata(stage.join(relative)).is_err() {
            let destination = output_path(&root, relative)?;
            if &contents(&destination, &root, &root)? != previous {
                bail!("Application changed during initialization: {}", destination.display());
            }
            removed.push(destination);
        }
    }
    let dryrun = selected.get_flag("dryrun");
    for (destination, source) in changed {
        if dryrun {
            println!("Would write {}", destination.display());
            continue;
        }
        if fs::symlink_metadata(&source)?.file_type().is_symlink()
            && fs::symlink_metadata(&destination).is_ok()
        {
            fs::remove_file(&destination)?;
        }
        copy_file(&source, &destination, &stage, &root)?;
    }
    for destination in removed {
        if dryrun { println!("Would remove {}", destination.display()); }
        else { fs::remove_file(destination)?; }
    }
    if !dryrun {
        println!(
            "Initialized TypeScript workspace and {kind}; native projects and SDK workspace preserved"
        );
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_arguments_keep_project_options_and_replace_path_and_dryrun() {
        for path in [
            vec!["--path", "/original"],
            vec!["-p/original"],
            vec!["--path=/original"],
        ] {
            let args = [
                vec![
                    "init",
                    "service",
                    "example",
                    "--language",
                    "typescript",
                    "-n",
                    "-D",
                    "Some description",
                ],
                path,
            ]
            .concat();
            let rewritten = project_args(args.into_iter().map(OsString::from), Path::new("/stage"));
            assert_eq!(
                rewritten,
                [
                    "init",
                    "service",
                    "example",
                    "--language",
                    "typescript",
                    "-D",
                    "Some description",
                    "--path",
                    "/stage"
                ]
                .map(OsString::from)
            );
        }
        assert_eq!(
            project_args([OsString::from("-nDdescription")], Path::new("/stage")),
            ["-Ddescription", "--path", "/stage"].map(OsString::from)
        );
    }

    #[test]
    fn workspace_tooling_preserves_existing_scripts_and_versions() {
        let mut original = serde_json::json!({"scripts":{"build":"custom build","rust:test":"cargo test"},"devDependencies":{"typescript":"custom"}});
        let generated = serde_json::json!({"scripts":{"build":"pnpm -r build","lint":"eslint ."},"devDependencies":{"typescript":"new","tsx":"version"}});
        merge_map_missing(&mut original, &generated, "scripts").unwrap();
        merge_map_missing(&mut original, &generated, "devDependencies").unwrap();
        assert_eq!(original["scripts"]["build"], "custom build");
        assert_eq!(original["scripts"]["rust:test"], "cargo test");
        assert_eq!(original["scripts"]["lint"], "eslint .");
        assert_eq!(original["devDependencies"]["typescript"], "custom");
        assert_eq!(original["devDependencies"]["tsx"], "version");
    }

    #[cfg(unix)]
    #[test]
    fn staging_rebases_internal_links_and_rejects_external_links() {
        use std::os::unix::fs::symlink;
        let original = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(original.path().join("source"), "original").unwrap();
        fs::write(outside.path().join("source"), "outside").unwrap();
        symlink(
            original.path().join("source"),
            original.path().join("alias"),
        )
        .unwrap();
        copy_file(
            &original.path().join("source"),
            &stage.path().join("source"),
            original.path(),
            stage.path(),
        )
        .unwrap();
        copy_file(
            &original.path().join("alias"),
            &stage.path().join("alias"),
            original.path(),
            stage.path(),
        )
        .unwrap();
        fs::write(stage.path().join("alias"), "staged").unwrap();
        assert_eq!(
            fs::read_to_string(original.path().join("source")).unwrap(),
            "original"
        );
        assert_eq!(
            fs::read_to_string(stage.path().join("source")).unwrap(),
            "staged"
        );
        assert!(
            contents(
                &original.path().join("alias"),
                original.path(),
                original.path()
            )
            .unwrap()
                == contents(&stage.path().join("alias"), stage.path(), original.path()).unwrap()
        );
        symlink(
            outside.path().join("source"),
            original.path().join("external"),
        )
        .unwrap();
        assert!(
            copy_file(
                &original.path().join("external"),
                &stage.path().join("external"),
                original.path(),
                stage.path()
            )
            .is_err()
        );
        assert!(!stage.path().join("external").exists());
    }
}

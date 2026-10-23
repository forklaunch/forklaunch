//! Production packaging for ordinary generated Node and Bun workspaces.
use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::constants::BUN_VERSION;
use crate::core::forklaunch_state;

/// Where the generated image writes the production bundler and its plan.
const BUILD_TOOL: &str = "/forklaunch/production-build.mjs";
const BUILD_PLAN: &str = "/forklaunch/production.json";
const PRODUCTION_BUILD_SOURCE: &str = include_str!("typescript_image/production_build.mjs");

/// `build:production` scripts earlier CLI builds wrote before the bundler moved
/// into the generated Dockerfile; they are removed when the image regenerates.
fn retired_production_script(script: &str) -> bool {
    script.contains(".forklaunch/tools/production-build.mjs")
        || script.contains(".forklaunch/typescript-build.mjs")
}

/// The generated TypeScript image of this application: `Dockerfile.typescript`
/// beside a Rust image, otherwise `Dockerfile`, when it carries a ForkLaunch
/// stamp. `Some(false)` means it was edited after generation.
pub(crate) fn managed_image(root: &Path) -> Option<(PathBuf, bool)> {
    ["Dockerfile.typescript", "Dockerfile"].into_iter().find_map(|name| {
        let path = root.join(name);
        forklaunch_state::file_stamped(&path).map(|unchanged| (path, unchanged))
    })
}

/// The production plan embedded in a generated Dockerfile.
pub(crate) fn embedded_plan(dockerfile: &str) -> Option<JsonValue> {
    let (_, rest) = dockerfile.split_once(&format!("COPY <<\"FORKLAUNCH_EOF\" {BUILD_PLAN}\n"))?;
    let (plan, _) = rest.split_once("\nFORKLAUNCH_EOF\n")?;
    serde_json::from_str(plan).ok()
}

/// The `{service: {directory, entry}}` map an embedded plan records.
fn plan_services(plan: &JsonValue) -> BTreeMap<String, (String, String)> {
    plan["services"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, process)| {
            Some((name.clone(), (process["directory"].as_str()?.to_owned(), process["entry"].as_str()?.to_owned())))
        })
        .collect()
}

/// The generated processes of this application's TypeScript image, by
/// Compose service name, as recorded in its embedded plan.
pub(crate) fn planned_processes(root: &Path) -> BTreeMap<String, (String, String)> {
    managed_image(root)
        .and_then(|(path, _)| read_to_string(path).ok())
        .and_then(|source| embedded_plan(&source))
        .map(|plan| plan_services(&plan))
        .unwrap_or_default()
}

/// What static analysis needs to know about the generated image: its image
/// name, runtime and processes (as `{name: {directory, entry}}`), its embedded
/// production plan, and whether the Dockerfile is unchanged since generation.
pub(crate) fn summary(root: &Path) -> Option<(JsonValue, Option<JsonValue>, bool)> {
    let (path, unchanged) = managed_image(root)?;
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml")).ok()?;
    let runtime = manifest.get("runtime").and_then(Value::as_str).unwrap_or("node").to_owned();
    let image = format!("{}-typescript:local", rust_app_name(root).ok()?);
    let plan = read_to_string(&path).ok().and_then(|source| embedded_plan(&source));
    let processes = plan.as_ref().map_or_else(|| json!({}), |plan| plan["services"].clone());
    Some((json!({"image": image, "runtime": runtime, "processes": processes}), plan, unchanged))
}

/// Compose services running the generated image, by name, with the workspace
/// directory and entry (`server`, `worker` or `migrate`) of each.
pub(crate) fn managed_processes(compose: &JsonValue, runtimes: &[&str]) -> BTreeMap<String, (String, String)> {
    let mut processes = BTreeMap::new();
    let Some(services) = compose["services"].as_object() else {
        return processes;
    };
    for (name, service) in services {
        let Some(command) = service["command"].as_array().filter(|command| command.len() == 3) else {
            continue;
        };
        if !command[0].as_str().is_some_and(|runtime| runtimes.contains(&runtime))
            || command[1] != "--enable-source-maps"
        {
            continue;
        }
        let Some((directory, entry)) = command[2]
            .as_str()
            .and_then(|path| path.strip_prefix("/app/"))
            .and_then(|path| path.split_once("/production/"))
            .and_then(|(directory, entry)| Some((directory, entry.strip_suffix(".mjs")?)))
        else {
            continue;
        };
        if matches!(entry, "server" | "worker" | "migrate") && service["working_dir"] == format!("/app/{directory}") {
            processes.insert(name.clone(), (directory.to_owned(), entry.to_owned()));
        }
    }
    processes
}

fn safe_bun_workspace_path(relative: &str) -> bool {
    !relative.starts_with('-')
        && relative.split('/').all(|component| {
            !matches!(component, "" | "." | "..")
                && component
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}

fn bun_sdk_runtime_copy(relative: &str) -> Result<String> {
    anyhow::ensure!(safe_bun_workspace_path(relative), "Unsupported Bun SDK workspace path");
    // Discover packages emitted after image planning, while copying only their
    // manifests and compiled artifacts. Source and installed dependencies stay
    // out of the production assembly, just as for the static package inventory.
    Ok(format!(r#"find {relative} -type d -name node_modules -prune -o -type f -name package.json -print | while IFS= read -r manifest; do directory="${{manifest%/package.json}}"; mkdir -p "/out/$directory"; cp "$manifest" "/out/$directory/"; for artifact in dist lib production; do if [ -d "$directory/$artifact" ]; then cp -R "$directory/$artifact" "/out/$directory/"; fi; done; done"#))
}

pub(super) fn preflight(root: &Path) -> Result<()> {
    if let Some((path, unchanged)) = managed_image(root) {
        anyhow::ensure!(
            unchanged,
            "Generated TypeScript Dockerfile was modified; reconcile it before changing projects"
        );
        let ignore = path.with_file_name(format!("{}.dockerignore", path.file_name().unwrap().to_string_lossy()));
        anyhow::ensure!(
            forklaunch_state::file_stamped(&ignore) != Some(false),
            "Generated TypeScript ignore rules were modified; reconcile before changing projects"
        );
    }
    Ok(())
}

/// Validate managed process commands/scripts before a runtime migration writes files.
pub(super) fn preflight_runtime(root: &Path) -> Result<()> {
    preflight(root)?;
    if managed_image(root).is_none() {
        return Ok(());
    }
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let workspace = root.join(
        manifest["modules_path"]
            .as_str()
            .context("Missing modules path")?,
    );
    let runtime = manifest
        .get("runtime")
        .and_then(Value::as_str)
        .context("Missing image runtime")?;
    let compose: JsonValue = serde_yml::from_str(&read_to_string(
        root.join(
            manifest
                .get("docker_compose_path")
                .and_then(Value::as_str)
                .unwrap_or("docker-compose.yaml"),
        ),
    )?)?;
    let package: JsonValue =
        serde_json::from_str(&read_to_string(workspace.join("package.json"))?)?;
    anyhow::ensure!(
        package["scripts"]["build:production"]
            .as_str()
            .is_none_or(retired_production_script),
        "Customized production build script must be reconciled before changing runtime"
    );
    let processes = planned_processes(root);
    for (name, (directory, entry)) in &processes {
        let service = &compose["services"][name];
        anyhow::ensure!(
            service["command"] == json!([runtime, "--enable-source-maps", format!("/app/{directory}/production/{entry}.mjs")])
                && service["working_dir"] == format!("/app/{directory}")
                && service.get("entrypoint").is_none(),
            "Customized production process {name} must be reconciled before changing runtime"
        );
        let migration = format!("{directory}-migrate");
        if let Some(service) = compose["services"].get(&migration) {
            anyhow::ensure!(
                service["command"] == json!([runtime, "--enable-source-maps", format!("/app/{directory}/production/migrate.mjs")]),
                "Customized migration process must be reconciled before changing runtime"
            );
        }
    }
    let migrations: BTreeSet<(String, String)> = processes
        .values()
        .filter(|(directory, _)| compose["services"].get(format!("{directory}-migrate")).is_some())
        .map(|(directory, _)| (directory.clone(), "migrate".to_owned()))
        .collect();
    for (directory, entry) in processes.into_values().chain(migrations) {
        let package: JsonValue = serde_json::from_str(&read_to_string(
            workspace.join(&directory).join("package.json"),
        )?)?;
        let (key, file) = if entry == "migrate" {
            ("migrate:production".to_owned(), "migrate")
        } else {
            (format!("start:production:{entry}"), entry.as_str())
        };
        anyhow::ensure!(
            package["scripts"][&key] == format!("{runtime} --enable-source-maps production/{file}.mjs"),
            "Customized production script {key} in {directory} must be reconciled before changing runtime"
        );
    }
    Ok(())
}

/// Move the known ordinary Dockerfile to the application root, where it becomes
/// the shared production image. A custom Dockerfile is left alone.
pub(super) fn adopt_image(root: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    anyhow::ensure!(
        matches!(
            manifest.get("runtime").and_then(Value::as_str),
            Some("node" | "bun")
        ),
        "Shared TypeScript images require Node or Bun"
    );
    preflight(&root)?;
    if manifest.get("language").and_then(Value::as_str) == Some("mixed") {
        return reconcile(&root);
    }
    let destination = root.join("Dockerfile");
    if destination.is_file() {
        anyhow::ensure!(
            forklaunch_state::file_stamped(&destination).is_some()
                || super::mixed::recognized_typescript_default(
                    &manifest,
                    &read_to_string(&destination)?
                )?,
            "Custom Dockerfile requires an explicit production image integration"
        );
        return reconcile(&root);
    }
    let modules = manifest
        .get("modules_path")
        .and_then(Value::as_str)
        .context("Modules path required")?;
    let source = crate::core::application_output::output_path(
        &root,
        &Path::new(modules).join("Dockerfile"),
    )?;
    let content = read_to_string(&source)
        .context("Generated Dockerfile required for the shared production image")?;
    anyhow::ensure!(
        super::mixed::recognized_typescript_default(&manifest, &content)?,
        "Custom Dockerfile requires an explicit production image integration"
    );
    let compose_path = root.join(
        manifest
            .get("docker_compose_path")
            .and_then(Value::as_str)
            .unwrap_or("docker-compose.yaml"),
    );
    let mut compose: JsonValue = serde_yml::from_str(&read_to_string(&compose_path)?)?;
    let base = compose_path
        .parent()
        .context("Compose directory required")?;
    let context = pathdiff::diff_paths(&root, base).context("Compose root context")?;
    let context = if context.as_os_str().is_empty() {
        ".".to_owned()
    } else {
        context.to_string_lossy().into_owned()
    };
    for service in compose["services"]
        .as_object_mut()
        .context("Compose services required")?
        .values_mut()
    {
        let Some(build) = service.get_mut("build").and_then(JsonValue::as_object_mut) else {
            continue;
        };
        let candidate = base
            .join(
                build
                    .get("context")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("."),
            )
            .join(
                build
                    .get("dockerfile")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("Dockerfile"),
            );
        if candidate.canonicalize().ok().as_ref() == Some(&source.canonicalize()?) {
            build.insert("context".into(), context.clone().into());
            build.insert("dockerfile".into(), "Dockerfile".into());
        }
    }
    write(&destination, content)?;
    let ignore = source.parent().unwrap().join(".dockerignore");
    let target = root.join("Dockerfile.dockerignore");
    if ignore.is_file() && !target.exists() {
        write(target, read_to_string(ignore)?)?;
    }
    write_compose(&compose_path, &compose)?;
    remove_file(source)?;
    reconcile(&root)
}

/// Write Compose through the same `DockerCompose` model every other command
/// writes it with, so a project add and its delete agree on key order (this
/// writer edited a JSON tree and appended the keys it added, which the next
/// command then moved), keeping the file's leading comment.
fn write_compose(path: &Path, compose: &JsonValue) -> Result<()> {
    let model: crate::core::docker::DockerCompose = serde_json::from_value(compose.clone())?;
    let header: String = read_to_string(path)
        .unwrap_or_default()
        .lines()
        .take_while(|line| line.starts_with('#') || line.trim().is_empty())
        .map(|line| format!("{line}\n"))
        .collect();
    write(path, format!("{header}{}", serde_yml::to_string(&model)?))?;
    Ok(())
}

pub(super) fn reconcile(root: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let runtime = manifest
        .get("runtime")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(runtime, "node" | "bun") {
        return Ok(());
    }
    let bun = runtime == "bun";
    let package_manager = if bun { "bun" } else { "pnpm" };
    // HyperExpress's Git-distributed uWebSockets peer is incompatible with
    // pnpm 11's transitive exotic-dependency admission for injected workspaces.
    // Own a compatible pin for new HyperExpress images, without weakening any
    // existing project policy or changing the ordinary Express toolchain.
    let default_pnpm_pin =
        if manifest.get("http_framework").and_then(Value::as_str) == Some("hyper-express") {
            "pnpm@10.33.2"
        } else {
            "pnpm@11.28.7"
        };

    let runtime_kind = if bun {
        crate::constants::Runtime::Bun
    } else {
        crate::constants::Runtime::Node
    };
    let docker_name = if manifest.get("language").and_then(Value::as_str) == Some("mixed") {
        "Dockerfile.typescript"
    } else {
        "Dockerfile"
    };
    let docker_path = root.join(docker_name);
    if !docker_path.is_file() {
        return Ok(());
    }
    preflight(&root)?;
    let managed = forklaunch_state::file_stamped(&docker_path).is_some();
    if !managed
        && !super::mixed::recognized_typescript_default(&manifest, &read_to_string(&docker_path)?)?
    {
        return Ok(());
    }
    let modules = manifest
        .get("modules_path")
        .and_then(Value::as_str)
        .unwrap_or("modules");
    let workspace = crate::core::application_output::output_path(&root, Path::new(modules))?;
    let workspace_file = workspace.join("pnpm-workspace.yaml");
    let mut workspace_config: JsonValue = if bun {
        let package: JsonValue =
            serde_json::from_str(&read_to_string(workspace.join("package.json"))?)?;
        json!({"packages": package["workspaces"]})
    } else {
        if !workspace_file.is_file() {
            return Ok(());
        }
        serde_yml::from_str(&read_to_string(&workspace_file)?)?
    };
    if !bun {
        anyhow::ensure!(
            workspace_config.get("injectWorkspacePackages").is_none()
                || workspace_config["injectWorkspacePackages"] == true,
            "Shared production images require injectWorkspacePackages=true for locked production dependency assembly"
        );
        workspace_config["injectWorkspacePackages"] = json!(true);
        if workspace_config
            .get("syncInjectedDepsAfterScripts")
            .is_none()
        {
            workspace_config["syncInjectedDepsAfterScripts"] = json!([]);
        }
        let scripts = workspace_config["syncInjectedDepsAfterScripts"]
            .as_array_mut()
            .context("syncInjectedDepsAfterScripts must be an array")?;
        if !scripts.contains(&json!("build")) {
            scripts.push(json!("build"));
        }
    }
    let patterns = workspace_config["packages"]
        .as_array()
        .context("Explicit workspace package paths required")?;
    let mut packages = BTreeMap::new();
    for pattern in patterns {
        let pattern = pattern
            .as_str()
            .context("Invalid pnpm workspace package path")?;
        anyhow::ensure!(
            !pattern.starts_with('!'),
            "Negative pnpm workspace patterns need a custom image"
        );
        for directory in glob::glob(
            workspace
                .join(pattern)
                .to_str()
                .context("UTF-8 workspace path required")?,
        )? {
            let directory = directory?.canonicalize()?;
            anyhow::ensure!(
                directory.starts_with(&workspace) && directory != workspace,
                "TypeScript package escapes workspace"
            );
            if !directory.join("package.json").is_file() {
                continue;
            }
            let package: JsonValue =
                serde_json::from_str(&read_to_string(directory.join("package.json"))?)?;
            let name = package["name"]
                .as_str()
                .context("Workspace package name required")?
                .to_owned();
            anyhow::ensure!(
                packages.insert(name, (directory, package)).is_none(),
                "Duplicate TypeScript workspace package name"
            );
        }
    }
    // Keep normal workspace dependencies as live links. Modern deployment needs
    // injection enabled globally, but honors per-dependency overrides and still
    // assembles a self-contained locked runtime. In particular, atomic Rust SDK
    // rewrites must be visible without a second pnpm install/build.
    let mut workspace_package_updates = Vec::new();
    for (directory, package) in packages.values_mut().filter(|_| !bun) {
        let mut local = BTreeSet::new();
        for section in ["dependencies", "devDependencies", "optionalDependencies"] {
            if let Some(dependencies) = package[section].as_object() {
                for (name, version) in dependencies {
                    if version
                        .as_str()
                        .is_some_and(|value| value.starts_with("workspace:"))
                    {
                        local.insert(name.clone());
                    }
                }
            }
        }
        // Entries this reconcile generated for dependencies that are gone
        // (a renamed scope, a deleted project) go with them.
        let mut pruned = false;
        if let Some(metadata) = package.get_mut("dependenciesMeta").and_then(JsonValue::as_object_mut) {
            let before = metadata.len();
            metadata.retain(|name, value| local.contains(name) || *value != json!({"injected": false}));
            pruned = metadata.len() != before;
            if metadata.is_empty() && local.is_empty() {
                package.as_object_mut().context("package.json must be an object")?.remove("dependenciesMeta");
            }
        }
        if local.is_empty() && pruned {
            workspace_package_updates.push((directory.join("package.json"), serde_json::to_string_pretty(package)?));
        }
        if local.is_empty() {
            continue;
        }
        if package.get("dependenciesMeta").is_none() {
            package["dependenciesMeta"] = json!({});
        }
        let metadata = package["dependenciesMeta"]
            .as_object_mut()
            .context("Workspace dependency metadata must be an object")?;
        for name in local {
            let dependency = metadata.entry(name).or_insert_with(|| json!({}));
            let dependency = dependency
                .as_object_mut()
                .context("Dependency metadata must be an object")?;
            dependency.entry("injected").or_insert(json!(false));
        }
        workspace_package_updates.push((
            directory.join("package.json"),
            serde_json::to_string_pretty(package)?,
        ));
    }
    let compose_path = root.join(
        manifest
            .get("docker_compose_path")
            .and_then(Value::as_str)
            .unwrap_or("docker-compose.yaml"),
    );
    let mut compose: JsonValue = serde_yml::from_str(&read_to_string(&compose_path)?)?;
    let image = format!("{}-typescript:local", rust_app_name(&root)?);
    let services = compose["services"]
        .as_object_mut()
        .context("Compose services missing")?;
    // Generated processes are recognizable from their Compose commands, under
    // either runtime while a runtime change is in progress.
    let previous_processes = read_to_string(&docker_path)
        .ok()
        .filter(|_| managed)
        .and_then(|source| embedded_plan(&source))
        .map(|plan| plan_services(&plan))
        .unwrap_or_else(|| managed_processes(&json!({"services": services.clone()}), &["node", "bun"]));
    let prior_runtime = previous_processes
        .keys()
        .find_map(|name| services.get(name)?["command"][0].as_str())
        .unwrap_or(runtime)
        .to_owned();
    let prior_runtime = prior_runtime.as_str();
    let mut processes = BTreeMap::new();
    let mut projects: BTreeMap<String, JsonValue> = BTreeMap::new();
    for (name, service) in services.iter() {
        let generated = service["build"]["dockerfile"].as_str().is_some_and(|file| {
            Path::new(file)
                .file_name()
                .is_some_and(|name| name == docker_name)
        }) || service["image"].as_str() == Some(&image)
            || previous_processes.contains_key(name);
        if !generated || name.ends_with("-migrate") {
            continue;
        }
        let (mut directory, entry) =
            if let Some((directory, entry)) = previous_processes.get(name) {
                (directory.clone(), entry.clone())
            } else if service["command"].as_array().is_some_and(|command| {
                command.len() == 3 && command[0] == runtime && command[1] == "--enable-source-maps"
            }) {
                let command = service["command"][2]
                    .as_str()
                    .context("Invalid generated TypeScript process")?;
                let relative = command
                    .strip_prefix("/app/")
                    .context("Generated TypeScript process path missing")?;
                let (directory, entry) = relative
                    .split_once("/production/")
                    .context("Generated TypeScript process path missing")?;
                let entry = entry
                    .strip_suffix(".mjs")
                    .context("Generated TypeScript process extension missing")?;
                anyhow::ensure!(
                    matches!(entry, "server" | "worker"),
                    "Unsupported generated TypeScript process"
                );
                (directory.to_owned(), entry.to_owned())
            } else {
                let working = service["working_dir"]
                    .as_str()
                    .context("Generated TypeScript process working directory missing")?;
                let directory = Path::new(working)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("Invalid process working directory")?
                    .to_owned();
                let command = service["entrypoint"]
                    .as_array()
                    .context("Custom TypeScript command requires a custom Dockerfile")?;
                anyhow::ensure!(
                    command.len() == 3 && command[0] == package_manager && command[1] == "run",
                    "Custom TypeScript command requires a custom Dockerfile"
                );
                let entry = match command[2].as_str() {
                    Some("dev" | "dev:server" | "start" | "start:server") => "server",
                    Some("dev:worker" | "start:worker") => "worker",
                    _ => bail!("Custom TypeScript process script requires a custom Dockerfile"),
                };
                (directory, entry.to_owned())
            };
        if !workspace.join(&directory).is_dir() {
            directory = packages
                .values()
                .filter_map(|(path, _)| path.file_name().and_then(|name| name.to_str()))
                .find(|candidate| {
                    name.as_str() == *candidate
                        || name == &format!("{candidate}-worker")
                        || name == &format!("{candidate}-server")
                })
                .context("Renamed TypeScript process has no owning package")?
                .to_owned();
        }
        let (package_name, (path, package)) = packages
            .iter()
            .find(|(_, (path, _))| path == &workspace.join(&directory))
            .context("TypeScript process must belong to a workspace package")?;
        anyhow::ensure!(
            path.join(format!("{entry}.ts")).is_file(),
            "Generated TypeScript process entrypoint missing"
        );
        let project = projects.entry(directory.clone()).or_insert_with(
            || json!({"package":package_name,"directory":directory,"entries":[],"database":null}),
        );
        let entries = project["entries"].as_array_mut().unwrap();
        if !entries.contains(&json!(entry)) {
            entries.push(json!(entry));
        }
        // A fresh service may not have created its first migration directory yet.
        // Its database contract still owns a migration process (initially a no-op).
        if let Some(database) = manifest
            .get("projects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|project| project.get("name").and_then(Value::as_str) == Some(directory.as_str()))
            .and_then(|project| project.get("resources"))
            .and_then(|resources| resources.get("database"))
            .and_then(Value::as_str)
            .filter(|database| *database != "mongodb")
        {
            project["database"] = json!(database);
        }
        for migration in std::fs::read_dir(path)? {
            let migration = migration?;
            if migration.file_type()?.is_dir() {
                if let Some(database) = migration
                    .file_name()
                    .to_str()
                    .and_then(|name| name.strip_prefix("migrations-"))
                    .filter(|name| *name != "mongodb")
                {
                    project["database"] = json!(database);
                }
            }
        }
        if let Some(script) = service["entrypoint"]
            .as_array()
            .and_then(|values| values.get(2))
            .and_then(JsonValue::as_str)
        {
            use crate::core::package_json::package_json_constants::*;
            let actual = package["scripts"][script]
                .as_str()
                .context("Generated TypeScript process script missing")?;
            let expected = [None, Some(crate::constants::Database::PostgreSQL)]
                .into_iter()
                .flat_map(|database| {
                    [
                        project_dev_server_script(&runtime_kind, database),
                        project_dev_worker_client_script(&runtime_kind),
                        project_start_server_script(&runtime_kind, database),
                        project_start_worker_script(&runtime_kind, database),
                    ]
                });
            anyhow::ensure!(
                expected.into_iter().any(|value| value == actual),
                "Custom TypeScript runtime script requires a custom Dockerfile"
            );
        }
        let build = package["scripts"]["build"]
            .as_str()
            .context("Missing TypeScript build script")?;
        anyhow::ensure!(
            build == "tsc -b",
            "Custom TypeScript build script requires a custom Dockerfile"
        );
        processes.insert(name.clone(), json!({"directory":directory,"entry":entry}));
    }
    {
        for (directory, _) in previous_processes.values() {
            let directory = directory.as_str();
            if !projects.contains_key(directory) {
                let migration = format!("{directory}-migrate");
                if let Some(service) = services.get(&migration) {
                    anyhow::ensure!(
                        service["command"]
                            == json!([
                                runtime,
                                "--enable-source-maps",
                                format!("/app/{directory}/production/migrate.mjs")
                            ]),
                        "Modified migration process requires review before removal"
                    );
                    services.remove(&migration);
                    for service in services.values_mut() {
                        // `get_mut`, not indexing: indexing inserts a null
                        // `depends_on` into every service without one.
                        if let Some(dependencies) =
                            service.get_mut("depends_on").and_then(JsonValue::as_object_mut)
                        {
                            dependencies.remove(&migration);
                        }
                    }
                }
            }
        }
    }
    let mut inputs = BTreeSet::new();
    for entry in std::fs::read_dir(&workspace)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_file()
            && (name == "package.json"
                || name == "pnpm-lock.yaml"
                || name == "bun.lock"
                || name == "pnpm-workspace.yaml"
                || (name.starts_with("tsconfig") && name.ends_with(".json")))
        {
            inputs.insert(format!("{modules}/{name}"));
        }
    }
    // A missing lock is an actionable first-build error, never an unlocked install.
    inputs.insert(format!(
        "{modules}/{}",
        if bun { "bun.lock" } else { "pnpm-lock.yaml" }
    ));
    for (path, _) in packages.values() {
        inputs.insert(format!("{}/", path.strip_prefix(&root)?.to_string_lossy()));
    }
    let sdk_root = root.join(crate::sdk::sdk_base(&root)?);
    let sdk_workspace = if sdk_root.exists() && sdk_root.starts_with(&workspace) {
        anyhow::ensure!(sdk_root.is_dir() && sdk_root.canonicalize()? == sdk_root,
            "Generated SDK image input must be a directory without symbolic aliases");
        inputs.insert(format!("{}/", sdk_root.strip_prefix(&root)?.to_string_lossy()));
        Some(sdk_root.strip_prefix(&workspace)?.to_string_lossy().into_owned())
    } else {
        None
    };
    let ignore_path = root.join(format!("{docker_name}.dockerignore"));
    let mut ignore = forklaunch_state::unstamped(&read_to_string(&ignore_path).unwrap_or_default()).to_owned();
    // Required exclusions must come after custom negations; retaining an earlier
    // duplicate is insufficient because Docker evaluates the last matching rule.
    const REQUIRED_IGNORE: &str = "\n# Forklaunch shared image exclusions\n.git/\n**/node_modules/\n**/target/\n**/.env*\n!**/.env.example\n**/dist/\n**/lib/\n**/.forklaunch-runtime/\n**/production/\n";
    if !ignore.ends_with(REQUIRED_IGNORE) {
        ignore.push_str(REQUIRED_IGNORE);
    }
    let mut package: JsonValue =
        serde_json::from_str(&read_to_string(workspace.join("package.json"))?)?;
    anyhow::ensure!(
        package["scripts"]["build"].as_str()
            == Some(if bun {
                "bunx -y @forklaunch/bunrun build"
            } else {
                "pnpm -r --no-bail run build"
            }),
        "Custom TypeScript workspace build requires a custom Dockerfile"
    );
    if package.get("devDependencies").is_none() {
        package["devDependencies"] = json!({});
    }
    if let Some(version) = package["devDependencies"]["esbuild"].as_str() {
        anyhow::ensure!(
            version == "0.25.12",
            "Review custom esbuild version before adopting generated production packaging"
        );
    }
    package["devDependencies"]["esbuild"] = json!("0.25.12");
    let mut pnpm_pin = package["packageManager"]
        .as_str()
        .unwrap_or(default_pnpm_pin)
        .to_owned();
    // pnpm 11.1.0 was the generated pin before 11.28.7. Its post-build
    // injected-dependency sync hard-links module symlinks (.prettierrc and
    // friends) into node_modules/.pnpm on Linux, so a second build fails with
    // EEXIST. Move projects still on the old generated pin forward.
    if pnpm_pin == "pnpm@11.1.0" {
        pnpm_pin = "pnpm@11.28.7".to_owned();
    }
    if !bun {
        anyhow::ensure!(
            matches!(pnpm_pin.as_str(), "pnpm@10.33.2" | "pnpm@11.28.7"),
            "Custom package manager version requires a custom Dockerfile"
        );
        package["packageManager"] = json!(pnpm_pin);
    }
    // The bundler runs only inside the image; nothing in the workspace calls it.
    if let Some(existing) = package["scripts"].get("build:production") {
        anyhow::ensure!(
            existing.as_str().is_some_and(retired_production_script),
            "Custom production build script requires a custom Dockerfile"
        );
        package["scripts"].as_object_mut().unwrap().remove("build:production");
    }
    let mut runtime_packages = workspace_package_updates;
    for project in projects.values() {
        let (directory, original) = &packages[project["package"].as_str().unwrap()];
        let mut updated = original.clone();
        for entry in project["entries"].as_array().unwrap() {
            let entry = entry.as_str().unwrap();
            let key = format!("start:production:{entry}");
            let script = format!("{runtime} --enable-source-maps production/{entry}.mjs");
            anyhow::ensure!(
                updated["scripts"].get(&key).is_none()
                    || updated["scripts"][&key] == script
                    || updated["scripts"][&key]
                        == format!("{prior_runtime} --enable-source-maps production/{entry}.mjs"),
                "Custom production process script requires a custom Dockerfile"
            );
            updated["scripts"][&key] = json!(script);
        }
        if !project["database"].is_null() {
            let script = format!("{runtime} --enable-source-maps production/migrate.mjs");
            anyhow::ensure!(
                updated["scripts"].get("migrate:production").is_none()
                    || updated["scripts"]["migrate:production"] == script
                    || updated["scripts"]["migrate:production"]
                        == format!("{prior_runtime} --enable-source-maps production/migrate.mjs"),
                "Custom production migration script requires a custom Dockerfile"
            );
            updated["scripts"]["migrate:production"] = json!(script);
        }
        let tsconfig_path = directory.join("tsconfig.json");
        if tsconfig_path.is_file() {
            let mut tsconfig: JsonValue = serde_json::from_str(&read_to_string(&tsconfig_path)?)
                .context("Custom JSONC compiler config requires an explicit production image")?;
            if tsconfig.get("exclude").is_none() {
                tsconfig["exclude"] = json!(["node_modules"]);
            }
            let exclude = tsconfig["exclude"]
                .as_array_mut()
                .context("Compiler exclusions must be an array")?;
            if !exclude.contains(&json!("production")) {
                exclude.push(json!("production"));
            }
            runtime_packages.push((tsconfig_path, serde_json::to_string_pretty(&tsconfig)?));
        }
        runtime_packages.push((
            directory.join("package.json"),
            serde_json::to_string_pretty(&updated)?,
        ));
    }

    // Native declarations are compiled without the server so the first Node image
    // build has the same service-local type packages as a normal Cargo build.
    let mut native_sdks = Vec::new();
    if let Some(entries) = manifest.get("projects").and_then(Value::as_array).filter(|_| !processes.is_empty()) {
        for project in entries {
            let metadata = project.get("metadata");
            if metadata
                .and_then(|value| value.get("sdkLayout"))
                .and_then(Value::as_str)
                != Some("project-local-v1")
                || metadata
                    .and_then(|value| value.get("language"))
                    .and_then(Value::as_str)
                    != Some("rust")
            {
                continue;
            }
            let directory = metadata
                .and_then(|value| value.get("cratePath"))
                .and_then(Value::as_str)
                .context("Native SDK requires cratePath")?;
            let sdk = root.join(directory).join("sdk/rust/Cargo.toml");
            let cargo: Value = toml::from_str(&read_to_string(sdk)?)?;
            let package = cargo["package"]["name"]
                .as_str()
                .context("Native SDK package name missing")?;
            anyhow::ensure!(
                package
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte)),
                "Invalid native SDK package name"
            );
            native_sdks.push((directory.to_owned(), package.to_owned()));
        }
    }
    let mut docker = "# Generated shared TypeScript production image. Install once to commit pnpm-lock.yaml.\nFROM node:22-bookworm-slim AS builder\nRUN apt-get update && apt-get install -y --no-install-recommends python3 make g++ git ca-certificates && rm -rf /var/lib/apt/lists/*\nRUN corepack enable && corepack prepare pnpm@11.28.7 --activate\nWORKDIR /app\n".to_owned();
    docker = docker.replace("pnpm@11.28.7", &pnpm_pin);
    if bun {
        docker = format!("# Generated shared Bun production image. Commit bun.lock before building.\nFROM oven/bun:{BUN_VERSION} AS builder\nUSER root\nRUN apt-get update && apt-get install -y --no-install-recommends python3 make g++ git ca-certificates && rm -rf /var/lib/apt/lists/*\nWORKDIR /app\n");
    }
    let native_stage = "native-sdk-compiler";
    if !native_sdks.is_empty() {
        let (_, native_inputs, _) = super::language_images::plan(&root)?;
        let (stages, copies) = super::language_images::framework_inputs(&root)?;
        let mut generation = format!("{stages}FROM rust:1.98.0-slim-bookworm AS native-sdk-compiler\nWORKDIR /app\nARG CARGO_BUILD_JOBS=2\nRUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*\n{copies}");
        for input in native_inputs {
            generation.push_str(&format!(
                "COPY {}\n",
                serde_json::to_string(&[input.clone(), format!("/app/{input}")])?
            ));
        }
        generation.push_str("RUN cargo check --locked");
        for (_, package) in &native_sdks {
            generation.push_str(&format!(" -p {package}"));
        }
        generation.push('\n');
        let stage = if bun { "FROM oven/bun:" } else { "FROM node:" };
        let position = docker
            .find(stage)
            .context("Managed Node builder stage missing")?;
        docker.insert_str(position, &generation);
    }
    for input in &inputs {
        docker.push_str(&format!(
            "COPY {}\n",
            serde_json::to_string(&[input.clone(), format!("/app/{input}")])?
        ));
    }
    for (directory, _) in &native_sdks {
        let path = format!("/app/{directory}/sdk/typescript/");
        docker.push_str(&format!(
            "COPY --from={native_stage} {}\n",
            serde_json::to_string(&[path.clone(), path])?
        ));
    }
    // The bundler and its plan travel inside this file; the application keeps no
    // copy of either.
    let production_plan = json!({"workspace":format!("/app/{modules}"),"packages":packages.values().map(|(path,_)|path.strip_prefix(&workspace).unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>(),"output":".forklaunch-runtime","packageOutputs":true,"processes":projects.values().collect::<Vec<_>>(),"services":processes});
    for (path, content) in [
        (BUILD_TOOL, PRODUCTION_BUILD_SOURCE.to_owned()),
        (BUILD_PLAN, format!("{}\n", serde_json::to_string_pretty(&production_plan)?)),
    ] {
        anyhow::ensure!(!content.lines().any(|line| line == "FORKLAUNCH_EOF"), "Heredoc delimiter in generated content");
        docker.push_str(&format!("COPY <<\"FORKLAUNCH_EOF\" {path}\n{content}FORKLAUNCH_EOF\n"));
    }
    docker.push_str(&format!("WORKDIR /app/{modules}\nRUN {package_manager} install --frozen-lockfile{}\nRUN {package_manager} run build\nRUN {runtime} {BUILD_TOOL} {BUILD_PLAN}\nRUN mkdir -p /out\n", if bun { " --ignore-scripts" } else { "" }));
    for project in projects.values() {
        let directory = project["directory"].as_str().unwrap();
        let name = project["package"].as_str().unwrap();
        anyhow::ensure!(
            name.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"@/._-".contains(&c))
                && directory
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
            "Unsupported TypeScript package identifier"
        );
        if !bun {
            // Not `--offline`: pnpm verifies every lockfile entry against
            // minimumReleaseAge, and for a package whose registry document
            // changed inside that window it needs the version's exact publish
            // time from the full metadata, which the frozen install does not
            // leave in the offline mirror (ERR_PNPM_NO_OFFLINE_META). The
            // frozen lockfile still forbids any re-resolution.
            docker.push_str(&format!("RUN pnpm --filter {name} --prod deploy --frozen-lockfile /out/{directory}\nRUN mkdir -p /out/{directory}/production && cp -R {directory}/production/. /out/{directory}/production/\n"));
        }
    }
    if bun {
        // Keep the original lock and workspace graph, install only production
        // dependencies, and copy compiled code rather than TypeScript sources.
        docker.push_str("RUN cp package.json bun.lock /out/\n");
        for (directory, _) in packages.values() {
            let relative = directory.strip_prefix(&workspace)?.to_string_lossy();
            anyhow::ensure!(
                safe_bun_workspace_path(&relative),
                "Unsupported Bun workspace path"
            );
            docker.push_str(&format!("RUN mkdir -p /out/{relative} && cp {relative}/package.json /out/{relative}/ && if [ -d {relative}/dist ]; then cp -R {relative}/dist /out/{relative}/; fi && if [ -d {relative}/lib ]; then cp -R {relative}/lib /out/{relative}/; fi && if [ -d {relative}/production ]; then cp -R {relative}/production /out/{relative}/; fi\n"));
        }
        if let Some(relative) = sdk_workspace {
            docker.push_str(&format!("RUN {}\n", bun_sdk_runtime_copy(&relative)?));
        }
        docker.push_str(
            "RUN cd /out && bun install --production --frozen-lockfile --ignore-scripts\n",
        );
    }
    let has_ca_bundle = workspace
        .join("core/certs/aws-rds-global-bundle.pem")
        .is_file();
    if has_ca_bundle {
        docker.push_str("RUN mkdir -p /out/.certs && cp core/certs/aws-rds-global-bundle.pem /out/.certs/aws-rds-global-bundle.pem\n");
    }
    if bun {
        docker.push_str(&format!("FROM oven/bun:{BUN_VERSION} AS runtime\nUSER root\nRUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*\nWORKDIR /app\nCOPY --from=builder --chown=bun:bun /out/ /app/\nENV NODE_ENV=production\nUSER bun\n"));
    } else {
        docker.push_str("FROM node:22-bookworm-slim AS runtime\nRUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*\nWORKDIR /app\nCOPY --from=builder --chown=node:node /out/ /app/\nENV NODE_ENV=production\nUSER node\n");
    }
    if has_ca_bundle {
        docker.push_str("ENV NODE_EXTRA_CA_CERTS=/app/.certs/aws-rds-global-bundle.pem\n");
    }
    let build_context = pathdiff::diff_paths(&root, compose_path.parent().unwrap())
        .context("Compose context path")?;
    let build_context = if build_context.as_os_str().is_empty() {
        ".".into()
    } else {
        build_context.to_string_lossy().into_owned()
    };
    for (index, (name, process)) in processes.iter().enumerate() {
        let directory = process["directory"].as_str().unwrap();
        let entry = process["entry"].as_str().unwrap();
        let service = services.get_mut(name).unwrap();
        service["image"] = json!(image);
        service.as_object_mut().unwrap().remove("entrypoint");
        service["command"] = json!([
            runtime,
            "--enable-source-maps",
            format!("/app/{directory}/production/{entry}.mjs")
        ]);
        if let Some(volumes) = service["volumes"].as_array_mut() {
            volumes.retain(|volume| {
                volume.as_str().is_none_or(|value| {
                    !value.ends_with("/node_modules")
                        && !value.ends_with("/dist")
                        && !value.starts_with(&format!("./{modules}/{directory}:"))
                })
            });
        }
        service["working_dir"] = json!(format!("/app/{directory}"));
        service.as_object_mut().unwrap().remove("build");
        if index == 0 {
            service["build"] = json!({"context":build_context,"dockerfile":docker_name});
        }
    }
    for (directory, project) in &projects {
        if project["database"].is_null() {
            continue;
        }
        let migration_name = format!("{directory}-migrate");
        let source_name = processes
            .iter()
            .find(|(_, process)| process["directory"] == *directory)
            .unwrap()
            .0;
        let mut migration = services[source_name].clone();
        for field in [
            "build",
            "ports",
            "healthcheck",
            "container_name",
            "hostname",
        ] {
            migration.as_object_mut().unwrap().remove(field);
        }
        migration["command"] = json!([
            runtime,
            "--enable-source-maps",
            format!("/app/{directory}/production/migrate.mjs")
        ]);
        migration["restart"] = json!("no");
        if let Some(depends) = migration["depends_on"].as_object_mut() {
            depends.remove(&migration_name);
        }
        services.insert(migration_name.clone(), migration);
        for (name, process) in &processes {
            if process["directory"] != *directory {
                continue;
            }
            let service = services.get_mut(name).unwrap();
            if !service["depends_on"].is_object() {
                service["depends_on"] = json!({});
            }
            service["depends_on"][&migration_name] =
                json!({"condition":"service_completed_successfully"});
        }
    }
    if !bun {
        write(&workspace_file, serde_yml::to_string(&workspace_config)?)?;
    }
    for (path, content) in runtime_packages {
        write(path, content)?;
    }
    write(
        workspace.join("package.json"),
        serde_json::to_string_pretty(&package)?,
    )?;
    write(&docker_path, forklaunch_state::stamp(&docker))?;
    write(ignore_path, forklaunch_state::stamp(&ignore))?;
    write_compose(&compose_path, &compose)?;
    // Files the bundler and its plan once lived in are no longer read.
    for retired in [".forklaunch/tools/production-build.mjs", ".forklaunch/typescript-build.mjs"] {
        let path = root.join(retired);
        if path.is_file() {
            remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundler_and_plan_travel_inside_the_stamped_dockerfile() {
        let root = fixture();
        reconcile(root.path()).unwrap();
        let docker = read_to_string(root.path().join("Dockerfile.typescript")).unwrap();
        assert_eq!(forklaunch_state::stamped(&docker), Some(true));
        assert!(docker.contains(&format!("COPY <<\"FORKLAUNCH_EOF\" {BUILD_TOOL}\n")));
        assert!(docker.contains(&format!("RUN node {BUILD_TOOL} {BUILD_PLAN}\n")));
        let plan = embedded_plan(&docker).unwrap();
        assert_eq!(plan["workspace"], "/app/modules");
        assert!(plan["processes"].as_array().is_some_and(|processes| !processes.is_empty()));
        for retired in [".forklaunch/build.json", ".forklaunch/generated.json", ".forklaunch/tools"] {
            assert!(!root.path().join(retired).exists(), "{retired}");
        }
        let package: JsonValue =
            serde_json::from_str(&read_to_string(root.path().join("modules/package.json")).unwrap()).unwrap();
        assert!(package["scripts"].get("build:production").is_none());
        // Regenerating an unchanged image is a no-op.
        reconcile(root.path()).unwrap();
        assert_eq!(read_to_string(root.path().join("Dockerfile.typescript")).unwrap(), docker);
    }

    #[test]
    fn runtime_migration_rejects_custom_process_and_build_configuration() {
        let root = fixture();
        reconcile(root.path()).unwrap();
        preflight_runtime(root.path()).unwrap();
        let path = root.path().join("docker-compose.yaml");
        let original = read_to_string(&path).unwrap();
        write(
            &path,
            original.replace("production/server.mjs", "production/custom.mjs"),
        )
        .unwrap();
        assert!(preflight_runtime(root.path())
            .unwrap_err()
            .to_string()
            .contains("Customized production process"));
        write(&path, original).unwrap();
        let path = root.path().join("Dockerfile.typescript");
        let generated = read_to_string(&path).unwrap();
        let custom = generated.replace("\"output\"", "\"custom\"");
        write(&path, &custom).unwrap();
        assert!(preflight_runtime(root.path())
            .unwrap_err()
            .to_string()
            .contains("Generated TypeScript Dockerfile was modified"));
        assert_eq!(read_to_string(path).unwrap(), custom);
    }

    #[test]
    fn hyperexpress_owns_compatible_manager_and_retains_supported_explicit_pins() {
        let root = fixture();
        let manifest = root.path().join(".forklaunch/manifest.toml");
        let original = read_to_string(&manifest).unwrap();
        write(
            &manifest,
            format!("{original}http_framework='hyper-express'\n"),
        )
        .unwrap();
        reconcile(root.path()).unwrap();
        let package_path = root.path().join("modules/package.json");
        let package: JsonValue =
            serde_json::from_str(&read_to_string(&package_path).unwrap()).unwrap();
        assert_eq!(package["packageManager"], "pnpm@10.33.2");
        assert!(read_to_string(root.path().join("Dockerfile.typescript"))
            .unwrap()
            .contains("corepack prepare pnpm@10.33.2"));
        // Returning to Express need not invalidate a supported existing lockfile.
        write(&manifest, format!("{original}http_framework='express'\n")).unwrap();
        reconcile(root.path()).unwrap();
        let package: JsonValue =
            serde_json::from_str(&read_to_string(package_path).unwrap()).unwrap();
        assert_eq!(package["packageManager"], "pnpm@10.33.2");
    }

    #[test]
    fn native_declarations_compile_without_a_typescript_contract_compiler() {
        let root = fixture();
        let path = root.path().join(".forklaunch/manifest.toml");
        write(&path, format!("{}\n[[projects]]\nname='native'\ntype='Service'\n[projects.metadata]\nlanguage='rust'\nsdkLayout='project-local-v1'\ncratePath='src/native'\n", read_to_string(&path).unwrap())).unwrap();
        let sdk = root.path().join("src/native/sdk/rust");
        create_dir_all(&sdk).unwrap();
        write(sdk.join("Cargo.toml"), "[package]\nname='native-sdk'\nversion='0.1.0'\n[lib]\npath='lib.rs'\n").unwrap();
        write(sdk.join("lib.rs"), "").unwrap();
        write(root.path().join("Cargo.toml"), "[workspace]\nmembers=['src/native/sdk/rust']\n").unwrap();
        reconcile(root.path()).unwrap();
        let docker = read_to_string(root.path().join("Dockerfile.typescript")).unwrap();
        assert!(docker.contains("AS native-sdk-compiler"));
        assert!(docker.contains("RUN cargo check --locked -p native-sdk"));
        assert!(docker.contains("COPY --from=native-sdk-compiler [\"/app/src/native/sdk/typescript/\""));
        assert!(!docker.contains("FORKLAUNCH_CONTRACT_COMPILER="));
    }

    #[test]
    fn shared_images_cover_later_generated_socket_sdk_members() {
        let root = fixture();
        let manifest = root.path().join(".forklaunch/manifest.toml");
        write(&manifest, format!("{}\n[[projects]]\nname='native'\ntype='Service'\n[projects.metadata]\nlanguage='rust'\nsdkLayout='project-local-v1'\ncratePath='src/native'\n", read_to_string(&manifest).unwrap())).unwrap();
        write(&manifest, format!("{}\n[framework]\nversion='0.1.1'\n", read_to_string(&manifest).unwrap())).unwrap();
        let sdk = root.path().join("src/native/sdk/rust");
        create_dir_all(&sdk).unwrap();
        write(sdk.join("Cargo.toml"), "[package]\nname='native-sdk'\nversion='0.1.0'\n[lib]\npath='lib.rs'\n").unwrap();
        write(sdk.join("lib.rs"), "").unwrap();
        write(root.path().join("Cargo.toml"), "[workspace]\nmembers=['src/native/sdk/rust']\n").unwrap();
        let sdk_root = crate::sdk::sdk_base(root.path()).unwrap();
        create_dir_all(root.path().join(&sdk_root)).unwrap();
        reconcile(root.path()).unwrap();
        let typescript_before = read_to_string(root.path().join("Dockerfile.typescript")).unwrap();
        let rust_before = super::super::language_images::plan(root.path()).unwrap();
        let copy = format!("COPY {}", serde_json::to_string(&[format!("{}/", sdk_root.display()), format!("/app/{}/", sdk_root.display())]).unwrap());
        assert!(typescript_before.contains(&copy));
        assert!(rust_before.0.contains(&copy));
        assert!(!rust_before.0.contains("modules/api/") && !rust_before.0.contains("COPY . ."));

        // Exercise the actual watcher's socket generator after image planning.
        let artifact = root.path().join("src/native/sdk/typescript/socket-contract.json");
        create_dir_all(artifact.parent().unwrap()).unwrap();
        write(artifact, include_str!("../../../tests/fixtures/rust_socket_contract.json")).unwrap();
        let mut stdout = termcolor::StandardStream::stdout(termcolor::ColorChoice::Never);
        crate::sdk::websocket::generate_from(root.path(), "rust", "socket-contract", Some("native"), false, &mut stdout, false).unwrap();
        let member = sdk_root.join("rust/native-websocket-sdk");
        assert!(root.path().join(&member).join("Cargo.toml").is_file());
        let cargo: Value = toml::from_str(&read_to_string(root.path().join("Cargo.toml")).unwrap()).unwrap();
        assert!(cargo["workspace"]["members"].as_array().unwrap().contains(&Value::String(member.to_string_lossy().into_owned())));
        assert_eq!(super::super::language_images::plan(root.path()).unwrap(), rust_before);
        reconcile(root.path()).unwrap();
        assert_eq!(read_to_string(root.path().join("Dockerfile.typescript")).unwrap(), typescript_before);
    }

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        for directory in [
            ".forklaunch",
            "modules/api/migrations-postgresql",
            "modules/jobs",
            "src/native",
        ] {
            create_dir_all(root.path().join(directory)).unwrap();
        }
        write(
            root.path().join(".forklaunch/manifest.toml"),
            "language='mixed'\nruntime='node'\nmodules_path='modules'\napp_name='demo'\n",
        )
        .unwrap();
        write(
            root.path().join("modules/package.json"),
            r#"{"name":"demo","scripts":{"build":"pnpm -r --no-bail run build"}}"#,
        )
        .unwrap();
        write(
            root.path().join("modules/pnpm-workspace.yaml"),
            "packages:\n  - api\n  - jobs\n",
        )
        .unwrap();
        for name in ["api", "jobs"] {
            write(
                root.path().join(format!("modules/{name}/package.json")),
                format!(r#"{{"name":"@demo/{name}","scripts":{{"build":"tsc -b","dev":"pnpm tsx watch server.ts","dev:worker":"pnpm tsx watch worker.ts"}}}}"#),
            )
            .unwrap();
            write(
                root.path().join(format!("modules/{name}/server.ts")),
                "export {};\n",
            )
            .unwrap();
        }
        write(root.path().join("modules/jobs/worker.ts"), "export {};\n").unwrap();
        let template = crate::core::template::render_embedded_template(
            "application/Dockerfile",
            &super::super::mixed::TypeScriptDockerTemplate {
                app_name: "demo",
                is_node: true,
                is_bun: false,
                is_in_memory_database: false,
            },
        )
        .unwrap();
        write(root.path().join("Dockerfile.typescript"), template).unwrap();
        write(
            root.path().join("Dockerfile.typescript.dockerignore"),
            "private-assets/\n",
        )
        .unwrap();
        write(
            root.path().join("docker-compose.yaml"),
            r#"services:
  api:
    build: {context: modules, dockerfile: ../Dockerfile.typescript}
    working_dir: /demo/api
    entrypoint: [pnpm, run, dev]
    environment: {DB_NAME: demo}
  jobs:
    build: {context: modules, dockerfile: ../Dockerfile.typescript}
    working_dir: /demo/jobs
    entrypoint: [pnpm, run, dev:worker]
"#,
        )
        .unwrap();
        root
    }

    #[test]
    fn bun_shared_image_preserves_runtime_lock_and_compiled_processes() {
        let root = fixture();
        let path = root.path();
        let manifest = path.join(".forklaunch/manifest.toml");
        write(
            &manifest,
            read_to_string(&manifest)
                .unwrap()
                .replace("runtime='node'", "runtime='bun'"),
        )
        .unwrap();
        write(path.join("modules/package.json"), r#"{"name":"demo","workspaces":["api","jobs",".forklaunch/universal-sdk"],"scripts":{"build":"bunx -y @forklaunch/bunrun build"}}"#).unwrap();
        let runtime = path.join("modules/.forklaunch/universal-sdk");
        create_dir_all(runtime.join("lib")).unwrap();
        write(runtime.join("package.json"), r#"{"name":"@forklaunch/universal-sdk","main":"lib/index.js","scripts":{"build":"tsup"}}"#).unwrap();
        write(runtime.join("lib/index.js"), "export const sdk = {};\n").unwrap();
        remove_file(path.join("modules/pnpm-workspace.yaml")).unwrap();
        let compose = path.join("docker-compose.yaml");
        write(
            &compose,
            read_to_string(&compose)
                .unwrap()
                .replace("[pnpm, run,", "[bun, run,"),
        )
        .unwrap();
        for name in ["api", "jobs"] {
            let file = path.join(format!("modules/{name}/package.json"));
            let mut package: JsonValue =
                serde_json::from_str(&read_to_string(&file).unwrap()).unwrap();
            package["scripts"]["dev"] = json!(
                crate::core::package_json::package_json_constants::project_dev_server_script(
                    &crate::constants::Runtime::Bun,
                    None
                )
            );
            package["scripts"]["dev:worker"] = json!(
                crate::core::package_json::package_json_constants::project_dev_worker_client_script(
                    &crate::constants::Runtime::Bun
                )
            );
            write(file, serde_json::to_string(&package).unwrap()).unwrap();
        }
        let template = crate::core::template::render_embedded_template(
            "application/Dockerfile",
            &super::super::mixed::TypeScriptDockerTemplate {
                app_name: "demo",
                is_node: false,
                is_bun: true,
                is_in_memory_database: false,
            },
        )
        .unwrap();
        write(path.join("Dockerfile.typescript"), template).unwrap();
        reconcile(path).unwrap();
        let docker = read_to_string(path.join("Dockerfile.typescript")).unwrap();
        assert!(docker.contains(&format!("FROM oven/bun:{} AS runtime", crate::constants::BUN_VERSION)));
        assert!(docker.contains(&format!("FROM oven/bun:{} AS builder", crate::constants::BUN_VERSION)));
        assert!(docker.contains("bun install --production --frozen-lockfile --ignore-scripts"));
        assert!(docker.contains("cp .forklaunch/universal-sdk/package.json /out/.forklaunch/universal-sdk/"));
        assert!(docker.contains("cp -R .forklaunch/universal-sdk/lib /out/.forklaunch/universal-sdk/"));
        let config = embedded_plan(&docker).unwrap();
        assert!(config["packages"].as_array().unwrap().contains(&json!(".forklaunch/universal-sdk")));
        assert!(docker.contains(&format!("RUN bun {BUILD_TOOL} {BUILD_PLAN}\n")));
        assert!(!docker.contains("pnpm"));
        assert!(!path.join("modules/pnpm-workspace.yaml").exists());
        let compose: JsonValue = serde_yml::from_str(&read_to_string(compose).unwrap()).unwrap();
        assert_eq!(compose["services"]["api"]["command"][0], "bun");
        assert_eq!(compose["services"]["api-migrate"]["command"][0], "bun");
        assert_eq!(
            compose["services"]["jobs"]["command"][2],
            "/app/jobs/production/worker.mjs"
        );
        reconcile(path).unwrap();
        assert_eq!(
            docker,
            read_to_string(path.join("Dockerfile.typescript")).unwrap()
        );
    }

    #[test]
    fn bun_workspace_paths_allow_hidden_packages_without_traversal_or_shell_options() {
        for path in ["api", ".forklaunch/universal-sdk", "domain.v1/sdk-typescript"] {
            assert!(safe_bun_workspace_path(path), "safe package rejected: {path}");
        }
        for path in ["", ".", "..", "../outside", "api/../outside", "/absolute", "api//sdk",
            "-rf", "api;touch", "api$(touch)", "api\\sdk", "api sdk", "api\nsdk"] {
            assert!(!safe_bun_workspace_path(path), "unsafe package accepted: {path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn bun_runtime_assembly_includes_late_socket_packages_without_source_or_node_modules() {
        // Plan while only the ordinary aggregate exists; emit the socket SDK
        // afterwards, exactly as the development SDK preparation does.
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join("client-sdk")).unwrap();
        write(root.path().join("client-sdk/package.json"), r#"{"name":"@demo/client-sdk","version":"1.0.0"}"#).unwrap();
        let planned = bun_sdk_runtime_copy("client-sdk").unwrap();
        let socket = root.path().join("client-sdk/typescript-websocket/native");
        for directory in ["dist", "lib", "production", "node_modules/private"] {
            create_dir_all(socket.join(directory)).unwrap();
        }
        write(socket.join("package.json"), r#"{"name":"native-websocket-sdk","version":"1.0.0","main":"dist/index.js"}"#).unwrap();
        write(socket.join("index.ts"), "export const source = true;\n").unwrap();
        write(socket.join("dist/index.js"), "export const compiled = true;\n").unwrap();
        write(socket.join("lib/index.js"), "module.exports = {};\n").unwrap();
        write(socket.join("production/client.mjs"), "export {};\n").unwrap();
        write(socket.join("node_modules/private/package.json"), r#"{"name":"private-dependency"}"#).unwrap();
        let out = root.path().join("assembled");
        let status = std::process::Command::new("sh").args(["-eu", "-c", &planned.replace("/out/", "$OUTPUT/")])
            .env("OUTPUT", &out).current_dir(root.path()).status().unwrap();
        assert!(status.success());
        let emitted = out.join("client-sdk/typescript-websocket/native");
        for file in ["package.json", "dist/index.js", "lib/index.js", "production/client.mjs"] {
            assert_eq!(std::fs::read(emitted.join(file)).unwrap(), std::fs::read(socket.join(file)).unwrap());
        }
        assert!(out.join("client-sdk/package.json").is_file());
        assert!(!emitted.join("index.ts").exists());
        assert!(!emitted.join("node_modules").exists());
        for invalid in ["../outside", "/absolute", ".", "sdk;touch", "sdk$(touch)"] {
            assert!(bun_sdk_runtime_copy(invalid).is_err());
        }
    }

    #[test]
    fn pure_node_contract_images_keep_typescript_language_and_root_dockerfile() {
        let root = fixture();
        let manifest = root.path().join(".forklaunch/manifest.toml");
        write(
            &manifest,
            read_to_string(&manifest)
                .unwrap()
                .replace("language='mixed'", "language='typescript'"),
        )
        .unwrap();
        std::fs::rename(
            root.path().join("Dockerfile.typescript"),
            root.path().join("modules/Dockerfile"),
        )
        .unwrap();
        let compose = root.path().join("docker-compose.yaml");
        write(
            &compose,
            read_to_string(&compose)
                .unwrap()
                .replace("../Dockerfile.typescript", "Dockerfile"),
        )
        .unwrap();
        adopt_image(root.path()).unwrap();
        assert!(root.path().join("Dockerfile").is_file());
        assert!(!root.path().join("modules/Dockerfile").exists());
        assert!(read_to_string(manifest)
            .unwrap()
            .contains("language='typescript'"));
        let compose: JsonValue = serde_yml::from_str(&read_to_string(compose).unwrap()).unwrap();
        assert_eq!(
            compose["services"]["api"]["build"]["dockerfile"],
            "Dockerfile"
        );
        assert_eq!(compose["services"]["api"]["build"]["context"], ".");
        preflight(root.path()).unwrap();
    }

    #[test]
    fn mandatory_exclusions_follow_custom_secret_negations() {
        let root = fixture();
        let ignore = root.path().join("Dockerfile.typescript.dockerignore");
        write(&ignore, "**/.env*\n!**/.env*\nprivate-assets/\n").unwrap();
        reconcile(root.path()).unwrap();
        let rules = read_to_string(&ignore).unwrap();
        assert!(rules.rfind("\n**/.env*\n").unwrap() > rules.find("!**/.env*").unwrap());
        reconcile(root.path()).unwrap();
        assert_eq!(rules, read_to_string(ignore).unwrap());
    }

    #[test]
    fn workspace_packages_keep_live_links_and_explicit_dependency_overrides() {
        for existing in [None, Some(true)] {
            let root = fixture();
            let path = root.path().join("modules/jobs/package.json");
            let mut package: JsonValue =
                serde_json::from_str(&read_to_string(&path).unwrap()).unwrap();
            package["dependencies"] = json!({"@demo/api":"workspace:*"});
            if let Some(injected) = existing {
                package["dependenciesMeta"] = json!({"@demo/api":{"injected":injected}});
            }
            write(&path, serde_json::to_string(&package).unwrap()).unwrap();
            reconcile(root.path()).unwrap();
            let package: JsonValue = serde_json::from_str(&read_to_string(path).unwrap()).unwrap();
            assert_eq!(
                package["dependenciesMeta"]["@demo/api"]["injected"],
                existing.unwrap_or(false)
            );
            let workspace: JsonValue = serde_yml::from_str(
                &read_to_string(root.path().join("modules/pnpm-workspace.yaml")).unwrap(),
            )
            .unwrap();
            assert_eq!(workspace["injectWorkspacePackages"], true);
        }
    }

    #[test]
    fn stale_generated_dependency_metadata_is_pruned() {
        let root = fixture();
        let path = root.path().join("modules/jobs/package.json");
        let mut package: JsonValue = serde_json::from_str(&read_to_string(&path).unwrap()).unwrap();
        package["dependencies"] = json!({"@demo/api":"workspace:*"});
        // A renamed scope or deleted project leaves generated entries behind;
        // explicit settings stay.
        package["dependenciesMeta"] = json!({"@old/api":{"injected":false},"@demo/gone":{"injected":false},"custom":{"injected":true}});
        write(&path, serde_json::to_string(&package).unwrap()).unwrap();
        reconcile(root.path()).unwrap();
        let package: JsonValue = serde_json::from_str(&read_to_string(&path).unwrap()).unwrap();
        assert_eq!(package["dependenciesMeta"], json!({"@demo/api":{"injected":false},"custom":{"injected":true}}));
    }

    #[test]
    fn declared_database_has_migration_process_before_first_migration_exists() {
        let root = fixture();
        std::fs::remove_dir(root.path().join("modules/api/migrations-postgresql")).unwrap();
        let manifest = root.path().join(".forklaunch/manifest.toml");
        let mut source = read_to_string(&manifest).unwrap();
        source
            .push_str("\n[[projects]]\nname='api'\n[projects.resources]\ndatabase='postgresql'\n");
        write(manifest, source).unwrap();
        reconcile(root.path()).unwrap();
        let compose: JsonValue =
            serde_yml::from_str(&read_to_string(root.path().join("docker-compose.yaml")).unwrap())
                .unwrap();
        assert_eq!(
            compose["services"]["api-migrate"]["command"][2],
            "/app/api/production/migrate.mjs"
        );
        assert_eq!(
            compose["services"]["api"]["depends_on"]["api-migrate"]["condition"],
            "service_completed_successfully"
        );
    }

    #[test]
    fn shared_node_image_scopes_packages_and_runs_migrations_separately() {
        let root = fixture();
        reconcile(root.path()).unwrap();
        let docker = read_to_string(root.path().join("Dockerfile.typescript")).unwrap();
        assert!(!docker.contains("COPY . .") && !docker.contains("src/native"));
        assert!(docker.contains("--frozen-lockfile") && docker.contains("--prod deploy --frozen-lockfile") && !docker.contains("--offline"));
        assert!(docker.contains("USER node"));
        let compose: JsonValue =
            serde_yml::from_str(&read_to_string(root.path().join("docker-compose.yaml")).unwrap())
                .unwrap();
        let services = compose["services"].as_object().unwrap();
        assert_eq!(
            services
                .values()
                .filter(|service| service.get("build").is_some())
                .count(),
            1
        );
        assert_eq!(
            compose["services"]["api"]["depends_on"]["api-migrate"]["condition"],
            "service_completed_successfully"
        );
        assert_eq!(
            compose["services"]["jobs"]["command"][2],
            "/app/jobs/production/worker.mjs"
        );
        assert_eq!(
            compose["services"]["api-migrate"]["environment"]["DB_NAME"],
            "demo"
        );
        assert!(
            read_to_string(root.path().join("Dockerfile.typescript.dockerignore"))
                .unwrap()
                .contains("private-assets/")
        );
        reconcile(root.path()).unwrap();
        assert_eq!(
            read_to_string(root.path().join("Dockerfile.typescript")).unwrap(),
            docker
        );
        write(root.path().join("Dockerfile.typescript"), docker.replace("FROM node", "FROM custom-node")).unwrap();
        assert!(preflight(root.path()).is_err());
    }

    #[test]
    fn custom_script_is_rejected_before_package_or_compose_mutation() {
        let root = fixture();
        let package = root.path().join("modules/api/package.json");
        write(
            &package,
            r#"{"name":"@demo/api","scripts":{"build":"custom-build","dev":"pnpm tsx watch server.ts"}}"#,
        )
        .unwrap();
        let compose = read_to_string(root.path().join("docker-compose.yaml")).unwrap();
        let root_package = read_to_string(root.path().join("modules/package.json")).unwrap();
        assert!(reconcile(root.path())
            .unwrap_err()
            .to_string()
            .contains("Custom TypeScript build"));
        assert_eq!(
            read_to_string(root.path().join("docker-compose.yaml")).unwrap(),
            compose
        );
        assert_eq!(
            read_to_string(root.path().join("modules/package.json")).unwrap(),
            root_package
        );
    }

    #[test]
    fn rename_refreshes_process_inventory_and_removes_old_migration_dependency() {
        let root = fixture();
        reconcile(root.path()).unwrap();
        std::fs::rename(
            root.path().join("modules/api"),
            root.path().join("modules/renamed"),
        )
        .unwrap();
        let package_path = root.path().join("modules/renamed/package.json");
        let mut package: JsonValue =
            serde_json::from_str(&read_to_string(&package_path).unwrap()).unwrap();
        package["name"] = json!("@demo/renamed");
        write(package_path, serde_json::to_string(&package).unwrap()).unwrap();
        write(
            root.path().join("modules/pnpm-workspace.yaml"),
            "packages: [renamed, jobs]\n",
        )
        .unwrap();
        let compose_path = root.path().join("docker-compose.yaml");
        let mut compose: JsonValue =
            serde_yml::from_str(&read_to_string(&compose_path).unwrap()).unwrap();
        let service = compose["services"]
            .as_object_mut()
            .unwrap()
            .remove("api")
            .unwrap();
        compose["services"]["renamed"] = service;
        write(&compose_path, serde_yml::to_string(&compose).unwrap()).unwrap();
        reconcile(root.path()).unwrap();
        let compose: JsonValue =
            serde_yml::from_str(&read_to_string(&compose_path).unwrap()).unwrap();
        assert!(compose["services"].get("api-migrate").is_none());
        assert!(compose["services"]["renamed"]["depends_on"]
            .get("api-migrate")
            .is_none());
        assert_eq!(
            compose["services"]["renamed"]["depends_on"]["renamed-migrate"]["condition"],
            "service_completed_successfully"
        );
        assert_eq!(
            compose["services"]["renamed"]["command"][2],
            "/app/renamed/production/server.mjs"
        );
    }
}

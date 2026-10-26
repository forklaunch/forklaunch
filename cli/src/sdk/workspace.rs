//! Keep generated SDKs in the application's JavaScript workspace.
use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde_json::{Value as Json, json};
use serde_yml::Value as Yaml;

use crate::{
    constants::Runtime,
    core::{
        application_output::output_path,
        package_json::package_json_constants::application_build_script,
    },
};

mod bun;

fn merge_missing(destination: &mut Json, source: &Json, field: &str) -> Result<()> {
    if let Some(entries) = source.get(field) {
        let entries = entries
            .as_object()
            .with_context(|| format!("{field} must be an object"))?;
        let destination = destination
            .as_object_mut()
            .context("package.json must be an object")?
            .entry(field)
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .with_context(|| format!("Root {field} must be an object"))?;
        for (key, value) in entries {
            destination
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }
    Ok(())
}

fn add_packages(workspace: &mut Yaml, additions: &[String]) -> Result<()> {
    let packages = workspace
        .as_mapping_mut()
        .context("PNPM workspace must be a mapping")?
        .entry(Yaml::String("packages".into()))
        .or_insert_with(|| Yaml::Sequence(vec![]))
        .as_sequence_mut()
        .context("PNPM packages must be a list")?;
    for addition in additions {
        let value = Yaml::String(addition.clone());
        if !packages.contains(&value) {
            packages.push(value);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) enum SdkWorkspace {
    Http,
    WebSocket,
}

pub(crate) fn include_typescript_sdk(root: &Path) -> Result<()> {
    for (path, content) in plan_typescript_sdk(root, SdkWorkspace::Http)? {
        super::write_if_changed(output_path(root, Path::new(&path))?, content)?;
    }
    Ok(())
}

/// Pure planning lets socket generation validate/check every output before writes.
pub(super) fn plan_typescript_sdk(
    root: &Path,
    kind: SdkWorkspace,
) -> Result<Vec<(String, String)>> {
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let manifest: toml::Value = if manifest_path.is_file() {
        toml::from_str(&fs::read_to_string(manifest_path)?)?
    } else {
        toml::Value::Table(Default::default())
    };
    let bun = manifest.get("runtime").and_then(toml::Value::as_str) == Some("bun");
    let package_path = output_path(root, Path::new("package.json"))?;
    let workspace_path = output_path(root, Path::new("pnpm-workspace.yaml"))?;
    let mut package: Json = if package_path.exists() {
        serde_json::from_str(&fs::read_to_string(&package_path)?)?
    } else {
        json!({})
    };
    let mut workspace: Yaml = if workspace_path.exists() {
        serde_yml::from_str(&fs::read_to_string(&workspace_path)?)?
    } else {
        Yaml::Mapping(Default::default())
    };
    let sdk_pattern = match kind {
        SdkWorkspace::Http => super::layout::typescript(root)?
            .to_string_lossy()
            .into_owned(),
        SdkWorkspace::WebSocket => super::sdk_base(root)?
            .join("typescript-websocket/*")
            .to_string_lossy()
            .into_owned(),
    };
    let mut additions = vec![sdk_pattern.to_owned()];
    let native = super::layout::native(root)?;
    let local_sdk = manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .is_some_and(|projects| {
            projects.iter().any(|project| {
                project.get("metadata").is_some_and(|metadata| {
                    metadata.get("sdkLayout").and_then(toml::Value::as_str)
                        == Some("project-local-v1")
                        && metadata.get("language").and_then(toml::Value::as_str) == Some("rust")
                })
            })
        });
    let local_http = matches!(kind, SdkWorkspace::Http) && local_sdk;
    let modules_path = manifest
        .get("modules_path")
        .and_then(toml::Value::as_str)
        .unwrap_or("src");
    let absent_generated_members = if bun {
        [
            super::layout::typescript(root)?
                .to_string_lossy()
                .into_owned(),
            format!("{modules_path}/client-sdk/events/typescript"),
        ]
        .into_iter()
        .filter(|path| !root.join(path).join("package.json").is_file())
        .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if native || local_sdk {
        additions.extend([
            format!("{modules_path}/client-sdk/events/typescript"),
            format!("{modules_path}/*/events/typescript"),
            format!("{modules_path}/*/sdk/typescript"),
        ]);
    }
    let mut outputs = Vec::new();
    let mut has_modules = false;
    let mut module_scripts = Vec::new();
    if let Some(relative) = manifest.get("modules_path").and_then(toml::Value::as_str) {
        let relative = Path::new(relative);
        let modules_package = output_path(root, &relative.join("package.json"))?;
        if modules_package != package_path && modules_package.is_file() {
            has_modules = true;
            if bun && let Some(policy) = self::bun::install_policy(root, relative)? {
                outputs.push(("bunfig.toml".into(), policy));
            }
            let mut modules: Json = serde_json::from_str(&fs::read_to_string(&modules_package)?)?;
            module_scripts = modules
                .get("scripts")
                .and_then(Json::as_object)
                .map(|scripts| scripts.keys().cloned().collect())
                .unwrap_or_default();
            if !bun
                && package.get("packageManager").is_none()
                && let Some(manager) = modules.get("packageManager")
            {
                package["packageManager"] = manager.clone();
            }
            for field in [
                "dependencies",
                "devDependencies",
                "overrides",
                "resolutions",
            ] {
                merge_missing(&mut package, &modules, field)?;
            }
            let prefix = relative
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(value) => Some(value.to_string_lossy()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/");
            // Both package managers use the nested workspace when invoked
            // inside the modules directory, including shared Docker builds.
            // Register SDK packages there as well as at application root.
            let nested_sdk_packages = additions
                .iter()
                .filter_map(|path| path.strip_prefix(&format!("{prefix}/")).map(str::to_owned))
                .collect::<Vec<_>>();
            additions.push(format!("{prefix}/*"));
            let nested_workspace = output_path(root, &relative.join("pnpm-workspace.yaml"))?;
            let mut nested: Yaml = if nested_workspace.is_file() {
                serde_yml::from_str(&fs::read_to_string(&nested_workspace)?)?
            } else {
                Yaml::Mapping(Default::default())
            };
            if !bun && !nested_sdk_packages.is_empty() {
                add_packages(&mut nested, &nested_sdk_packages)?;
                outputs.push((
                    relative
                        .join("pnpm-workspace.yaml")
                        .to_string_lossy()
                        .into_owned(),
                    serde_yml::to_string(&nested)?,
                ));
            }
            if bun && !nested_sdk_packages.is_empty() {
                let absent_nested = absent_generated_members
                    .iter()
                    .filter_map(|path| path.strip_prefix(&format!("{prefix}/")).map(str::to_owned))
                    .collect::<Vec<_>>();
                self::bun::include_packages(
                    modules
                        .as_object_mut()
                        .context("Module package.json must be an object")?,
                    &nested_sdk_packages,
                    &absent_nested,
                )?;
                outputs.push((
                    relative.join("package.json").to_string_lossy().into_owned(),
                    format!("{}\n", serde_json::to_string_pretty(&modules)?),
                ));
            }
            if let Some(mapping) = nested.as_mapping() {
                for (key, value) in mapping {
                    if key.as_str() != Some("packages") {
                        workspace
                            .as_mapping_mut()
                            .context("PNPM workspace must be a mapping")?
                            .entry(key.clone())
                            .or_insert_with(|| value.clone());
                    }
                }
            }
            // Retain nested patterns, including exclusions, with their new base.
            let patterns = if bun {
                modules.get("workspaces").cloned()
            } else {
                nested
                    .get("packages")
                    .map(serde_json::to_value)
                    .transpose()?
            };
            if let Some(patterns) = patterns {
                for pattern in patterns
                    .as_array()
                    .context("Module workspace packages must be a list")?
                {
                    let pattern = pattern
                        .as_str()
                        .context("Workspace patterns must be strings")?;
                    let (negative, path) = pattern
                        .strip_prefix('!')
                        .map_or(("", pattern), |path| ("!", path));
                    if Path::new(path).is_absolute()
                        || Path::new(path)
                            .components()
                            .any(|part| matches!(part, std::path::Component::ParentDir))
                    {
                        bail!("Module workspace patterns must stay inside the modules directory");
                    }
                    // The modules/* entry already includes direct children.
                    // Bun executes builds twice for overlapping explicit members.
                    if negative.is_empty() && path != "." && !path.contains('/') {
                        continue;
                    }
                    additions.push(format!("{negative}{prefix}/{path}"));
                }
            }
        }
    }
    let object = package
        .as_object_mut()
        .context("Root package.json must be an object")?;
    object.entry("private").or_insert(json!(true));
    object.entry("type").or_insert(json!("module"));
    if !bun {
        object
            .entry("packageManager")
            .or_insert(json!("pnpm@11.28.7"));
    }
    if bun {
        object.entry("name").or_insert_with(|| {
            json!(
                manifest
                    .get("app_name")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("forklaunch-sdk-workspace")
            )
        });
        self::bun::include_packages(object, &additions, &absent_generated_members)?;
    }
    let scripts = object
        .entry("scripts")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("Root scripts must be an object")?;
    let (prefix, source, filter) = match kind {
        SdkWorkspace::Http => (
            "sdk",
            "contract",
            if native || local_http {
                "@forklaunch-generated/*"
            } else {
                "@forklaunch-generated/sdk"
            },
        ),
        SdkWorkspace::WebSocket => ("sdk:socket", "socket-contract", "*-websocket-sdk"),
    };
    if (native || local_http) && matches!(kind, SdkWorkspace::Http) {
        let key = "sdk:typecheck";
        let previous_bun_local = format!(
            "bun run --filter '@forklaunch-generated/*' --filter './{modules_path}/*/sdk/typescript' --filter '@forklaunch/universal-sdk' build"
        );
        if scripts
            .get(key)
            .and_then(Json::as_str)
            .is_some_and(|value| {
                matches!(
                    value,
                    "pnpm --filter @forklaunch-generated/sdk build"
                        | "pnpm --filter '@forklaunch-generated/sdk' build"
                        | "bun run --filter '@forklaunch-generated/sdk' build"
                ) || (local_http && bun && value == previous_bun_local)
                || (local_http && matches!(value,
                    "pnpm --filter '@forklaunch-generated/*' build"
                    | "bun run --filter '@forklaunch-generated/*' build"
                    | "pnpm --filter '@forklaunch-generated/sdk' --filter './src/*/sdk/typescript' --filter '@forklaunch/universal-sdk' build"
                    | "bun run --filter '@forklaunch-generated/sdk' --filter './src/*/sdk/typescript' --filter '@forklaunch/universal-sdk' build"
                    | "pnpm --filter '@forklaunch-generated/*' --filter './src/*/sdk/typescript' --filter '@forklaunch/universal-sdk' build"
                    | "bun run --filter '@forklaunch-generated/*' --filter './src/*/sdk/typescript' --filter '@forklaunch/universal-sdk' build"))
            })
        {
            scripts.remove(key);
        }
    }
    let sdk_target = if manifest
        .get("sdkTargets")
        .or_else(|| manifest.get("sdk_targets"))
        .and_then(toml::Value::as_array)
        .is_some_and(|targets| targets.iter().any(|target| target.as_str() == Some("rust")))
    {
        "all"
    } else {
        "typescript"
    };
    if local_http {
        // Earlier releases also built a vendored universal SDK copy.
        let vendored = [
            format!("bun run --filter '@forklaunch/universal-sdk' build && bun run --filter '{filter}' --filter './{modules_path}/*/sdk/typescript' build"),
            format!("{} --filter '{filter}' --filter './{modules_path}/*/sdk/typescript' --filter '@forklaunch/universal-sdk' build", if bun { "bun run" } else { "pnpm" }),
        ];
        let key = format!("{prefix}:typecheck");
        if scripts.get(&key).and_then(Json::as_str).is_some_and(|value| vendored.iter().any(|previous| previous == value)) {
            scripts.remove(&key);
        }
    }
    for (name, command) in [
        (
            "generate",
            format!("forklaunch sdk generate --target {sdk_target} --from {source}"),
        ),
        (
            "check",
            format!("forklaunch sdk generate --target {sdk_target} --from {source} --check"),
        ),
        (
            "watch",
            format!("forklaunch sdk watch --target {sdk_target} --from {source}"),
        ),
        (
            "typecheck",
            if local_http {
                // The shared runtime is the published @forklaunch/universal-sdk
                // package, so only the workspace SDK packages are built.
                format!(
                    "{} --filter '{filter}' --filter './{modules_path}/*/sdk/typescript' build",
                    if bun { "bun run" } else { "pnpm" }
                )
            } else if bun {
                format!("bun run --filter '{filter}' build")
            } else {
                format!("pnpm --filter '{filter}' build")
            },
        ),
    ] {
        let key = format!("{prefix}:{name}");
        let legacy = command.replace("--target all", "--target typescript");
        if scripts.get(&key).and_then(Json::as_str) == Some(legacy.as_str()) {
            scripts.insert(key, json!(command));
        } else {
            scripts.entry(key).or_insert(json!(command));
        }
    }
    if root.join("Cargo.toml").is_file() {
        scripts.entry("dev").or_insert(json!("forklaunch dev"));
    }
    if has_modules {
        let build = if bun {
            application_build_script(&Runtime::Bun)
        } else {
            "pnpm -r --if-present run build".to_owned()
        };
        if bun
            && scripts
                .get("build")
                .and_then(Json::as_str)
                .is_some_and(|script| {
                    matches!(
                        script,
                        "bun run --filter '*' --if-present build"
                            | "cargo build --workspace && bun run --filter '*' --if-present build"
                    )
                })
        {
            scripts.remove("build");
        }
        scripts
            .entry("build")
            .or_insert(json!(if root.join("Cargo.toml").is_file() {
                format!("cargo build --workspace && {build}")
            } else {
                build
            }));
    }
    if has_modules {
        // The TypeScript tooling (linter, formatter, tests) lives with the
        // modules; the root runs it there, plus the Cargo equivalent.
        let cargo = root.join("Cargo.toml").is_file();
        for (name, rust) in [
            ("lint", Some("cargo clippy --workspace --all-targets --no-deps")),
            ("lint:fix", None),
            ("format", Some("cargo fmt --all")),
            ("test", Some("cargo test --workspace")),
        ] {
            if !module_scripts.iter().any(|script| script == name) {
                continue;
            }
            let typescript = if bun {
                format!("bun run --cwd {modules_path} {name}")
            } else {
                format!("pnpm --dir {modules_path} run {name}")
            };
            let command = match rust.filter(|_| cargo) {
                Some(rust) => format!("{typescript} && {rust}"),
                None => typescript,
            };
            scripts.entry(name).or_insert(json!(command));
        }
    }
    add_packages(&mut workspace, &additions)?;
    // Parse and validate both files before publishing either update.
    // An existing root package.json keeps its key order and final newline.
    let package = match fs::read_to_string(&package_path) {
        Ok(original) => crate::core::package_json::preserve_json_layout(
            &original,
            &serde_json::to_string_pretty(&package)?,
        ),
        Err(_) => serde_json::to_string_pretty(&package)? + "\n",
    };
    let workspace = serde_yml::to_string(&workspace)?;
    outputs.push(("package.json".into(), package));
    outputs.push(("pnpm-workspace.yaml".into(), workspace));
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_workspace_registers_split_sdk_packages_idempotently() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "runtime='node'\nmodules_path='src'\n",
        )
        .unwrap();
        fs::write(root.path().join("src/package.json"), r#"{"private":true}"#).unwrap();
        let path = root.path().join("src/pnpm-workspace.yaml");
        fs::write(
            &path,
            "packages: ['core', 'client-sdk', '!private/*']\nminimumReleaseAge: 1440\n",
        )
        .unwrap();
        include_typescript_sdk(root.path()).unwrap();
        let first = fs::read_to_string(&path).unwrap();
        let workspace: Yaml = serde_yml::from_str(&first).unwrap();
        let packages = workspace["packages"].as_sequence().unwrap();
        for member in [
            "core",
            "client-sdk",
            "!private/*",
            "client-sdk/typescript",
            "client-sdk/events/typescript",
            "*/events/typescript",
        ] {
            assert!(
                packages.contains(&Yaml::String(member.into())),
                "missing {member}"
            );
        }
        assert_eq!(workspace["minimumReleaseAge"].as_u64(), Some(1440));
        include_typescript_sdk(root.path()).unwrap();
        assert_eq!(first, fs::read_to_string(path).unwrap());
    }

    #[test]
    fn workspace_scripts_honor_both_targets_and_preserve_custom_commands() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "runtime='node'\nmodules_path='src'\nsdkTargets=['typescript','rust']\n",
        )
        .unwrap();
        fs::write(root.path().join("package.json"), r#"{"scripts":{"sdk:generate":"forklaunch sdk generate --target typescript --from contract","sdk:watch":"custom watcher"}}"#).unwrap();
        include_typescript_sdk(root.path()).unwrap();
        let package: Json =
            serde_json::from_str(&fs::read_to_string(root.path().join("package.json")).unwrap())
                .unwrap();
        assert_eq!(
            package["scripts"]["sdk:generate"],
            "forklaunch sdk generate --target all --from contract"
        );
        assert_eq!(
            package["scripts"]["sdk:check"],
            "forklaunch sdk generate --target all --from contract --check"
        );
        assert_eq!(package["scripts"]["sdk:watch"], "custom watcher");
    }

    #[test]
    fn local_sdk_workspaces_follow_custom_modules_paths_and_repair_owned_scripts() {
        for runtime in ["node", "bun"] {
            for modules in ["modules", "src/modules"] {
                let root = tempfile::tempdir().unwrap();
                fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
                fs::create_dir_all(root.path().join(modules)).unwrap();
                let events = root
                    .path()
                    .join(modules)
                    .join("client-sdk/events/typescript");
                fs::create_dir_all(&events).unwrap();
                fs::write(events.join("package.json"), r#"{"private":true}"#).unwrap();
                fs::write(root.path().join(".forklaunch/manifest.toml"), format!(
                    "runtime='{runtime}'\nmodules_path='{modules}'\nprojects=[{{name='notes',metadata={{language='rust',sdkLayout='project-local-v1'}}}}]\n"
                )).unwrap();
                fs::write(
                    root.path().join(modules).join("package.json"),
                    r#"{"private":true,"workspaces":["*","!private/*"]}"#,
                )
                .unwrap();
                fs::write(
                    root.path().join(modules).join("pnpm-workspace.yaml"),
                    "packages: ['*', '!private/*']\nminimumReleaseAge: 1440\n",
                )
                .unwrap();
                let runner = if runtime == "bun" { "bun run" } else { "pnpm" };
                for script in [
                    format!(
                        "{runner} --filter '@forklaunch-generated/sdk' --filter './src/*/sdk/typescript' --filter '@forklaunch/universal-sdk' build"
                    ),
                    format!(
                        "{runner} --filter '@forklaunch-generated/*' --filter './{modules}/*/sdk/typescript' --filter '@forklaunch/universal-sdk' build"
                    ),
                    "custom-sdk-check".to_owned(),
                ] {
                    fs::write(
                        root.path().join("package.json"),
                        serde_json::to_string(&json!({"scripts":{"sdk:typecheck":script}}))
                            .unwrap(),
                    )
                    .unwrap();
                    include_typescript_sdk(root.path()).unwrap();
                    let first = fs::read_to_string(root.path().join("package.json")).unwrap();
                    let package: Json = serde_json::from_str(&first).unwrap();
                    let workspace: Yaml = serde_yml::from_str(
                        &fs::read_to_string(root.path().join("pnpm-workspace.yaml")).unwrap(),
                    )
                    .unwrap();
                    for suffix in [
                        "*/sdk/typescript",
                        "*/events/typescript",
                        "client-sdk/events/typescript",
                    ] {
                        let member = format!("{modules}/{suffix}");
                        assert!(
                            workspace["packages"]
                                .as_sequence()
                                .unwrap()
                                .contains(&Yaml::String(member.clone()))
                        );
                        if runtime == "bun" {
                            assert!(
                                package["workspaces"]
                                    .as_array()
                                    .unwrap()
                                    .contains(&json!(member))
                            );
                            let nested: Json = serde_json::from_str(
                                &fs::read_to_string(root.path().join(modules).join("package.json"))
                                    .unwrap(),
                            )
                            .unwrap();
                            assert!(
                                nested["workspaces"]
                                    .as_array()
                                    .unwrap()
                                    .contains(&json!(suffix))
                            );
                            assert!(
                                nested["workspaces"]
                                    .as_array()
                                    .unwrap()
                                    .contains(&json!("!private/*"))
                            );
                        } else {
                            let nested: Yaml = serde_yml::from_str(
                                &fs::read_to_string(
                                    root.path().join(modules).join("pnpm-workspace.yaml"),
                                )
                                .unwrap(),
                            )
                            .unwrap();
                            assert!(
                                nested["packages"]
                                    .as_sequence()
                                    .unwrap()
                                    .contains(&Yaml::String(suffix.into()))
                            );
                            assert_eq!(nested["minimumReleaseAge"].as_u64(), Some(1440));
                        }
                    }
                    let expected = if script == "custom-sdk-check" {
                        script
                    } else {
                        format!(
                            "{runner} --filter '@forklaunch-generated/*' --filter './{modules}/*/sdk/typescript' build"
                        )
                    };
                    assert_eq!(package["scripts"]["sdk:typecheck"], expected);
                    include_typescript_sdk(root.path()).unwrap();
                    assert_eq!(
                        fs::read_to_string(root.path().join("package.json")).unwrap(),
                        first
                    );
                }
            }
        }
    }

    #[test]
    fn bun_nested_workspace_preserves_authored_metadata_and_tracks_optional_sdk_packages() {
        for modules in ["src", "backend/domains"] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
            let directory = root.path().join(modules);
            fs::create_dir_all(directory.join("notes/sdk/typescript")).unwrap();
            fs::write(
                directory.join("notes/sdk/typescript/package.json"),
                r#"{"name":"@demo/notes"}"#,
            )
            .unwrap();
            fs::write(root.path().join(".forklaunch/manifest.toml"), format!(
                "app_name='demo'\nruntime='bun'\nmodules_path='{modules}'\nprojects=[{{name='notes',metadata={{language='rust',sdkLayout='project-local-v1'}}}}]\n"
            )).unwrap();
            let path = directory.join("package.json");
            let authored = json!({
                "name": "authored-workspace", "private": true,
                "workspaces": ["core", "tools/*", "!private/*", "user/missing", "client-sdk/typescript", "client-sdk/events/typescript"],
                "scripts": {"build": "authored build"},
                "overrides": {"typescript": "custom"},
                "custom": {"retained": true}
            });
            let original = serde_json::to_string(&authored).unwrap();
            fs::write(&path, &original).unwrap();
            fs::write(
                directory.join("bunfig.toml"),
                "[install]\nminimumReleaseAge=172800\n",
            )
            .unwrap();
            let plan = plan_typescript_sdk(root.path(), SdkWorkspace::Http).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
            assert!(
                plan.iter()
                    .any(|(path, _)| path == &format!("{modules}/package.json"))
            );
            include_typescript_sdk(root.path()).unwrap();
            let changed = fs::read_to_string(&path).unwrap();
            let nested: Json = serde_json::from_str(&changed).unwrap();
            for field in ["name", "private", "scripts", "overrides", "custom"] {
                assert_eq!(nested[field], authored[field]);
            }
            let members = nested["workspaces"].as_array().unwrap();
            for member in [
                "core",
                "tools/*",
                "!private/*",
                "user/missing",
                "*/sdk/typescript",
                "*/events/typescript",
            ] {
                assert!(members.contains(&json!(member)), "missing {member}");
            }
            assert!(!members.contains(&json!("client-sdk/events/typescript")));
            // Older custom module layouts keep their aggregate at sdks/typescript.
            // A different nested literal is authored, so preserve it even absent.
            assert_eq!(
                members.contains(&json!("client-sdk/typescript")),
                modules != "src"
            );
            assert_eq!(
                fs::read_to_string(directory.join("bunfig.toml")).unwrap(),
                "[install]\nminimumReleaseAge=172800\n"
            );
            include_typescript_sdk(root.path()).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), changed);
            let optional = directory.join("client-sdk/events/typescript");
            fs::create_dir_all(&optional).unwrap();
            fs::write(optional.join("package.json"), "{}").unwrap();
            include_typescript_sdk(root.path()).unwrap();
            let present: Json = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            assert!(
                present["workspaces"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("client-sdk/events/typescript"))
            );
            fs::remove_file(optional.join("package.json")).unwrap();
            include_typescript_sdk(root.path()).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), changed);
        }
    }

    #[test]
    fn native_sdk_tasks_cover_events_and_preserve_custom_commands() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "language='mixed'\nmodules_path='src'\n",
        )
        .unwrap();
        for command in [
            "pnpm --filter @forklaunch-generated/sdk build",
            "custom-sdk-check",
        ] {
            fs::write(
                root.path().join("package.json"),
                serde_json::to_string(&json!({"scripts":{"sdk:typecheck":command}})).unwrap(),
            )
            .unwrap();
            let outputs = plan_typescript_sdk(root.path(), SdkWorkspace::Http).unwrap();
            let package: Json = serde_json::from_str(
                &outputs
                    .iter()
                    .find(|(name, _)| *name == "package.json")
                    .unwrap()
                    .1,
            )
            .unwrap();
            let workspace = &outputs
                .iter()
                .find(|(name, _)| *name == "pnpm-workspace.yaml")
                .unwrap()
                .1;
            assert!(workspace.contains("src/client-sdk/events/typescript"));
            assert!(workspace.contains("src/*/events/typescript"));
            if command == "custom-sdk-check" {
                assert_eq!(package["scripts"]["sdk:typecheck"], command);
            } else {
                assert!(
                    package["scripts"]["sdk:typecheck"]
                        .as_str()
                        .unwrap()
                        .contains("@forklaunch-generated/*")
                );
            }
        }
    }

    fn fixture(runtime: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::create_dir_all(root.path().join("src/modules")).unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            format!("runtime = '{runtime}'\nmodules_path = 'src/modules'\n"),
        )
        .unwrap();
        fs::write(root.path().join("src/modules/package.json"), r#"{"devDependencies":{"typescript":"custom","tsx":"version"},"workspaces":["*","core","!excluded"]}"#).unwrap();
        fs::write(root.path().join("src/modules/pnpm-workspace.yaml"), "packages: ['*', '!excluded']\nminimumReleaseAge: 1440\nallowBuilds:\n  esbuild: true\n").unwrap();
        if runtime == "bun" {
            fs::create_dir_all(root.path().join("sdks/typescript")).unwrap();
            fs::write(
                root.path().join("sdks/typescript/package.json"),
                r#"{"name":"fixture-sdk","private":true}"#,
            )
            .unwrap();
        }
        root
    }

    #[test]
    fn bun_optional_generated_members_follow_real_packages_and_preserve_user_members() {
        for modules in ["src", "modules", "src/modules"] {
            let root = tempfile::tempdir().unwrap();
            fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
            fs::write(
                root.path().join(".forklaunch/manifest.toml"),
                format!("runtime='bun'\nmodules_path='{modules}'\n"),
            )
            .unwrap();
            let http = super::super::layout::typescript(root.path())
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let events = format!("{modules}/client-sdk/events/typescript");
            let package = root.path().join("package.json");
            fs::write(
                &package,
                serde_json::to_string(
                    &json!({"workspaces":[http, events, "user/missing", "tools/*"]}),
                )
                .unwrap(),
            )
            .unwrap();
            include_typescript_sdk(root.path()).unwrap();
            let members = || -> Vec<Json> {
                serde_json::from_str::<Json>(&fs::read_to_string(&package).unwrap()).unwrap()["workspaces"].as_array().unwrap().clone()
            };
            assert!(!members().contains(&json!(http)));
            assert!(!members().contains(&json!(events)));
            assert!(members().contains(&json!("user/missing")));
            assert!(members().contains(&json!("tools/*")));
            let first = fs::read_to_string(&package).unwrap();
            include_typescript_sdk(root.path()).unwrap();
            assert_eq!(fs::read_to_string(&package).unwrap(), first);
            for path in [&http, &events] {
                fs::create_dir_all(root.path().join(path)).unwrap();
                fs::write(
                    root.path().join(path).join("package.json"),
                    r#"{"private":true}"#,
                )
                .unwrap();
            }
            // Both optional packages are now real; HTTP and socket planning
            // preserve them while still retaining unrelated authored entries.
            include_typescript_sdk(root.path()).unwrap();
            assert!(members().contains(&json!(http)));
            // Non-native legacy layouts do not add events until explicitly
            // registered; keep the existing member for that compatibility case.
            if modules == "src" {
                assert!(members().contains(&json!(events)));
            }
            let outputs = plan_typescript_sdk(root.path(), SdkWorkspace::WebSocket).unwrap();
            let planned: Json = serde_json::from_str(
                &outputs
                    .iter()
                    .find(|(path, _)| path == "package.json")
                    .unwrap()
                    .1,
            )
            .unwrap();
            assert!(
                planned["workspaces"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(http))
            );
            fs::remove_file(root.path().join(&http).join("package.json")).unwrap();
            include_typescript_sdk(root.path()).unwrap();
            assert!(!members().contains(&json!(http)));
            assert!(members().contains(&json!("user/missing")));
        }
    }

    #[test]
    fn merges_inline_workspaces_and_preserves_custom_settings_idempotently() {
        let root = fixture("node");
        fs::write(root.path().join("package.json"), r#"{"devDependencies":{"typescript":"override"},"scripts":{"build":"custom build","sdk:watch":"custom watch"}}"#).unwrap();
        fs::write(
            root.path().join("pnpm-workspace.yaml"),
            "packages: ['tools/*']\nminimumReleaseAge: 2880\n",
        )
        .unwrap();
        include_typescript_sdk(root.path()).unwrap();
        let package: Json =
            serde_json::from_str(&fs::read_to_string(root.path().join("package.json")).unwrap())
                .unwrap();
        assert_eq!(package["devDependencies"]["typescript"], "override");
        assert_eq!(package["devDependencies"]["tsx"], "version");
        assert_eq!(package["scripts"]["build"], "custom build");
        assert_eq!(package["scripts"]["sdk:watch"], "custom watch");
        let first = fs::read_to_string(root.path().join("pnpm-workspace.yaml")).unwrap();
        let workspace: Yaml = serde_yml::from_str(&first).unwrap();
        for path in [
            "tools/*",
            "sdks/typescript",
            "src/modules/*",
            "!src/modules/excluded",
        ] {
            assert!(
                workspace["packages"]
                    .as_sequence()
                    .unwrap()
                    .contains(&Yaml::String(path.into()))
            );
        }
        assert_eq!(workspace["minimumReleaseAge"].as_u64(), Some(2880));
        assert_eq!(workspace["allowBuilds"]["esbuild"].as_bool(), Some(true));
        include_typescript_sdk(root.path()).unwrap();
        assert_eq!(
            fs::read_to_string(root.path().join("pnpm-workspace.yaml")).unwrap(),
            first
        );
    }

    #[test]
    fn bun_membership_and_commands_include_modules_and_sdk() {
        let root = fixture("bun");
        fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        include_typescript_sdk(root.path()).unwrap();
        let package: Json =
            serde_json::from_str(&fs::read_to_string(root.path().join("package.json")).unwrap())
                .unwrap();
        assert!(
            package["workspaces"]
                .as_array()
                .unwrap()
                .contains(&json!("src/modules/*"))
        );
        assert!(
            package["workspaces"]
                .as_array()
                .unwrap()
                .contains(&json!("sdks/typescript"))
        );
        assert_eq!(
            package["scripts"]["build"],
            "cargo build --workspace && bunx -y @forklaunch/bunrun build"
        );
        assert_eq!(package["name"], "forklaunch-sdk-workspace");
        assert!(
            package["scripts"]["sdk:typecheck"]
                .as_str()
                .unwrap()
                .starts_with("bun ")
        );
        assert!(package.get("packageManager").is_none());
        assert!(
            !package["workspaces"]
                .as_array()
                .unwrap()
                .contains(&json!("src/modules/core"))
        );
    }

    #[test]
    fn bun_build_repairs_known_parallel_scripts_and_preserves_custom_scripts() {
        for native in [false, true] {
            for previous in [
                "bun run --filter '*' --if-present build",
                "cargo build --workspace && bun run --filter '*' --if-present build",
                "custom-build",
            ] {
                let root = fixture("bun");
                if native {
                    fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
                }
                fs::write(
                    root.path().join("package.json"),
                    serde_json::to_string(&json!({
                        "name":"application-owned", "scripts":{"build":previous}
                    }))
                    .unwrap(),
                )
                .unwrap();
                include_typescript_sdk(root.path()).unwrap();
                let first = fs::read_to_string(root.path().join("package.json")).unwrap();
                let package: Json = serde_json::from_str(&first).unwrap();
                assert_eq!(package["name"], "application-owned");
                let expected = if previous == "custom-build" {
                    previous
                } else if native {
                    "cargo build --workspace && bunx -y @forklaunch/bunrun build"
                } else {
                    "bunx -y @forklaunch/bunrun build"
                };
                assert_eq!(package["scripts"]["build"], expected);
                include_typescript_sdk(root.path()).unwrap();
                assert_eq!(
                    fs::read_to_string(root.path().join("package.json")).unwrap(),
                    first
                );
            }
        }
    }

    #[test]
    fn socket_planning_is_read_only_and_coexists_with_http_generation() {
        let root = fixture("bun");
        let files = plan_typescript_sdk(root.path(), SdkWorkspace::WebSocket).unwrap();
        assert!(!root.path().join("package.json").exists());
        assert!(!root.path().join("pnpm-workspace.yaml").exists());
        for (path, content) in files {
            fs::write(root.path().join(path), content).unwrap();
        }
        include_typescript_sdk(root.path()).unwrap();
        let package: Json =
            serde_json::from_str(&fs::read_to_string(root.path().join("package.json")).unwrap())
                .unwrap();
        assert!(
            package["workspaces"]
                .as_array()
                .unwrap()
                .contains(&json!("sdks/typescript-websocket/*"))
        );
        assert!(
            package["workspaces"]
                .as_array()
                .unwrap()
                .contains(&json!("sdks/typescript"))
        );
        assert!(
            package["scripts"]["sdk:socket:generate"]
                .as_str()
                .unwrap()
                .ends_with("--from socket-contract")
        );
        assert!(
            package["scripts"]["sdk:generate"]
                .as_str()
                .unwrap()
                .ends_with("--from contract")
        );
        assert_eq!(
            package["scripts"]["sdk:socket:typecheck"],
            "bun run --filter '*-websocket-sdk' build"
        );
    }

    #[test]
    fn invalid_workspace_does_not_partially_rewrite_package() {
        let root = fixture("node");
        let original = "{\"private\":true}\n";
        fs::write(root.path().join("package.json"), original).unwrap();
        fs::write(
            root.path().join("pnpm-workspace.yaml"),
            "packages: invalid\n",
        )
        .unwrap();
        assert!(include_typescript_sdk(root.path()).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join("package.json")).unwrap(),
            original
        );
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "modules_path = '../outside'\n",
        )
        .unwrap();
        assert!(include_typescript_sdk(root.path()).is_err());
    }
}

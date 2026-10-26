//! Own SDK watch for exactly the lifetime of the development stack.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};

/// Resolve the same service-local Rust outputs that SDK generation owns.
/// Keep the aggregate layout for older projects and already-existing clients.
pub(super) fn rust_paths(root: &Path, manifest: &toml::Value) -> Result<Vec<PathBuf>> {
    let projects = manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut paths = BTreeSet::new();
    let mut legacy = projects.is_empty();
    for project in projects {
        if !matches!(
            project.get("type").and_then(toml::Value::as_str),
            Some("Service" | "Worker")
        ) {
            continue;
        }
        let metadata = project.get("metadata");
        if metadata
            .and_then(|metadata| {
                metadata
                    .get("contractExport")
                    .or_else(|| metadata.get("contract_export"))
            })
            .and_then(toml::Value::as_bool)
            == Some(false)
        {
            continue;
        }
        if metadata
            .and_then(|metadata| {
                metadata
                    .get("sdkLayout")
                    .or_else(|| metadata.get("sdk_layout"))
            })
            .and_then(toml::Value::as_str)
            == Some("project-local-v1")
        {
            let directory = metadata
                .and_then(|metadata| {
                    metadata
                        .get("cratePath")
                        .or_else(|| metadata.get("crate_path"))
                        .or_else(|| metadata.get("staticPath"))
                        .or_else(|| metadata.get("static_path"))
                })
                .and_then(toml::Value::as_str)
                .context("Service-local SDK requires project path metadata")?;
            paths.insert(crate::core::application_output::output_path(
                root,
                &Path::new(directory).join("sdk/rust"),
            )?);
        } else {
            legacy = true;
        }
    }
    let aggregate = crate::core::application_output::output_path(
        root,
        &crate::sdk::sdk_base(root)?.join("rust"),
    )?;
    if legacy || aggregate.is_dir() {
        paths.insert(aggregate);
    }
    Ok(paths.into_iter().collect())
}

/// Plan watches before the first Rust SDK exists; create directories only after dry-run.
pub(super) fn rust_watch(
    root: &Path,
    manifest: &toml::Value,
    target: Option<&str>,
    compose: &mut serde_json::Value,
    changes: &mut serde_json::Value,
) -> Result<Vec<PathBuf>> {
    if !matches!(target, Some("rust" | "all")) {
        return Ok(Vec::new());
    }
    let canonical_root = root.canonicalize()?;
    let root = canonical_root.as_path();
    let paths = rust_paths(root, manifest)?;
    let mut directories = BTreeSet::new();
    // With reloads the builder watches these paths; processes restart when
    // the build relinks their binary.
    let mut builder = false;
    for path in &paths {
        if super::watch_native_path(compose, root, path) {
            builder = true;
            directories.insert(path.clone());
        }
    }
    if builder {
        return Ok(directories.into_iter().collect());
    }
    for (name, change) in changes
        .as_object_mut()
        .context("Native development changes missing")?
    {
        let watches = change["develop"]["watch"]
            .as_array_mut()
            .context("Development watches missing")?;
        for path in &paths {
            if !watches
                .iter()
                .any(|watch| super::watch_covers(root, watch, path))
            {
                watches.push(super::restart_watch_rule(root, manifest, path)?);
                directories.insert(path.clone());
            }
        }
        compose["services"][name]["develop"]["watch"] = serde_json::json!(watches);
    }
    Ok(directories.into_iter().collect())
}

pub(super) fn command(root: &Path, target: &str, service: Option<&str>) -> Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["sdk", "watch", "--path"])
        .arg(root)
        .args(["--target", target]);
    if let Some(service) = service {
        command.args(["--service", service]);
    }
    command
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    Ok(command)
}

/// Development exports live OpenAPI before starting each TypeScript process.
/// Keep ordinary SDK watch's explicit compiled-contract semantics unchanged.
pub(super) fn source(command: &mut Command, surfaces: Option<&str>) {
    command.env_remove(crate::sdk::watch::DEV_HTTP_SOURCE);
    let source = match surfaces {
        Some("websocket") => "socket-contract",
        Some("all") => {
            command.env(crate::sdk::watch::DEV_HTTP_SOURCE, "openapi");
            "all"
        }
        _ => "openapi",
    };
    command.args(["--from", source]);
}

fn dependency_commands(root: &Path, manifest: &toml::Value, target: &str) -> Result<Vec<Command>> {
    let mut commands = Vec::new();
    if matches!(target, "rust" | "all") && root.join("Cargo.toml").is_file() {
        let mut cargo = Command::new(crate::core::openapi_export::resolve_command("cargo"));
        // Resolve newly generated workspace packages while preserving unrelated
        // locked dependencies. This also repairs a previously interrupted pass.
        cargo.args(["update", "--workspace"]).current_dir(root);
        commands.push(cargo);
    }
    if matches!(target, "typescript" | "all") {
        let modules = manifest
            .get("modules_path")
            .and_then(toml::Value::as_str)
            .unwrap_or(".");
        let modules = if modules == "." {
            root.to_owned()
        } else {
            crate::core::application_output::output_path(root, Path::new(modules))?
        };
        let workspace = if modules.join("package.json").is_file() {
            modules
        } else {
            root.to_owned()
        };
        if workspace.join("package.json").is_file() {
            let bun = manifest.get("runtime").and_then(toml::Value::as_str) == Some("bun");
            let mut install = Command::new(crate::core::openapi_export::resolve_command(if bun {
                "bun"
            } else {
                "pnpm"
            }));
            install.arg("install").current_dir(workspace);
            if !bun {
                // CI environments default to frozen installs; development must
                // first admit the SDK package it has just generated.
                install.arg("--no-frozen-lockfile");
            }
            commands.push(install);
        }
    }
    Ok(commands)
}

/// Complete the first SDK pass and its real dependency locks before Docker
/// snapshots the workspace. The production builders remain strictly locked.
pub(super) fn prepare(
    root: &Path,
    manifest: &toml::Value,
    target: &str,
    mut command: Command,
) -> Result<()> {
    command.arg("--once");
    anyhow::ensure!(
        command
            .status()
            .context("Unable to prepare development SDKs")?
            .success(),
        "Initial development SDK generation failed before image build"
    );
    for mut command in dependency_commands(root, manifest, target)? {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        anyhow::ensure!(
            command
                .status()
                .context("Unable to prepare development SDK dependencies")?
                .success(),
            "Development SDK dependency preparation failed before image build"
        );
    }
    Ok(())
}

pub(super) struct Watcher {
    child: Child,
    reported: bool,
}
impl Watcher {
    pub(super) fn spawn(mut command: Command) -> Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        Ok(Self {
            child: command
                .spawn()
                .context("Unable to start development SDK watch")?,
            reported: false,
        })
    }
    pub(super) fn report_exit(&mut self) {
        if !self.reported
            && let Ok(Some(status)) = self.child.try_wait()
        {
            eprintln!(
                "Development SDK watch exited with {status}; service development is still running"
            );
            self.reported = true;
        }
    }
}
impl Drop for Watcher {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(Some(_))) {
            return;
        }
        #[cfg(unix)]
        {
            use nix::{
                sys::signal::{Signal, killpg},
                unistd::Pid,
            };
            if let Ok(id) = i32::try_from(self.child.id())
                && id > 1
            {
                let _ = killpg(Pid::from_raw(id), Signal::SIGTERM);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn development_sdk_preparation_resolves_new_workspace_members_before_locked_builds() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        for name in ["existing", "generated"] {
            std::fs::create_dir_all(root.join(name).join("src")).unwrap();
            std::fs::write(
                root.join(name).join("Cargo.toml"),
                format!("[package]\nname='{name}'\nversion='0.1.0'\nedition='2024'\n"),
            )
            .unwrap();
            std::fs::write(root.join(name).join("src/lib.rs"), "").unwrap();
        }
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver='2'\nmembers=['existing']\nexclude=['generated']\n",
        )
        .unwrap();
        assert!(
            Command::new("cargo")
                .args(["generate-lockfile", "--offline"])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        let old_lock = std::fs::read(root.join("Cargo.lock")).unwrap();
        let manifest = toml::Value::Table(toml::Table::new());
        let mut failed = Command::new("sh");
        failed.args(["-c", "exit 9"]).current_dir(&root);
        assert!(prepare(&root, &manifest, "rust", failed).is_err());
        assert_eq!(std::fs::read(root.join("Cargo.lock")).unwrap(), old_lock);

        let mut initial_pass = Command::new("sh");
        initial_pass.args(["-c", "printf \"[workspace]\\nresolver='2'\\nmembers=['existing','generated']\\n\" > Cargo.toml"]).current_dir(&root);
        prepare(&root, &manifest, "rust", initial_pass).unwrap();
        let lock: toml::Value =
            toml::from_str(&std::fs::read_to_string(root.join("Cargo.lock")).unwrap()).unwrap();
        assert!(
            lock["package"]
                .as_array()
                .unwrap()
                .iter()
                .any(|package| package["name"].as_str() == Some("generated"))
        );
        assert!(
            Command::new("cargo")
                .args(["metadata", "--locked", "--offline", "--format-version", "1"])
                .current_dir(&root)
                .output()
                .unwrap()
                .status
                .success()
        );

        // Recovery must work even when the SDK package already exists from a
        // previous failed startup and this generation pass changes no files.
        std::fs::write(root.join("Cargo.lock"), old_lock).unwrap();
        let mut unchanged_pass = Command::new("sh");
        unchanged_pass.args(["-c", "exit 0"]);
        prepare(&root, &manifest, "rust", unchanged_pass).unwrap();
        assert!(
            Command::new("cargo")
                .args(["metadata", "--locked", "--offline", "--format-version", "1"])
                .current_dir(&root)
                .output()
                .unwrap()
                .status
                .success()
        );
    }

    #[test]
    fn development_sdk_installs_use_the_actual_bun_or_pnpm_workspace() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        std::fs::write(root.join("package.json"), "{}").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        let modules = root.join("backend/domains");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::write(modules.join("package.json"), "{}").unwrap();
        for runtime in ["node", "bun"] {
            let manifest: toml::Value = toml::from_str(&format!(
                "runtime='{runtime}'\nmodules_path='backend/domains'\n"
            ))
            .unwrap();
            let commands = dependency_commands(&root, &manifest, "all").unwrap();
            assert_eq!(commands.len(), 2);
            assert_eq!(
                commands[0].get_args().collect::<Vec<_>>(),
                ["update", "--workspace"]
            );
            let install = &commands[1];
            assert_eq!(install.get_current_dir(), Some(modules.as_path()));
            assert_eq!(
                Path::new(install.get_program()).file_name().unwrap(),
                if runtime == "bun" { "bun" } else { "pnpm" }
            );
            assert_eq!(
                install.get_args().collect::<Vec<_>>(),
                if runtime == "bun" {
                    vec!["install"]
                } else {
                    vec!["install", "--no-frozen-lockfile"]
                }
            );
            assert_eq!(
                dependency_commands(&root, &manifest, "rust").unwrap().len(),
                1
            );
        }
        std::fs::remove_file(modules.join("package.json")).unwrap();
        let manifest: toml::Value = toml::from_str("modules_path='backend/domains'\n").unwrap();
        assert_eq!(
            dependency_commands(&root, &manifest, "typescript").unwrap()[0].get_current_dir(),
            Some(root.as_path())
        );
    }

    #[test]
    fn development_sdk_prepared_socket_directory_gains_a_watch_without_replacing_authored_rules() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let manifest: toml::Value = toml::from_str("modules_path='src'\n[[projects]]\nname='native'\ntype='Service'\n[projects.metadata]\nsdkLayout='project-local-v1'\ncratePath='src/native'\n").unwrap();
        std::fs::create_dir_all(root.join(".forklaunch")).unwrap();
        std::fs::write(
            root.join(".forklaunch/manifest.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let original = serde_json::json!([
            {"action":"sync","path":"assets","target":"/app/assets","ignore":["private/"]},
            {"action":"restart","path":"src/native","ignore":["target/"]}
        ]);
        let mut changes = serde_json::json!({"native":{"develop":{"watch":original}}});
        let mut compose = serde_json::json!({"services":changes});
        assert!(
            rust_watch(&root, &manifest, Some("all"), &mut compose, &mut changes)
                .unwrap()
                .is_empty()
        );
        let aggregate = root.join("src/client-sdk/rust");
        assert!(!aggregate.exists());
        std::fs::create_dir_all(aggregate.join("native-websocket-sdk")).unwrap();
        std::fs::write(
            aggregate.join("native-websocket-sdk/socket.rs"),
            "pub struct Message;\n",
        )
        .unwrap();
        assert_eq!(
            rust_watch(&root, &manifest, Some("all"), &mut compose, &mut changes).unwrap(),
            [aggregate.clone()]
        );
        let watches = changes["native"]["develop"]["watch"].as_array().unwrap();
        assert_eq!(&watches[..2], original.as_array().unwrap().as_slice());
        assert_eq!(watches[2]["path"], serde_json::json!(aggregate));
        assert_eq!(watches[2]["action"], "restart");
        assert_eq!(
            compose["services"]["native"]["develop"],
            changes["native"]["develop"]
        );
        assert!(
            rust_watch(&root, &manifest, Some("all"), &mut compose, &mut changes)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            changes["native"]["develop"]["watch"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn development_sdk_commands_use_live_openapi_for_http_and_combined_surfaces() {
        for (surfaces, expected, combined) in [
            (None, "openapi", false),
            (Some("http"), "openapi", false),
            (Some("all"), "all", true),
            (Some("websocket"), "socket-contract", false),
        ] {
            let mut process = command(Path::new("/application"), "all", Some("notes")).unwrap();
            process.env(
                crate::sdk::watch::DEV_HTTP_SOURCE,
                "stale inherited setting",
            );
            source(&mut process, surfaces);
            let arguments: Vec<_> = process
                .get_args()
                .map(|value| value.to_string_lossy())
                .collect();
            assert_eq!(&arguments[arguments.len() - 2..], ["--from", expected]);
            assert_eq!(
                process
                    .get_envs()
                    .find(|(key, _)| *key == crate::sdk::watch::DEV_HTTP_SOURCE)
                    .unwrap()
                    .1
                    .and_then(|value| value.to_str()),
                combined.then_some("openapi")
            );
        }
    }

    #[test]
    fn rust_sdk_watch_is_planned_before_directory_creation() {
        let root = tempfile::tempdir().unwrap();
        let manifest = toml::Value::Table(toml::Table::new());
        let mut changes = serde_json::json!({"api":{"develop":{"watch":[]}}});
        let mut compose = serde_json::json!({"services":{"api":{"develop":{"watch":[]}}}});
        let paths = rust_watch(
            root.path(),
            &manifest,
            Some("all"),
            &mut compose,
            &mut changes,
        )
        .unwrap();
        let path = &paths[0];
        assert!(!path.exists());
        assert_eq!(
            compose["services"]["api"]["develop"]["watch"][0]["path"],
            serde_json::json!(path)
        );
        assert!(
            rust_watch(
                root.path(),
                &manifest,
                Some("rust"),
                &mut compose,
                &mut changes
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(
            changes["api"]["develop"]["watch"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn local_rust_sdk_watches_follow_registered_paths_without_replacing_authored_rules() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let mut manifest: toml::Value = toml::from_str(
            r#"
            modules_path = "modules"
            [[projects]]
            name = "native"
            type = "Service"
            [projects.metadata]
            sdkLayout = "project-local-v1"
            cratePath = "services/native"
            [[projects]]
            name = "legacy"
            type = "Service"
            [projects.metadata]
            sdkLayout = "project-local-v1"
            staticPath = "modules/legacy"
            [[projects]]
            name = "consumer"
            type = "Worker"
            [projects.metadata]
            contractExport = false
        "#,
        )
        .unwrap();
        std::fs::create_dir_all(root.join(".forklaunch")).unwrap();
        std::fs::write(
            root.join(".forklaunch/manifest.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let authored = serde_json::json!({"action":"sync","path":root.join("assets"),"target":"/app/assets","ignore":["private/"]});
        let base = serde_json::json!({"api":{"develop":{"watch":[
            {"action":"restart","path":"services/native","ignore":["target/"]}, authored
        ]}}});
        let mut changes = base.clone();
        let mut compose = serde_json::json!({"services":changes});
        let paths = rust_watch(&root, &manifest, Some("all"), &mut compose, &mut changes).unwrap();
        let local = root.join("modules/legacy/sdk/rust");
        assert_eq!(paths, vec![local.clone()]);
        assert!(!local.exists());
        let watches = changes["api"]["develop"]["watch"].as_array().unwrap();
        assert_eq!(watches.len(), 3);
        assert_eq!(watches[0], base["api"]["develop"]["watch"][0]);
        assert_eq!(watches[1], authored);
        assert_eq!(watches[2]["path"], serde_json::json!(local));
        assert_eq!(
            compose["services"]["api"]["develop"],
            changes["api"]["develop"]
        );
        assert!(
            rust_watch(&root, &manifest, Some("all"), &mut compose, &mut changes)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            changes["api"]["develop"]["watch"].as_array().unwrap().len(),
            3
        );
        assert!(!root.join("sdks").exists());

        // Removing a registered project stops planning its SDK watch, while
        // existing client source and application-authored watch rules survive.
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(local.join("models.rs"), "// application-retained client\n").unwrap();
        manifest["projects"].as_array_mut().unwrap().remove(1);
        let mut changes = base.clone();
        assert!(
            rust_watch(&root, &manifest, Some("all"), &mut compose, &mut changes)
                .unwrap()
                .is_empty()
        );
        assert_eq!(changes, base);
        assert!(local.join("models.rs").is_file());
        assert!(!rust_paths(&root, &manifest).unwrap().contains(&local));

        manifest["projects"][0]["metadata"]["cratePath"] = toml::Value::String("../outside".into());
        assert!(rust_paths(&root, &manifest).is_err());
    }

    #[test]
    fn development_sdk_watcher_is_reaped_on_scope_exit() {
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 60"]);
        let watcher = Watcher::spawn(command).unwrap();
        let pid = nix::unistd::Pid::from_raw(watcher.child.id() as i32);
        drop(watcher);
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH)
        );
    }
}

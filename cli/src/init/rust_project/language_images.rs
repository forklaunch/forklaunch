//! Owned shared Rust image inputs for mixed workspaces.
use std::collections::{BTreeSet, VecDeque};

use super::*;
use crate::core::forklaunch_state;

pub(super) const RDS_CA_BUNDLE_URL: &str = "https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem";
pub(super) const RDS_CA_BUNDLE_PATH: &str = "/app/certs/rds-global-bundle.pem";

fn contained(root: &Path, path: &Path) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    anyhow::ensure!(
        path.starts_with(root),
        "Docker input escapes application: {}",
        path.display()
    );
    Ok(path)
}

fn workspace(root: &Path, directory: &Path) -> Result<(PathBuf, Value)> {
    for ancestor in directory
        .ancestors()
        .take_while(|path| path.starts_with(root))
    {
        let manifest = ancestor.join("Cargo.toml");
        if manifest.is_file() {
            let value: Value = toml::from_str(&read_to_string(&manifest)?)?;
            if value.get("workspace").is_some() {
                return Ok((ancestor.to_owned(), value));
            }
        }
    }
    bail!("Cargo workspace missing for {}", directory.display())
}

fn dependencies(value: &Value) -> Vec<(&str, &Value)> {
    let mut result = Vec::new();
    for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
        if let Some(entries) = value.get(section).and_then(Value::as_table) {
            result.extend(entries.iter().map(|(name, value)| (name.as_str(), value)));
        }
    }
    if let Some(targets) = value.get("target").and_then(Value::as_table) {
        for target in targets.values() {
            result.extend(dependencies(target));
        }
    }
    result
}

/// A local framework checkout (`[framework] path`) reaches the image through
/// named build contexts, so its crates are provided rather than copied from
/// the application. Anything else outside the application is refused.
fn provided(external: &[PathBuf], path: &Path) -> bool {
    path.canonicalize().is_ok_and(|path| external.iter().any(|context| path.starts_with(context)))
}

/// Dockerfile lines for a local framework checkout: empty named stages that
/// docker-compose.yaml overrides with build contexts, and the COPY lines that
/// recreate the checkout at its host path (workspace manifests and crates/
/// only, never target/). Both empty for published crates.
pub(super) fn framework_inputs(root: &Path) -> Result<(String, String)> {
    let Some((_, contexts)) = super::local_runtime::framework_contexts(root) else {
        return Ok((String::new(), String::new()));
    };
    let mut stages = String::new();
    let mut copies = String::new();
    for (name, path) in contexts {
        stages.push_str(&format!("FROM scratch AS {name}\n"));
        let path = path.to_string_lossy();
        copies.push_str(&format!(
            "COPY --from={name} {}\nCOPY --from={name} {}\n",
            serde_json::to_string(&vec!["Cargo.tom[l]".to_owned(), "Cargo.loc[k]".to_owned(), format!("{path}/")])?,
            serde_json::to_string(&vec!["crates".to_owned(), format!("{path}/crates")])?,
        ));
    }
    Ok((stages, copies))
}

pub(super) fn plan(root: &Path) -> Result<(String, Vec<String>, Vec<String>)> {
    let framework = super::local_runtime::framework_contexts(root);
    let external: Vec<PathBuf> = framework
        .iter()
        .flat_map(|(_, contexts)| contexts.iter().filter_map(|(_, path)| path.canonicalize().ok()))
        .collect();
    let root = root.canonicalize()?;
    let cargo: Value = toml::from_str(&read_to_string(root.join("Cargo.toml"))?)?;
    let members = cargo
        .get("workspace")
        .and_then(|value| value.get("members"))
        .and_then(Value::as_array)
        .context("Explicit Cargo workspace members required")?;
    let mut pending = VecDeque::new();
    let mut runnable = BTreeSet::new();
    for member in members {
        let pattern = root.join(member.as_str().context("Cargo member must be a path")?);
        let mut found = false;
        for directory in glob::glob(pattern.to_str().context("UTF-8 Cargo path required")?)? {
            found = true;
            let directory = contained(&root, &directory?)?;
            anyhow::ensure!(
                directory != root,
                "A root Cargo package needs explicit scoped Docker inputs"
            );
            let manifest: Value = toml::from_str(&read_to_string(directory.join("Cargo.toml"))?)?;
            let mut package_bins = BTreeSet::new();
            if let Some(binaries) = manifest.get("bin").and_then(Value::as_array) {
                for binary in binaries {
                    package_bins.insert(
                        binary
                            .get("name")
                            .and_then(Value::as_str)
                            .context("Explicit binary name required")?
                            .to_owned(),
                    );
                }
            }
            if manifest
                .get("package")
                .and_then(|p| p.get("autobins"))
                .and_then(Value::as_bool)
                != Some(false)
            {
                if directory.join("src/main.rs").is_file() {
                    package_bins.insert(
                        manifest["package"]["name"]
                            .as_str()
                            .context("Package name missing")?
                            .to_owned(),
                    );
                }
                let bin_dir = directory.join("src/bin");
                if bin_dir.is_dir() {
                    for entry in std::fs::read_dir(bin_dir)? {
                        let path = entry?.path();
                        let name = if path.is_file()
                            && path.extension().is_some_and(|extension| extension == "rs")
                        {
                            path.file_stem()
                        } else if path.is_dir() && path.join("main.rs").is_file() {
                            path.file_name()
                        } else {
                            None
                        };
                        if let Some(name) = name {
                            package_bins.insert(
                                name.to_str()
                                    .context("UTF-8 binary name required")?
                                    .to_owned(),
                            );
                        }
                    }
                }
            }
            for binary in package_bins {
                anyhow::ensure!(
                    runnable.insert(binary.clone()),
                    "Duplicate workspace binary name: {binary}"
                );
            }
            pending.push_back(directory);
        }
        anyhow::ensure!(
            found,
            "Cargo member pattern has no packages: {}",
            pattern.display()
        );
    }
    // Registry patches are source inputs, including the protocol-compatible parser.
    if let Some(patches) = cargo.get("patch").and_then(Value::as_table) {
        for registry in patches.values().filter_map(Value::as_table) {
            for dependency in registry.values() {
                if let Some(path) = dependency.get("path").and_then(Value::as_str) {
                    if provided(&external, &root.join(path)) {
                        continue;
                    }
                    pending.push_back(contained(&root, &root.join(path))?);
                }
            }
        }
    }
    let mut directories = BTreeSet::new();
    let mut provider_workspaces = BTreeSet::new();
    let mut files = BTreeSet::from([
        "Cargo.toml".to_owned(),
        "Cargo.lock".to_owned(),
        ".forklaunch/manifest.toml".to_owned(),
    ]);
    // Workspace Cargo settings (for example SQLX_OFFLINE) apply to image builds too.
    if root.join(".cargo/config.toml").is_file() {
        files.insert(".cargo/config.toml".to_owned());
    }
    while let Some(directory) = pending.pop_front() {
        if !directories.insert(directory.clone()) {
            continue;
        }
        let manifest: Value = toml::from_str(&read_to_string(directory.join("Cargo.toml"))?)?;
        let (workspace_root, workspace_manifest) = workspace(&root, &directory)?;
        if workspace_root != root {
            provider_workspaces.insert(workspace_root.clone());
        }
        files.insert(
            workspace_root
                .join("Cargo.toml")
                .strip_prefix(&root)?
                .to_string_lossy()
                .into_owned(),
        );
        for (name, dependency) in dependencies(&manifest) {
            let (base, dependency) =
                if dependency.get("workspace").and_then(Value::as_bool) == Some(true) {
                    (
                        &workspace_root,
                        workspace_manifest
                            .get("workspace")
                            .and_then(|value| value.get("dependencies"))
                            .and_then(|value| value.get(name))
                            .context("Inherited Cargo dependency missing")?,
                    )
                } else {
                    (&directory, dependency)
                };
            if let Some(path) = dependency.get("path").and_then(Value::as_str) {
                if provided(&external, &base.join(path)) {
                    continue;
                }
                pending.push_back(contained(&root, &base.join(path))?);
            }
        }
    }
    directories.extend(provider_workspaces);
    // Socket SDK generation can add Cargo members after these images are
    // planned (including the first development watch). Keep the existing
    // managed SDK parent as a stable, scoped input so that both language
    // builders receive those later members with the updated Cargo.toml.
    // Optional roots stay optional; never create packages to satisfy COPY.
    let sdk_root = root.join(crate::sdk::sdk_base(&root)?);
    if sdk_root.exists() {
        anyhow::ensure!(sdk_root.is_dir(), "Generated SDK Docker input must be a directory");
        let canonical = contained(&root, &sdk_root)?;
        anyhow::ensure!(canonical == sdk_root, "Generated SDK Docker input must not be a symbolic alias");
        directories.insert(canonical);
    }
    let mut inputs: Vec<String> = files.into_iter().collect();
    // A copied package already contains its nested source packages and metadata.
    for directory in &directories {
        if !directories
            .iter()
            .any(|other| other != directory && directory.starts_with(other))
        {
            inputs.push(format!(
                "{}/",
                directory.strip_prefix(&root)?.to_string_lossy()
            ));
        }
    }
    inputs.retain(|input| {
        input.ends_with('/')
            || !directories
                .iter()
                .any(|directory| root.join(input).starts_with(directory))
    });
    inputs.sort();
    inputs.dedup();
    for binary in &runnable {
        anyhow::ensure!(
            !binary.is_empty()
                && binary
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "Unsupported Docker binary name"
        );
    }
    let (stages, copies) = framework_inputs(&root)?;
    let mut docker = "# Generated shared Rust image. Run cargo generate-lockfile before the first image build.\n".to_owned();
    docker.push_str(&stages);
    docker.push_str("FROM rust:1.98.0-slim-bookworm AS builder\nWORKDIR /app\nRUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev libcurl4-openssl-dev ca-certificates cmake make g++ perl && rm -rf /var/lib/apt/lists/*\n");
    docker.push_str(&copies);
    for input in &inputs {
        docker.push_str(&format!(
            "COPY {}\n",
            serde_json::to_string(&vec![input.clone(), format!("/app/{input}")])?
        ));
    }
    docker.push_str("ENV SQLX_OFFLINE=true\nARG CARGO_BUILD_JOBS=2\nRUN cargo build --locked --release --workspace --bins");
    docker.push_str("\nFROM debian:bookworm-slim AS runtime\nWORKDIR /app\nRUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl && rm -rf /var/lib/apt/lists/*\n");
    // Deployed PostgreSQL URLs verify the server against the AWS RDS CA bundle.
    docker.push_str(&format!("ADD --chmod=0644 {RDS_CA_BUNDLE_URL} {RDS_CA_BUNDLE_PATH}\n"));
    for binary in &runnable {
        docker.push_str(&format!(
            "COPY --from=builder /app/target/release/{binary} /app/bin/{binary}\n"
        ));
    }
    docker.push_str(
        "ENV HOST=0.0.0.0 PROTOCOL=http PORT=8000 VERSION=v1 DOCS_PATH=/docs\nUSER 65532:65532\n",
    );
    // Process selection belongs to explicit Compose commands; no arbitrary default.

    Ok((docker, inputs, runnable.into_iter().collect()))
}

/// The generated Rust image of this application: its image name and binary
/// inventory, and whether `Dockerfile.rust` is unchanged since generation.
pub(crate) fn summary(root: &Path) -> Option<(String, Vec<String>, bool)> {
    let source = read_to_string(root.join("Dockerfile.rust")).ok()?;
    let unchanged = forklaunch_state::stamped(&source)?;
    // The runtime stage copies exactly the binaries the image can run.
    let binaries = source
        .lines()
        .filter_map(|line| line.strip_prefix("COPY --from=builder /app/target/release/")?.split_once(' '))
        .map(|(binary, _)| binary.to_owned())
        .collect();
    Some((format!("{}-rust:local", rust_app_name(root).ok()?), binaries, unchanged))
}

pub(super) fn preflight(root: &Path) -> Result<()> {
    super::typescript_image::preflight(root)?;
    if let Some(unchanged) = forklaunch_state::file_stamped(&root.join("Dockerfile.rust")) {
        anyhow::ensure!(
            unchanged,
            "Shared Rust Dockerfile was modified; reconcile it before changing projects"
        );
        let expected = render_static_rust_template("rust/application/dockerignore");
        anyhow::ensure!(
            read_to_string(root.join("Dockerfile.rust.dockerignore"))?.trim_end()
                == expected.trim_end(),
            "Shared Rust Dockerfile ignore rules were modified; reconcile them before changing projects"
        );
    }
    Ok(())
}

pub(super) fn reconcile(root: &Path) -> Result<()> {
    // Workspace Cargo settings are an image input; create them before planning.
    super::local_runtime::ensure_cargo_config(root)?;
    super::local_runtime::prune_stale_migrations(root)?;
    reconcile_images(root)?;
    super::auth_env::compose(root)?;
    super::local_runtime::reconcile(root)?;
    super::auth_env::local_envs(root)?;
    refresh_client_compliance(root)
}

/// The TypeScript client SDK's compliance aggregate lists every database
/// service; native renames and deletes keep it current like TypeScript ones.
fn refresh_client_compliance(root: &Path) -> Result<()> {
    let Ok(source) = read_to_string(root.join(".forklaunch/manifest.toml")) else { return Ok(()) };
    let Ok(manifest) = toml::from_str::<crate::core::manifest::application::ApplicationManifestData>(&source) else {
        return Ok(());
    };
    let modules = root.join(&manifest.modules_path);
    if !modules.join("client-sdk/compliance.ts").is_file() {
        return Ok(());
    }
    let mut cache = crate::core::rendered_template::RenderedTemplatesCache::new();
    crate::core::client_sdk::regenerate_client_sdk_compliance(&mut cache, &modules, &manifest.projects)?;
    for (_, template) in cache.drain() {
        let content = crate::core::watermark::apply_watermark(&template)?;
        if read_to_string(&template.path).ok().as_deref() != Some(content.as_str()) {
            write(&template.path, content)?;
        }
    }
    Ok(())
}

fn reconcile_images(root: &Path) -> Result<()> {
    crate::core::github_configs::reconcile_github_configs(root)?;
    super::typescript_image::reconcile(root)?;
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    if manifest.get("language").and_then(Value::as_str) != Some("mixed") {
        return Ok(());
    }
    let destination = root.join("Dockerfile.rust");
    let actual = read_to_string(&destination).ok();
    let template = render_static_rust_template("rust/application/Dockerfile");
    if let Some(unchanged) = forklaunch_state::file_stamped(&destination) {
        anyhow::ensure!(
            unchanged,
            "Shared Rust Dockerfile was modified; reconcile the custom image before changing generated image inputs"
        );
    } else if actual
        .as_deref()
        .is_some_and(|source| source.trim_end() != template.trim_end())
    {
        // An existing custom image is authoritative; never adopt it by heuristic.
        return Ok(());
    }
    let (docker, _, binaries) = plan(root)?;
    let compose_path = root.join(
        manifest
            .get("docker_compose_path")
            .and_then(Value::as_str)
            .unwrap_or("docker-compose.yaml"),
    );
    let mut compose = read_or_create_rust_docker_compose(&compose_path)?;
    let image = format!("{}-rust:local", rust_app_name(root)?);
    let builds_rust = |service: &DockerService| {
        service.build.as_ref().is_some_and(|build| {
            matches!(
                build.dockerfile.as_str(),
                "Dockerfile.rust" | "./Dockerfile.rust"
            )
        })
    };
    // The image the builder currently tags: after an application rename it
    // still carries the previous name, as do the processes that share it.
    let built: Vec<String> = compose
        .services
        .values()
        .filter(|service| builds_rust(service))
        .filter_map(|service| service.image.clone())
        .collect();
    let mut processes = Vec::new();
    for (name, service) in &compose.services {
        let uses_rust = builds_rust(service)
            || service
                .image
                .as_ref()
                .is_some_and(|current| current == &image || built.contains(current));
        if !uses_rust {
            continue;
        }
        let Some(DockerCommand::Multiple(command)) = &service.command else {
            bail!("Shared Rust images require an explicit exec-form command for {name}");
        };
        let first = command.first().context("Empty Rust process command")?;
        let binary = first.rsplit('/').next().unwrap_or(first);
        anyhow::ensure!(
            binaries.iter().any(|entry| entry == binary),
            "Compose command is absent from the shared Rust binary inventory: {binary}"
        );
        processes.push(name.clone());
    }
    processes.sort();
    let context = pathdiff::diff_paths(
        root.canonicalize()?,
        compose_path.parent().unwrap().canonicalize()?,
    )
    .context("Compose context path")?;
    let context = if context.as_os_str().is_empty() {
        ".".into()
    } else {
        context.to_string_lossy().into_owned()
    };
    for (index, name) in processes.iter().enumerate() {
        let service = compose.services.get_mut(name).unwrap();
        let Some(DockerCommand::Multiple(command)) = &mut service.command else {
            unreachable!()
        };
        let binary = command[0].rsplit('/').next().unwrap().to_owned();
        command[0] = format!("/app/bin/{binary}");
        service.image = Some(image.clone());
        service.build = if index == 0 {
            Some(DockerBuild {
                context: context.clone(),
                dockerfile: "Dockerfile.rust".into(),
                ..Default::default()
            })
        } else {
            None
        };
    }
    let ignore_path = root.join("Dockerfile.rust.dockerignore");
    let ignore = render_static_rust_template("rust/application/dockerignore");
    if ignore_path.exists() {
        anyhow::ensure!(
            read_to_string(&ignore_path)?.trim_end() == ignore.trim_end(),
            "Custom Rust Dockerfile ignore rules require explicit review before generated image inputs change"
        );
    }
    write(destination, forklaunch_state::stamp(&docker))?;
    write(ignore_path, ignore)?;
    write(compose_path, yaml_to_string(&compose)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(
            root.path().join(".forklaunch/manifest.toml"),
            "language='mixed'\napp_name='demo'\n",
        )
        .unwrap();
        write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers=['src/api','src/jobs']\n",
        )
        .unwrap();
        for (name, bin) in [("api", "api"), ("jobs", "jobs-worker")] {
            let dir = root.path().join("src").join(name);
            create_dir_all(&dir).unwrap();
            write(dir.join("Cargo.toml"),format!("[package]\nname='{name}'\nversion='0.1.0'\n[[bin]]\nname='{bin}'\npath='main.rs'\n")).unwrap();
            write(dir.join("main.rs"), "fn main(){}\n").unwrap();
        }
        create_dir_all(root.path().join("src/web")).unwrap();
        write(root.path().join("src/web/package.json"), "{}").unwrap();
        write(
            root.path().join("Dockerfile.rust"),
            render_static_rust_template("rust/application/Dockerfile"),
        )
        .unwrap();
        write(root.path().join("docker-compose.yaml"),"services:\n  api:\n    build: {context: ., dockerfile: Dockerfile.rust}\n    command: ['./target/release/api']\n  jobs:\n    build: {context: ., dockerfile: Dockerfile.rust}\n    command: ['./target/release/jobs-worker']\n").unwrap();
        root
    }
    #[test]
    fn shared_image_has_scoped_inputs_locked_build_and_distinct_processes() {
        let root = fixture();
        reconcile(root.path()).unwrap();
        let docker = read_to_string(root.path().join("Dockerfile.rust")).unwrap();
        assert!(!docker.contains("COPY . ."));
        assert!(!docker.contains("src/web"));
        assert!(docker.contains("cargo build --locked --release --workspace --bins"));
        assert!(docker.contains("/app/bin/api"));
        assert!(docker.contains("/app/bin/jobs-worker"));
        let compose =
            read_or_create_rust_docker_compose(&root.path().join("docker-compose.yaml")).unwrap();
        assert_eq!(
            compose
                .services
                .values()
                .filter(|service| service.build.is_some())
                .count(),
            1
        );
        for service in compose.services.values() {
            assert_eq!(service.image.as_deref(), Some("demo-rust:local"));
        }
        reconcile(root.path()).unwrap();
        assert_eq!(
            read_to_string(root.path().join("Dockerfile.rust")).unwrap(),
            docker
        );
        let edited = docker.replace("FROM rust", "FROM custom-rust");
        write(root.path().join("Dockerfile.rust"), &edited).unwrap();
        assert!(reconcile(root.path()).is_err());
        assert_eq!(
            read_to_string(root.path().join("Dockerfile.rust")).unwrap(),
            edited
        );
    }
    #[test]
    fn local_framework_checkout_is_a_named_build_context_not_an_escape() {
        let root = fixture();
        let checkout = tempfile::tempdir().unwrap();
        let framework = checkout.path().join("framework-rs");
        create_dir_all(framework.join("crates/core")).unwrap();
        write(framework.join("Cargo.toml"), "[workspace]\nmembers=['crates/core']\n").unwrap();
        write(framework.join("crates/core/Cargo.toml"), "[package]\nname='forklaunch-core'\nversion='0.2.0'\n").unwrap();
        write(root.path().join(".forklaunch/manifest.toml"), format!("language='mixed'\napp_name='demo'\n[framework]\npath='{}'\n", framework.display())).unwrap();
        write(root.path().join("src/api/Cargo.toml"), format!("[package]\nname='api'\nversion='0.1.0'\n[[bin]]\nname='api'\npath='main.rs'\n[dependencies]\nforklaunch-core={{path='{}'}}\n", framework.join("crates/core").display())).unwrap();
        let (docker, inputs, _) = plan(root.path()).unwrap();
        assert!(docker.contains("FROM scratch AS framework\n"), "{docker}");
        assert!(docker.contains(&format!("COPY --from=framework [\"crates\",\"{}/crates\"]", framework.display())), "{docker}");
        assert!(!inputs.iter().any(|input| input.contains("framework-rs")));
        super::super::local_runtime::reconcile(root.path()).unwrap();
        let compose = read_or_create_rust_docker_compose(&root.path().join("docker-compose.yaml")).unwrap();
        let build = compose.services["api"].build.as_ref().unwrap();
        assert_eq!(build.additional_properties["additional_contexts"]["framework"].as_str(), Some(framework.to_str().unwrap()));
    }

    #[test]
    fn application_rename_retags_every_process_of_the_shared_image() {
        let root = fixture();
        // Before the rename: the builder and a process share the old tag.
        write(root.path().join("docker-compose.yaml"), "services:\n  api:\n    image: old-rust:local\n    build: {context: ., dockerfile: Dockerfile.rust}\n    command: ['/app/bin/api']\n  jobs:\n    image: old-rust:local\n    command: ['/app/bin/jobs-worker']\n  cache:\n    image: redis:7\n").unwrap();
        reconcile_images(root.path()).unwrap();
        let compose =
            read_or_create_rust_docker_compose(&root.path().join("docker-compose.yaml")).unwrap();
        for name in ["api", "jobs"] {
            assert_eq!(compose.services[name].image.as_deref(), Some("demo-rust:local"), "{name}");
        }
        assert_eq!(compose.services["cache"].image.as_deref(), Some("redis:7"));
    }

    #[test]
    fn discovers_automatic_bins_and_rejects_cross_package_collisions() {
        let root = fixture();
        let bin = root.path().join("src/api/src/bin");
        create_dir_all(&bin).unwrap();
        write(bin.join("admin.rs"), "fn main() {}\n").unwrap();
        create_dir_all(bin.join("replay")).unwrap();
        write(bin.join("replay/main.rs"), "fn main() {}\n").unwrap();
        let binaries = plan(root.path()).unwrap().2;
        assert_eq!(binaries, ["admin", "api", "jobs-worker", "replay"]);
        let other = root.path().join("src/jobs/src/bin");
        create_dir_all(&other).unwrap();
        write(other.join("admin.rs"), "fn main() {}\n").unwrap();
        assert!(
            plan(root.path())
                .unwrap_err()
                .to_string()
                .contains("Duplicate workspace binary")
        );
    }

    #[test]
    fn sdk_inputs_preserve_optional_roots_and_reject_missing_or_external_members() {
        for modules in ["src", "backend/domains"] {
            let root = fixture();
            let manifest = root.path().join(".forklaunch/manifest.toml");
            write(&manifest, format!("{}modules_path='{modules}'\n", read_to_string(&manifest).unwrap())).unwrap();
            let relative = crate::sdk::sdk_base(root.path()).unwrap();
            let directory = root.path().join(&relative);
            let input = format!("{}/", relative.display());
            assert!(!plan(root.path()).unwrap().1.contains(&input));
            assert!(!directory.exists(), "planning must not create optional SDK roots");
            create_dir_all(&directory).unwrap();
            assert!(plan(root.path()).unwrap().1.contains(&input));

            // A stable parent must not hide missing declared Cargo packages.
            let cargo_path = root.path().join("Cargo.toml");
            let original = read_to_string(&cargo_path).unwrap();
            let mut cargo: Value = toml::from_str(&original).unwrap();
            cargo["workspace"]["members"].as_array_mut().unwrap().push(
                Value::String(format!("{}/rust/missing-websocket-sdk", relative.display())));
            write(&cargo_path, toml::to_string(&cargo).unwrap()).unwrap();
            assert!(plan(root.path()).unwrap_err().to_string().contains("Cargo member pattern has no packages"));
            write(cargo_path, original).unwrap();

            #[cfg(unix)]
            {
                remove_dir_all(&directory).unwrap();
                let external = tempfile::tempdir().unwrap();
                std::os::unix::fs::symlink(external.path(), &directory).unwrap();
                assert!(plan(root.path()).unwrap_err().to_string().contains("escapes application"));
                remove_file(&directory).unwrap();
                std::os::unix::fs::symlink(root.path(), &directory).unwrap();
                assert!(plan(root.path()).unwrap_err().to_string().contains("symbolic alias"));
            }
        }
    }

    #[test]
    fn path_dependency_closure_is_included_and_external_sources_fail() {
        let root = fixture();
        let dependency = root.path().join("shared/model");
        create_dir_all(&dependency).unwrap();
        write(
            dependency.join("Cargo.toml"),
            "[package]\nname='model'\nversion='0.1.0'\n",
        )
        .unwrap();
        let manifest = root.path().join("src/api/Cargo.toml");
        let original = read_to_string(&manifest).unwrap();
        write(
            &manifest,
            format!("{original}\n[dependencies]\nmodel={{path='../../shared/model'}}\n"),
        )
        .unwrap();
        assert!(
            plan(root.path())
                .unwrap()
                .1
                .contains(&"shared/model/".to_owned())
        );
        let outside = tempfile::tempdir().unwrap();
        write(
            outside.path().join("Cargo.toml"),
            "[package]\nname='outside'\nversion='0.1.0'\n",
        )
        .unwrap();
        write(
            &manifest,
            format!(
                "{original}\n[dependencies]\noutside={{path={:?}}}\n",
                outside.path().to_str().unwrap()
            ),
        )
        .unwrap();
        assert!(
            plan(root.path())
                .unwrap_err()
                .to_string()
                .contains("escapes")
        );
    }
}

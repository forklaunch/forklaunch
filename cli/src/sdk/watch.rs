use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    thread::sleep,
    time::Duration,
};

use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use sha2::{Digest, Sha256};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{
        artifact_paths::{self, Artifact},
        command::command,
    },
    init::rust_project,
    sdk::generate::generate_sdk_outputs_while_watching,
};

#[derive(Debug)]
pub(crate) struct WatchCommand;

/// Internal child-process setting used only by `fl dev`'s combined watcher.
pub(crate) const DEV_HTTP_SOURCE: &str = "FORKLAUNCH_DEV_SDK_HTTP_SOURCE";

fn development_source<'a>(source: &'a str, override_source: Option<&str>) -> Result<&'a str> {
    if source != "all" {
        return Ok(source);
    }
    match override_source {
        None => Ok(source),
        Some("openapi") => Ok("all-openapi"),
        Some(_) => anyhow::bail!("Invalid internal development HTTP SDK source"),
    }
}

impl WatchCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for WatchCommand {
    fn command(&self) -> Command {
        command("watch", "Regenerate SDKs when service artifacts change")
            .arg(
                Arg::new("base_path")
                    .long("path")
                    .short('p')
                    .help("Path to application root"),
            )
            .arg(
                Arg::new("target")
                    .long("target")
                    .short('t')
                    .default_value("typescript")
                    .value_parser(["typescript", "rust", "all"])
                    .help("SDK target to generate"),
            )
            .arg(
                Arg::new("service")
                    .long("service")
                    .short('s')
                    .help("Generate SDKs for one service"),
            )
            .arg(
                Arg::new("from")
                    .long("from")
                    .default_value("contract")
                    .value_parser(["openapi", "contract", "socket-contract", "asyncapi", "all"])
                    .help("Artifact source (all watches HTTP contracts and socket contracts)"),
            )
            .arg(
                Arg::new("interval_ms")
                    .long("interval-ms")
                    .default_value("1000")
                    .value_parser(clap::value_parser!(u64).range(100..))
                    .help("Polling interval in milliseconds"),
            )
            .arg(
                Arg::new("once")
                    .long("once")
                    .action(ArgAction::SetTrue)
                    .help("Run one regeneration pass and exit"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let (app_root, manifest) = crate::core::validate::require_manifest(matches)?;

        if !rust_project::is_rust_app_root(&app_root)? {
            anyhow::bail!("SDK watch is currently implemented for Rust and mixed Rust apps");
        }

        let target = matches.get_one::<String>("target").unwrap().as_str();
        let source = matches.get_one::<String>("from").unwrap().as_str();
        let dev_source = std::env::var(DEV_HTTP_SOURCE).ok();
        let source = development_source(source, dev_source.as_deref())?;
        let selected_service = matches.get_one::<String>("service").map(String::as_str);
        if let Some(service) = selected_service {
            anyhow::ensure!(
                manifest
                    .projects
                    .iter()
                    .any(|project| project.name == service
                        && matches!(
                            project.r#type,
                            crate::core::manifest::ProjectType::Service
                                | crate::core::manifest::ProjectType::Worker
                        )),
                "Unknown SDK service: {service}"
            );
        }
        let interval = Duration::from_millis(*matches.get_one::<u64>("interval_ms").unwrap());
        let once = matches.get_flag("once");

        run_sdk_watch(
            &app_root,
            target,
            source,
            selected_service,
            interval,
            once,
            &mut stdout,
        )
    }
}

fn run_sdk_watch(
    app_root: &Path,
    target: &str,
    source: &str,
    selected_service: Option<&str>,
    interval: Duration,
    once: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    if once {
        generate_sdk_outputs_while_watching(app_root, target, source, selected_service, stdout)?;
        log_ok!(stdout, "SDK watch pass completed");
        return Ok(());
    }

    // Capture before generation so an export arriving during it is processed next.
    let mut last_fingerprint = artifact_fingerprint(app_root, source, selected_service)?;
    if let Err(error) =
        generate_sdk_outputs_while_watching(app_root, target, source, selected_service, stdout)
    {
        log_error!(
            stdout,
            "SDK regeneration failed; waiting for artifact changes: {error:#}"
        );
    }
    log_info!(
        stdout,
        "Watching {source} artifacts every {}ms",
        interval.as_millis()
    );

    let mut fingerprint_error = None;
    loop {
        sleep(interval);
        let fingerprint = match artifact_fingerprint(app_root, source, selected_service) {
            Ok(fingerprint) => {
                fingerprint_error = None;
                fingerprint
            }
            Err(error) => {
                let message = format!("{error:#}");
                if fingerprint_error.as_ref() != Some(&message) {
                    log_error!(
                        stdout,
                        "Unable to read SDK artifacts; waiting for changes: {message}"
                    );
                    fingerprint_error = Some(message);
                }
                continue;
            }
        };
        if fingerprint == last_fingerprint {
            continue;
        }

        let root = app_root.canonicalize().unwrap_or_else(|_| app_root.to_path_buf());
        let changed: Vec<String> = fingerprint
            .iter()
            .filter(|(path, digest)| last_fingerprint.get(*path) != Some(*digest))
            .map(|(path, _)| path)
            .chain(last_fingerprint.keys().filter(|path| !fingerprint.contains_key(*path)))
            .map(|path| path.strip_prefix(&root).unwrap_or(path).display().to_string())
            .collect();
        last_fingerprint = fingerprint;
        log_info!(stdout, "{source} artifacts changed ({}); regenerating SDKs", changed.join(", "));
        if let Err(error) =
            generate_sdk_outputs_while_watching(app_root, target, source, selected_service, stdout)
        {
            log_error!(
                stdout,
                "SDK regeneration failed; waiting for artifact changes: {error:#}"
            );
        }
    }
}

fn artifact_digest(path: &Path) -> Result<[u8; 32]> {
    let mut input = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            return Ok(digest.finalize().into());
        }
        digest.update(&buffer[..count]);
    }
}

fn artifact_fingerprint(
    app_root: &Path,
    source: &str,
    selected_service: Option<&str>,
) -> Result<BTreeMap<PathBuf, [u8; 32]>> {
    if matches!(source, "all" | "all-openapi") {
        let http_source = if source == "all-openapi" {
            "openapi"
        } else {
            "contract"
        };
        let mut fingerprint = artifact_fingerprint(app_root, http_source, selected_service)?;
        fingerprint.extend(socket_artifact_fingerprint(
            app_root,
            selected_service,
            "socket-contract",
        )?);
        return Ok(fingerprint);
    }
    if matches!(source, "socket-contract" | "asyncapi") {
        return socket_artifact_fingerprint(app_root, selected_service, source);
    }
    let root = app_root.canonicalize()?;
    let mut fingerprint = BTreeMap::new();
    for path in super::http_sources::watched_paths(&root, source, selected_service)? {
        if !path.exists() {
            continue;
        }
        let canonical = path.canonicalize()?;
        anyhow::ensure!(
            canonical.starts_with(&root),
            "SDK artifact path must stay inside the application"
        );
        let digest = artifact_digest(&canonical)?;
        fingerprint.insert(canonical, digest);
    }
    Ok(fingerprint)
}

fn socket_artifact_fingerprint(
    app_root: &Path,
    selected: Option<&str>,
    source: &str,
) -> Result<BTreeMap<PathBuf, [u8; 32]>> {
    let root = app_root.canonicalize()?;
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(&manifest_path)?)?;
    let mut paths = vec![manifest_path];
    for project in manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        if !matches!(
            project.get("type").and_then(toml::Value::as_str),
            Some("Service" | "Worker")
        ) {
            continue;
        }
        let Some(name) = project.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        if selected.is_some_and(|selected| selected != name) {
            continue;
        }
        let metadata = project.get("metadata").and_then(toml::Value::as_table);
        let configured = metadata
            .and_then(|metadata| {
                metadata.get(if source == "asyncapi" {
                    "asyncapiArtifactPath"
                } else {
                    "socketContractArtifactPath"
                })
            })
            .and_then(toml::Value::as_str);
        paths.push(root.join(configured.map_or_else(
            || {
                artifact_paths::path(
                    name,
                    metadata,
                    if source == "asyncapi" {
                        Artifact::AsyncApi
                    } else {
                        Artifact::SocketContract
                    },
                )
            },
            str::to_owned,
        )));
        if source == "asyncapi" {
            continue;
        }
        paths.push(root.join(artifact_paths::socket_registry_marker(name, metadata)));
        let contract = artifact_paths::path(name, metadata, Artifact::Contract);
        crate::core::application_output::output_path(&root, Path::new(&contract))?;
        paths.push(root.join(contract));
    }
    let mut fingerprint = BTreeMap::new();
    for path in paths {
        if !path.exists() {
            continue;
        }
        let canonical = path.canonicalize()?;
        anyhow::ensure!(
            canonical.starts_with(&root),
            "Socket artifact path must stay inside the application"
        );
        let digest = artifact_digest(&canonical)?;
        fingerprint.insert(canonical, digest);
    }
    Ok(fingerprint)
}

#[cfg(test)]
mod socket_tests {
    use super::*;
    #[test]
    fn fingerprints_content_despite_timestamp_changes_or_collisions() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join(".forklaunch")).unwrap();
        fs::write(
            root.join(".forklaunch/manifest.toml"),
            r#"[[projects]]
name = "api"
type = "Service"
[projects.metadata]
language = "rust"
"#,
        )
        .unwrap();
        for (source, relative) in [
            ("contract", ".forklaunch/contracts/api/contract.json"),
            ("all", ".forklaunch/contracts/api/contract.json"),
            (
                "all",
                ".forklaunch/socket-contracts/api/socket-contract.json",
            ),
            (
                "socket-contract",
                ".forklaunch/socket-contracts/api/socket-contract.json",
            ),
            (
                "socket-contract",
                ".forklaunch/socket-contracts/api/registry-export.json",
            ),
            ("asyncapi", ".forklaunch/asyncapi/api/asyncapi.json"),
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "aa").unwrap();
            let original = artifact_fingerprint(root, source, Some("api")).unwrap();
            let timestamp = std::time::UNIX_EPOCH + Duration::from_secs(123456789);
            let times = fs::FileTimes::new().set_modified(timestamp);
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(times)
                .unwrap();
            assert_eq!(
                original,
                artifact_fingerprint(root, source, Some("api")).unwrap()
            );
            fs::write(&path, "bb").unwrap();
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_times(times)
                .unwrap();
            assert_ne!(
                original,
                artifact_fingerprint(root, source, Some("api")).unwrap()
            );
        }
    }

    #[test]
    fn tracks_asyncapi_inputs_without_generated_contract_outputs() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join(".forklaunch/contracts/chat")).unwrap();
        fs::create_dir_all(root.join("custom")).unwrap();
        fs::write(
            root.join(".forklaunch/manifest.toml"),
            r#"[[projects]]
name = "chat"
type = "Service"
[projects.metadata]
asyncapiArtifactPath = "custom/server.json"
"#,
        )
        .unwrap();
        fs::write(root.join("custom/server.json"), "{}").unwrap();
        let initial = artifact_fingerprint(root, "asyncapi", Some("chat")).unwrap();
        assert_eq!(initial.len(), 2);
        fs::write(
            root.join(".forklaunch/contracts/chat/contract.json"),
            "changed",
        )
        .unwrap();
        assert_eq!(
            initial,
            artifact_fingerprint(root, "asyncapi", Some("chat")).unwrap()
        );
        fs::write(root.join("custom/server.json"), "changed input").unwrap();
        assert_ne!(
            initial,
            artifact_fingerprint(root, "asyncapi", Some("chat")).unwrap()
        );
    }

    #[test]
    fn tracks_configured_socket_artifacts_and_service_selection() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join(".forklaunch")).unwrap();
        fs::create_dir_all(root.join("custom")).unwrap();
        fs::write(
            root.join(".forklaunch/manifest.toml"),
            r#"[[projects]]
name = "chat"
type = "Service"
[projects.metadata]
socketContractArtifactPath = "custom/chat.json"
[[projects]]
name = "other"
type = "Service"
[projects.metadata]
socketContractArtifactPath = "custom/other.json"
"#,
        )
        .unwrap();
        fs::write(root.join("custom/chat.json"), "{}").unwrap();
        fs::write(root.join("custom/other.json"), "{}").unwrap();
        let initial = artifact_fingerprint(root, "socket-contract", Some("chat")).unwrap();
        assert_eq!(initial.len(), 2);
        fs::write(root.join("custom/other.json"), "changed other").unwrap();
        assert_eq!(
            initial,
            artifact_fingerprint(root, "socket-contract", Some("chat")).unwrap()
        );
        fs::write(root.join("custom/chat.json"), "changed chat").unwrap();
        assert_ne!(
            initial,
            artifact_fingerprint(root, "socket-contract", Some("chat")).unwrap()
        );
        assert_eq!(
            artifact_fingerprint(root, "socket-contract", None)
                .unwrap()
                .len(),
            3
        );
        fs::create_dir_all(root.join(".forklaunch/contracts/chat")).unwrap();
        fs::write(
            root.join(".forklaunch/contracts/chat/contract.json"),
            "empty registry",
        )
        .unwrap();
        assert_eq!(
            artifact_fingerprint(root, "socket-contract", Some("chat"))
                .unwrap()
                .len(),
            3
        );
    }
}

#[cfg(test)]
mod mixed_http_tests {
    use super::*;

    #[test]
    fn development_combined_watch_uses_live_openapi_and_preserves_contract_defaults() {
        assert_eq!(development_source("all", None).unwrap(), "all");
        assert_eq!(
            development_source("all", Some("openapi")).unwrap(),
            "all-openapi"
        );
        assert!(development_source("all", Some("invalid")).is_err());
        for source in ["contract", "openapi", "socket-contract", "asyncapi"] {
            assert_eq!(development_source(source, Some("openapi")).unwrap(), source);
            assert_eq!(development_source(source, Some("invalid")).unwrap(), source);
        }
        assert!(
            WatchCommand::new()
                .command()
                .version("0.0.0")
                .try_get_matches_from(["watch", "--from", "all-openapi"])
                .is_err()
        );
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join(".forklaunch")).unwrap();
        fs::create_dir_all(root.join("src/api/sdk")).unwrap();
        fs::write(
            root.join(".forklaunch/manifest.toml"),
            r#"language = "mixed"
modules_path = "src"
app_name = "fixture"
[[projects]]
name = "api"
type = "Service"
[projects.metadata]
language = "typescript"
staticPath = "src/api"
sdkLayout = "project-local-v1"
contractArtifactPath = "src/api/sdk/contract.json"
openapiPath = "src/api/sdk/openapi.json"
"#,
        )
        .unwrap();
        let manifest_source = fs::read_to_string(root.join(".forklaunch/manifest.toml")).unwrap();
        fs::write(root.join(".forklaunch/manifest.toml"), format!("{manifest_source}\n[framework]\nversion = \"0.1.2\"\nblueprint_version = \"0.1.0\"\n")).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers=[]\n[workspace.package]\nversion='0.1.0'\nedition='2024'\nlicense='MIT'\n").unwrap();
        let contract = root.join("src/api/sdk/contract.json");
        let openapi = root.join("src/api/sdk/openapi.json");
        fs::write(
            &contract,
            "stale compiled contract must not be consumed by live development",
        )
        .unwrap();
        let document = |property: &str| {
            serde_json::json!({"openapi":"3.1.0","info":{"title":"api","version":"1"},
            "paths":{"/api":{"get":{"operationId":"getApi","responses":{"200":{"description":"ok", "content":{"application/json":{"schema":{
                "type":"object","properties":{property:{"type":"boolean"}},"required":[property]}}}}}}}}})
        };
        fs::write(&openapi, document("firstMarker").to_string()).unwrap();
        let public = artifact_fingerprint(root, "all", None).unwrap();
        let development = artifact_fingerprint(root, "all-openapi", None).unwrap();
        assert!(!public.contains_key(&openapi.canonicalize().unwrap()));
        assert!(development.contains_key(&openapi.canonicalize().unwrap()));
        let mut stdout = StandardStream::stdout(ColorChoice::Never);
        crate::sdk::generate::generate_or_check_sdk_outputs(root, "rust", "all-openapi", None, false, &mut stdout)
            .unwrap();
        let models = root.join("src/api/sdk/rust/models.rs");
        assert!(
            fs::read_to_string(&models)
                .unwrap()
                .contains("first_marker")
        );
        fs::write(&openapi, document("secondMarker").to_string()).unwrap();
        assert_ne!(
            artifact_fingerprint(root, "all-openapi", None).unwrap(),
            development
        );
        assert_eq!(artifact_fingerprint(root, "all", None).unwrap(), public);
        crate::sdk::generate::generate_or_check_sdk_outputs(root, "rust", "all-openapi", None, false, &mut stdout)
            .unwrap();
        assert!(
            fs::read_to_string(&models)
                .unwrap()
                .contains("second_marker")
        );
        assert!(
            fs::read_to_string(&contract)
                .unwrap()
                .contains("stale compiled contract")
        );
        crate::sdk::generate::generate_or_check_sdk_outputs(root, "rust", "all-openapi", None, true, &mut stdout)
            .unwrap();
    }

    #[test]
    fn contract_watch_tracks_custom_typescript_exports_and_selected_services() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path();
        fs::create_dir_all(root.join(".forklaunch/contracts/native-api")).unwrap();
        fs::create_dir_all(root.join("artifacts/typescript")).unwrap();
        fs::write(
            root.join(".forklaunch/manifest.toml"),
            r#"
[[projects]]
name = "native-api"
type = "Service"
[projects.metadata]
language = "rust"
[[projects]]
name = "typescript-api"
type = "Service"
[projects.metadata]
openapiPath = "artifacts/typescript/spec.json"
[[projects]]
name = "minio"
type = "Service"
"#,
        )
        .unwrap();
        fs::write(
            root.join(".forklaunch/contracts/native-api/contract.json"),
            "{}",
        )
        .unwrap();
        let spec = root.join("artifacts/typescript/spec.json");
        fs::write(&spec, "{}").unwrap();
        let initial = artifact_fingerprint(root, "contract", None).unwrap();
        assert_eq!(initial.len(), 3);
        assert!(initial.contains_key(&spec.canonicalize().unwrap()));
        let native = artifact_fingerprint(root, "contract", Some("native-api")).unwrap();
        assert_eq!(native.len(), 2);
        fs::write(&spec, "{\"changed\":true}").unwrap();
        assert_ne!(
            artifact_fingerprint(root, "contract", None).unwrap(),
            initial
        );
        assert_eq!(
            artifact_fingerprint(root, "contract", Some("native-api")).unwrap(),
            native
        );
        fs::remove_file(&spec).unwrap();
        assert_eq!(
            artifact_fingerprint(root, "contract", None).unwrap().len(),
            2
        );
        fs::write(&spec, "{\"restored\":true}").unwrap();
        assert_eq!(
            artifact_fingerprint(root, "contract", None).unwrap().len(),
            3
        );
    }
}

//! Run mixed applications with native rebuild/restart and existing TS dev commands.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command as Process, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use convert_case::{Case, Casing};
use serde_json::{Value, json};

use crate::{
    CliCommand,
    core::{application_output::output_path, command::command, env::find_workspace_root},
};

mod contracts;
mod sdk;
mod sockets;
mod typescript;

pub(crate) struct DevCommand;

/// One-shot service that compiles every selected native binary once before
/// any native process starts.
const NATIVE_BUILD: &str = "forklaunch-native-build";

/// Every native process compiles with the same scrubbed environment: Cargo
/// fingerprints include environment a build script or macro reads, so service
/// settings (PORT, DATABASE_URL, ...) would otherwise make each container
/// rebuild what the warm-up and the other containers just built.
///
/// Development binaries are never shipped, so they link with lld and keep
/// line tables only: full debug info makes 400 MB binaries that GNU ld takes
/// most of an edit-to-ready cycle to link. Backtraces keep file and line.
const NATIVE_BUILD_SCRIPT: &str = r#"COPY --chmod=0755 <<'SCRIPT' /usr/local/bin/forklaunch-native-build
#!/bin/sh
exec env -i HOME=/root PATH="$PATH" CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" \
  CARGO_TARGET_DIR=/app/target SQLX_OFFLINE=true \
  CARGO_PROFILE_DEV_DEBUG=line-tables-only RUSTFLAGS="-C link-arg=-fuse-ld=lld" \
  ${CARGO_BUILD_JOBS:+CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS"} ${CARGO_INCREMENTAL:+CARGO_INCREMENTAL="$CARGO_INCREMENTAL"} \
  cargo build "$@"
SCRIPT
"#;

/// Pinned file watcher for in-container reloads: a static musl build, verified
/// against the release's published SHA-256 and cached in its own image layer.
const WATCHEXEC_VERSION: &str = "2.7.4";
const WATCHEXEC_SHA256: [(&str, &str); 2] = [
    ("x86_64", "00a4be01add74acfaa37978081b331d60f4691f7e39b2028e5301242d61a464b"),
    ("aarch64", "8596e3082e7116d4438f71f3e8eb1ad25d39dc4fe8560d2b1240af86e0c56df8"),
];

/// `forklaunch-native-rebuild`: the reload builder's command. It builds the
/// full selection (the same Cargo invocation as the warm-up, so features
/// resolve identically), then stamps exactly the binaries the build relinked.
/// A failed build stamps nothing: every service keeps its last good binary.
const NATIVE_REBUILD_SCRIPT: &str = r#"COPY --chmod=0755 <<'SCRIPT' /usr/local/bin/forklaunch-native-rebuild
#!/bin/sh
started=$(date +%s.%N)
if ! forklaunch-native-build "$@"; then
  echo "forklaunch dev: build failed; services keep running their last good build" >&2
  exit 1
fi
mkdir -p /app/target/forklaunch-dev
changed=""
previous=""
for argument in "$@"; do
  if [ "$previous" = --bin ]; then
    stamp="/app/target/forklaunch-dev/$argument.ready"
    if [ ! -e "$stamp" ] || [ -n "$(find "/app/target/debug/$argument" -newer "$stamp")" ]; then
      date > "$stamp.tmp" && mv -f "$stamp.tmp" "$stamp"
      changed="$changed $argument"
    fi
  fi
  previous=$argument
done
touch /tmp/forklaunch-native-ready
elapsed=$(awk "BEGIN { printf \"%.1f\", $(date +%s.%N) - $started }")
echo "forklaunch dev: built in ${elapsed}s; restarting:${changed:- nothing (no binary changed)}"
SCRIPT
"#;

/// `forklaunch-native-run [--watch] BINARY [ARGS...]`: a native service's
/// entrypoint. It waits for the binary's first build, runs
/// FORKLAUNCH_DEV_PRESTART (the contract export, when configured), then the
/// binary. With --watch it stays up and restarts only the process, in place,
/// each time the builder relinks that binary.
const NATIVE_RUN_SCRIPT: &str = r#"COPY --chmod=0755 <<'SCRIPT' /usr/local/bin/forklaunch-native-run
#!/bin/sh
mode=""
case "$1" in --watch|--supervise) mode=$1; shift ;; esac
name=$(basename "$1")
until [ -e "/app/target/forklaunch-dev/$name.ready" ]; do sleep 1; done
if [ "$mode" = --watch ]; then
  exec watchexec --shell=none --restart --stop-signal=SIGTERM --stop-timeout=5s --debounce=100ms \
    --no-discover-ignore --project-origin=/app/target/forklaunch-dev \
    -W /app/target/forklaunch-dev -f "$name.ready" -- forklaunch-native-run --supervise "$@"
fi
start() {
  if [ -n "${FORKLAUNCH_DEV_PRESTART:-}" ]; then eval "$FORKLAUNCH_DEV_PRESTART" || return; fi
  "$@"
}
if [ "$mode" = --supervise ]; then
  # A crash restarts the process (as a restart policy would restart the
  # container); a relink restarts it at once through watchexec.
  delay=1
  while :; do
    started=$(date +%s)
    start "$@"
    status=$?
    [ "$status" -eq 0 ] && exit 0
    # Back off while it keeps failing at start (up to 30s); an edit that
    # relinks it restarts it at once.
    if [ $(( $(date +%s) - started )) -gt 30 ]; then delay=1; else delay=$(( delay * 2 > 30 ? 30 : delay * 2 )); fi
    echo "forklaunch dev: $name exited with status $status; restarting in ${delay}s" >&2
    sleep "$delay"
  done
fi
if [ -n "${FORKLAUNCH_DEV_PRESTART:-}" ]; then eval "$FORKLAUNCH_DEV_PRESTART" || exit; fi
exec "$@"
SCRIPT
"#;

/// Dockerfile lines installing the pinned watcher for the image's architecture.
fn watchexec_install() -> String {
    let mut cases = String::new();
    for (arch, sha) in WATCHEXEC_SHA256 {
        cases.push_str(&format!("{arch}) sha={sha} ;; "));
    }
    format!(
        "RUN arch=$(uname -m) && case \"$arch\" in {cases}*) echo \"No pinned watchexec for $arch\" >&2; exit 1 ;; esac \\\n\
    && name=watchexec-{WATCHEXEC_VERSION}-$arch-unknown-linux-musl \\\n\
    && curl -fsSL -o /tmp/watchexec.tar.xz https://github.com/watchexec/watchexec/releases/download/v{WATCHEXEC_VERSION}/$name.tar.xz \\\n\
    && echo \"$sha  /tmp/watchexec.tar.xz\" | sha256sum -c - \\\n\
    && tar -xJf /tmp/watchexec.tar.xz -C /usr/local/bin --strip-components=1 $name/watchexec \\\n\
    && rm /tmp/watchexec.tar.xz && watchexec --version\n"
    )
}

/// How native processes reload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Reload {
    /// `--no-watch`: build once, start each process once.
    Off,
    /// A long-lived builder container rebuilds on source changes; processes
    /// restart in place. `poll_ms` polls the mounted source instead of using
    /// inotify, which host file changes do not reach on macOS/Windows VMs.
    Watch { poll_ms: Option<u64> },
}

impl Reload {
    fn from_environment(no_watch: bool) -> Result<Self> {
        if no_watch {
            return Ok(Reload::Off);
        }
        // Docker Desktop and OrbStack bind mounts do not reliably deliver host
        // edits as inotify events inside containers; Linux hosts do.
        let default = if cfg!(target_os = "linux") { None } else { Some(500) };
        let poll_ms = match std::env::var("FORKLAUNCH_DEV_POLL_MS") {
            Ok(value) if value == "0" || value.eq_ignore_ascii_case("off") => None,
            Ok(value) => Some(
                value
                    .parse::<u64>()
                    .context("FORKLAUNCH_DEV_POLL_MS must be milliseconds, 0 or off")?,
            ),
            Err(_) => default,
        };
        Ok(Reload::Watch { poll_ms })
    }
}

impl CliCommand for DevCommand {
    fn command(&self) -> Command {
        command(
            "dev",
            "Run TypeScript and Rust services with local development reloads",
        )
        .arg(Arg::new("base_path").long("path").short('p'))
        .arg(
            Arg::new("services")
                .num_args(0..)
                .help("Compose services to start; defaults to the application"),
        )
        .arg(
            Arg::new("compose_file")
                .long("compose-file")
                .short('f')
                .action(ArgAction::Append)
                .help("Additional Compose overrides, resolved from the application root"),
        )
        .arg(Arg::new("project_name").long("project-name"))
        .arg(
            Arg::new("sdk_target")
                .long("sdk-target")
                .value_parser(["typescript", "rust", "all"])
                .help("Run SDK watch with development; implies --export-contracts"),
        )
        .arg(
            Arg::new("sdk_source")
                .long("sdk-source")
                .value_parser(["http", "websocket", "all"])
                .requires("sdk_target")
                .help("SDK surfaces to watch (default: http)"),
        )
        .arg(
            Arg::new("sdk_service")
                .long("sdk-service")
                .requires("sdk_target")
                .help("Limit SDK generation to one registered service"),
        )
        .arg(
            Arg::new("export_contracts")
                .long("export-contracts")
                .action(ArgAction::SetTrue)
                .help(
                    "Export native contracts and TypeScript OpenAPI before startup and each reload",
                ),
        )
        .arg(
            Arg::new("no_watch")
                .long("no-watch")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no_deps")
                .long("no-deps")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no_build")
                .long("no-build")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .visible_alias("dry-run")
                .action(ArgAction::SetTrue)
                .help("Print development changes without starting containers or writing app files"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let start = matches
            .get_one::<String>("base_path")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir()?);
        let root = find_workspace_root(&start)?;
        let base = [
            "docker-compose.yaml",
            "docker-compose.yml",
            "compose.yaml",
            "compose.yml",
        ]
        .into_iter()
        .map(|name| root.join(name))
        .find(|path| path.is_file())
        .context("Application Compose file missing")?;
        let docker = crate::core::openapi_export::resolve_command("docker");
        let mut config = Process::new(&docker);
        config
            .args(["compose", "--project-directory"])
            .arg(&root)
            .arg("-f")
            .arg(base);
        if let Some(files) = matches.get_many::<String>("compose_file") {
            for file in files {
                config.arg("-f").arg(root.join(file));
            }
        }
        if let Some(name) = matches.get_one::<String>("project_name") {
            config.arg("--project-name").arg(name);
        }
        let output = config
            .args(["config", "--format", "json"])
            .current_dir(&root)
            .output()
            .context("Docker Compose is required for local development")?;
        if !output.status.success() {
            bail!(
                "Compose configuration failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let base: Value =
            serde_json::from_slice(&output.stdout).context("Invalid Compose configuration")?;
        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?;
        if let Some(service) = matches.get_one::<String>("sdk_service") {
            anyhow::ensure!(
                manifest
                    .get("projects")
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                    .any(|project| project.get("name").and_then(toml::Value::as_str)
                        == Some(service.as_str())
                        && matches!(
                            project.get("type").and_then(toml::Value::as_str),
                            Some("Service" | "Worker")
                        )),
                "Unknown SDK service: {service}"
            );
        }
        let selected: Vec<String> = matches
            .get_many::<String>("services")
            .map(|names| names.cloned().collect())
            .unwrap_or_default();
        let reload = Reload::from_environment(matches.get_flag("no_watch"))?;
        let (mut planned, mut changes) = plan_with(
            &root,
            &manifest,
            base.clone(),
            &selected,
            !matches.get_flag("no_deps"),
            reload,
        )?;
        let mut typescript_changes = json!({});
        if matches.get_flag("export_contracts")
            || matches.contains_id("sdk_target")
            || (!matches.get_flag("no_watch")
                && crate::init::rust_project::managed_typescript_image(&root).is_some())
        {
            contracts::enable(&root, &manifest, &mut planned, &mut changes)?;
            let active = active_services(&planned, &selected, !matches.get_flag("no_deps"))?;
            typescript_changes =
                typescript::enable(&root, &manifest, &active, &mut planned, &changes)?;
        }
        let mut socket_exports =
            if matches.get_flag("export_contracts") || matches.contains_id("sdk_target") {
                Some(sockets::Exports::new(&root, &manifest, &changes)?)
            } else {
                None
            };
        let rust_sdk_directories = sdk::rust_watch(
            &root,
            &manifest,
            matches.get_one::<String>("sdk_target").map(String::as_str),
            &mut planned,
            &mut changes,
        )?;
        let watch = !matches.get_flag("no_watch")
            && (!changes.as_object().unwrap().is_empty()
                || !typescript_changes.as_object().unwrap().is_empty());
        // Native reloads run inside the containers; Compose watch is only
        // for TypeScript restarts and authored rules. A rust-toolchain.toml
        // change needs a new `forklaunch dev` (the image installs it): a
        // Compose rebuild would recreate every dependent container.
        let compose_watch = watch && uses_compose_watch(&planned);
        let reuse_watched_images = compose_watch && matches.get_flag("no_build");
        if reuse_watched_images {
            reuse_images(&mut planned)?;
        }
        if matches.get_flag("dryrun") {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"root":root,"watch":watch,"composeWatch":compose_watch,"nativeBuilder":planned["services"].get(NATIVE_BUILD),"selectedServices":selected,"nativeServices":changes,"typescriptServices":typescript_changes,"sdkTarget":matches.get_one::<String>("sdk_target"),"sdkSource":matches.get_one::<String>("sdk_source"),"sdkService":matches.get_one::<String>("sdk_service")})
                )?
            );
            return Ok(());
        }
        if compose_watch {
            let version = Process::new(&docker)
                .args(["compose", "version", "--short"])
                .output()?;
            if !version.status.success()
                || !supports_restart(&String::from_utf8_lossy(&version.stdout))
            {
                bail!(
                    "Development reloads require Docker Compose 2.32 or later; --no-watch starts without reloads"
                );
            }
        }
        for directory in rust_sdk_directories {
            fs::create_dir_all(directory)?;
        }
        if !matches.get_flag("no_build")
            && let Some(target) = matches.get_one::<String>("sdk_target")
        {
            if let Some(exports) = &mut socket_exports {
                exports.prepare()?;
            }
            let service = matches.get_one::<String>("sdk_service").map(String::as_str);
            let mut command = sdk::command(&root, target, service)?;
            sdk::source(
                &mut command,
                matches.get_one::<String>("sdk_source").map(String::as_str),
            );
            sdk::prepare(&root, &manifest, target, command)?;
            // The first pass may have created an aggregate socket SDK that
            // did not exist when the read-only development plan was built.
            // Reconcile through the same helper to retain authored watches.
            for directory in
                sdk::rust_watch(&root, &manifest, Some(target), &mut planned, &mut changes)?
            {
                fs::create_dir_all(directory)?;
            }
        }
        if !matches.get_flag("no_build") {
            let active = active_services(&base, &selected, !matches.get_flag("no_deps"))?;
            let production_consumers: Vec<String> = active
                .into_iter()
                .filter(|name| {
                    base["services"][name].get("build").is_none()
                        && base["services"][name]["image"].is_string()
                        && base["services"][name]["image"] == planned["services"][name]["image"]
                })
                .collect();
            let builders = shared_image_builders(&base, &production_consumers);
            if !production_consumers.is_empty() && !builders.is_empty() {
                let mut original = tempfile::Builder::new()
                    .prefix("forklaunch-production-build-")
                    .suffix(".json")
                    .tempfile()?;
                serde_json::to_writer(&mut original, &base)?;
                original.flush()?;
                let status = Process::new(&docker)
                    .args(["compose", "--project-directory"])
                    .arg(&root)
                    .arg("-f")
                    .arg(original.path())
                    .arg("build")
                    .args(builders)
                    .current_dir(&root)
                    .stdin(Stdio::null())
                    .status()?;
                anyhow::ensure!(
                    status.success(),
                    "Shared production image prerequisite failed"
                );
            }
        }
        // Resolved Compose may contain environment values; tempfile keeps it private.
        let mut temporary = tempfile::Builder::new()
            .prefix("forklaunch-dev-")
            .suffix(".json")
            .tempfile()?;
        serde_json::to_writer(&mut temporary, &planned)?;
        temporary.flush()?;
        if !matches.get_flag("no_build") {
            let builders = shared_image_builders(&planned, &selected);
            if !builders.is_empty() {
                let status = Process::new(&docker)
                    .args(["compose", "--project-directory"])
                    .arg(&root)
                    .arg("-f")
                    .arg(temporary.path())
                    .arg("build")
                    .args(&builders)
                    .current_dir(&root)
                    .stdin(Stdio::null())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()?;
                anyhow::ensure!(
                    status.success(),
                    "Shared language image build failed before starting selected processes"
                );
            }
        }
        // Fail before Compose starts anything: a taken host port otherwise leaves
        // a container created with no published port that still looks healthy.
        let starting: Vec<String> =
            active_services(&planned, &selected, !matches.get_flag("no_deps"))?.into_iter().collect();
        crate::core::host_ports::preflight(&planned, &starting)?;
        let mut run = Process::new(&docker);
        run.args(["compose", "--project-directory"])
            .arg(&root)
            .arg("-f")
            .arg(temporary.path())
            .arg("up");
        if !reuse_watched_images {
            run.arg(if matches.get_flag("no_build") {
                "--no-build"
            } else {
                "--build"
            });
        }
        if compose_watch {
            run.arg("--watch");
        }
        if matches.get_flag("no_deps") {
            run.arg("--no-deps");
        }
        run.args(&selected);
        // Selected native processes need the builder even under --no-deps.
        if !selected.is_empty() && planned["services"].get(NATIVE_BUILD).is_some() {
            run.arg(NATIVE_BUILD);
        }
        run
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        run.env("COMPOSE_MENU", "false");
        let watcher = matches
            .get_one::<String>("sdk_target")
            .map(|target| -> Result<Process> {
                let service = matches.get_one::<String>("sdk_service").map(String::as_str);
                let mut command = sdk::command(&root, target, service)?;
                sdk::source(
                    &mut command,
                    matches.get_one::<String>("sdk_source").map(String::as_str),
                );
                Ok(command)
            })
            .transpose()?;
        // After an interrupt, stop anything a late Compose watch restart
        // started again while the stack was shutting down.
        let mut stop = Process::new(&docker);
        stop.args(["compose", "--project-directory"])
            .arg(&root)
            .arg("-f")
            .arg(temporary.path())
            .arg("stop")
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        supervise(run, watcher, socket_exports, stop)
    }
}

fn uses_compose_watch(compose: &Value) -> bool {
    compose["services"].as_object().is_some_and(|services| {
        services.values().any(|service| {
            service["develop"]["watch"].as_array().is_some_and(|rules| !rules.is_empty())
        })
    })
}

/// Image-only services need their shared language image built even when the
/// owning build service is not selected. Building does not start that service.
fn shared_image_builders(compose: &Value, selected: &[String]) -> Vec<String> {
    let Some(services) = compose.get("services").and_then(Value::as_object) else {
        return vec![];
    };
    let mut builders = std::collections::BTreeSet::new();
    for (name, service) in services {
        if (!selected.is_empty() && !selected.contains(name)) || service.get("build").is_some() {
            continue;
        }
        let Some(image) = service.get("image").and_then(Value::as_str) else {
            continue;
        };
        if let Some((owner, _)) = services.iter().find(|(_, candidate)| {
            candidate.get("build").is_some()
                && candidate.get("image").and_then(Value::as_str) == Some(image)
        }) {
            builders.insert(owner.clone());
        }
    }
    builders.into_iter().collect()
}

// Compose rejects `up --watch --no-build`. Remove build definitions from this
// private plan instead, retaining the same local image names and restart rules.
// Never pull a replacement for an image that would otherwise be built locally.
fn reuse_images(compose: &mut Value) -> Result<()> {
    let project = compose
        .get("name")
        .and_then(Value::as_str)
        .context("Resolved Compose project name missing")?
        .to_owned();
    let services = compose
        .get_mut("services")
        .and_then(Value::as_object_mut)
        .context("Resolved Compose services missing")?;
    for (name, service) in services {
        let service = service.as_object_mut().context("Invalid Compose service")?;
        if service
            .get("develop")
            .and_then(|value| value.get("watch"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|rule| rule.get("action").and_then(Value::as_str) == Some("rebuild"))
        {
            bail!("Service {name} has a rebuild watch rule incompatible with --no-build");
        }
        if service.remove("build").is_some() {
            service
                .entry("image")
                .or_insert_with(|| json!(format!("{project}-{name}")));
            service.insert("pull_policy".into(), json!("never"));
        }
    }
    Ok(())
}

fn supports_restart(version: &str) -> bool {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    matches!((parts.next().and_then(|v| v.parse::<u32>().ok()), parts.next().and_then(|v| v.parse::<u32>().ok())), (Some(major), Some(minor)) if major > 2 || major == 2 && minor >= 32)
}

fn active_services(
    compose: &Value,
    selected: &[String],
    dependencies: bool,
) -> Result<BTreeSet<String>> {
    let services = compose
        .get("services")
        .and_then(Value::as_object)
        .context("Compose services must be an object")?;
    for name in selected {
        if !services.contains_key(name) {
            bail!("Unknown Compose service: {name}");
        }
    }
    let mut active: BTreeSet<String> = if selected.is_empty() {
        services.keys().cloned().collect()
    } else {
        selected.iter().cloned().collect()
    };
    if dependencies {
        loop {
            let before = active.len();
            for name in active.clone() {
                if let Some(dependencies) = services
                    .get(&name)
                    .and_then(|service| service.get("depends_on"))
                    .and_then(Value::as_object)
                {
                    active.extend(dependencies.keys().cloned());
                }
            }
            if active.len() == before {
                break;
            }
        }
    }
    Ok(active)
}

/// Contract documents and provenance are build outputs, not service inputs.
/// Keep their SDK source siblings watched so cross-language type edits still reload.
fn metadata_watch_ignores(
    root: &Path,
    manifest: &toml::Value,
    watched: &Path,
) -> Result<Vec<String>> {
    let watched = contracts::resolved_output(watched)?;
    let mut outputs = BTreeSet::new();
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
        let metadata = project.get("metadata");
        let table = metadata.and_then(toml::Value::as_table);
        let Some(name) = project.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        // Socket exports and their bookkeeping share the service's SDK folder.
        use crate::core::artifact_paths::{self as paths, Artifact};
        for relative in [
            paths::path(name, table, Artifact::AsyncApi),
            paths::path(name, table, Artifact::SocketContract),
            paths::socket_registry_marker(name, table).to_string_lossy().into_owned(),
            paths::socket_receipt(name, table, "asyncapi").to_string_lossy().into_owned(),
        ] {
            outputs.insert(contracts::resolved_output(&output_path(root, Path::new(&relative))?)?);
        }
        for (field, alias) in [
            ("contractArtifactPath", "contract_artifact_path"),
            ("openapiPath", "openapi_path"),
        ] {
            let Some(relative) = metadata
                .and_then(|metadata| metadata.get(field).or_else(|| metadata.get(alias)))
                .and_then(toml::Value::as_str)
            else {
                continue;
            };
            let output = contracts::resolved_output(&output_path(root, Path::new(relative))?)?;
            if field == "contractArtifactPath"
                && metadata
                    .and_then(|metadata| metadata.get("language"))
                    .and_then(toml::Value::as_str)
                    == Some("rust")
                && metadata
                    .and_then(|metadata| metadata.get("sdkLayout"))
                    .and_then(toml::Value::as_str)
                    == Some("project-local-v1")
            {
                outputs.insert(output.with_file_name("provenance.json"));
            }
            outputs.insert(output);
        }
    }
    let mut ignores = Vec::new();
    for output in outputs {
        let Ok(relative) = output.strip_prefix(&watched) else {
            continue;
        };
        let path = relative
            .to_str()
            .context("Invalid development artifact path")?;
        // Compose interprets ignores as patterns relative to each watch root.
        ignores.push(glob::Pattern::escape(&path.replace('\\', "/")));
        // core::build::write_generated_output atomically replaces this exact
        // metadata file via <stem>.sdk-tmp-<pid>-<sequence>. Watching that staging
        // event would restart the service even when its final path is ignored.
        let staged = relative.with_extension("sdk-tmp-");
        let staged = staged
            .to_str()
            .context("Invalid development artifact staging path")?;
        ignores.push(format!(
            "{}*",
            glob::Pattern::escape(&staged.replace('\\', "/"))
        ));
    }
    Ok(ignores)
}

fn restart_watch_rule(root: &Path, manifest: &toml::Value, path: &Path) -> Result<Value> {
    let mut ignores = vec![
        "target/".into(),
        "**/target/".into(),
        "**/node_modules/".into(),
        "**/.git/".into(),
    ];
    ignores.extend(metadata_watch_ignores(root, manifest, path)?);
    Ok(json!({"action":"restart","path":path,"ignore":ignores}))
}

fn watch_covers(root: &Path, rule: &Value, path: &Path) -> bool {
    rule["path"].as_str().is_some_and(|watched| {
        let watched = Path::new(watched);
        path.starts_with(if watched.is_absolute() {
            watched.to_path_buf()
        } else {
            root.join(watched)
        })
    })
}

#[cfg(test)]
fn plan(
    root: &Path,
    manifest: &toml::Value,
    compose: Value,
    selected: &[String],
    dependencies: bool,
) -> Result<(Value, Value)> {
    plan_with(root, manifest, compose, selected, dependencies, Reload::Off)
}

/// One-shot jobs (migrations, seeds) run once per `forklaunch dev`: another
/// service waits for them to complete, or they never restart.
fn one_shot(services: &serde_json::Map<String, Value>, name: &str) -> bool {
    services[name].get("restart").and_then(Value::as_str) == Some("no")
        || services.values().any(|service| {
            service["depends_on"][name]["condition"] == "service_completed_successfully"
        })
}

/// The application path as the containers see it: the root is mounted at
/// /app, a local framework checkout at its own absolute path.
fn container_path(root: &Path, path: &Path) -> PathBuf {
    match path.strip_prefix(root) {
        Ok(relative) => Path::new("/app").join(relative),
        Err(_) => path.to_path_buf(),
    }
}

/// The reload builder: watches every native source path and rebuilds the
/// whole selection on change; `forklaunch-native-rebuild` then stamps the
/// binaries Cargo relinked, which restarts only their processes.
fn watch_builder(
    root: &Path,
    manifest: &toml::Value,
    paths: &BTreeSet<PathBuf>,
    build_arguments: &[String],
    poll_ms: Option<u64>,
) -> Result<Vec<String>> {
    let mut command: Vec<String> = [
        "watchexec",
        "--shell=none",
        // Edits made during a build run one more build afterwards.
        "--on-busy-update=queue",
        "--debounce=200ms",
        "--no-discover-ignore",
        "--no-follow-symlinks",
        "--project-origin=/app",
    ]
    .map(str::to_owned)
    .into();
    if let Some(interval) = poll_ms {
        command.push(format!("--poll={interval}ms"));
    }
    for path in paths {
        command.push("-w".into());
        command.push(container_path(root, path).to_string_lossy().into_owned());
    }
    for pattern in [
        "**/target/**",
        "**/node_modules/**",
        "**/.git/**",
        // Build-script and SDK outputs: written by builds, not inputs to them.
        "**/sdk/typescript",
        "**/sdk/typescript/**",
        "**/*.sdk-tmp-*",
        "**/*.tsbuildinfo",
    ] {
        command.push("-i".into());
        command.push(pattern.into());
    }
    for pattern in metadata_watch_ignores(root, manifest, root)? {
        // Custom artifact locations; the defaults are covered above.
        if pattern.contains("sdk/typescript/") || pattern.ends_with(".sdk-tmp-*") {
            continue;
        }
        command.push("-i".into());
        command.push(format!("/{pattern}"));
    }
    command.push("--".into());
    command.push("forklaunch-native-rebuild".into());
    command.extend(build_arguments.iter().cloned());
    Ok(command)
}

/// Add source paths to a reload builder's watch list (before its `--`).
/// Returns false when the plan has no reload builder.
pub(super) fn watch_native_path(compose: &mut Value, root: &Path, path: &Path) -> bool {
    // `get_mut`, not indexing: indexing would insert an empty service.
    let Some(entrypoint) = compose
        .get_mut("services")
        .and_then(|services| services.get_mut(NATIVE_BUILD))
        .and_then(|builder| builder.get_mut("entrypoint"))
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    if entrypoint.first().and_then(Value::as_str) != Some("watchexec") {
        return false;
    }
    let target = container_path(root, path);
    let watched = entrypoint.windows(2).any(|pair| {
        pair[0] == "-w" && pair[1].as_str().is_some_and(|existing| target.starts_with(existing))
    });
    if !watched {
        let separator = entrypoint.iter().position(|value| value == "--").unwrap_or(entrypoint.len());
        entrypoint.insert(separator, json!(target));
        entrypoint.insert(separator, json!("-w"));
    }
    true
}

fn plan_with(
    root: &Path,
    manifest: &toml::Value,
    mut compose: Value,
    selected: &[String],
    dependencies: bool,
    reload: Reload,
) -> Result<(Value, Value)> {
    let canonical_root = root.canonicalize()?;
    let root = canonical_root.as_path();
    let active = active_services(&compose, selected, dependencies)?;
    let services = compose["services"]
        .as_object()
        .context("Compose services missing")?;
    let mut required = active.clone();
    for name in &active {
        if let Some(binary) = services
            .get(name)
            .and_then(|service| service.get("command"))
            .and_then(Value::as_array)
            .and_then(|args| args.first())
            .and_then(Value::as_str)
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
        {
            required.insert(binary.to_owned());
        }
    }
    let native_project = |project: &toml::Value| {
        if project.get("variant").and_then(toml::Value::as_str) == Some("storefront") {
            return false;
        }
        let language = project
            .get("metadata")
            .and_then(|metadata| metadata.get("language"))
            .and_then(toml::Value::as_str);
        language
            .map(|language| language == "rust")
            .unwrap_or_else(|| {
                project
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|name| root.join("crates").join(name).join("Cargo.toml").is_file())
            })
    };
    let projects = manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let selected_native_command = active.iter().any(|name| {
        services
            .get(name)
            .and_then(|service| service.get("command"))
            .and_then(Value::as_array)
            .and_then(|command| command.first())
            .and_then(Value::as_str)
            .is_some_and(|command| {
                command.starts_with("/app/bin/") || command.contains("target/release/")
            })
    });
    if !selected_native_command
        && !projects.iter().any(|project| {
            native_project(project)
                && project
                    .get("name")
                    .and_then(toml::Value::as_str)
                    .is_some_and(|name| required.contains(name))
        })
    {
        return Ok((compose, json!({})));
    }
    let mut native = BTreeMap::new();
    let mut paths = BTreeSet::new();
    for project in manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let name = project
            .get("name")
            .and_then(toml::Value::as_str)
            .context("Project name missing")?;
        let metadata = project.get("metadata");
        if !native_project(project) {
            continue;
        }
        let relative = metadata
            .and_then(|metadata| {
                metadata
                    .get("cratePath")
                    .or_else(|| metadata.get("crate_path"))
            })
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("crates/{name}"));
        let directory = output_path(root, Path::new(&relative))?;
        if directory.is_dir() {
            paths.insert(directory.clone());
        }
        if matches!(
            project.get("type").and_then(toml::Value::as_str),
            Some("Service" | "Worker")
        ) {
            let cargo: toml::Value =
                toml::from_str(&fs::read_to_string(directory.join("Cargo.toml"))?)?;
            let package = cargo
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
                .context("Native Cargo package name missing")?;
            if package.is_empty()
                || package.starts_with('-')
                || !package
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                bail!("Unsupported native Cargo package name");
            }
            if name.is_empty()
                || name.starts_with('-')
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                bail!("Unsupported native binary name");
            }
            let mut binaries = BTreeSet::new();
            if let Some(entries) = cargo.get("bin").and_then(toml::Value::as_array) {
                for binary in entries {
                    binaries.insert(
                        binary
                            .get("name")
                            .and_then(toml::Value::as_str)
                            .context("Native Cargo binary name missing")?
                            .to_owned(),
                    );
                }
            }
            if binaries.is_empty() {
                binaries.insert(name.to_owned());
            }
            for binary in binaries {
                if required.contains(&binary) {
                    if binary.is_empty()
                        || !binary
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                    {
                        bail!("Unsupported native binary name");
                    }
                    if native.insert(binary, package.to_owned()).is_some() {
                        bail!("Duplicate native binary name in development process inventory");
                    }
                }
            }
        }
    }
    for relative in ["Cargo.toml", "Cargo.lock"] {
        let path = output_path(root, Path::new(relative))?;
        if path.exists() {
            paths.insert(path);
        }
    }
    for path in sdk::rust_paths(root, manifest)? {
        if path.is_dir() && !paths.iter().any(|watched| path.starts_with(watched)) {
            paths.insert(path);
        }
    }
    let project = compose
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("forklaunch")
        .to_case(Case::Kebab);
    // The toolchain stage only: sources (and a local framework checkout) are
    // bind-mounted, never copied into the development image.
    let template = include_str!("templates/rust/application/Dockerfile");
    let toolchain = template
        .find("FROM rust:")
        .context("Native builder template missing toolchain stage")?;
    let dockerfile = template[toolchain..]
        .split("\nCOPY ")
        .next()
        .context("Native builder template missing source-copy boundary")?
        .replace("pkg-config libssl-dev", "pkg-config curl xz-utils lld libssl-dev")
        + "\nENV SQLX_OFFLINE=true\n"
        + &watchexec_install()
        // Install the application's pinned toolchain and components once in the
        // image, not in every container at start.
        + "COPY rust-toolchain.tom[l] /opt/forklaunch-toolchain/\nRUN cd /opt/forklaunch-toolchain && if [ -f rust-toolchain.toml ]; then rustup toolchain install; fi\n"
        + NATIVE_BUILD_SCRIPT
        + NATIVE_REBUILD_SCRIPT
        + NATIVE_RUN_SCRIPT;
    // Compose interpolates the inline Dockerfile; keep the script's `$` literal.
    let dockerfile = dockerfile.replace('$', "$$");
    // Host checkout of a `[framework] path` application, mounted read-only at
    // the same absolute path its Cargo manifests reference.
    let framework_mounts: Vec<Value> = crate::init::rust_project::framework_mounts(root)
        .into_iter()
        .map(|path| json!({"type":"bind","source":path,"target":path,"read_only":true}))
        .collect();
    let mut cache_names = Vec::new();
    for suffix in ["target", "registry", "git"] {
        let base = format!("forklaunch-native-{suffix}");
        let mut name = base.clone();
        let mut index = 0;
        while compose
            .get("volumes")
            .and_then(|volumes| volumes.get(&name))
            .is_some()
        {
            index += 1;
            name = format!("{base}-{index}");
        }
        cache_names.push(name);
    }
    // Every native process builds the same package and binary set, so Cargo
    // resolves features identically: after one warm-up build each start (and
    // each watch restart that changed nothing it uses) is a no-op.
    let mut selection = BTreeSet::new();
    for service in compose["services"].as_object().unwrap().iter().filter(|(name, _)| active.contains(*name)).map(|(_, s)| s) {
        let binary = service
            .get("command")
            .and_then(Value::as_array)
            .and_then(|args| args.first())
            .and_then(Value::as_str)
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str());
        if let Some((binary, package)) = binary.and_then(|binary| native.get(binary).map(|package| (binary, package))) {
            selection.insert((package.clone(), binary.to_owned()));
        }
    }
    let packages: BTreeSet<&String> = selection.iter().map(|(package, _)| package).collect();
    let mut build_command = String::from("forklaunch-native-build");
    for package in &packages {
        build_command.push_str(&format!(" --package {package}"));
    }
    for (_, binary) in &selection {
        build_command.push_str(&format!(" --bin {binary}"));
    }
    let image = format!("forklaunch-dev-{project}-native:local");
    let mut changes = serde_json::Map::new();
    let services = compose["services"].as_object_mut().unwrap();
    let one_shots: BTreeSet<String> =
        services.keys().filter(|name| one_shot(services, name)).cloned().collect();
    for (name, service) in services.iter_mut() {
        if !active.contains(name) {
            continue;
        }
        // This also finds generated auxiliary workers using their owner's binary.
        let executable = service
            .get("command")
            .and_then(Value::as_array)
            .and_then(|args| args.first())
            .and_then(Value::as_str);
        let binary = executable
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str());
        let Some((binary, package)) = binary.and_then(|binary| {
            native
                .get(binary)
                .map(|package| (binary.to_owned(), package))
        }) else {
            if native.contains_key(name) {
                bail!(
                    "Native service {name} has a custom launch command; configure a generated binary command before using forklaunch dev"
                );
            }
            continue;
        };
        if service.get("entrypoint").is_some_and(|entry| {
            !entry.is_null() && entry.as_array().is_none_or(|args| !args.is_empty())
        }) {
            bail!(
                "Native service {name} has a custom entrypoint; configure its development startup explicitly"
            );
        }
        let command = service["command"].as_array_mut().unwrap();
        command[0] = json!(format!("/app/target/debug/{binary}"));
        let watching = matches!(reload, Reload::Watch { .. });
        let mut develop = service.get("develop").cloned().unwrap_or_else(|| json!({}));
        let mut watch = match develop.get("watch") {
            Some(watch) => watch
                .as_array()
                .context("Development watches must be an array")?
                .clone(),
            None => Vec::new(),
        };
        // Reloads happen inside the container (the builder's watcher and
        // forklaunch-native-run), so Compose watch adds no restart rules;
        // only authored rules stay.
        if !watching {
            for path in &paths {
                if !watch.iter().any(|rule| watch_covers(root, rule, path)) {
                    watch.push(restart_watch_rule(root, manifest, path)?);
                }
            }
        }
        if !watch.is_empty() || service.get("develop").is_some() {
            develop
                .as_object_mut()
                .context("Development settings must be an object")?
                .insert("watch".into(), json!(watch));
        }
        let _ = package;
        let entrypoint = match reload {
            Reload::Off => json!(["sh","-c",format!("{build_command} && exec \"$@\""),"--"]),
            Reload::Watch { .. } if one_shots.contains(name) => json!(["forklaunch-native-run"]),
            Reload::Watch { .. } => json!(["forklaunch-native-run", "--watch"]),
        };
        let mut settings = json!({
            "image":image,
            "build":{"context":root,"dockerfile_inline":dockerfile},
            "working_dir":"/app",
            "entrypoint":entrypoint,
        });
        if !watch.is_empty() || !watching {
            settings["develop"] = develop;
        }
        let object = service
            .as_object_mut()
            .context("Compose service must be an object")?;
        for (key, value) in settings.as_object().unwrap() {
            object.insert(key.clone(), value.clone());
        }
        let volumes = object
            .entry("volumes")
            .or_insert(json!([]))
            .as_array_mut()
            .context("Resolved Compose volumes must be an array")?;
        for volume in [
            json!({"type":"bind","source":root,"target":"/app"}),
            json!({"type":"volume","source":cache_names[0],"target":"/app/target"}),
            json!({"type":"volume","source":cache_names[1],"target":"/usr/local/cargo/registry"}),
            json!({"type":"volume","source":cache_names[2],"target":"/usr/local/cargo/git"}),
        ]
        .into_iter()
        .chain(framework_mounts.iter().cloned())
        {
            volumes.retain(|existing| existing.get("target") != volume.get("target"));
            volumes.push(volume);
        }
        let environment = object
            .entry("environment")
            .or_insert(json!({}))
            .as_object_mut()
            .context("Resolved Compose environment must be an object")?;
        environment.insert("CARGO_TARGET_DIR".into(), json!("/app/target"));
        // The container builds with DATABASE_URL set; keep sqlx macros offline.
        environment.insert("SQLX_OFFLINE".into(), json!("true"));
        // Debug builds stay incremental, so a watch restart recompiles only
        // what changed; set CARGO_INCREMENTAL in Compose to override.
        // Healthchecks start counting once the binary runs: a (re)build can
        // take minutes, and failures during the start period are ignored.
        // With reloads the binary is built before the process starts, and a
        // rebuild leaves the running process serving, so the authored
        // healthcheck applies unchanged.
        if !watching
            && let Some(healthcheck) = object.get_mut("healthcheck").and_then(Value::as_object_mut)
            && !healthcheck.get("disable").and_then(Value::as_bool).unwrap_or(false)
        {
            healthcheck.insert("start_period".into(), json!("900s"));
            healthcheck.insert("start_interval".into(), json!("2s"));
        }
        let environment = object["environment"].clone();
        let mut settings = settings;
        settings["command"] = object["command"].clone();
        settings["volumes"] = object["volumes"].clone();
        settings["environment"] = json!({"CARGO_TARGET_DIR":"/app/target","SQLX_OFFLINE":"true"});
        if let Some(incremental) = environment.get("CARGO_INCREMENTAL") {
            settings["environment"]["CARGO_INCREMENTAL"] = incremental.clone();
        }
        changes.insert(name.clone(), settings);
    }
    // One warm-up build before any native process starts: processes would
    // otherwise all compile at once against one registry volume and race.
    if !changes.is_empty() {
        let mut volumes = vec![
            json!({"type":"bind","source":root,"target":"/app"}),
            json!({"type":"volume","source":cache_names[0],"target":"/app/target"}),
            json!({"type":"volume","source":cache_names[1],"target":"/usr/local/cargo/registry"}),
            json!({"type":"volume","source":cache_names[2],"target":"/usr/local/cargo/git"}),
        ];
        volumes.extend(framework_mounts.iter().cloned());
        let mut builder = json!({
            "image":image,
            "build":{"context":root,"dockerfile_inline":dockerfile},
            "working_dir":"/app",
            "entrypoint":["sh","-c",build_command],
            "environment":{"CARGO_TARGET_DIR":"/app/target","SQLX_OFFLINE":"true"},
            "volumes":volumes,
            "restart":"no",
        });
        let mut condition = "service_completed_successfully";
        if let Reload::Watch { poll_ms } = reload {
            // The warm-up job becomes the one long-lived builder: one Cargo
            // invocation per edit, so containers never contend for the build
            // lock and features always resolve as in the warm-up.
            let arguments: Vec<String> = build_command
                .split(' ')
                .skip(1)
                .map(str::to_owned)
                .collect();
            let mut watched = paths.clone();
            for mount in &framework_mounts {
                if let Some(path) = mount["source"].as_str().map(Path::new) {
                    for member in ["crates", "Cargo.toml"] {
                        if path.join(member).exists() {
                            watched.insert(path.join(member));
                        }
                    }
                }
            }
            builder["entrypoint"] = json!(watch_builder(root, manifest, &watched, &arguments, poll_ms)?);
            builder["restart"] = json!("unless-stopped");
            // Healthy after the first successful build; it stays healthy while
            // later builds run or fail, since the processes keep serving.
            builder["healthcheck"] = json!({
                "test":["CMD","test","-f","/tmp/forklaunch-native-ready"],
                "interval":"10s",
                "timeout":"5s",
                "retries":3,
                "start_period":"1800s",
                "start_interval":"1s",
            });
            condition = "service_healthy";
        }
        let services = compose["services"].as_object_mut().unwrap();
        for name in changes.keys() {
            let depends = services[name]
                .as_object_mut()
                .unwrap()
                .entry("depends_on")
                .or_insert(json!({}));
            if let Some(depends) = depends.as_object_mut() {
                depends.insert(NATIVE_BUILD.into(), json!({"condition":condition,"required":true}));
            }
        }
        services.insert(NATIVE_BUILD.into(), builder);
    }
    if !changes.is_empty() {
        let volumes = compose
            .as_object_mut()
            .unwrap()
            .entry("volumes")
            .or_insert(json!({}))
            .as_object_mut()
            .context("Compose volumes must be an object")?;
        for name in cache_names {
            volumes.entry(name).or_insert(json!({}));
        }
    }
    Ok((compose, Value::Object(changes)))
}

/// How long Compose may take to stop gracefully after an interrupt before it
/// is asked to force-stop, and how long that may take before it is killed.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);
const SHUTDOWN_FORCE: Duration = Duration::from_secs(15);

fn supervise(
    mut command: Process,
    sdk: Option<Process>,
    mut socket_exports: Option<sockets::Exports>,
    mut stop: Process,
) -> Result<()> {
    let interrupts = Arc::new(AtomicUsize::new(0));
    let handler = interrupts.clone();
    ctrlc::set_handler(move || {
        handler.fetch_add(1, Ordering::SeqCst);
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut sdk = sdk.map(sdk::Watcher::spawn).transpose()?;
    let mut child = command
        .spawn()
        .context("Failed to start development stack")?;
    let mut forwarded = 0;
    // Compose can keep running after a graceful stop (some Compose 2.x
    // releases leave `up --watch` alive once its containers stopped), so an
    // interrupt escalates: a second SIGINT (Compose's force stop) after a
    // grace period, then SIGKILL. `compose stop` afterwards stops anything
    // still running.
    let mut interrupted_at: Option<Instant> = None;
    let mut escalated = 0;
    loop {
        if let Some(watcher) = &mut sdk {
            watcher.report_exit();
        }
        if let Some(exports) = &mut socket_exports {
            exports.poll();
        }
        if let Some(status) = child.try_wait()? {
            if forwarded > 0 {
                let _ = stop.status();
                return Ok(());
            }
            if status.success() {
                return Ok(());
            }
            bail!("Development stack exited with {status}");
        }
        let received = interrupts.load(Ordering::SeqCst);
        if let Some(since) = interrupted_at {
            let due = match escalated {
                0 => Some(SHUTDOWN_GRACE),
                1 => Some(SHUTDOWN_GRACE + SHUTDOWN_FORCE),
                _ => None,
            };
            if due.is_some_and(|due| since.elapsed() >= due) {
                escalated += 1;
                eprintln!(
                    "forklaunch dev: Compose did not stop within {}s; {}",
                    since.elapsed().as_secs(),
                    if escalated == 1 { "forcing it to stop" } else { "killing it" }
                );
                #[cfg(unix)]
                {
                    use nix::{
                        sys::signal::{Signal, killpg},
                        unistd::Pid,
                    };
                    let id = i32::try_from(child.id()).context("Invalid Compose process ID")?;
                    if id > 1 {
                        let signal = if escalated == 1 { Signal::SIGINT } else { Signal::SIGKILL };
                        let _ = killpg(Pid::from_raw(id), signal);
                    }
                }
                #[cfg(not(unix))]
                if escalated == 2 {
                    let _ = child.kill();
                }
            }
        }
        if received > forwarded {
            interrupted_at.get_or_insert_with(Instant::now);
            // Stop regenerating SDKs first: a regeneration during shutdown
            // would trigger Compose watch restarts.
            sdk = None;
            socket_exports = None;
            #[cfg(unix)]
            {
                use nix::{
                    sys::signal::{Signal, killpg},
                    unistd::Pid,
                };
                let id = i32::try_from(child.id()).context("Invalid Compose process ID")?;
                if id > 1 {
                    let _ = killpg(Pid::from_raw(id), Signal::SIGINT);
                }
            }
            forwarded = received;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn fixture() -> (tempfile::TempDir, toml::Value, Value) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("services/native-api")).unwrap();
        fs::create_dir_all(root.path().join("shared/models")).unwrap();
        fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        fs::write(
            root.path().join("services/native-api/Cargo.toml"),
            "[package]\nname = 'native-core'\n",
        )
        .unwrap();
        let manifest = toml::from_str(
            r#"
            runtime = "node"
            [[projects]]
            name = "native-api"
            type = "Service"
            [projects.metadata]
            language = "rust"
            cratePath = "services/native-api"
            [[projects]]
            name = "models"
            type = "Library"
            [projects.metadata]
            language = "rust"
            cratePath = "shared/models"
            [[projects]]
            name = "legacy-api"
            type = "Service"
        "#,
        )
        .unwrap();
        let compose = json!({"name":"test-app","services":{
            "native-api":{"command":["./target/release/native-api"],"environment":{"PORT":"8000"},"volumes":[{"type":"volume","source":"data","target":"/app/data"}]},
            "native-outbox":{"command":["./target/release/native-api","--worker"]},
            "legacy-api":{"entrypoint":["pnpm","run","dev"],"environment":{"PORT":"8001"}}
        },"volumes":{"data":{}}});
        (root, manifest, compose)
    }

    #[test]
    fn selected_image_only_process_builds_language_owner_without_starting_it() {
        let compose = json!({"services":{"api":{"image":"demo-rust:local","build":{"dockerfile":"Dockerfile.rust"}},"worker":{"image":"demo-rust:local"},"auth":{"image":"demo-ts:local","build":{"dockerfile":"Dockerfile.typescript"}}}});
        assert_eq!(
            shared_image_builders(&compose, &["worker".into()]),
            vec!["api"]
        );
        assert!(shared_image_builders(&compose, &["auth".into()]).is_empty());
    }

    #[test]
    fn native_processes_share_one_image_and_wait_for_one_warm_up_build() {
        let (root, manifest, mut compose) = fixture();
        fs::write(root.path().join("services/native-api/Cargo.toml"),
            "[package]\nname='native-core'\n[[bin]]\nname='native-api'\n[[bin]]\nname='events-consumer'\n").unwrap();
        compose["services"]["native-outbox"]["command"] = json!(["./target/release/events-consumer"]);
        compose["services"]["native-api"]["healthcheck"] = json!({"test":["CMD","true"],"start_period":"40s"});
        let (planned, changes) = plan(root.path(), &manifest, compose, &[], true).unwrap();
        let services = &planned["services"];
        let build = "forklaunch-native-build --package native-core --bin events-consumer --bin native-api";
        assert_eq!(services[NATIVE_BUILD]["entrypoint"][2], build);
        assert_eq!(services[NATIVE_BUILD]["environment"]["SQLX_OFFLINE"], "true");
        for name in ["native-api", "native-outbox"] {
            // Identical Cargo invocations resolve features identically, so each
            // start after the warm-up is a no-op instead of a racing compile.
            assert_eq!(services[name]["entrypoint"][2], format!("{build} && exec \"$@\""));
            assert_eq!(services[name]["image"], "forklaunch-dev-test-app-native:local");
            assert_eq!(services[name]["depends_on"][NATIVE_BUILD]["condition"], "service_completed_successfully");
            assert_eq!(services[name]["environment"]["SQLX_OFFLINE"], "true");
            assert!(services[name]["environment"].get("CARGO_INCREMENTAL").is_none());
        }
        assert_eq!(services["native-api"]["healthcheck"]["start_period"], "900s");
        assert!(changes.get(NATIVE_BUILD).is_none());
        assert!(!services[NATIVE_BUILD]["build"]["dockerfile_inline"].as_str().unwrap().contains("COPY . ."));
    }

    #[test]
    fn reloads_rebuild_in_one_builder_and_restart_processes_in_place() {
        let (root, manifest, mut compose) = fixture();
        fs::write(root.path().join("services/native-api/Cargo.toml"),
            "[package]\nname='native-core'\n[[bin]]\nname='native-api'\n[[bin]]\nname='events-consumer'\n").unwrap();
        compose["services"]["native-outbox"]["command"] = json!(["./target/release/events-consumer"]);
        compose["services"]["native-migrate"] = json!({"command":["./target/release/native-api","--migrate"],"restart":"no"});
        let healthcheck = json!({"test":["CMD","true"],"start_period":"40s"});
        compose["services"]["native-api"]["healthcheck"] = healthcheck.clone();
        let authored = json!({"action":"sync","path":"assets","target":"/app/assets"});
        compose["services"]["native-outbox"]["develop"] = json!({"watch":[authored]});
        let reload = Reload::Watch { poll_ms: Some(500) };
        let (mut planned, changes) = plan_with(root.path(), &manifest, compose, &[], true, reload).unwrap();
        let services = &planned["services"];
        let builder = services[NATIVE_BUILD]["entrypoint"].as_array().unwrap();
        let text: Vec<&str> = builder.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(text[0], "watchexec");
        assert!(text.contains(&"--poll=500ms") && text.contains(&"--on-busy-update=queue"));
        let separator = text.iter().position(|v| *v == "--").unwrap();
        // One Cargo invocation for the whole selection, as in the warm-up.
        assert_eq!(text[separator + 1..].join(" "),
            "forklaunch-native-rebuild --package native-core --bin events-consumer --bin native-api");
        // Watched paths are container paths: each crate and the shared library.
        let watched: Vec<&str> = text.windows(2).filter(|p| p[0] == "-w").map(|p| p[1]).collect();
        for path in ["/app/services/native-api", "/app/shared/models", "/app/Cargo.toml"] {
            assert!(watched.contains(&path), "{path} not watched: {watched:?}");
        }
        assert!(text.contains(&"**/target/**"));
        assert_eq!(services[NATIVE_BUILD]["restart"], "unless-stopped");
        // No Compose rebuild rule: it would recreate every dependent container.
        assert!(services[NATIVE_BUILD].get("develop").is_none());
        assert!(services[NATIVE_BUILD]["healthcheck"]["test"].to_string().contains("forklaunch-native-ready"));
        for name in ["native-api", "native-outbox"] {
            assert_eq!(services[name]["entrypoint"], json!(["forklaunch-native-run", "--watch"]));
            assert_eq!(services[name]["depends_on"][NATIVE_BUILD]["condition"], "service_healthy");
        }
        // One-shot jobs run once, after the build, and complete.
        assert_eq!(services["native-migrate"]["entrypoint"], json!(["forklaunch-native-run"]));
        assert_eq!(services["native-migrate"]["command"][0], "/app/target/debug/native-api");
        // No Compose restart rules: reloads happen inside the containers.
        assert!(services["native-api"].get("develop").is_none());
        assert_eq!(services["native-outbox"]["develop"]["watch"], json!([authored]));
        assert!(!uses_compose_watch(&json!({"services":{"native-api":services["native-api"].clone()}})));
        // Authored healthchecks apply unchanged; the old process serves during rebuilds.
        assert_eq!(services["native-api"]["healthcheck"], healthcheck);
        assert!(changes["native-api"].get("develop").is_none());
        let dockerfile = services[NATIVE_BUILD]["build"]["dockerfile_inline"].as_str().unwrap();
        assert!(dockerfile.contains(&format!("v{WATCHEXEC_VERSION}/")));
        for (_, sha) in WATCHEXEC_SHA256 {
            assert!(dockerfile.contains(sha));
        }
        assert!(dockerfile.contains("forklaunch-native-rebuild") && dockerfile.contains("forklaunch-native-run"));
        // Without a reload builder nothing is added to the plan.
        let mut plain = json!({"services":{"api":{}}});
        assert!(!watch_native_path(&mut plain, root.path(), &root.path().join("sdk")));
        assert_eq!(plain, json!({"services":{"api":{}}}));
        // Later SDK paths join the builder's watch list, before its command.
        let sdk = root.path().canonicalize().unwrap().join("sdks/rust");
        assert!(watch_native_path(&mut planned, &root.path().canonicalize().unwrap(), &sdk));
        let builder = planned["services"][NATIVE_BUILD]["entrypoint"].as_array().unwrap().clone();
        let separator = builder.iter().position(|v| v == "--").unwrap();
        assert_eq!(builder[separator - 1], "/app/sdks/rust");
        assert!(watch_native_path(&mut planned, &root.path().canonicalize().unwrap(), &sdk));
        assert_eq!(planned["services"][NATIVE_BUILD]["entrypoint"].as_array().unwrap().len(), builder.len());
    }

    #[cfg(unix)]
    #[test]
    fn rebuild_script_stamps_only_relinked_binaries() {
        // Exercise the script body the image installs, against a fake build.
        let script = NATIVE_REBUILD_SCRIPT
            .split_once("\n").unwrap().1
            .rsplit_once("SCRIPT\n").unwrap().0
            .replace("/app/target", "$FIXTURE")
            .replace("/tmp/forklaunch-native-ready", "$FIXTURE/ready");
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("debug")).unwrap();
        let bin = root.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let build = bin.join("forklaunch-native-build");
        fs::write(&build, "#!/bin/sh\nfor b in $RELINK; do rm -f \"$FIXTURE/debug/$b\"; echo $RANDOM > \"$FIXTURE/debug/$b\"; done\nexit ${FAIL:-0}\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&build, fs::Permissions::from_mode(0o755)).unwrap();
        let run = |relink: &str, fail: &str| {
            let output = Process::new("sh").arg("-c").arg(&script).arg("forklaunch-native-rebuild")
                .args(["--package", "core", "--bin", "api", "--bin", "worker"])
                .env("FIXTURE", root.path()).env("RELINK", relink).env("FAIL", fail)
                .env("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()))
                .output().unwrap();
            (output.status.success(), String::from_utf8_lossy(&output.stdout).into_owned())
        };
        let (ok, out) = run("api worker", "0");
        assert!(ok && out.contains("restarting: api worker"), "{out}");
        assert!(root.path().join("ready").is_file());
        let stamp = |name: &str| fs::read_to_string(root.path().join(format!("forklaunch-dev/{name}.ready"))).unwrap();
        let worker = stamp("worker");
        let (ok, out) = run("api", "0");
        assert!(ok && out.contains("restarting: api") && !out.contains("worker"), "{out}");
        assert_eq!(stamp("worker"), worker);
        let (ok, out) = run("", "0");
        assert!(ok && out.contains("nothing"), "{out}");
        let api = stamp("api");
        let (ok, _) = run("api", "1");
        assert!(!ok);
        assert_eq!(stamp("api"), api, "a failed build must not restart anything");
    }

    #[test]
    fn reload_mode_follows_no_watch_and_poll_override() {
        assert_eq!(Reload::from_environment(true).unwrap(), Reload::Off);
        assert!(matches!(Reload::from_environment(false).unwrap(), Reload::Watch { .. }));
    }

    #[test]
    fn selected_role_uses_declared_binary_in_owning_package() {
        let (root, manifest, mut compose) = fixture();
        fs::write(root.path().join("services/native-api/Cargo.toml"),
            "[package]\nname='native-core'\n[[bin]]\nname='native-api'\n[[bin]]\nname='events-consumer'\n").unwrap();
        compose["services"]["native-outbox"]["command"] = json!(["/app/bin/events-consumer"]);
        let (planned, _) = plan(
            root.path(),
            &manifest,
            compose,
            &["native-outbox".into()],
            true,
        )
        .unwrap();
        assert_eq!(
            planned["services"]["native-outbox"]["entrypoint"][2],
            "forklaunch-native-build --package native-core --bin events-consumer && exec \"$@\""
        );
        assert_eq!(
            planned["services"]["native-outbox"]["command"][0],
            "/app/target/debug/events-consumer"
        );
    }

    #[test]
    fn existing_typescript_service_rust_sdks_reload_native_consumers_without_owned_sdk_watch() {
        let (root, mut manifest, mut compose) = fixture();
        let authored =
            json!({"action":"sync","path":"assets","target":"/app/assets","ignore":["private/"]});
        compose["services"]["native-api"]["develop"] = json!({"watch":[authored]});
        manifest["projects"][2].as_table_mut().unwrap().insert(
            "metadata".into(),
            toml::Value::try_from(json!({
                "language":"typescript", "sdkLayout":"project-local-v1", "staticPath":"modules/legacy-api"
            })).unwrap(),
        );
        let sdk = root.path().join("modules/legacy-api/sdk/rust");
        fs::create_dir_all(&sdk).unwrap();
        fs::write(sdk.join("models.rs"), "// generated Rust wire models\n").unwrap();
        let (_, changes) = plan(root.path(), &manifest, compose, &[], true).unwrap();
        let watches = changes["native-api"]["develop"]["watch"]
            .as_array()
            .unwrap();
        assert_eq!(watches[0], authored);
        let rule = watches
            .iter()
            .find(|rule| rule["path"] == json!(sdk.canonicalize().unwrap()))
            .unwrap();
        assert_eq!(rule["action"], "restart");
        for source in ["models.rs", "client.rs", "schemas.rs", "Cargo.toml"] {
            assert!(
                !rule["ignore"].as_array().unwrap().iter().any(|pattern| {
                    glob::Pattern::new(pattern.as_str().unwrap())
                        .unwrap()
                        .matches(source)
                }),
                "Rust SDK input ignored: {source}"
            );
        }
        assert!(!root.path().join("sdks").exists());
    }

    #[test]
    fn metadata_outputs_do_not_restart_native_or_typescript_but_sdk_sources_do() {
        let (root, mut manifest, mut base) = fixture();
        manifest.as_table_mut().unwrap().insert(
            "modules_path".into(),
            toml::Value::String("services".into()),
        );
        let metadata = manifest["projects"][0]["metadata"].as_table_mut().unwrap();
        for (field, value) in [
            (
                "contract_artifact_path",
                "services/native-api/sdk/typescript/contract.json",
            ),
            (
                "openapi_path",
                "services/native-api/sdk/typescript/openapi.json",
            ),
            ("sdkLayout", "project-local-v1"),
        ] {
            metadata.insert(field.into(), toml::Value::String(value.into()));
        }
        manifest["projects"][2].as_table_mut().unwrap().insert(
            "metadata".into(),
            toml::Value::try_from(json!({
                "language":"typescript",
                "contractArtifactPath":"services/legacy-api/sdk/contract.json",
                "openapiPath":"services/legacy-api/sdk/openapi.json"
            }))
            .unwrap(),
        );
        fs::create_dir_all(root.path().join("services/legacy-api")).unwrap();
        fs::write(root.path().join("services/legacy-api/server.ts"), "").unwrap();
        base["services"]["legacy-api"]["working_dir"] = json!("/app/services/legacy-api");
        base["services"]["legacy-api"]["volumes"] = json!([]);
        let (mut planned, native) = plan(root.path(), &manifest, base, &[], true).unwrap();
        typescript::enable(
            root.path(),
            &manifest,
            &BTreeSet::from(["legacy-api".into()]),
            &mut planned,
            &native,
        )
        .unwrap();
        let native_rule = planned["services"]["native-api"]["develop"]["watch"]
            .as_array()
            .unwrap()
            .iter()
            .find(|rule| Path::new(rule["path"].as_str().unwrap()).ends_with("services/native-api"))
            .unwrap();
        let ts_rule = &planned["services"]["legacy-api"]["develop"]["watch"][0];
        let ignored = |rule: &Value, path: &str| {
            rule["ignore"].as_array().unwrap().iter().any(|pattern| {
                glob::Pattern::new(pattern.as_str().unwrap())
                    .unwrap()
                    .matches(path)
            })
        };
        for file in ["contract.json", "openapi.json", "provenance.json"] {
            assert!(ignored(native_rule, &format!("sdk/typescript/{file}")));
            assert!(ignored(
                ts_rule,
                &format!("native-api/sdk/typescript/{file}")
            ));
            let staged = Path::new(file).with_extension("sdk-tmp-1201-0");
            assert!(ignored(
                native_rule,
                &format!("sdk/typescript/{}", staged.display())
            ));
            assert!(ignored(
                ts_rule,
                &format!("native-api/sdk/typescript/{}", staged.display())
            ));
        }
        for file in ["contract.json", "openapi.json"] {
            assert!(ignored(ts_rule, &format!("legacy-api/sdk/{file}")));
        }
        for source in [
            "sdk/typescript/sdk.ts",
            "sdk/typescript/types.ts",
            "sdk/typescript/types.sdk-tmp-1201-0",
            "sdk/typescript/bindings.json",
            "sdk/rust/mod.rs",
            "sdk/rust/mod.sdk-tmp-1201-0",
            ".forklaunch/contracts/lib.rs",
            "domain/schemas/mod.rs",
            "domain/schemas/authored.json",
            "domain/schemas/contract.sdk-tmp-1201-0",
        ] {
            assert!(
                !ignored(native_rule, source),
                "native source ignored: {source}"
            );
            assert!(
                !ignored(ts_rule, &format!("native-api/{source}")),
                "mixed source ignored: {source}"
            );
        }
        for source in [
            "legacy-api/sdk/rust/lib.rs",
            "legacy-api/sdk/index.ts",
            "legacy-api/domain/schemas/api.schema.ts",
        ] {
            assert!(
                !ignored(ts_rule, source),
                "TypeScript source ignored: {source}"
            );
        }
        // Planning remains read-only, including before artifacts have been emitted.
        assert!(!root.path().join("services/native-api/sdk").exists());
        assert!(!root.path().join("services/legacy-api/sdk").exists());
    }

    #[test]
    fn plans_custom_native_paths_and_auxiliary_workers_without_changing_typescript() {
        let (root, manifest, base) = fixture();
        let (planned, changes) = plan(root.path(), &manifest, base.clone(), &[], true).unwrap();
        assert_eq!(
            planned["services"]["legacy-api"],
            base["services"]["legacy-api"]
        );
        assert_eq!(
            planned["services"]["native-outbox"]["command"][1],
            "--worker"
        );
        assert_eq!(
            planned["services"]["native-api"]["command"][0],
            "/app/target/debug/native-api"
        );
        assert_eq!(
            planned["services"]["native-api"]["entrypoint"][2],
            "forklaunch-native-build --package native-core --bin native-api && exec \"$@\""
        );
        assert_eq!(
            planned["services"]["native-api"]["environment"]["PORT"],
            "8000"
        );
        assert_eq!(
            planned["services"]["native-api"]["volumes"]
                .as_array()
                .unwrap()
                .len(),
            5
        );
        let paths: Vec<_> = changes["native-api"]["develop"]["watch"]
            .as_array()
            .unwrap()
            .iter()
            .map(|rule| rule["path"].as_str().unwrap())
            .collect();
        assert!(
            paths.contains(
                &root
                    .path()
                    .canonicalize()
                    .unwrap()
                    .join("shared/models")
                    .to_str()
                    .unwrap()
            )
        );
        assert!(
            paths.contains(
                &root
                    .path()
                    .canonicalize()
                    .unwrap()
                    .join("services/native-api")
                    .to_str()
                    .unwrap()
            )
        );
        assert!(
            !changes["native-api"]["build"]["dockerfile_inline"]
                .as_str()
                .unwrap()
                .contains("COPY . .")
        );
        assert!(!root.path().join("docker-compose.dev.yaml").exists());
    }

    #[test]
    fn rejects_unknown_services_external_paths_and_custom_launches() {
        let (root, mut manifest, mut base) = fixture();
        assert!(
            plan(
                root.path(),
                &manifest,
                base.clone(),
                &["missing".into()],
                true
            )
            .is_err()
        );
        base["services"]["native-api"]["entrypoint"] = json!(["custom"]);
        assert!(plan(root.path(), &manifest, base.clone(), &[], true).is_err());
        manifest["projects"][0]["metadata"]["cratePath"] = toml::Value::String("../outside".into());
        assert!(plan(root.path(), &manifest, base, &[], true).is_err());
    }

    #[test]
    fn selected_typescript_does_not_require_unselected_native_source() {
        let (root, manifest, base) = fixture();
        fs::remove_file(root.path().join("services/native-api/Cargo.toml")).unwrap();
        let (planned, changes) = plan(
            root.path(),
            &manifest,
            base.clone(),
            &["legacy-api".into()],
            false,
        )
        .unwrap();
        assert_eq!(planned, base);
        assert_eq!(changes, json!({}));
    }

    #[test]
    fn infrastructure_containers_are_not_native_crates() {
        let (root, mut manifest, mut base) = fixture();
        manifest["runtime"] = toml::Value::String("tokio".into());
        manifest["projects"]
            .as_array_mut()
            .unwrap()
            .push(toml::from_str("name = 'postgresql'\ntype = 'Service'\n").unwrap());
        base["services"]["postgresql"] = json!({"image":"postgres:17","command":["postgres"]});
        let (planned, changes) = plan(
            root.path(),
            &manifest,
            base.clone(),
            &["postgresql".into()],
            false,
        )
        .unwrap();
        assert_eq!(planned, base);
        assert_eq!(changes, json!({}));
    }

    #[test]
    fn watched_no_build_reuses_images_without_mutating_restart_rules() {
        let mut compose = json!({"name":"fixture", "services": {
            "native": {"image":"native:dev", "build":{"context":"."}, "pull_policy":"build", "develop":{"watch":[{"action":"restart","path":"src"}]}},
            "legacy": {"build":"."},
            "database": {"image":"postgres:17", "pull_policy":"missing"}
        }});
        let restart = compose["services"]["native"]["develop"].clone();
        reuse_images(&mut compose).unwrap();
        assert_eq!(compose["services"]["native"]["image"], "native:dev");
        assert_eq!(compose["services"]["legacy"]["image"], "fixture-legacy");
        for name in ["native", "legacy"] {
            assert!(compose["services"][name].get("build").is_none());
            assert_eq!(compose["services"][name]["pull_policy"], "never");
        }
        assert_eq!(compose["services"]["native"]["develop"], restart);
        assert_eq!(compose["services"]["database"]["pull_policy"], "missing");
        let mut rebuild = json!({"name":"fixture", "services":{"native":{"build":".","develop":{"watch":[{"action":"rebuild","path":"src"}]}}}});
        assert!(
            reuse_images(&mut rebuild)
                .unwrap_err()
                .to_string()
                .contains("rebuild watch rule")
        );
    }

    #[test]
    fn compose_restart_version_and_cli_selection_are_checked() {
        for version in ["2.32.0", "v2.32.1-desktop.1", "5.1.2\n"] {
            assert!(supports_restart(version));
        }
        for version in ["2.31.9", "1.29.0", "invalid"] {
            assert!(!supports_restart(version));
        }
        let command = DevCommand.command().version("test");
        let parsed = command
            .try_get_matches_from(["dev", "native-api", "legacy-api", "--no-deps", "--dry-run"])
            .unwrap();
        assert_eq!(parsed.get_many::<String>("services").unwrap().count(), 2);
        assert!(parsed.get_flag("dryrun"));
    }
}

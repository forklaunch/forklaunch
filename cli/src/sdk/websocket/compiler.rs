//! Socket compilation uses the CLI's checked compiler by default. Explicit
//! schema-engine opt-ins run the selected framework's compiler with those features.
use std::{
    fs,
    io::{Read as _, Seek as _},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Process entry for the selected framework's socket compiler; the artifact
/// was already bounded when the CLI read it.
const PROCESS_ENTRY: &str = r#"use anyhow::{Context, Result};
use std::io::Write as _;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let bytes = std::fs::read(args.next().context("socket contract artifact path")?)?;
    let target = args.next().context("target: typescript, rust or all")?;
    let source = args.next().context("artifact source")?;
    let output = forklaunch_core::build::socket_artifact_sources(&bytes, &target, &source)?;
    std::io::stdout().write_all(&serde_json::to_vec(&output)?)?;
    Ok(())
}
"#;

pub(super) fn compile(
    root: &Path,
    artifact: &Path,
    target: &str,
    source: &str,
    features: &[&str],
) -> Result<Value> {
    let mut bytes = Vec::new();
    fs::File::open(artifact)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Socket artifact exceeds 16 MiB"
    );
    if features.is_empty() {
        return forklaunch_core::build::socket_artifact_sources(&bytes, target, source);
    }
    anyhow::ensure!(
        features
            .iter()
            .all(|feature| matches!(*feature, "zod-uri" | "zod-checks" | "zod-regex")),
        "Unsupported socket compiler feature"
    );
    let directory = tempfile::tempdir()?;
    let manifest = compiler_manifest(root, features)?;
    fs::write(directory.path().join("Cargo.toml"), manifest)?;
    fs::create_dir(directory.path().join("src"))?;
    fs::write(directory.path().join("src/main.rs"), PROCESS_ENTRY)?;
    let input = directory.path().join("artifact.json");
    fs::write(&input, bytes)?;
    let mut output = tempfile::tempfile()?;
    let mut child = Command::new(crate::core::openapi_export::resolve_command("cargo"))
        .args(["run", "--quiet", "--manifest-path"])
        .arg(directory.path().join("Cargo.toml"))
        .arg("--")
        .arg(input)
        .arg(target)
        .arg(source)
        .current_dir(root)
        .env("CARGO_INCREMENTAL", "0")
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(Stdio::inherit())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if let Some(status) = child.try_wait()? {
            anyhow::ensure!(
                status.success(),
                "Selected socket compiler exited with {status}"
            );
            break;
        }
        if Instant::now() >= deadline || output.metadata()?.len() > 64 * 1024 * 1024 {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Selected socket compiler exceeded its time/output limit");
        }
        thread::sleep(Duration::from_millis(100));
    }
    anyhow::ensure!(
        output.metadata()?.len() <= 64 * 1024 * 1024,
        "Socket compiler output exceeds 64 MiB"
    );
    output.rewind()?;
    let mut bytes = Vec::new();
    output.read_to_end(&mut bytes)?;
    serde_json::from_slice(&bytes).context("Invalid selected socket compiler output")
}

fn compiler_manifest(root: &Path, features: &[&str]) -> Result<String> {
    let mut features = features.to_vec();
    features.extend(["build", "asyncapi-import"]);
    let manifest = format!(
        r#"[package]
name = "forklaunch-socket-artifact-compiler"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
anyhow = "1"
serde_json = "1"
forklaunch-core = {{ path = {}, features = {} }}
"#,
        // Rewritten to the application's `[framework]` selection below.
        toml::Value::String(".forklaunch/framework-rs/crates/core".into()),
        toml::Value::Array(
            features
                .into_iter()
                .map(|feature| toml::Value::String(feature.into()))
                .collect()
        )
    );
    let rendered = crate::init::rust_project::dependencies::render_for_path(
        &root.join(".forklaunch/socket-compiler/Cargo.toml"),
        &manifest,
    )?;
    let document: toml::Value = toml::from_str(&rendered)?;
    if let Some(path) = document["dependencies"]["forklaunch-core"]
        .get("path")
        .and_then(toml::Value::as_str)
    {
        anyhow::ensure!(
            Path::new(path).join("Cargo.toml").is_file(),
            "Selected socket compiler framework is missing: {path}"
        );
    }
    Ok(toml::to_string_pretty(&document)?)
}

#[cfg(test)]
mod tests {
    use forklaunch_core::ws::{SocketContract, SocketContractArtifact, SocketSchemaEntry};
    use serde_json::json;

    use super::*;

    #[test]
    fn default_socket_compiler_needs_no_source_tree_or_cargo_process() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir(root.join(".forklaunch")).unwrap();
        { let manifest = root.join(".forklaunch/manifest.toml"); let prior = std::fs::read_to_string(&manifest).unwrap_or_default(); std::fs::write(&manifest, format!("{prior}\n[framework]\n{}\n", "version = '0.1.0'")).unwrap(); }
        let artifact = SocketContractArtifact::new("Fixture", "1").with_socket(
            "chat",
            SocketContract::new("/chat")
                .with_client_message("chat", SocketSchemaEntry::new(json!({"type":"string"}))),
        );
        let input = root.join("socket.json");
        fs::write(&input, serde_json::to_vec(&artifact).unwrap()).unwrap();
        let before = fs::read_dir(root).unwrap().count();
        let output = compile(root, &input, "all", "socket-contract", &[]).unwrap();
        assert!(
            output["typescript"]["socket_chat"]
                .as_str()
                .unwrap()
                .contains("export class SocketClient")
        );
        assert!(
            output["rust"]["socket_chat"]
                .as_str()
                .unwrap()
                .contains("pub struct SocketClient")
        );
        assert_eq!(fs::read_dir(root).unwrap().count(), before);
        assert!(!root.join("vendor").exists());
        assert!(!root.join("Cargo.toml").exists());
        assert!(compile(root, &input, "all", "openapi", &[]).is_err());
    }

    #[test]
    fn optional_socket_compiler_preserves_selected_sources_and_features() {
        for selection in ["registry", "local"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path();
            fs::create_dir(root.join(".forklaunch")).unwrap();
            let expected = root.join("custom/framework/crates/core");
            if selection == "local" {
                fs::create_dir_all(&expected).unwrap();
                fs::write(
                    expected.join("Cargo.toml"),
                    "[package]\nname='forklaunch-core'",
                )
                .unwrap();
            }
            if selection == "registry" {
                { let manifest = root.join(".forklaunch/manifest.toml"); let prior = std::fs::read_to_string(&manifest).unwrap_or_default(); std::fs::write(&manifest, format!("{prior}\n[framework]\n{}\n", "version='0.1.0'")).unwrap(); }
            } else {
                { let manifest = root.join(".forklaunch/manifest.toml"); let prior = std::fs::read_to_string(&manifest).unwrap_or_default(); std::fs::write(&manifest, format!("{prior}\n[framework]\n{}\n", format!(
                        "path={}",
                        toml::Value::String(
                            root.join("custom/framework").to_string_lossy().into_owned()
                        )
                    ))).unwrap(); }
            }
            let document: toml::Value = toml::from_str(
                &compiler_manifest(root, &["zod-checks", "zod-regex"]).unwrap(),
            )
            .unwrap();
            let dependency = &document["dependencies"]["forklaunch-core"];
            if selection == "registry" {
                assert_eq!(dependency["version"].as_str(), Some("=0.1.0"));
                assert!(dependency.get("path").is_none());
            } else {
                assert_eq!(dependency["path"].as_str(), expected.to_str());
            }
            let features = dependency["features"].as_array().unwrap();
            for expected in ["zod-checks", "zod-regex", "build", "asyncapi-import"] {
                assert!(
                    features
                        .iter()
                        .any(|feature| feature.as_str() == Some(expected))
                );
            }
            assert!(document.get("patch").is_none(), "Compilers use upstream serde_json");
            assert!(
                !root.join(".forklaunch/socket-compiler").exists(),
                "Planning must not change app files"
            );
        }
    }
}

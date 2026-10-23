//! Generate HTTP and WebSocket SDKs directly from host-compiled contracts in `build.rs`.
//! This library never invokes Cargo, the CLI, or a service executable.
mod channels;
mod events;
pub mod provenance;
pub use channels::{ChannelContextFields, ChannelEvent, ChannelWire, channel_events};
mod service;
pub mod service_package;
pub use service::{ServiceSdkPaths, application_root, service_sdks, service_sdks_at};
mod http_artifact;
#[cfg(feature = "asyncapi-import")]
mod socket_artifact;
#[cfg(feature = "asyncapi-import")]
pub use socket_artifact::socket_artifact_sources;
pub mod imports;
#[cfg(feature = "typescript-source")]
mod typescript_source;
use anyhow::{Context, Result};
pub use events::worker_events;
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use forklaunch_core::contract::ContractRegistry;
use forklaunch_core::typescript::TypeScriptHttpSdkOptions;
pub use http_artifact::{
    HttpSdkSources, ImportedHttpSdkSources, http_artifact_sdk, http_artifact_sources,
    http_sdk_file_plan, openapi_artifact_sources,
};
use std::{
    fs,
    io::Write as _,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(feature = "typescript-source")]
pub use typescript_source::{
    export_typescript_openapi, export_typescript_openapi_with_runtime, typescript_contract_inputs,
};
static OUTPUT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Generate the SDK and portable contract artifacts. Unchanged files retain their
/// modification times. Output watches recover deleted files. Callers declare their contract inputs using Cargo's
/// `rerun-if-changed` directives; runtime-only dependencies need not be built.
///
/// # Errors
/// Returns schema/generation or filesystem errors, causing the build to fail.
pub fn typescript(
    registry: &ContractRegistry,
    service: &str,
    output: impl AsRef<Path>,
) -> Result<()> {
    typescript_source(registry, service, output.as_ref(), None)?;
    service_package::http(output.as_ref())
}

fn typescript_source(
    registry: &ContractRegistry,
    service: &str,
    output: &Path,
    rust_client_name: Option<&str>,
) -> Result<()> {
    let plan = SdkGenerationPlan::from_registry(SdkTarget::TypeScript, registry);
    // Finish generation before creating or changing any output file.
    let shared_runtime =
        rust_client_name.is_some() && std::env::var_os("FORKLAUNCH_TYPESCRIPT_SDK_DIR").is_none();
    let sdk = crate::typescript::generate_typescript_http_sdk_files_with_runtime(
        &plan,
        &TypeScriptHttpSdkOptions::new(service),
        if shared_runtime {
            "../runtime"
        } else {
            "./runtime"
        },
    )?;
    let mut artifact: serde_json::Value =
        serde_json::from_str(&registry.clone().into_artifact().to_json())?;
    if let Some(name) = rust_client_name {
        artifact["rustClientName"] = serde_json::Value::String(name.to_owned());
        artifact["rustOutputLayout"] = serde_json::Value::String("service-module".to_owned());
    }
    artifact["typescriptRuntimeLayout"] = serde_json::Value::String(
        if shared_runtime {
            "shared-parent"
        } else {
            "local"
        }
        .to_owned(),
    );
    let contract = serde_json::to_string(&artifact)?;
    let openapi = forklaunch_core::openapi::generate_openapi_json(registry);
    for (name, contents) in sdk {
        let target = if shared_runtime && name == "runtime.ts" {
            output
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Missing SDK root"))?
                .join(name)
        } else {
            output.join(name)
        };
        write_output(&target, &contents)?;
    }
    for (name, contents) in [("contract.json", contract), ("openapi.json", openapi)] {
        write_output(&output.join(name), &contents)?;
    }
    Ok(())
}

/// Replace one generated file atomically, preserving unchanged-file timestamps.
/// This helper does not emit Cargo directives and can also be used by CLI watchers.
///
/// # Errors
/// Rejects symlink or non-file destinations, and reports filesystem failures.
pub fn write_generated_output(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    let contents = contents.as_ref();
    match fs::symlink_metadata(path) {
        Ok(meta) => anyhow::ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Generated SDK output must be a regular file: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if fs::read(path).ok().as_deref() != Some(contents) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Editors/watchers must see either complete version, never a truncated SDK.
        let staged = path.with_extension(format!(
            "sdk-tmp-{}-{}",
            std::process::id(),
            OUTPUT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)
            .with_context(|| format!("stage {}", path.display()))?;
        let result = (|| -> std::io::Result<()> {
            file.write_all(contents)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&staged, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staged);
        }
        result.with_context(|| format!("write {}", path.display()))?;
    }
    Ok(())
}

fn write_output(path: &Path, contents: &str) -> Result<()> {
    write_generated_output(path, contents)?;
    settle_generated_mtime(path);
    // A deleted generated file must trigger regeneration even with cached Cargo artifacts.
    println!("cargo:rerun-if-changed={}", path.display());
    Ok(())
}

/// Cargo treats a `rerun-if-changed` file newer than the build script's
/// `invoked.timestamp` as dirty, so output this script just wrote would rerun it
/// on the next build. Backdate freshly written output to that timestamp. Outside
/// a build script, or if the timestamp is missing, leave the file as written.
fn settle_generated_mtime(path: &Path) {
    let Some(out_dir) = std::env::var_os("OUT_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let Some(invoked) = out_dir
        .parent()
        .and_then(|dir| fs::metadata(dir.join("invoked.timestamp")).ok())
        .and_then(|meta| meta.modified().ok())
    else {
        return;
    };
    let Ok(file) = fs::File::options().write(true).open(path) else {
        return;
    };
    if file
        .metadata()
        .and_then(|meta| meta.modified())
        .is_ok_and(|written| written > invoked)
    {
        let _ = file.set_modified(invoked);
    }
}

/// Generate a typed WebSocket client and its portable contract without running a server.
/// # Errors
/// Returns contract, generator or filesystem errors.
pub fn typescript_websocket(
    contract: &forklaunch_core::ws::SocketContract,
    client_name: &str,
    sdk_file: impl AsRef<Path>,
    contract_file: impl AsRef<Path>,
) -> Result<()> {
    let plan = forklaunch_core::codegen::websocket::SocketSdkPlan::from_contract(contract)?;
    let source = forklaunch_core::typescript::generate_typescript_websocket_sdk_source(
        &plan,
        &forklaunch_core::typescript::TypeScriptWebSocketSdkOptions::new(client_name),
    )?;
    let artifact = serde_json::to_string_pretty(contract)?;
    write_output(sdk_file.as_ref(), &source)?;
    let browser_source =
        forklaunch_core::typescript::generate_typescript_browser_websocket_sdk_source(
            &plan,
            &forklaunch_core::typescript::TypeScriptWebSocketSdkOptions::new(client_name),
        )?;
    let stem = sdk_file
        .as_ref()
        .file_stem()
        .context("Socket SDK requires a file stem")?
        .to_string_lossy();
    write_output(
        &sdk_file
            .as_ref()
            .with_file_name(format!("{stem}.browser.ts")),
        &browser_source,
    )?;
    write_output(contract_file.as_ref(), &artifact)?;
    service_package::websocket(
        sdk_file
            .as_ref()
            .parent()
            .context("Socket SDK requires an output directory")?,
    )
}

fn module_source(source: &str) -> String {
    // Generated crates forbid unsafe code at their root. Included modules cannot
    // contain that inner crate attribute; the enclosing SDK crate retains it.
    source
        .lines()
        .filter(|line| *line != "#![forbid(unsafe_code)]")
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Generate an include!-ready Rust HTTP client, normally into Cargo `OUT_DIR`.
/// # Errors
/// Returns schema, generator or filesystem errors.
pub fn rust_http_module(
    registry: &ContractRegistry,
    package: &str,
    client_name: &str,
    output: impl AsRef<Path>,
) -> Result<()> {
    let plan = SdkGenerationPlan::from_registry(SdkTarget::Rust, registry);
    let sources = forklaunch_core::rust_sdk::generate_rust_http_sdk_files(
        &plan,
        &forklaunch_core::rust_sdk::RustSdkOptions::new(package).with_client_struct(client_name),
    )?;
    write_rust_http_files(output.as_ref(), &sources)
}

/// Write a facade at the requested include path and model/client modules beside it.
fn write_rust_http_files(
    output: &Path,
    sources: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid Rust SDK output file"))?;
    anyhow::ensure!(
        stem.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')),
        "Invalid Rust SDK module output name"
    );
    let directory = output.with_file_name(stem);
    for name in ["models.rs", "client.rs", "schemas.rs"] {
        write_output(&directory.join(name), &sources[name])?;
    }
    let facade = rust_http_facade(output, sources)?;
    write_output(output, &facade)
}

/// Render the export facade for a flat service module and its sibling files.
/// # Errors
/// Rejects output paths without a filename stem or missing facade source.
pub fn rust_http_facade(
    output: &Path,
    sources: &std::collections::BTreeMap<String, String>,
) -> Result<String> {
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid Rust SDK output file"))?;
    let models_path = format!("{stem}/models.rs");
    let client_path = format!("{stem}/client.rs");
    let schemas_path = format!("{stem}/schemas.rs");
    let facade = sources
        .get("lib.rs")
        .context("Missing Rust SDK facade source")?
        .replace(
            "mod models;",
            &format!("#[path = {models_path:?}] mod models;"),
        )
        .replace(
            "mod schemas;",
            &format!("#[path = {schemas_path:?}] mod schemas;"),
        )
        .replace(
            "mod client;",
            &format!("#[path = {client_path:?}] mod client;"),
        );
    Ok(facade)
}

/// Generate an include!-ready Rust WebSocket client, normally into Cargo `OUT_DIR`.
/// # Errors
/// Returns contract, generator or filesystem errors.
pub fn rust_websocket_module(
    contract: &forklaunch_core::ws::SocketContract,
    package: &str,
    client_name: &str,
    output: impl AsRef<Path>,
) -> Result<()> {
    let plan = forklaunch_core::codegen::websocket::SocketSdkPlan::from_contract(contract)?;
    let source = forklaunch_core::rust_sdk::generate_rust_websocket_sdk_source(
        &plan,
        &forklaunch_core::rust_sdk::RustSdkOptions::new(package).with_client_struct(client_name),
    )?;
    write_output(output.as_ref(), &module_source(&source))
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_json_keeps_declaration_order_in_every_build() {
        // A lone package build and a workspace build unify serde_json
        // features differently; artifacts must not depend on which ran.
        let value =
            serde_json::json!({"name": "x", "version": "0", "dependencies": {"b": 1, "a": 2}});
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            r#"{"name":"x","version":"0","dependencies":{"b":1,"a":2}}"#
        );
    }

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(unix)]
    #[test]
    fn output_replacement_does_not_truncate_existing_readers_or_follow_symlinks() {
        use std::io::Read as _;
        let directory =
            std::env::temp_dir().join(format!("forklaunch-build-atomic-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let file = directory.join("client.ts");
        write_output(&file, "previous complete SDK").unwrap();
        let mut reader = fs::File::open(&file).unwrap();
        write_output(&file, "next complete SDK").unwrap();
        let mut old = String::new();
        reader.read_to_string(&mut old).unwrap();
        assert_eq!(old, "previous complete SDK");
        assert_eq!(fs::read_to_string(&file).unwrap(), "next complete SDK");
        let link = directory.join("linked.ts");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(write_output(&link, "overwrite").is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "next complete SDK");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn output_recovery_preserves_unchanged_file() {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "forklaunch-build-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = directory.join("nested/client.ts");
        write_output(&file, "export type Note = string;\n").unwrap();
        let modified = fs::metadata(&file).unwrap().modified().unwrap();
        write_output(&file, "export type Note = string;\n").unwrap();
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), modified);
        fs::remove_file(&file).unwrap();
        write_output(&file, "export type Note = string;\n").unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "export type Note = string;\n"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}

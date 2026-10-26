//! Offline raw socket artifacts use the framework compiler embedded in the CLI.
use std::{
    collections::BTreeMap,
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use termcolor::{StandardStream, WriteColor};

use crate::core::artifact_paths::{self, Artifact};

mod compiler;
mod ownership;

#[cfg(test)]
use std::process::Command;

pub(crate) fn generate_from(
    root: &Path,
    target: &str,
    source: &str,
    selected: Option<&str>,
    check: bool,
    stdout: &mut StandardStream,
    allow_absent: bool,
) -> Result<()> {
    anyhow::ensure!(
        matches!(source, "socket-contract" | "asyncapi"),
        "Invalid socket SDK source"
    );
    let _ = zod_uri_enabled(std::env::var_os("FORKLAUNCH_SDK_ZOD_URI").as_deref())?;
    let _ = zod_checks_enabled(std::env::var_os("FORKLAUNCH_SDK_ZOD_CHECKS").as_deref())?;
    let _ = zod_regex_enabled(std::env::var_os("FORKLAUNCH_SDK_ZOD_REGEX").as_deref())?;
    let imported = source == "asyncapi";
    let root = root.canonicalize()?;
    let manifest: toml::Value =
        toml::from_str(&fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?;
    let mut files = BTreeMap::new();
    let mut members = Vec::new();
    let mut retired_members = Vec::new();
    let mut removals = BTreeMap::new();
    let mut count = 0;
    let mut inputs = Vec::new();
    let mut registry_snapshots = Vec::new();
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
        let name = project
            .get("name")
            .and_then(toml::Value::as_str)
            .context("Socket service name missing")?;
        if selected.is_some_and(|selected| selected != name) {
            continue;
        }
        // A separately imported language still depends on its document when
        // another target is regenerated from native socket contracts.
        if !imported && target != "all" {
            let other = if target == "rust" {
                "typescript"
            } else {
                "rust"
            };
            if ownership::imported_target(&root, name, other)? {
                let input = imported_asyncapi(project, name);
                let path = root.join(input).canonicalize()?;
                anyhow::ensure!(
                    path.starts_with(&root),
                    "Socket artifact path must stay inside the application"
                );
                inputs.push(path);
            }
        }
        let configured = project
            .get("metadata")
            .and_then(|metadata| {
                metadata.get(if imported {
                    "asyncapiArtifactPath"
                } else {
                    "socketContractArtifactPath"
                })
            })
            .and_then(toml::Value::as_str);
        let metadata = project.get("metadata").and_then(toml::Value::as_table);
        let artifact = root.join(configured.map_or_else(
            || {
                if imported {
                    imported_asyncapi(project, name)
                } else {
                    artifact_paths::path(name, metadata, Artifact::SocketContract)
                }
            },
            str::to_owned,
        ));
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            bail!("Invalid socket service name {name}");
        }
        if !artifact.exists() {
            let mut retired = false;
            for language in ["typescript", "rust"] {
                if !imported
                    && (target == "all" || target == language)
                    && ownership::stage_empty_registry(
                        &root,
                        &mut files,
                        &mut removals,
                        name,
                        language,
                        project,
                    )?
                {
                    retired = true;
                    if language == "rust" {
                        retired_members.push(
                            crate::sdk::sdk_base(&root)?
                                .join(format!("rust/{name}-websocket-sdk"))
                                .to_string_lossy()
                                .into_owned(),
                        );
                    }
                }
            }
            if retired {
                ownership::stage_empty_asyncapi(&root, &mut files, &mut removals, name)?;
                count += 1;
                continue;
            }
            if configured.is_none() && (selected.is_none() || allow_absent) {
                continue;
            }
        }
        let artifact = artifact
            .canonicalize()
            .with_context(|| format!("Missing socket contract artifact {}", artifact.display()))?;
        if !artifact.starts_with(&root) {
            bail!("Socket artifact path must stay inside the application");
        }
        inputs.push(artifact.clone());
        let registry = if imported {
            None
        } else {
            ownership::registry_snapshot(&root, name, &artifact, project)?
        };
        let registry_exported = registry.is_some();
        registry_snapshots.extend(registry);
        let compiled = compile(&root, &artifact, target, source)
            .with_context(|| format!("Socket artifact compilation failed for {name}"))?;
        if compiled["asyncapi"]["asyncapi"] != "3.0.0" || !compiled["asyncapi"]["info"].is_object()
        {
            bail!("Invalid compiled AsyncAPI document");
        }
        if !imported {
            let asyncapi_output = artifact_paths::path(name, metadata, Artifact::AsyncApi);
            stage(&mut files, &asyncapi_output, pretty(&compiled["asyncapi"])?)?;
            ownership::stage_asyncapi(
                &root,
                &mut files,
                &mut removals,
                name,
                Path::new(&asyncapi_output),
                registry_exported,
            )?;
        } else {
            ownership::release_asyncapi(&root, &mut removals, name)?;
        }
        if matches!(target, "typescript" | "all") {
            stage_typescript(&root, &mut files, name, &compiled)?;
            ownership::stage_package(
                &root,
                &mut files,
                &mut removals,
                name,
                "typescript",
                registry_exported,
                source,
            )?;
        }
        if matches!(target, "rust" | "all") {
            let member = crate::sdk::sdk_base(&root)?
                .join(format!("rust/{name}-websocket-sdk"))
                .to_string_lossy()
                .into_owned();
            stage_rust(&root, &mut files, name, &member, &compiled)?;
            ownership::stage_package(
                &root,
                &mut files,
                &mut removals,
                name,
                "rust",
                registry_exported,
                source,
            )?;
            members.push(member);
        }
        count += 1;
    }
    if count == 0 {
        if allow_absent {
            return Ok(());
        }
        bail!("No socket contract artifacts found for the selected services");
    }
    stage_workspace(&root, &mut files, &members, target)?;
    if !retired_members.is_empty() {
        let cargo = files.get_mut(Path::new("Cargo.toml"));
        let content = match cargo.as_ref() {
            Some(content) => content.to_string(),
            None => fs::read_to_string(root.join("Cargo.toml"))?,
        };
        let mut document = content.parse::<toml_edit::DocumentMut>()?;
        let array = document["workspace"]["members"]
            .as_array_mut()
            .context("Root Cargo workspace members missing")?;
        array.retain(|value| {
            !value
                .as_str()
                .is_some_and(|member| retired_members.iter().any(|retired| retired == member))
        });
        files.insert(PathBuf::from("Cargo.toml"), document.to_string());
    }
    for relative in files.keys() {
        validate_destination(&root, relative)?;
        if root
            .join(relative)
            .canonicalize()
            .ok()
            .is_some_and(|path| inputs.contains(&path))
        {
            bail!(
                "Socket SDK output would overwrite an input artifact: {}",
                relative.display()
            );
        }
    }
    for relative in removals.keys() {
        validate_destination(&root, relative)?;
        if files.contains_key(relative) {
            bail!("Conflicting socket output removal: {}", relative.display());
        }
    }
    for snapshot in registry_snapshots {
        snapshot.verify(&root)?;
    }
    if check {
        if let Some(relative) = removals.keys().next() {
            bail!("Stale generated socket SDK output: {}", relative.display());
        }
        for (relative, content) in &files {
            if fs::read_to_string(root.join(relative)).ok().as_deref() != Some(content) {
                bail!("Socket SDK output is out of date: {}", relative.display());
            }
        }
        log_ok!(
            stdout,
            "{}",
            if imported {
                "Imported socket SDK outputs are up to date"
            } else {
                "Socket SDK and AsyncAPI outputs are up to date"
            }
        );
    } else {
        ownership::remove_unchanged(&root, &removals)?;
        write_staged_outputs(&root, &files)?;
        log_ok!(
            stdout,
            "Generated socket SDKs{} for {count} service(s)",
            if imported {
                " from AsyncAPI"
            } else {
                " and AsyncAPI"
            }
        );
    }
    Ok(())
}

/// An imported AsyncAPI document defaults to the location socket export writes.
fn imported_asyncapi(project: &toml::Value, name: &str) -> String {
    let metadata = project.get("metadata").and_then(toml::Value::as_table);
    metadata
        .and_then(|metadata| metadata.get("asyncapiArtifactPath"))
        .and_then(toml::Value::as_str)
        .map_or_else(
            || artifact_paths::path(name, metadata, Artifact::AsyncApi),
            str::to_owned,
        )
}

fn write_staged_outputs(root: &Path, files: &BTreeMap<PathBuf, String>) -> Result<()> {
    // Watchers use ownership hashes to observe completed generation. Publish
    // those receipts only after every source and package file is on disk.
    for receipts in [false, true] {
        for (relative, content) in files {
            if ownership::is_receipt(relative) != receipts {
                continue;
            }
            let destination = root.join(relative);
            fs::create_dir_all(destination.parent().context("Socket output parent")?)?;
            super::write_if_changed(destination, content)?;
        }
    }
    Ok(())
}
fn zod_uri_enabled(value: Option<&std::ffi::OsStr>) -> Result<bool> {
    match value {
        None => Ok(false),
        Some(value) if value == "0" => Ok(false),
        Some(value) if value == "1" => Ok(true),
        Some(_) => bail!("FORKLAUNCH_SDK_ZOD_URI must be unset, 0 or 1"),
    }
}
#[cfg(test)]
fn configure_uri_feature(command: &mut Command, value: Option<&std::ffi::OsStr>) -> Result<()> {
    if zod_uri_enabled(value)? {
        command.args(["--features", "zod-uri"]);
    }
    Ok(())
}
fn zod_checks_enabled(value: Option<&std::ffi::OsStr>) -> Result<bool> {
    match value {
        None => Ok(false),
        Some(value) if value == "0" => Ok(false),
        Some(value) if value == "1" => Ok(true),
        Some(_) => bail!("FORKLAUNCH_SDK_ZOD_CHECKS must be unset, 0 or 1"),
    }
}
#[cfg(test)]
fn configure_checks_feature(command: &mut Command, value: Option<&std::ffi::OsStr>) -> Result<()> {
    if zod_checks_enabled(value)? {
        command.args(["--features", "zod-checks"]);
    }
    Ok(())
}
fn zod_regex_enabled(value: Option<&std::ffi::OsStr>) -> Result<bool> {
    match value {
        None => Ok(false),
        Some(value) if value == "0" => Ok(false),
        Some(value) if value == "1" => Ok(true),
        Some(_) => bail!("FORKLAUNCH_SDK_ZOD_REGEX must be unset, 0 or 1"),
    }
}
#[cfg(test)]
fn configure_regex_feature(command: &mut Command, value: Option<&std::ffi::OsStr>) -> Result<()> {
    if zod_regex_enabled(value)? {
        command.args(["--features", "zod-regex"]);
    }
    Ok(())
}
fn compile(root: &Path, artifact: &Path, target: &str, source: &str) -> Result<Value> {
    let mut features = Vec::new();
    if zod_uri_enabled(std::env::var_os("FORKLAUNCH_SDK_ZOD_URI").as_deref())? {
        features.push("zod-uri");
    }
    if zod_checks_enabled(std::env::var_os("FORKLAUNCH_SDK_ZOD_CHECKS").as_deref())? {
        features.push("zod-checks");
    }
    if zod_regex_enabled(std::env::var_os("FORKLAUNCH_SDK_ZOD_REGEX").as_deref())? {
        features.push("zod-regex");
    }
    compiler::compile(root, artifact, target, source, &features)
}

fn pretty(value: &Value) -> Result<String> {
    Ok(format!("{}\n", serde_json::to_string_pretty(value)?))
}
fn stage(
    files: &mut BTreeMap<PathBuf, String>,
    path: impl Into<PathBuf>,
    content: String,
) -> Result<()> {
    let path = path.into();
    if path.is_absolute()
        || path.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        bail!("Invalid socket output path");
    }
    let normalized: PathBuf = path
        .components()
        .filter(|component| !matches!(component, std::path::Component::CurDir))
        .collect();
    if normalized.as_os_str().is_empty() {
        bail!("Socket output path must name a file");
    }
    if files.insert(normalized, content).is_some() {
        bail!("Conflicting socket output paths");
    }
    Ok(())
}
fn sources<'a>(compiled: &'a Value, target: &str) -> Result<&'a serde_json::Map<String, Value>> {
    let sources = compiled[target]
        .as_object()
        .context("Socket compiler sources missing")?;
    if sources.is_empty() {
        bail!("Socket compiler returned no {target} sources");
    }
    for (name, source) in sources {
        if name.len() > 135
            || !name.starts_with("socket_")
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            || !source.is_string()
        {
            bail!("Invalid compiled socket module {name}");
        }
    }
    Ok(sources)
}
fn stage_typescript(
    root: &Path,
    files: &mut BTreeMap<PathBuf, String>,
    name: &str,
    compiled: &Value,
) -> Result<()> {
    let base = crate::sdk::sdk_base(root)?;
    let native = base != Path::new("sdks");
    let directory = base
        .join(format!("typescript-websocket/{name}"))
        .to_string_lossy()
        .into_owned();
    let source_directory = if native {
        directory.clone()
    } else {
        format!("{directory}/src")
    };
    let mut index = String::new();
    for (module, source) in sources(compiled, "typescript")? {
        index.push_str(&format!("export * as {module} from './{module}.js';\n"));
        stage(
            files,
            format!("{source_directory}/{module}.ts"),
            source.as_str().unwrap().into(),
        )?;
    }
    stage(files, format!("{source_directory}/index.ts"), index)?;
    stage(
        files,
        format!("{directory}/package.json"),
        pretty(&json!({
            "name":format!("{name}-websocket-sdk"),"version":"0.1.0","private":true,"type":"module",
            "main":"./dist/index.js","types":"./dist/index.d.ts","exports":{".":{"types":"./dist/index.d.ts","import":"./dist/index.js"}},
            "scripts":{"build":"tsc --project tsconfig.json","typecheck":"tsc --project tsconfig.json --noEmit"},
            "dependencies":{"ws":"^8.22.0","ajv":"^8.20.0","ajv-draft-04":"^1.0.0","ajv-formats":"^3.0.1"},
            "devDependencies":{"typescript":"^5.9.3","@types/ws":"^8.18.1","@types/node":"^22.0.0"}
        }))?,
    )?;
    let config = format!("{directory}/tsconfig.json");
    if !root.join(&config).exists() {
        stage(
            files,
            config,
            pretty(
                &json!({"compilerOptions":{"strict":true,"target":"ES2022","module":"NodeNext","moduleResolution":"NodeNext","esModuleInterop":true,"declaration":true,"outDir":"dist","types":["node"]},"include":[if native { "*.ts" } else { "src/**/*.ts" }]}),
            )?,
        )?;
    }
    Ok(())
}
fn stage_rust(
    root: &Path,
    files: &mut BTreeMap<PathBuf, String>,
    name: &str,
    directory: &str,
    compiled: &Value,
) -> Result<()> {
    let native = crate::sdk::sdk_base(root)? != Path::new("sdks");
    let source_directory = if native {
        directory.to_owned()
    } else {
        format!("{directory}/src")
    };
    let lib = if native {
        "[lib]\npath = \"lib.rs\"\n"
    } else {
        ""
    };
    let framework = format!(
        "{}.forklaunch/framework-rs",
        "../".repeat(Path::new(directory).components().count())
    );
    let mut index = String::from("#![forbid(unsafe_code)]\n");
    for (module, source) in sources(compiled, "rust")? {
        index.push_str(&format!("pub mod {module};\n"));
        stage(
            files,
            format!("{source_directory}/{module}.rs"),
            source.as_str().unwrap().into(),
        )?;
    }
    stage(files, format!("{source_directory}/lib.rs"), index)?;
    let manifest = format!(
        r#"[package]
name = "{name}-websocket-sdk"
version = "0.1.0"
edition = "2024"

{lib}
[features]
default = []
zod-uri = ["forklaunch-ws/zod-uri"]
zod-checks = ["forklaunch-ws/zod-checks"]
zod-regex = ["forklaunch-ws/zod-regex"]

[dependencies]
forklaunch-ws = {{ path = "{framework}/crates/ws", features = ["transport", "json-schema"] }}
serde = {{ features = ["derive"] }}
serde_json = {{}}
tokio = {{ features = ["net"] }}
tokio-tungstenite = {{}}
"#
    );
    let manifest_path = format!("{directory}/Cargo.toml");
    let manifest = crate::init::rust_project::dependencies::render_for_path(
        &root.join(&manifest_path),
        &manifest,
    )?;
    stage(files, manifest_path, manifest)?;
    Ok(())
}
fn stage_workspace(
    root: &Path,
    files: &mut BTreeMap<PathBuf, String>,
    members: &[String],
    target: &str,
) -> Result<()> {
    if !members.is_empty() {
        let mut cargo =
            fs::read_to_string(root.join("Cargo.toml"))?.parse::<toml_edit::DocumentMut>()?;
        let array = cargo["workspace"]["members"]
            .as_array_mut()
            .context("Root Cargo workspace members missing")?;
        for member in members {
            if !array.iter().any(|value| value.as_str() == Some(member)) {
                array.push(member);
            }
        }
        stage(files, "Cargo.toml", cargo.to_string())?;
    }
    if matches!(target, "typescript" | "all") {
        let content = match fs::read_to_string(root.join("pnpm-workspace.yaml")) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "packages: []\n".into(),
            Err(error) => return Err(error.into()),
        };
        let mut workspace: serde_yml::Value = serde_yml::from_str(&content)?;
        let packages = workspace
            .get_mut("packages")
            .and_then(serde_yml::Value::as_sequence_mut)
            .context("pnpm workspace packages must be an array")?;
        let pattern = crate::sdk::sdk_base(root)?
            .join("typescript-websocket/*")
            .to_string_lossy()
            .into_owned();
        let sdk = serde_yml::Value::String(pattern.clone());
        let updated = if packages.contains(&sdk) {
            content
        } else {
            packages.push(sdk);
            if let Some((offset, indent)) = content
                .split_inclusive('\n')
                .scan(0, |offset, line| {
                    let start = *offset;
                    *offset += line.len();
                    Some((start, line))
                })
                .find_map(|(offset, line)| {
                    (line.trim() == "packages:").then_some((
                        offset + line.len(),
                        line.len() - line.trim_start().len() + 2,
                    ))
                })
            {
                // Append after existing members so exclusions and comments
                // retain their order, using the existing sequence indentation.
                let mut insertion = offset;
                let mut sequence_indent = indent;
                let mut cursor = offset;
                for line in content[offset..].split_inclusive('\n') {
                    let trimmed = line.trim();
                    if trimmed.starts_with("- ") {
                        sequence_indent = line.len() - line.trim_start().len();
                        insertion = cursor + line.len();
                    } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                        break;
                    }
                    cursor += line.len();
                }
                let separator = if content[..insertion].ends_with('\n') {
                    ""
                } else {
                    "\n"
                };
                let mut updated = content;
                updated.insert_str(
                    insertion,
                    &format!("{separator}{}- '{pattern}'\n", " ".repeat(sequence_indent)),
                );
                updated
            } else {
                serde_yml::to_string(&workspace)?
            }
        };
        serde_yml::from_str::<serde_yml::Value>(&updated)
            .context("Invalid generated pnpm workspace")?;
        // Shared planning also supplies Bun membership and mixed TS modules.
        // Keep the existing comment-preserving YAML edit when it already has
        // the complete planned membership/policies.
        let planned =
            super::workspace::plan_typescript_sdk(root, super::workspace::SdkWorkspace::WebSocket)?;
        for (path, content) in planned {
            if path == "pnpm-workspace.yaml" {
                let final_content = if serde_yml::from_str::<serde_yml::Value>(&updated)?
                    == serde_yml::from_str::<serde_yml::Value>(&content)?
                {
                    updated.clone()
                } else {
                    content
                };
                stage(files, path, final_content)?;
            } else {
                stage(files, path, content)?;
            }
        }
    }
    Ok(())
}
fn validate_destination(root: &Path, relative: &Path) -> Result<()> {
    crate::core::application_output::output_path(root, relative).map(|_| ())
}

pub(crate) fn cleanup_service(
    root: &Path,
    name: &str,
    metadata: Option<&toml::Table>,
) -> Result<bool> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("Invalid socket service name {name}");
    }
    ownership::cleanup_service(&root.canonicalize()?, name, metadata)
}

pub(crate) fn rename_service(
    root: &Path,
    old: &str,
    new: &str,
    stdout: &mut StandardStream,
) -> Result<()> {
    ownership::rename_service(&root.canonicalize()?, old, new, stdout)
}

#[cfg(test)]
mod tests {
    #[test]
    fn socket_registry_export_gap_preserves_edited_sdk_before_retirement() {
        use super::*;
        fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
            let mut files = BTreeMap::new();
            let mut directories = vec![root.to_path_buf()];
            while let Some(directory) = directories.pop() {
                for entry in fs::read_dir(directory).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        directories.push(path);
                    } else {
                        files.insert(
                            path.strip_prefix(root).unwrap().into(),
                            fs::read(path).unwrap(),
                        );
                    }
                }
            }
            files
        }
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        fs::create_dir_all(root.join(".forklaunch")).unwrap();
        fs::write(root.join(".forklaunch/manifest.toml"),
            "[[projects]]\nname='realtime'\ntype='Service'\n[projects.metadata]\ncontractArtifactPath='custom/registry.json'\n").unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\nmembers=[]\n").unwrap();
        fs::create_dir_all(root.join("custom")).unwrap();
        let portable: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/rust_socket_contract.json"
        ))
        .unwrap();
        let mut registry = json!({"generator":"framework-rs","service":{"name":"realtime","version":"1.0.0"},"sockets":portable["sockets"]});
        let contract = root.join("custom/registry.json");
        fs::write(&contract, serde_json::to_vec(&registry).unwrap()).unwrap();
        crate::core::openapi_export::export_registered_sockets(&root, "realtime", None, &registry)
            .unwrap();
        let mut stdout = StandardStream::stdout(termcolor::ColorChoice::Never);
        generate_from(
            &root,
            "all",
            "socket-contract",
            Some("realtime"),
            false,
            &mut stdout,
            false,
        )
        .unwrap();
        let edited = root.join("sdks/typescript-websocket/realtime/src/socket_chat.ts");
        let text = format!(
            "{}\n// authored obsolete source\n",
            fs::read_to_string(&edited).unwrap()
        );
        fs::write(&edited, &text).unwrap();
        // Compilation updates its registry before the runtime export can remove
        // the old portable artifact. Both generation and check must be read-only.
        registry["sockets"] = json!({});
        fs::write(&contract, serde_json::to_vec(&registry).unwrap()).unwrap();
        let before = snapshot(&root);
        for check in [false, true] {
            let error = generate_from(
                &root,
                "all",
                "socket-contract",
                Some("realtime"),
                check,
                &mut stdout,
                false,
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("does not match the current service registry"),
                "{error:#}"
            );
            assert_eq!(snapshot(&root), before);
        }
        crate::core::openapi_export::export_registered_sockets(&root, "realtime", None, &registry)
            .unwrap();
        generate_from(
            &root,
            "all",
            "socket-contract",
            Some("realtime"),
            false,
            &mut stdout,
            false,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(edited).unwrap(), text);
        assert!(
            !root
                .join("sdks/typescript-websocket/realtime/package.json")
                .exists()
        );
        assert!(
            !root
                .join("sdks/rust/realtime-websocket-sdk/Cargo.toml")
                .exists()
        );
    }

    #[test]
    fn socket_ownership_receipts_remain_old_until_all_outputs_are_written() {
        use super::*;
        let root = tempfile::tempdir().unwrap();
        let receipt = PathBuf::from("sdks/rust/.chat-websocket-sdk.forklaunch.json");
        fs::create_dir_all(root.path().join(receipt.parent().unwrap())).unwrap();
        fs::write(root.path().join(&receipt), "previous verified outputs").unwrap();
        // A blocked destination fails after another SDK source was updated.
        // Its receipt must not advertise a completed generation.
        fs::create_dir_all(root.path().join("src/z.rs")).unwrap();
        let files = BTreeMap::from([
            (receipt.clone(), "next verified outputs".to_owned()),
            (PathBuf::from("src/a.rs"), "pub const A: u8 = 1;".to_owned()),
            (PathBuf::from("src/z.rs"), "pub const Z: u8 = 2;".to_owned()),
        ]);
        assert!(write_staged_outputs(root.path(), &files).is_err());
        assert_eq!(
            fs::read_to_string(root.path().join(&receipt)).unwrap(),
            "previous verified outputs"
        );
        fs::remove_dir(root.path().join("src/z.rs")).unwrap();
        write_staged_outputs(root.path(), &files).unwrap();
        for (path, source) in files {
            assert_eq!(fs::read_to_string(root.path().join(path)).unwrap(), source);
        }
    }
    #[test]
    fn socket_sdk_respects_registry_and_local_framework_selection() {
        for modules in ["src", "modules"] {
            for config in ["version = '0.1.0'", "path = '/custom/framework'"] {
                let temporary = tempfile::tempdir().unwrap();
                let root = temporary.path();
                fs::create_dir_all(root.join(".forklaunch")).unwrap();
                fs::write(
                    root.join(".forklaunch/manifest.toml"),
                    format!("modules_path = '{modules}'\n"),
                )
                .unwrap();
                { let manifest = root.join(".forklaunch/manifest.toml"); let prior = std::fs::read_to_string(&manifest).unwrap_or_default(); std::fs::write(&manifest, format!("{prior}\n[framework]\n{}\n", config)).unwrap(); }
                let directory = if modules == "src" {
                    "src/client-sdk/rust/notes-websocket-sdk"
                } else {
                    "sdks/rust/notes-websocket-sdk"
                };
                let compiled = json!({"rust": {"socket_notes": "pub const READY: bool = true;"}});
                let mut files = BTreeMap::new();
                stage_rust(root, &mut files, "notes", directory, &compiled).unwrap();
                let document: toml::Value =
                    toml::from_str(&files[Path::new(&format!("{directory}/Cargo.toml"))]).unwrap();
                let dependency = &document["dependencies"]["forklaunch-ws"];
                assert_eq!(
                    dependency["features"].as_array().unwrap(),
                    &vec![
                        toml::Value::String("transport".into()),
                        toml::Value::String("json-schema".into()),
                    ]
                );
                if config.starts_with("version") {
                    assert_eq!(dependency["version"].as_str(), Some("=0.1.0"));
                    assert!(dependency.get("path").is_none());
                } else {
                    assert_eq!(
                        dependency["path"].as_str(),
                        Some("/custom/framework/crates/ws")
                    );
                }
                assert_eq!(
                    document["features"]["zod-uri"][0].as_str(),
                    Some("forklaunch-ws/zod-uri")
                );
                assert!(
                    !root.join(directory).exists(),
                    "planning must not write SDK files"
                );
            }
        }
    }

    #[test]
    fn native_socket_outputs_share_client_sdk_layout_and_flat_sources() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join(".forklaunch")).unwrap();
        fs::write(
            root.join(".forklaunch/manifest.toml"),
            "language = \"mixed\"\nmodules_path = \"src\"\n",
        )
        .unwrap();
        let compiled = json!({"typescript": {"socket_notes": "export const ready = true;"}, "rust": {"socket_notes": "pub const READY: bool = true;"}});
        let mut files = BTreeMap::new();
        stage_typescript(root, &mut files, "notes", &compiled).unwrap();
        stage_rust(
            root,
            &mut files,
            "notes",
            "src/client-sdk/rust/notes-websocket-sdk",
            &compiled,
        )
        .unwrap();
        assert!(files.contains_key(Path::new(
            "src/client-sdk/typescript-websocket/notes/index.ts"
        )));
        assert!(files.contains_key(Path::new("src/client-sdk/rust/notes-websocket-sdk/lib.rs")));
        let cargo: toml::Value =
            toml::from_str(&files[Path::new("src/client-sdk/rust/notes-websocket-sdk/Cargo.toml")])
                .unwrap();
        assert_eq!(cargo["lib"]["path"].as_str(), Some("lib.rs"));
        assert_eq!(
            cargo["dependencies"]["forklaunch-ws"]["path"].as_str(),
            Some("../../../../.forklaunch/framework-rs/crates/ws")
        );
    }

    #[test]
    fn uri_generator_opt_in_is_explicit() {
        use std::ffi::OsStr;
        assert!(!super::zod_uri_enabled(None).unwrap());
        assert!(!super::zod_uri_enabled(Some(OsStr::new("0"))).unwrap());
        assert!(super::zod_uri_enabled(Some(OsStr::new("1"))).unwrap());
        let mut command = Command::new("cargo");
        configure_uri_feature(&mut command, Some(OsStr::new("1"))).unwrap();
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["--features", "zod-uri"]
        );
        let mut default = Command::new("cargo");
        configure_uri_feature(&mut default, None).unwrap();
        assert_eq!(default.get_args().count(), 0);
        for value in ["", "true", "2", " 1"] {
            assert!(super::zod_uri_enabled(Some(OsStr::new(value))).is_err());
        }
    }

    use super::*;
    #[test]
    fn combined_generation_allows_absent_sockets_but_requires_explicit_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir(root.join(".forklaunch")).unwrap();
        let manifest = root.join(".forklaunch/manifest.toml");
        let base = "[[projects]]\nname='api'\ntype='Service'\n";
        fs::write(&manifest, base).unwrap();
        let mut stdout = StandardStream::stdout(termcolor::ColorChoice::Never);
        assert!(
            generate_from(
                root,
                "typescript",
                "socket-contract",
                Some("api"),
                false,
                &mut stdout,
                true
            )
            .is_ok()
        );
        assert!(
            generate_from(
                root,
                "typescript",
                "socket-contract",
                Some("api"),
                false,
                &mut stdout,
                false
            )
            .is_err()
        );
        assert!(!root.join("sdks").exists());
        fs::write(
            &manifest,
            format!("{base}[projects.metadata]\nsocketContractArtifactPath='missing.json'\n"),
        )
        .unwrap();
        assert!(
            generate_from(
                root,
                "typescript",
                "socket-contract",
                Some("api"),
                false,
                &mut stdout,
                true
            )
            .is_err()
        );
        assert!(!root.join("sdks").exists());
    }

    #[test]
    fn compiled_module_names_cannot_choose_output_paths() {
        assert!(sources(&json!({"rust":{"../escape":"source"}}), "rust").is_err());
        assert!(sources(&json!({"rust":{"socket_ok":null}}), "rust").is_err());
        assert!(sources(&json!({"rust":{}}), "rust").is_err());
        assert!(sources(&json!({"rust":{"socket_type":"source"}}), "rust").is_ok());
    }
    #[test]
    fn workspace_registration_preserves_cargo_comments_and_other_packages() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::write(
            root.join("Cargo.toml"),
            "# user comment\n[workspace]\nmembers = [\"crates/chat\"]\n",
        )
        .unwrap();
        fs::write(
            root.join("pnpm-workspace.yaml"),
            "# user pnpm comment\npackages:\n  - crates/*\n",
        )
        .unwrap();
        let mut files = BTreeMap::new();
        stage_workspace(
            root,
            &mut files,
            &["sdks/rust/chat-websocket-sdk".into()],
            "all",
        )
        .unwrap();
        let cargo = &files[Path::new("Cargo.toml")];
        assert!(cargo.starts_with("# user comment"));
        assert!(cargo.contains("crates/chat"));
        assert!(cargo.contains("sdks/rust/chat-websocket-sdk"));
        let pnpm = &files[Path::new("pnpm-workspace.yaml")];
        assert!(pnpm.starts_with("# user pnpm comment"));
        assert!(pnpm.contains("crates/*"));
        assert!(pnpm.contains("sdks/typescript-websocket/*"));
    }
    #[test]
    fn socket_workspace_preserves_yaml_sequence_layout() {
        for source in [
            "# owned\npackages:\n- modules/*",
            "# owned\npackages:\n  - modules/*\n# policy comment\nminimumReleaseAge: 1440\n",
        ] {
            let root = tempfile::tempdir().unwrap();
            fs::write(root.path().join("pnpm-workspace.yaml"), source).unwrap();
            let mut files = BTreeMap::new();
            stage_workspace(root.path(), &mut files, &[], "typescript").unwrap();
            let output = &files[Path::new("pnpm-workspace.yaml")];
            assert!(output.starts_with("# owned\n"));
            if source.contains("# policy comment") {
                assert!(output.contains("# policy comment"));
            }
            let value: serde_yml::Value = serde_yml::from_str(output).unwrap();
            assert_eq!(value["packages"].as_sequence().unwrap().len(), 2);
            assert_eq!(
                fs::read_to_string(root.path().join("pnpm-workspace.yaml")).unwrap(),
                source
            );
            assert!(!root.path().join("package.json").exists());
        }
    }

    #[test]
    fn explicit_zod_checks_compiler_feature_is_bounded_and_additive() {
        for (input, expected) in [(None, false), (Some("0"), false), (Some("1"), true)] {
            let value = input.map(std::ffi::OsStr::new);
            assert_eq!(zod_checks_enabled(value).unwrap(), expected);
            let mut command = Command::new("cargo");
            configure_checks_feature(&mut command, value).unwrap();
            let arguments: Vec<_> = command.get_args().collect();
            assert_eq!(arguments.is_empty(), !expected);
            if expected {
                assert_eq!(arguments, ["--features", "zod-checks"]);
            }
        }
        for value in ["", "true", "2"] {
            assert!(zod_checks_enabled(Some(std::ffi::OsStr::new(value))).is_err());
        }
    }

    #[test]
    fn explicit_zod_regex_compiler_feature_is_bounded_and_additive() {
        for (input, expected) in [(None, false), (Some("0"), false), (Some("1"), true)] {
            let value = input.map(std::ffi::OsStr::new);
            assert_eq!(zod_regex_enabled(value).unwrap(), expected);
            let mut command = Command::new("cargo");
            configure_regex_feature(&mut command, value).unwrap();
            let arguments: Vec<_> = command.get_args().collect();
            assert_eq!(arguments.is_empty(), !expected);
            if expected {
                assert_eq!(arguments, ["--features", "zod-regex"]);
            }
        }
        for value in ["", "true", "2"] {
            assert!(zod_regex_enabled(Some(std::ffi::OsStr::new(value))).is_err());
        }
    }

    #[test]
    fn normalized_output_paths_detect_collisions_before_writes() {
        let mut files = BTreeMap::new();
        stage(&mut files, "./custom/asyncapi.json", "first".into()).unwrap();
        assert!(stage(&mut files, "custom/asyncapi.json", "second".into()).is_err());
        assert!(stage(&mut files, "../outside.json", "invalid".into()).is_err());
        assert!(stage(&mut files, ".", "invalid".into()).is_err());
    }
    #[test]
    fn output_paths_stay_inside_the_application() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        assert!(validate_destination(&root, Path::new("sdks/rust/chat/src/lib.rs")).is_ok());
        assert!(validate_destination(&root, Path::new("../escape")).is_err());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(), root.join("sdks")).unwrap();
            assert!(validate_destination(&root, Path::new("sdks/chat/src/lib.rs")).is_err());
        }
    }
}

pub(crate) fn imported_service(root: &Path, name: &str) -> Result<bool> {
    ownership::imported_service(root, name)
}

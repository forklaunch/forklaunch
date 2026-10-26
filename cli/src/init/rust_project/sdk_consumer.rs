//! A reusable Cargo SDK consumer built from host-compilable service contracts.
use super::*;

/// Predeclare a service-owned client crate so the first workspace build has a
/// complete, acyclic graph: SDK -> host declarations, never SDK -> server.
pub(super) fn local_templates(dir: &Path, name: &str, host: &Path) -> Vec<RenderedTemplate> {
    local_templates_with_registry(dir, name, host, "contracts::registry()", &[])
}

/// Built-in modules select the same fallible, profile-aware registry as their
/// runtime. The SDK's host dependency never links the application server.
pub(super) fn local_templates_with_registry(
    dir: &Path,
    name: &str,
    host: &Path,
    registry: &str,
    environment: &[&str],
) -> Vec<RenderedTemplate> {
    let sdk = dir.join("sdk/rust");
    let root = dir
        .ancestors()
        .find(|path| path.join(".forklaunch/manifest.toml").is_file());
    let framework = root
        .map(|root| {
            pathdiff::diff_paths(root.join(".forklaunch/framework-rs"), &sdk)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| "../../../../.forklaunch/framework-rs".into());
    let host = pathdiff::diff_paths(host, &sdk)
        .expect("relative host contract path")
        .to_string_lossy()
        .into_owned();
    let cargo = format!(
        r#"[package]
name = "{name}-sdk"
version.workspace = true
edition.workspace = true
license.workspace = true
[lib]
path = "lib.rs"
[dependencies]
serde = {{ features = ["derive"] }}
serde_json = {{}}
reqwest = {{ default-features = false, features = ["json", "query", "stream", "multipart", "rustls"] }}
futures-util = {{}}
multer = {{}}
forklaunch-core = {{ path = "{framework}/crates/core", features = ["schema-validation"] }}
forklaunch-universal-sdk = {{ path = "{framework}/crates/universal-sdk" }}
[build-dependencies]
contracts = {{ package = "{name}-contracts", path = "{host}" }}
forklaunch-core = {{ path = "{framework}/crates/core", features = ["build"] }}
"#
    );
    let client = "Client";
    let environment = environment.iter().map(|name| {
        format!("    println!(\"cargo:rerun-if-env-changed={name}\");\n")
    }).collect::<String>();
    let build = format!(
        r#"use std::{{env, error::Error, path::PathBuf}};

use forklaunch_core::build;

fn main() -> Result<(), Box<dyn Error>> {{
    println!("cargo:rerun-if-changed={host}");
{environment}    let registry = {registry};
    let service_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("../..");
    println!("cargo:rerun-if-changed={{}}", service_dir.join("api").display());
    build::service_sdks_at(&service_dir, &registry, "{name}", "{client}")?;
    let output = PathBuf::from(env::var("OUT_DIR")?);
    build::rust_http_module(&registry, "{name}-sdk", "{client}", output.join("generated.rs"))?;
    Ok(())
}}
"#
    );
    let mut files = [
        ("Cargo.toml", cargo),
        ("build.rs", build),
        (
            "lib.rs",
            r#"//! Typed service client and public wire models.
include!(concat!(env!("OUT_DIR"), "/generated.rs"));
"#
            .into(),
        ),
        (".forklaunch-sdk-local", "forklaunch.sdk.local.v1\n".into()),
        (
            "Makefile",
            native_layout::makefile(&format!("{name}-sdk"), false),
        ),
    ]
    .into_iter()
    .map(|(file, content)| RenderedTemplate {
        path: sdk.join(file),
        content,
        context: None,
    })
    .collect::<Vec<_>>();
    let app = root
        .and_then(|root| read_manifest_value(&root.join(".forklaunch/manifest.toml")).ok())
        .and_then(|manifest| {
            manifest
                .get("app_name")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "app".into());
    files.push(RenderedTemplate {path:dir.join("sdk/typescript/package.json"), content:serde_json::to_string_pretty(&json!({"name":format!("@{app}/{name}"),"version":"0.1.0","private":true,"type":"module","exports":{".":{"types":"./index.ts"},"./sdk":{"types":"./sdk.ts"}},"scripts":{"build":"tsc --noEmit"},"devDependencies":{"typescript":"5.9.3","@forklaunch/universal-sdk":crate::core::package_json::package_json_constants::UNIVERSAL_SDK_VERSION}})).unwrap()+"\n",context:None});
    files.push(RenderedTemplate {path:dir.join("sdk/typescript/tsconfig.json"), content:"{\"compilerOptions\":{\"strict\":true,\"module\":\"NodeNext\",\"moduleResolution\":\"NodeNext\",\"target\":\"ES2022\",\"noEmit\":true},\"include\":[\"*.ts\"]}\n".into(),context:None});
    files
}

fn service_sdk_path(root: &Path, manifest: &Value, service: &str) -> Option<PathBuf> {
    let metadata = project_table(manifest, service)?.get("metadata")?;
    if metadata.get("sdkLayout").and_then(Value::as_str) != Some("project-local-v1") {
        return None;
    }
    let path = metadata
        .get("cratePath")
        .or_else(|| metadata.get("staticPath"))?
        .as_str()?;
    Some(root.join(path).join("sdk/rust"))
}

fn aggregate_directory(root: &Path) -> Result<PathBuf> {
    let path = root.join(".forklaunch/manifest.toml");
    let manifest = if path.is_file() { read_manifest_value(&path)? } else { Value::Table(Map::new()) };
    Ok(root.join(manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src")).join("client-sdk/rust"))
}

pub(super) fn generate(matches: &ArgMatches, services: Vec<String>) -> Result<()> {
    let name = required_name(matches, "library")?;
    let root = find_manifest_root(
        &matches
            .get_one::<String>("base_path")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_dir()?),
    )?
    .canonicalize()?;
    let manifest_path = root.join(".forklaunch/manifest.toml");
    let mut manifest = read_manifest_value(&manifest_path)?;
    let relative = format!("{}/client-sdk/rust", manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src"));
    let dir = root.join(&relative);
    if dir.join("Cargo.toml").exists() {
        bail!("Rust SDK consumer already exists; preserve its customizations");
    }
    if project_table(&manifest, &name).is_some() {
        bail!("Project name already exists");
    }
    if services.iter().all(|service| {
        service_sdk_path(&root, &manifest, service)
            .is_some_and(|path| path.join("Cargo.toml").is_file())
    }) {
        let dryrun = matches.get_flag("dryrun");
        generate_local_aggregate(&root, &mut manifest, &name, &services, dryrun)?;
        if !dryrun {
            let mut stdout = StandardStream::stdout(ColorChoice::Always);
            log_ok!(stdout, "Generated Rust SDK library {} at {}", name, dir.display());
        }
        return Ok(());
    }
    let mut cargo = toml_edit::DocumentMut::new();
    for section in ["package", "lib", "dependencies", "build-dependencies"] {
        cargo[section] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    cargo["package"]["name"] = toml_edit::value(&name);
    cargo["package"]["version"] = toml_edit::value("0.1.0");
    cargo["package"]["edition"] = toml_edit::value("2024");
    cargo["lib"]["path"] = toml_edit::value("lib.rs");
    let base:toml_edit::DocumentMut="[dependencies]\nserde={features=['derive']}\nserde_json={}\nreqwest={default-features=false,features=['json','query','stream','multipart','rustls']}\nfutures-util={}\ntokio={features=['net']}\ntokio-tungstenite={}\n".parse()?;
    cargo["dependencies"] = base["dependencies"].clone();
    let framework = "../../../.forklaunch/framework-rs";
    cargo["dependencies"]["forklaunch-universal-sdk"]["path"] =
        toml_edit::value(format!("{framework}/crates/universal-sdk"));
    cargo["dependencies"]["forklaunch-core"]["path"] =
        toml_edit::value(format!("{framework}/crates/core"));
    cargo["dependencies"]["forklaunch-core"]["features"] =
        toml_edit::value(toml_edit::Array::from_iter(["schema-validation"]));
    cargo["dependencies"]["forklaunch-ws"]["path"] =
        toml_edit::value(format!("{framework}/crates/ws"));
    cargo["dependencies"]["forklaunch-ws"]["features"] =
        toml_edit::value(toml_edit::Array::from_iter(["transport", "json-schema"]));
    cargo["build-dependencies"]["forklaunch-core"]["path"] =
        toml_edit::value(format!("{framework}/crates/core"));
    cargo["build-dependencies"]["forklaunch-core"]["features"] =
        toml_edit::value(toml_edit::Array::from_iter(["build"]));
    let mut build="use std::{env, error::Error, path::PathBuf};\n\nuse forklaunch_core::build;\n\nmod build_assets;\nfn main()->Result<(),Box<dyn Error>> {\nlet out=PathBuf::from(env::var(\"OUT_DIR\")?);\nprintln!(\"cargo:rerun-if-changed=build_assets.rs\");\n".to_string();
    let mut exports = "#![forbid(unsafe_code)]\npub mod extensions;\n#[allow(unused_imports)]\npub use extensions::*;\n".to_string();
    let mut seen = HashSet::new();
    for (index, service) in services.iter().enumerate() {
        if !validate_name(service) || !seen.insert(service) {
            bail!("Invalid or duplicate SDK service name");
        }
        let path =
            crate_path_for_project(&manifest, service).context("SDK service is not registered")?;
        let contract_path = contracts_layout::directory(&root.join(&path))
            .context("Service has no host contract package")?
            .canonicalize()?
            .join("Cargo.toml");
        let contract: Value = toml::from_str(
            &read_to_string(&contract_path)
                .context("SDK consumer requires a host-compilable contract crate")?,
        )?;
        let package = contract["package"]["name"]
            .as_str()
            .context("Contract package name")?;
        let alias = format!("contract_{index}");
        cargo["build-dependencies"][&alias]["package"] = toml_edit::value(package);
        let relative_contract = pathdiff::diff_paths(contract_path.parent().unwrap(), &dir)
            .context("SDK contract path")?;
        cargo["build-dependencies"][&alias]["path"] =
            toml_edit::value(relative_contract.to_string_lossy().as_ref());
        let watch = relative_contract.to_string_lossy();
        build.push_str(&format!("println!(\"cargo:rerun-if-changed={watch}\");\nbuild::rust_http_module(&{alias}::registry(), {name:?}, {:?}, out.join({:?}))?;\n",format!("{}Client",service.to_case(Case::Pascal)),format!("{service}.rs")));
        exports.push_str(&format!(
            "pub mod {} {{ include!(concat!(env!(\"OUT_DIR\"), {:?})); }}\n",
            service.to_case(Case::Snake),
            format!("/{service}.rs")
        ));
    }
    if services.len() == 1 {
        exports.push_str(&format!(
            "pub use {}::*;\n",
            services[0].to_case(Case::Snake)
        ));
    }
    build.push_str("build_assets::emit(&out)?;\nOk(())\n}\n");
    if matches.get_flag("dryrun") {
        println!(
            "Would generate SDK consumer {name} from {}",
            services.join(", ")
        );
        return Ok(());
    }
    create_dir_all(&dir)?;
    write(
        dir.join("Cargo.toml"),
        dependencies::render_for_path(&dir.join("Cargo.toml"), &cargo.to_string())?,
    )?;
    write(
        dir.join(".forklaunch-sdk-consumer"),
        "forklaunch.sdk.consumer.v1\n",
    )?;
    write(
        dir.join("build.rs"),
        native_layout::format_source(&dir.join("build.rs"), &build),
    )?;
    write(
        dir.join("lib.rs"),
        native_layout::format_source(&dir.join("lib.rs"), &exports),
    )?;
    write(
        dir.join("extensions.rs"),
        "//! Domain-specific SDK exports.\n",
    )?;
    write(
        dir.join("build_assets.rs"),
        "use std::{error::Error, path::Path};\n\n#[allow(clippy::unnecessary_wraps)]\npub fn emit(_out: &Path) -> Result<(), Box<dyn Error>> {\n    Ok(())\n}\n",
    )?;
    write(dir.join("Makefile"), native_layout::makefile(&name, false))?;
    let entry = toml::Value::try_from(
        json!({"type":"Library","name":name,"description":"Generated Rust service SDK consumer","routers":[],"metadata":{"language":"rust","crateName":name,"cratePath":relative,"contractExport":false}}),
    )?;
    manifest
        .get_mut("projects")
        .and_then(Value::as_array_mut)
        .context("Project list")?
        .push(entry);
    write_manifest_value(&manifest_path, &manifest)?;
    add_cargo_member(&root, &relative, false)?;
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    log_ok!(stdout, "Generated Rust SDK library {} at {}", name, dir.display());
    Ok(())
}

fn generate_local_aggregate(
    root: &Path,
    manifest: &mut Value,
    name: &str,
    services: &[String],
    dryrun: bool,
) -> Result<()> {
    let relative = format!("{}/client-sdk/rust", manifest.get("modules_path").and_then(Value::as_str).unwrap_or("src"));
    let dir = root.join(&relative);
    let mut cargo: toml_edit::DocumentMut = format!("[package]\nname={name:?}\nversion='0.1.0'\nedition='2024'\n[lib]\npath='lib.rs'\n[dependencies]\n").parse()?;
    let mut exports = "//! Public service SDK exports.\n#![forbid(unsafe_code)]\npub mod extensions;\n// The application extension module starts empty.\n#[allow(unused_imports)]\npub use extensions::*;\n".to_owned();
    let mut seen = HashSet::new();
    for service in services {
        if !validate_name(service) || !seen.insert(service) {
            bail!("Invalid or duplicate SDK service name");
        }
    }
    // `--sdk-services` names the services the caller needs now; the aggregate
    // re-exports every service-local SDK and is kept complete as services change.
    let mut services = services.to_vec();
    for service in local_sdk_services(root, manifest) {
        if !services.contains(&service) {
            services.push(service);
        }
    }
    for service in &services {
        let path = service_sdk_path(root, manifest, service)
            .context("Local SDK service is not registered")?;
        let alias = service.to_case(Case::Snake);
        cargo["dependencies"][&alias]["package"] = toml_edit::value(format!("{service}-sdk"));
        cargo["dependencies"][&alias]["path"] = toml_edit::value(
            pathdiff::diff_paths(&path, &dir)
                .context("Local SDK path")?
                .to_string_lossy()
                .as_ref(),
        );
        exports.push_str(&format!("pub use {alias};\n"));
    }
    if services.len() == 1 {
        exports.push_str(&format!(
            "pub use {}::*;\n",
            services[0].to_case(Case::Snake)
        ));
    }
    if dryrun {
        println!("Would generate SDK reexports {name}");
        return Ok(());
    }
    create_dir_all(&dir)?;
    write(dir.join("Cargo.toml"), cargo.to_string())?;
    write(
        dir.join("lib.rs"),
        native_layout::format_source(&dir.join("lib.rs"), &exports),
    )?;
    write(
        dir.join("extensions.rs"),
        "//! Application-specific client exports.\n",
    )?;
    write(
        dir.join(".forklaunch-sdk-consumer"),
        "forklaunch.sdk.consumer.v2\n",
    )?;
    write(dir.join("Makefile"), native_layout::makefile(name, false))?;
    manifest.get_mut("projects").and_then(Value::as_array_mut).context("Project list")?.push(toml::Value::try_from(json!({"type":"Library","name":name,"description":"Service SDK reexports","routers":[],"metadata":{"language":"rust","crateName":name,"cratePath":relative,"contractExport":false}}))?);
    write_manifest_value(&root.join(".forklaunch/manifest.toml"), manifest)?;
    add_cargo_member(root, &relative, false)
}

/// Keep generated consumer contract bindings valid when a service moves/renames.
/// Public Rust module/client names intentionally remain stable for existing callers.
pub(super) fn rename_service(
    root: &Path,
    old_path: &str,
    new_path: &str,
    old: &str,
    new: &str,
) -> Result<()> {
    if root
        .join(new_path)
        .join("sdk/rust/.forklaunch-sdk-local")
        .is_file()
    {
        replace_cargo_member(
            root,
            &format!("{old_path}/sdk/rust"),
            &format!("{new_path}/sdk/rust"),
        )?;
        let package = root.join(new_path).join("sdk/typescript/package.json");
        if package.is_file() {
            let mut value: serde_json::Value = serde_json::from_str(&read_to_string(&package)?)?;
            if let Some(name) = value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .and_then(|name| name.strip_suffix(&format!("/{old}")))
            {
                value["name"] = format!("{name}/{new}").into();
                write(&package, serde_json::to_string_pretty(&value)? + "\n")?;
            }
        }
    }
    let dir = aggregate_directory(root)?;
    if !dir.join(".forklaunch-sdk-consumer").is_file() {
        return Ok(());
    }
    let path = dir.join("Cargo.toml");
    let mut cargo: toml_edit::DocumentMut = read_to_string(&path)?.parse()?;
    let from_sdk =
        pathdiff::diff_paths(root.join(old_path).join("sdk/rust"), &dir).context("Old SDK path")?;
    let to_sdk =
        pathdiff::diff_paths(root.join(new_path).join("sdk/rust"), &dir).context("New SDK path")?;
    if let Some(dependencies) = cargo
        .get_mut("dependencies")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        for (_, dependency) in dependencies.iter_mut() {
            if dependency.get("path").and_then(toml_edit::Item::as_str) == from_sdk.to_str() {
                dependency["path"] = toml_edit::value(to_sdk.to_string_lossy().as_ref());
                dependency["package"] = toml_edit::value(format!("{new}-sdk"));
            }
        }
    }
    if read_to_string(dir.join(".forklaunch-sdk-consumer"))? == "forklaunch.sdk.consumer.v2\n" {
        write(path, cargo.to_string())?;
        return Ok(());
    }
    let new_service = root.join(new_path);
    let sibling = root
        .join(old_path)
        .with_file_name(format!("{old}-contracts"));
    let old_contract = if sibling.exists() {
        sibling
    } else {
        root.join(old_path).join("domain")
    };
    let new_contract = if old_contract
        .file_name()
        .is_some_and(|n| n == format!("{old}-contracts").as_str())
    {
        new_service.with_file_name(format!("{new}-contracts"))
    } else {
        new_service.join("domain")
    };
    let from = pathdiff::diff_paths(&old_contract, &dir)
        .context("Old contract path")?
        .to_string_lossy()
        .into_owned();
    let to = pathdiff::diff_paths(&new_contract, &dir)
        .context("New contract path")?
        .to_string_lossy()
        .into_owned();
    let mut changed = false;
    if let Some(dependencies) = cargo
        .get_mut("build-dependencies")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        for (_, dependency) in dependencies.iter_mut() {
            if dependency.get("path").and_then(toml_edit::Item::as_str) == Some(from.as_str()) {
                dependency["path"] = toml_edit::value(&to);
                if dependency.get("package").and_then(toml_edit::Item::as_str)
                    == Some(format!("{old}-contracts").as_str())
                {
                    dependency["package"] = toml_edit::value(format!("{new}-contracts"));
                }
                changed = true;
            }
        }
    }
    if changed {
        let build = dir.join("build.rs");
        let contents = read_to_string(&build)?.replace(
            &format!("cargo:rerun-if-changed={from}"),
            &format!("cargo:rerun-if-changed={to}"),
        );
        write(path, cargo.to_string())?;
        write(build, contents)?;
    }
    Ok(())
}

/// Refresh service identity in already emitted local artifacts before recompiling SDKs.
pub(super) fn rename_local_artifacts(root: &Path, old: &str, new: &str) -> Result<bool> {
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let Some(metadata) = project_table(&manifest, new).and_then(|project| project.get("metadata")) else { return Ok(false); };
    if metadata.get("sdkLayout").and_then(Value::as_str) != Some("project-local-v1") { return Ok(false); }
    let mut contract_present = false;
    for key in ["contractArtifactPath", "openapiPath"] {
        let Some(relative) = metadata.get(key).and_then(Value::as_str) else { continue; };
        let path = crate::core::application_output::output_path(root, Path::new(relative))?;
        if !path.is_file() { continue; }
        let mut artifact: JsonValue = serde_json::from_str(&read_to_string(&path)?)?;
        if artifact.pointer("/service/name").and_then(JsonValue::as_str) == Some(old) {
            artifact["service"]["name"] = new.into();
        }
        if artifact.pointer("/info/title").and_then(JsonValue::as_str) == Some(old) {
            artifact["info"]["title"] = new.into();
        }
        if let Some(paths) = artifact.get_mut("paths").and_then(JsonValue::as_object_mut) {
            for path in paths.values_mut().filter_map(JsonValue::as_object_mut) {
                for operation in path.values_mut() {
                    if operation.pointer("/x-forklaunch-operation/service").and_then(JsonValue::as_str) == Some(old) {
                        operation["x-forklaunch-operation"]["service"] = new.into();
                    }
                }
            }
        }
        write(path, serde_json::to_string_pretty(&artifact)? + "\n")?;
        contract_present |= key == "contractArtifactPath";
    }
    Ok(contract_present)
}

/// A generated consumer is application-owned. Do not silently leave its host
/// contract dependency dangling, or discard customized consumer APIs on delete.
/// Services whose SDK is a service-local Rust crate, in manifest order.
fn local_sdk_services(root: &Path, manifest: &Value) -> Vec<String> {
    manifest
        .get("projects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|project| project.get("name").and_then(Value::as_str))
        .filter(|name| {
            service_sdk_path(root, manifest, name)
                .is_some_and(|path| path.join("Cargo.toml").is_file())
        })
        .map(str::to_owned)
        .collect()
}

/// Keep the generated Rust client aggregate (`clients`) re-exporting every
/// service-local SDK: new services are added, edits to existing entries and the
/// extension module are preserved. Hand-written (v1) consumers are left alone.
pub(crate) fn reconcile_local_aggregate(root: &Path) -> Result<()> {
    let dir = aggregate_directory(root)?;
    let Ok(marker) = read_to_string(dir.join(".forklaunch-sdk-consumer")) else {
        return Ok(());
    };
    if marker != "forklaunch.sdk.consumer.v2\n" {
        return Ok(());
    }
    let manifest = read_manifest_value(&root.join(".forklaunch/manifest.toml"))?;
    let cargo_path = dir.join("Cargo.toml");
    let source = read_to_string(&cargo_path)?;
    let mut cargo: toml_edit::DocumentMut = source.parse()?;
    let lib_path = dir.join("lib.rs");
    let mut exports = read_to_string(&lib_path)?;
    let original_exports = exports.clone();
    for service in local_sdk_services(root, &manifest) {
        let alias = service.to_case(Case::Snake);
        let package = format!("{service}-sdk");
        let present = cargo
            .get("dependencies")
            .and_then(toml_edit::Item::as_table_like)
            .is_some_and(|deps| {
                deps.iter().any(|(name, dep)| {
                    name == alias || dep.get("package").and_then(toml_edit::Item::as_str) == Some(&package)
                })
            });
        if present {
            continue;
        }
        let path = service_sdk_path(root, &manifest, &service).context("Local SDK path")?;
        cargo["dependencies"][&alias]["package"] = toml_edit::value(&package);
        cargo["dependencies"][&alias]["path"] = toml_edit::value(
            pathdiff::diff_paths(&path, &dir)
                .context("Local SDK path")?
                .to_string_lossy()
                .as_ref(),
        );
        if !exports.contains(&format!("pub use {alias};")) {
            exports = format!("{}\npub use {alias};\n", exports.trim_end());
        }
    }
    if cargo.to_string() != source {
        write(&cargo_path, cargo.to_string())?;
    }
    if exports != original_exports {
        write(&lib_path, native_layout::format_source(&lib_path, &exports))?;
    }
    Ok(())
}

fn remove_from_local_aggregate(root: &Path, dir: &Path, service_path: &str) -> Result<()> {
    let Ok(local) = root.join(service_path).join("sdk/rust").canonicalize() else {
        return Ok(());
    };
    let cargo_path = dir.join("Cargo.toml");
    let mut cargo: toml_edit::DocumentMut = read_to_string(&cargo_path)?.parse()?;
    let Some(dependencies) = cargo
        .get_mut("dependencies")
        .and_then(toml_edit::Item::as_table_like_mut)
    else {
        return Ok(());
    };
    let aliases = dependencies
        .iter()
        .filter(|(_, dep)| {
            dep.get("path")
                .and_then(toml_edit::Item::as_str)
                .is_some_and(|path| dir.join(path).canonicalize().is_ok_and(|path| path == local))
        })
        .map(|(name, _)| name.to_owned())
        .collect::<Vec<_>>();
    if aliases.is_empty() {
        return Ok(());
    }
    for alias in &aliases {
        dependencies.remove(alias);
    }
    write(&cargo_path, cargo.to_string())?;
    let lib_path = dir.join("lib.rs");
    let exports = read_to_string(&lib_path)?;
    let kept = exports
        .lines()
        .filter(|line| {
            !aliases.iter().any(|alias| {
                let line = line.trim();
                line == format!("pub use {alias};") || line == format!("pub use {alias}::*;")
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    if kept != exports {
        write(&lib_path, kept)?;
    }
    Ok(())
}

pub(super) fn ensure_service_deletable(root: &Path, service_path: &str) -> Result<()> {
    let dir = aggregate_directory(root)?;
    if !dir.join(".forklaunch-sdk-consumer").is_file() {
        return Ok(());
    }
    if read_to_string(dir.join(".forklaunch-sdk-consumer"))? == "forklaunch.sdk.consumer.v2\n" {
        // The re-export aggregate is generated from the service list; deleting a
        // service removes its re-export rather than blocking on it.
        return remove_from_local_aggregate(root, &dir, service_path);
    }
    let cargo: Value = toml::from_str(&read_to_string(dir.join("Cargo.toml"))?)?;
    let local = root.join(service_path).join("sdk/rust");
    if let Ok(local) = local.canonicalize() {
        if cargo
            .get("dependencies")
            .and_then(Value::as_table)
            .is_some_and(|deps| {
                deps.values().any(|dep| {
                    dep.get("path").and_then(Value::as_str).is_some_and(|path| {
                        dir.join(path)
                            .canonicalize()
                            .is_ok_and(|path| path == local)
                    })
                })
            })
        {
            bail!("Service SDK is referenced by the Rust client aggregate; remove or update that dependency before deleting the service");
        }
    }
    let Some(expected) = contracts_layout::directory(&root.join(service_path)) else {
        return Ok(());
    };
    if !expected.is_dir() {
        return Ok(());
    }
    let expected = expected.canonicalize()?;
    if cargo
        .get("build-dependencies")
        .and_then(Value::as_table)
        .is_some_and(|deps| {
            deps.values()
                .filter_map(|dep| dep.get("path").and_then(Value::as_str))
                .any(|path| {
                    dir.join(path)
                        .canonicalize()
                        .is_ok_and(|path| path == expected)
                })
        })
    {
        bail!(
            "Service is referenced by the generated Rust SDK consumer; remove or update that consumer before deleting its contract source"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn local_aggregate_make_lint_accepts_empty_extensions_without_hiding_authored_warnings() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let sdk = root.join("src/notes/sdk/rust");
        create_dir_all(root.join(".forklaunch")).unwrap();
        create_dir_all(&sdk).unwrap();
        write(root.join(".forklaunch/manifest.toml"), "app_name='demo'\n").unwrap();
        write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver='3'\nmembers=[]\n",
        )
        .unwrap();
        write(
            sdk.join("Cargo.toml"),
            "[package]\nname='notes-sdk'\nversion='0.0.0'\nedition='2024'\n[lib]\npath='lib.rs'\n",
        )
        .unwrap();
        write(sdk.join("lib.rs"), "pub struct Client;\n").unwrap();
        let mut manifest: Value = toml::from_str("app_name='demo'\n[[projects]]\nname='notes'\ntype='Service'\n[projects.metadata]\ncratePath='src/notes'\nlanguage='rust'\nsdkLayout='project-local-v1'\n").unwrap();
        generate_local_aggregate(root, &mut manifest, "clients", &["notes".into()], false).unwrap();
        let aggregate = root.join("src/client-sdk/rust");
        let lint = || {
            std::process::Command::new("make")
                .arg("lint")
                .current_dir(&aggregate)
                .env("CARGO_TARGET_DIR", root.join("lint-target"))
                .env("CARGO_NET_OFFLINE", "true")
                .output()
                .expect("run the generated library lint command")
        };
        let empty = lint();
        assert!(
            empty.status.success(),
            "{}",
            String::from_utf8_lossy(&empty.stderr)
        );

        write(
            aggregate.join("extensions.rs"),
            "pub fn client() -> notes::Client { notes::Client }\n",
        )
        .unwrap();
        create_dir_all(aggregate.join("tests")).unwrap();
        write(
            aggregate.join("tests/exports.rs"),
            "#[test]\nfn exports() { let _: clients::Client = clients::client(); }\n",
        )
        .unwrap();
        let authored = lint();
        assert!(
            authored.status.success(),
            "{}",
            String::from_utf8_lossy(&authored.stderr)
        );

        write(
            aggregate.join("extensions.rs"),
            "pub fn client() -> notes::Client { notes::Client }\nfn unused_application_code() {}\n",
        )
        .unwrap();
        let invalid = lint();
        assert!(
            !invalid.status.success(),
            "Authored dead code must still fail strict lint"
        );
        assert!(String::from_utf8_lossy(&invalid.stderr).contains("unused_application_code"));
    }

    #[test]
    fn local_sdk_and_aggregate_have_no_server_dependency_or_duplicate_codegen() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        create_dir_all(root.join(".forklaunch")).unwrap();
        write(root.join(".forklaunch/manifest.toml"), "app_name='demo'\n").unwrap();
        write(root.join("Cargo.toml"), "[workspace]\nmembers=[]\n").unwrap();
        let dir = root.join("src/notes");
        let host = dir.join(".forklaunch/contracts");
        let files = local_templates(&dir, "notes", &host);
        let cargo: Value = toml::from_str(
            &files
                .iter()
                .find(|file| file.path == dir.join("sdk/rust/Cargo.toml"))
                .unwrap()
                .content,
        )
        .unwrap();
        assert_eq!(
            cargo["build-dependencies"]["contracts"]["path"].as_str(),
            Some("../../.forklaunch/contracts")
        );
        assert!(cargo["dependencies"].get("notes").is_none());
        let build = &files
            .iter()
            .find(|file| file.path == dir.join("sdk/rust/build.rs"))
            .unwrap()
            .content;
        assert!(build.contains("OUT_DIR") && build.contains("contracts::registry()"));
        for file in files {
            create_dir_all(file.path.parent().unwrap()).unwrap();
            write(file.path, file.content).unwrap();
        }
        let mut manifest: Value = toml::from_str("app_name='demo'\n[[projects]]\nname='notes'\ntype='Service'\n[projects.metadata]\ncratePath='src/notes'\nlanguage='rust'\nsdkLayout='project-local-v1'\n").unwrap();
        generate_local_aggregate(root, &mut manifest, "client", &["notes".into()], false).unwrap();
        let aggregate = root.join("src/client-sdk/rust");
        let cargo: Value =
            toml::from_str(&read_to_string(aggregate.join("Cargo.toml")).unwrap()).unwrap();
        assert!(cargo.get("build-dependencies").is_none());
        assert_eq!(
            cargo["dependencies"]["notes"]["path"].as_str(),
            Some("../../notes/sdk/rust")
        );
        assert!(!aggregate.join("build.rs").exists());
        assert!(read_to_string(aggregate.join("lib.rs"))
            .unwrap()
            .contains("pub use notes;"));
        // The generated aggregate drops a deleted service's re-export instead of blocking.
        ensure_service_deletable(root, "src/notes").unwrap();
        let cargo: Value =
            toml::from_str(&read_to_string(aggregate.join("Cargo.toml")).unwrap()).unwrap();
        assert!(cargo["dependencies"].get("notes").is_none());
        assert!(!read_to_string(aggregate.join("lib.rs")).unwrap().contains("pub use notes"));
    }

    #[test]
    fn sibling_contract_rename_updates_sdk_dependency_and_watch() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let service = root.join("src/api");
        let host = root.join("src/api-contracts");
        let sdk = root.join("src/client-sdk/rust");
        for path in [&service, &host, &sdk] {
            create_dir_all(path).unwrap();
        }
        write(
            service.join("Cargo.toml"),
            "[build-dependencies]\ncontracts={path='../api-contracts'}\n",
        )
        .unwrap();
        write(sdk.join(".forklaunch-sdk-consumer"), "v1").unwrap();
        write(sdk.join("Cargo.toml"), "[build-dependencies]\ncontract_0={path='../../api-contracts',package='api-contracts'}\n").unwrap();
        write(
            sdk.join("build.rs"),
            "println!(\"cargo:rerun-if-changed=../../api-contracts\");",
        )
        .unwrap();
        assert!(ensure_service_deletable(root, "src/api").is_err());
        rename_service(root, "src/api", "src/records", "api", "records").unwrap();
        assert!(read_to_string(sdk.join("Cargo.toml"))
            .unwrap()
            .contains("../../records-contracts"));
        assert!(read_to_string(sdk.join("build.rs"))
            .unwrap()
            .contains("../../records-contracts"));
    }
    #[test]
    fn bound_contract_prevents_delete_and_rename_updates_source() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let sdk = root.join("src/client-sdk/rust");
        create_dir_all(&sdk).unwrap();
        create_dir_all(root.join("src/api/domain")).unwrap();
        write(
            root.join("src/api/Cargo.toml"),
            "[build-dependencies]\ncontracts={path='domain'}\n",
        )
        .unwrap();
        write(sdk.join(".forklaunch-sdk-consumer"), "v1").unwrap();
        write(
            sdk.join("Cargo.toml"),
            "[build-dependencies]\ncontract_0={path='../../api/domain',package='api-contracts'}\n",
        )
        .unwrap();
        write(
            sdk.join("build.rs"),
            "println!(\"cargo:rerun-if-changed=../../api/domain\");",
        )
        .unwrap();
        assert!(ensure_service_deletable(root, "src/api").is_err());
        rename_service(root, "src/api", "src/renamed-api", "api", "renamed-api").unwrap();
        std::fs::rename(root.join("src/api"), root.join("src/renamed-api")).unwrap();
        let manifest = read_to_string(sdk.join("Cargo.toml")).unwrap();
        assert!(manifest.contains("../../renamed-api/domain"));
        assert!(manifest.contains("renamed-api-contracts"));
        assert!(read_to_string(sdk.join("build.rs"))
            .unwrap()
            .contains("../../renamed-api/domain"));
        assert!(ensure_service_deletable(root, "src/renamed-api").is_err());
        write(sdk.join("Cargo.toml"), "[build-dependencies]\n").unwrap();
        assert!(ensure_service_deletable(root, "src/renamed-api").is_ok());
    }
}

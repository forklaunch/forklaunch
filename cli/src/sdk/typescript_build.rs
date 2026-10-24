//! Build explicit TypeScript contract modules without loading service bootstraps.
//!
//! A service has a contract build when its manifest project carries
//! `contractBuild = true`; every path it writes follows from the manifest and the
//! SDK layout. `forklaunch sdk build` compiles in-process, so the application
//! keeps no build tools or plans of its own.
use sha2::{Digest, Sha256};
use std::{fs, path::Path, process::Command as ProcessCommand, thread, time::Duration};

use anyhow::{Context, Result, ensure};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream};

use crate::{CliCommand, core::command::command};

/// Manifest project metadata marking a service with a contract build.
const CONTRACT_BUILD: &str = "contractBuild";

#[derive(Debug)]
pub(crate) struct BuildCommand;

impl CliCommand for BuildCommand {
    fn command(&self) -> Command {
        command(
            "build",
            "Compile TypeScript and emit both SDK languages from isolated contracts",
        )
        .arg(Arg::new("base_path").long("path").short('p'))
        .arg(Arg::new("service").long("service").required(true))
        .arg(Arg::new("watch").long("watch").action(ArgAction::SetTrue))
        .arg(
            Arg::new("install_hooks")
                .long("install-hooks")
                .action(ArgAction::SetTrue)
                .conflicts_with("watch"),
        )
        .arg(
            Arg::new("skip_compile")
                .long("skip-compile")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let (root, manifest) = crate::core::validate::require_manifest(matches)?;
        let name = matches.get_one::<String>("service").unwrap();
        let project = manifest
            .projects
            .iter()
            .find(|project| project.name == *name)
            .context("Unknown TypeScript contract service")?;
        ensure!(
            project
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.language.as_deref())
                != Some("rust"),
            "Rust contracts are compiled through Cargo"
        );
        let service = crate::core::application_output::output_path(
            &root,
            &Path::new(&manifest.modules_path).join(name),
        )?;
        ensure!(
            service.join("api/contracts").is_dir(),
            "Service needs explicit api/contracts modules before enabling isolated SDK builds"
        );
        let watch = matches.get_flag("watch");
        let mut previous = None;
        loop {
            let current = fingerprint(&root.join(&manifest.modules_path))?;
            if previous.as_ref() != Some(&current) {
                previous = Some(current);
                let result = compile(
                    &root,
                    &service,
                    name,
                    &manifest.validator,
                    &manifest.runtime,
                    matches.get_flag("skip_compile"),
                );
                if !watch {
                    result?;
                    if matches.get_flag("install_hooks") {
                        install_hooks(&root, &service, name)?;
                    }
                    return Ok(());
                }
                if let Err(error) = result {
                    eprintln!("Contract build failed; last-good SDKs retained: {error:#}");
                }
            }
            thread::sleep(Duration::from_millis(300));
        }
    }
}

fn compile(
    root: &Path,
    service: &Path,
    name: &str,
    validator: &str,
    runtime: &str,
    skip_compile: bool,
) -> Result<()> {
    if !skip_compile {
        let status = ProcessCommand::new(if runtime == "bun" { "bun" } else { "pnpm" })
            .args(if runtime == "bun" {
                vec!["x", "--bun", "tsc", "-b"]
            } else {
                vec!["exec", "tsc", "-b"]
            })
            .current_dir(service)
            .status()
            .context("Failed to start TypeScript compiler")?;
        ensure!(
            status.success(),
            "TypeScript compilation failed; SDKs were not regenerated"
        );
    }
    export(root, service, name, validator, runtime)?;
    super::generate::generate_or_check_sdk_outputs(
        root,
        "all",
        "contract",
        Some(name),
        false,
        &mut StandardStream::stdout(ColorChoice::Always),
    )
}

/// The `sdk:build` script of a service with a contract build.
fn sdk_script(name: &str) -> String {
    format!("forklaunch sdk build --service {name}")
}

fn manifest(root: &Path) -> Result<toml::Value> {
    Ok(toml::from_str(&fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?)
}

fn project<'a>(manifest: &'a toml::Value, name: &str) -> Option<&'a toml::Value> {
    manifest
        .get("projects")?
        .as_array()?
        .iter()
        .find(|project| project.get("name").and_then(toml::Value::as_str) == Some(name))
}

/// Whether `name` has a contract build.
pub(crate) fn has_contract_build(root: &Path, name: &str) -> Result<bool> {
    Ok(project(&manifest(root)?, name)
        .and_then(|project| project.get("metadata")?.get(CONTRACT_BUILD)?.as_bool())
        == Some(true))
}

/// Whether any service of the application has a contract build.
pub(crate) fn any_contract_builds(root: &Path) -> Result<bool> {
    let manifest = manifest(root)?;
    Ok(manifest
        .get("projects")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .any(|project| {
            project
                .get("metadata")
                .and_then(|metadata| metadata.get(CONTRACT_BUILD))
                .and_then(toml::Value::as_bool)
                == Some(true)
        }))
}

/// Install the proven default profile; retain unsupported profiles explicitly.
pub(crate) fn prepare_default_service(
    root: &Path,
    name: &str,
    runtime: &str,
    _protected: bool,
) -> Result<()> {
    ensure!(matches!(runtime, "node" | "bun"), "Unsupported TypeScript runtime");
    if !uses_rust(root)? {
        return Ok(());
    }
    prepare_new_service(root, name)
}

/// Contract builds exist so Rust code can consume TypeScript services. A
/// TypeScript-only application keeps `tsc -b` and needs no Rust toolchain.
fn uses_rust(root: &Path) -> Result<bool> {
    Ok(matches!(
        manifest(root)?.get("language").and_then(toml::Value::as_str),
        Some("mixed" | "rust")
    ))
}

/// Install contract builds for TypeScript services written before the
/// application gained a Rust project. Services without contract modules are
/// left as they are.
pub(crate) fn prepare_existing_services(root: &Path) -> Result<()> {
    if !uses_rust(root)? {
        return Ok(());
    }
    let Some(modules) = manifest(root)?
        .get("modules_path")
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
    else {
        return Ok(());
    };
    let Ok(entries) = fs::read_dir(root.join(&modules)) else {
        return Ok(());
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            path.join("api/contracts").is_dir() && path.join("package.json").is_file()
        })
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    for name in names {
        if !has_contract_build(root, &name)? {
            prepare_new_service(root, &name)?;
        }
    }
    Ok(())
}

/// Scaffold consumers without inventing a portable contract before compilation.
pub(crate) fn prepare_new_service(root: &Path, name: &str) -> Result<()> {
    let manifest = manifest(root)?;
    let modules = manifest["modules_path"]
        .as_str()
        .context("Modules path missing")?;
    let service = root.join(modules).join(name);
    ensure!(
        service.join("api/contracts").is_dir(),
        "Explicit contract modules required for compile hooks"
    );
    if !has_contract_build(root, name)? {
        record_local_ownership(root, &service, name)?;
    }
    super::generate::prepare_contract_consumers(root, name)?;
    install_hooks(root, &service, name)?;
    record_contract_path(root, name, contract_path(root, name)?)
}

/// Where a contract build writes its Rust SDK, relative to the application.
fn rust_sdk(root: &Path, service: &Path, name: &str) -> Result<std::path::PathBuf> {
    Ok(if super::layout::typescript_authored(root, name)? {
        service.strip_prefix(root)?.join("sdk/rust/lib.rs")
    } else {
        super::layout::source(root, &super::layout::rust(root)?.join(format!("{name}-sdk")))?
            .join("lib.rs")
    })
}

fn install_hooks(root: &Path, service: &Path, name: &str) -> Result<()> {
    let package_path = service.join("package.json");
    let mut package: serde_json::Value = serde_json::from_slice(&fs::read(&package_path)?)?;
    let script = sdk_script(name);
    let scripts = package["scripts"]
        .as_object_mut()
        .context("Service scripts missing")?;
    // Earlier CLI builds routed `build` through a generated runner; SDKs are now
    // built by the CLI, so the service builds with plain tsc again.
    let retired = |value: &str| value.contains(".forklaunch/tools/contract-build.mjs") || value.contains(".forklaunch/typescript-build/");
    ensure!(
        scripts
            .get("build")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == "tsc -b" || retired(value)),
        "Custom build script must be reconciled before installing the contract hook"
    );
    for (key, value) in [
        ("sdk:build", script.clone()),
        ("sdk:watch", format!("{script} --watch")),
    ] {
        ensure!(
            scripts.get(key).and_then(serde_json::Value::as_str).is_none_or(|existing| existing == value || retired(existing)),
            "Custom SDK script must be preserved"
        );
        scripts.insert(key.into(), value.into());
    }
    scripts.insert("build".into(), "tsc -b".into());
    set_contract_build(root, name, true)?;
    // Every build rewrites this SDK; formatting it would only fight freshness checks.
    if let Some(sdk) = rust_sdk(root, service, name)?.parent() {
        let rustfmt = root.join(sdk).join("rustfmt.toml");
        if !rustfmt.exists() {
            fs::create_dir_all(root.join(sdk))?;
            super::write_if_changed(rustfmt, GENERATED_RUSTFMT)?;
        }
    }
    super::write_if_changed(
        package_path,
        format!("{}\n", serde_json::to_string_pretty(&package)?),
    )?;
    crate::init::rust_project::refresh_mixed_images(root)
}

fn set_contract_build(root: &Path, name: &str, enabled: bool) -> Result<()> {
    let path = root.join(".forklaunch/manifest.toml");
    let source = fs::read_to_string(&path)?;
    let mut document: toml_edit::DocumentMut = source.parse()?;
    let projects = document["projects"]
        .as_array_of_tables_mut()
        .context("Manifest projects missing")?;
    let project = projects
        .iter_mut()
        .find(|project| project.get("name").and_then(|name| name.as_str()) == Some(name))
        .context("Contract project missing")?;
    let metadata = project
        .entry("metadata")
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_like_mut()
        .context("Contract metadata must be a table")?;
    if enabled {
        metadata.insert(CONTRACT_BUILD, toml_edit::value(true));
    } else {
        metadata.remove(CONTRACT_BUILD);
    }
    let updated = document.to_string();
    if updated != source {
        fs::write(path, updated)?;
    }
    Ok(())
}

/// Opts a build-owned Rust directory out of the application's `cargo fmt`.
pub(crate) const GENERATED_RUSTFMT: &str = "# Generated by ForkLaunch; regenerated on build.\ndisable_all_formatting = true\n";

/// Validate a service's contract build scripts before a project change, and
/// report whether it has one.
pub(crate) fn preflight_lifecycle(root: &Path, name: &str) -> Result<bool> {
    if !has_contract_build(root, name)? {
        return Ok(false);
    }
    let modules = manifest(root)?["modules_path"]
        .as_str()
        .context("Modules path missing")?
        .to_owned();
    let service = crate::core::application_output::output_path(root, &Path::new(&modules).join(name))?;
    let package: serde_json::Value =
        serde_json::from_slice(&fs::read(service.join("package.json"))?)?;
    let expected = sdk_script(name);
    ensure!(
        package["scripts"]["build"] == "tsc -b"
            && package["scripts"]["sdk:build"] == expected
            && package["scripts"]["sdk:watch"] == format!("{expected} --watch"),
        "Customized contract build scripts must be reconciled before changing or deleting this project"
    );
    Ok(true)
}

/// Validate every contract build before an application-wide change. Plans
/// follow the manifest, so a validator or runtime change needs no plan edits.
pub(crate) fn preflight_application(root: &Path) -> Result<()> {
    let manifest = manifest(root)?;
    for project in manifest["projects"].as_array().context("Missing projects")? {
        let name = project["name"].as_str().context("Missing project name")?;
        preflight_lifecycle(root, name)?;
    }
    Ok(())
}

pub(crate) fn remove_hook(root: &Path, name: &str) -> Result<()> {
    if !has_contract_build(root, name)? {
        return Ok(());
    }
    let local = super::layout::typescript_authored(root, name)?;
    if local {
        let modules = manifest(root)?["modules_path"].as_str().context("Modules path missing")?.to_owned();
        let service = root.join(&modules).join(name);
        if let Some(path) = rust_sdk(root, &service, name)?.parent() {
            crate::init::rust_project::remove_cargo_member(root, &path.to_string_lossy())?;
        }
    }
    set_contract_build(root, name, false)?;
    // Only the legacy shared exports are removed here; service-owned SDK folders
    // go with their service.
    for path in [
        crate::core::artifact_paths::legacy(name, crate::core::artifact_paths::Artifact::Contract),
        crate::core::artifact_paths::legacy(name, crate::core::artifact_paths::Artifact::OpenApi),
    ]
    .map(|path| root.join(path))
    {
        if path.is_file() {
            fs::remove_file(path)?;
        }
    }
    super::generate::cleanup_service_generated_files(root, name)?;
    if local {
        return Ok(());
    }
    super::generate::refresh_contract_consumers(root)
}

pub(crate) fn rename_hook(root: &Path, old: &str, new: &str) -> Result<()> {
    if old == new {
        return Ok(());
    }
    // `change service` writes the renamed manifest entry first, so the hook's
    // metadata may already sit under the new name while still describing the
    // old directory and scripts.
    let entry_renamed = !has_contract_build(root, old)? && has_contract_build(root, new)?;
    if !entry_renamed {
        if !has_contract_build(root, old)? {
            return Ok(());
        }
        ensure!(!has_contract_build(root, new)?, "Destination contract hook already exists");
    }
    let source = if entry_renamed { new } else { old };
    let modules = manifest(root)?["modules_path"]
        .as_str()
        .context("Missing hook modules path")?
        .to_owned();
    let service = root.join(&modules).join(new);
    let local = super::layout::typescript_authored(root, source)?;
    // Transports are recorded in the compiled contract, not in a plan. The
    // directory has already moved, so prefer the contract under its new path.
    let (request_multipart, response_multipart) = fs::read_to_string(service.join("sdk/contract.json"))
        .ok()
        .or_else(|| fs::read_to_string(root.join(contract_path(root, source).ok()?)).ok())
        .and_then(|contract| forklaunch_core::build::http_artifact_sources(&contract, old).ok())
        .map_or((false, false), |compiled| (compiled.request_multipart, compiled.response_multipart));
    if entry_renamed {
        // The generated SDK scripts still name the old service.
        let package_path = service.join("package.json");
        let mut package: serde_json::Value = serde_json::from_slice(&fs::read(&package_path)?)?;
        if let Some(scripts) = package["scripts"].as_object_mut() {
            for (key, value) in [("sdk:build", sdk_script(old)), ("sdk:watch", format!("{} --watch", sdk_script(old)))] {
                if scripts.get(key).and_then(serde_json::Value::as_str) == Some(value.as_str()) {
                    scripts.remove(key);
                }
            }
            fs::write(&package_path, format!("{}\n", serde_json::to_string_pretty(&package)?))?;
        }
    }
    if local {
        record_local_ownership(root, &service, new)?;
        crate::init::rust_project::rename_service_sdk_dependency(root, &format!("{modules}/{old}"), &format!("{modules}/{new}"), old, new)?;
    }
    super::generate::prepare_contract_consumers_with_transports(
        root,
        new,
        request_multipart,
        response_multipart,
    )?;
    install_hooks(root, &service, new)?;
    record_contract_path(root, new, contract_path(root, new)?)?;
    if entry_renamed {
        Ok(())
    } else {
        remove_hook(root, old)
    }
}

fn fingerprint(directory: &Path) -> Result<Vec<u8>> {
    fn visit(directory: &Path, digest: &mut Sha256) -> Result<()> {
        let mut entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if !matches!(
                    name.as_ref(),
                    "node_modules"
                        | "dist"
                        | "lib"
                        | "target"
                        | "production"
                        | "client-sdk"
                        | ".git"
                        | ".forklaunch"
                ) {
                    visit(&entry.path(), digest)?;
                }
            } else if kind.is_file()
                && (name.ends_with(".ts") || name == "package.json" || name.starts_with("tsconfig"))
            {
                digest.update(entry.path().to_string_lossy().as_bytes());
                digest.update(fs::read(entry.path())?);
            }
        }
        Ok(())
    }
    let mut digest = Sha256::new();
    visit(directory, &mut digest)?;
    Ok(digest.finalize().to_vec())
}

fn contract_path(root: &Path, name: &str) -> Result<String> {
    Ok(if let Some(local) = super::layout::local_sdk(root, name, "typescript")? {
        local.join("contract.json").strip_prefix(root.canonicalize()?)?.to_string_lossy().into_owned()
    } else { crate::core::artifact_paths::application_path(root, name, crate::core::artifact_paths::Artifact::Contract) })
}
fn openapi_path(root: &Path, name: &str) -> Result<String> {
    Ok(if let Some(local) = super::layout::local_sdk(root, name, "typescript")? {
        local.join("openapi.json").strip_prefix(root.canonicalize()?)?.to_string_lossy().into_owned()
    } else { crate::core::artifact_paths::application_path(root, name, crate::core::artifact_paths::Artifact::OpenApi) })
}
fn record_local_ownership(root: &Path, service: &Path, name: &str) -> Result<()> {
    let path = root.join(".forklaunch/manifest.toml");
    let mut manifest: toml::Value = toml::from_str(&fs::read_to_string(&path)?)?;
    let project = manifest["projects"].as_array_mut().context("Missing projects")?.iter_mut().find(|project|project["name"].as_str()==Some(name)).context("Missing service")?;
    let metadata = project.as_table_mut().context("Invalid project")?.entry("metadata").or_insert_with(||toml::Value::Table(Default::default())).as_table_mut().context("Invalid metadata")?;
    metadata.insert("sdkLayout".into(), "project-local-v1".into());
    metadata.insert("language".into(), "typescript".into());
    metadata.insert("staticPath".into(), service.strip_prefix(root)?.to_string_lossy().into_owned().into());
    metadata.insert("contractArtifactPath".into(), service.strip_prefix(root)?.join("sdk/contract.json").to_string_lossy().into_owned().into());
    metadata.insert("openapiPath".into(), service.strip_prefix(root)?.join("sdk/openapi.json").to_string_lossy().into_owned().into());
    super::write_if_changed(path, toml::to_string_pretty(&manifest)?)?;
    Ok(())
}

fn export(root: &Path, service: &Path, name: &str, validator: &str, runtime: &str) -> Result<()> {
    let document = forklaunch_core::build::export_typescript_openapi_with_runtime(service, validator, runtime)?;
    let openapi: serde_json::Value = serde_json::from_str(&document)?;
    // The source-distributed Linux build tool and installed CLI use this same
    // importer/compiler; both validate every target before publishing outputs.
    let compiled =
        forklaunch_core::build::openapi_artifact_sources(&serde_json::to_string(&openapi)?, name)?;
    let relative = contract_path(root, name)?;
    let mut contract: serde_json::Value = serde_json::from_str(&compiled.contract)?;
    if super::layout::typescript_authored(root, name)? {
        contract["sdkLayout"] = "project-local-v1".into();
            contract["rustClientName"] = "Client".into();
        contract["typescriptRuntimeLayout"] = "local".into();
    }
    super::write_if_changed(root.join(&relative), serde_json::to_string(&contract)?)?;
    super::write_if_changed(
        root.join(openapi_path(root, name)?),
        serde_json::to_string_pretty(&openapi)?,
    )?;
    record_contract_path(root, name, relative)
}

fn record_contract_path(root: &Path, name: &str, relative: String) -> Result<()> {
    let path = root.join(".forklaunch/manifest.toml");
    let mut manifest: toml::Value = toml::from_str(&fs::read_to_string(&path)?)?;
    let project = manifest["projects"]
        .as_array_mut()
        .context("Manifest projects missing")?
        .iter_mut()
        .find(|project| project["name"].as_str() == Some(name))
        .context("Contract project missing")?;
    let metadata = project
        .as_table_mut()
        .context("Contract project must be a table")?
        .entry("metadata")
        .or_insert_with(|| toml::Value::Table(Default::default()))
        .as_table_mut()
        .context("Contract metadata must be a table")?;
    metadata.insert("contractArtifactPath".into(), toml::Value::String(relative));
    super::write_if_changed(path, toml::to_string_pretty(&manifest)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_typescript_sdk_keeps_canonical_types_and_local_artifacts() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        let service = root.path().join("src/iam");
        fs::create_dir_all(&service).unwrap();
        fs::write(service.join("sdk.ts"), "export type IamSdkClient = { canonical: true };\n").unwrap();
        fs::write(root.path().join(".forklaunch/manifest.toml"), "modules_path='src'\n[[projects]]\nname='iam'\ntype='Service'\n").unwrap();
        record_local_ownership(root.path(), &service, "iam").unwrap();
        assert_eq!(contract_path(root.path(), "iam").unwrap(), "src/iam/sdk/contract.json");
        assert_eq!(openapi_path(root.path(), "iam").unwrap(), "src/iam/sdk/openapi.json");
        assert!(super::super::layout::typescript_authored(root.path(), "iam").unwrap());
        assert_eq!(super::super::layout::local_sdk(root.path(), "iam", "rust").unwrap().unwrap(), root.path().canonicalize().unwrap().join("src/iam/sdk/rust"));
        assert_eq!(fs::read_to_string(service.join("sdk.ts")).unwrap(), "export type IamSdkClient = { canonical: true };\n");
    }

    #[test]
    fn contract_builds_are_manifest_metadata() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "# kept\nmodules_path = 'src'\n\n[[projects]]\nname = 'iam'\ntype = 'Service'\n",
        )
        .unwrap();
        assert!(!has_contract_build(root.path(), "iam").unwrap());
        set_contract_build(root.path(), "iam", true).unwrap();
        assert!(has_contract_build(root.path(), "iam").unwrap());
        assert!(any_contract_builds(root.path()).unwrap());
        let text = fs::read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap();
        assert!(text.starts_with("# kept\n"), "{text}");
        set_contract_build(root.path(), "iam", false).unwrap();
        assert!(!any_contract_builds(root.path()).unwrap());
    }

    #[test]
    fn watch_tracks_contract_dependencies_without_rebuilding_on_generated_outputs() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("api/domain/schemas")).unwrap();
        let schema = root.path().join("api/domain/schemas/note.ts");
        fs::write(&schema, "export const size = 1;").unwrap();
        let original = fingerprint(root.path()).unwrap();
        for folder in ["api/dist", "api/production", "client-sdk"] {
            fs::create_dir_all(root.path().join(folder)).unwrap();
            fs::write(root.path().join(folder).join("index.ts"), "generated").unwrap();
        }
        assert_eq!(original, fingerprint(root.path()).unwrap());
        fs::write(schema, "export const size = 2;").unwrap();
        assert_ne!(original, fingerprint(root.path()).unwrap());
    }

    #[test]
    fn typescript_only_applications_keep_tsc_without_contract_hooks() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        let service = root.path().join("src/iam");
        fs::create_dir_all(service.join("api/contracts")).unwrap();
        let package = r#"{"scripts":{"build":"tsc -b"}}"#;
        fs::write(service.join("package.json"), package).unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            "language = \"typescript\"\nmodules_path = \"src\"\n",
        )
        .unwrap();
        prepare_default_service(root.path(), "iam", "node", true).unwrap();
        prepare_existing_services(root.path()).unwrap();
        assert_eq!(fs::read_to_string(service.join("package.json")).unwrap(), package);
        assert!(!root.path().join("Cargo.toml").exists());
        assert!(uses_rust_manifest(root.path(), "mixed"));
        assert!(uses_rust_manifest(root.path(), "rust"));
    }

    fn uses_rust_manifest(root: &Path, language: &str) -> bool {
        fs::write(
            root.join(".forklaunch/manifest.toml"),
            format!("language = \"{language}\"\n"),
        )
        .unwrap();
        uses_rust(root).unwrap()
    }
}

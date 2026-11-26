//! Shared service build layout for ordinary CLI projects.
use crate::build::{rust_http_module, typescript_source};
use anyhow::{Context, Result, bail};
use forklaunch_core::contract::ContractRegistry;
use std::path::{Component, Path, PathBuf};

pub struct ServiceSdkPaths {
    pub typescript: PathBuf,
    pub rust: PathBuf,
}

/// Find the owning application even after a service is relocated deeper in src.
/// # Errors
/// Rejects build scripts outside a generated application.
pub fn application_root(manifest_dir: impl AsRef<Path>) -> Result<PathBuf> {
    manifest_dir
        .as_ref()
        .ancestors()
        .find(|path| path.join(".forklaunch/manifest.toml").is_file())
        .map(Path::to_path_buf)
        .context("Service must belong to a ForkLaunch application")
}

fn aggregate_sdk_root(root: &Path) -> Result<PathBuf> {
    let manifest = root.join(".forklaunch/manifest.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let manifest: toml::Value = toml::from_str(&std::fs::read_to_string(manifest)?)?;
    let modules = manifest.get("modules_path").map_or(Ok("src"), |value| {
        value
            .as_str()
            .context("Application modules_path must be a relative directory")
    })?;
    let relative = Path::new(modules);
    anyhow::ensure!(
        !modules.is_empty()
            && relative
                .components()
                .all(|component| { matches!(component, Component::Normal(_) | Component::CurDir) }),
        "Application modules_path must stay inside the application"
    );
    let destination = root.join(relative).join("client-sdk");
    // Resolve the nearest existing ancestor before creating any output. This
    // also rejects a modules directory symlink that leaves the application.
    let ancestor = destination
        .ancestors()
        .find(|path| path.exists())
        .context("SDK output must belong to an existing application")?;
    anyhow::ensure!(
        ancestor.canonicalize()?.starts_with(root.canonicalize()?),
        "Application SDK directory must stay inside the application"
    );
    Ok(destination)
}

/// Emit both language SDKs, contract and `OpenAPI` from the same service registry.
/// Environment output overrides match the CLI's ordinary generated services.
/// Returned paths let optional socket/event assets share the same layout.
/// # Errors
/// Rejects invalid service names, schemas, application roots and filesystem failures.
pub fn service_sdks(
    registry: &ContractRegistry,
    service: &str,
    client_name: &str,
) -> Result<ServiceSdkPaths> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    service_sdks_at(&manifest_dir, registry, service, client_name)
}

/// Emit service-owned artifacts from either its server or its SDK build script.
/// The explicit directory avoids changing process-wide Cargo environment values.
/// # Errors
/// Rejects invalid declarations, output paths and filesystem failures.
pub fn service_sdks_at(
    manifest_dir: &Path,
    registry: &ContractRegistry,
    service: &str,
    client_name: &str,
) -> Result<ServiceSdkPaths> {
    if service.is_empty()
        || service.len() > 64
        || !service
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        bail!("Invalid service SDK output name");
    }
    println!("cargo:rerun-if-env-changed=FORKLAUNCH_TYPESCRIPT_SDK_DIR");
    println!("cargo:rerun-if-env-changed=FORKLAUNCH_RUST_SDK_FILE");
    let root = application_root(manifest_dir)?;
    let aggregate = aggregate_sdk_root(&root)?;
    let local = manifest_dir
        .join("sdk/rust/.forklaunch-sdk-local")
        .is_file();
    let client_name = if local { "Client" } else { client_name };
    let paths = ServiceSdkPaths {
        typescript: std::env::var_os("FORKLAUNCH_TYPESCRIPT_SDK_DIR").map_or_else(
            || {
                if local {
                    manifest_dir.join("sdk/typescript")
                } else {
                    aggregate.join("typescript").join(service)
                }
            },
            PathBuf::from,
        ),
        rust: std::env::var_os("FORKLAUNCH_RUST_SDK_FILE").map_or_else(
            || {
                if local {
                    manifest_dir.join("sdk/rust/generated.rs")
                } else {
                    aggregate.join("rust").join(format!("{service}.rs"))
                }
            },
            PathBuf::from,
        ),
    };
    if local {
        let mut artifact: serde_json::Value =
            serde_json::from_str(&registry.clone().into_artifact().to_json())?;
        artifact["rustClientName"] = client_name.into();
        artifact["rustOutputLayout"] = "service-module".into();
        artifact["sdkLayout"] = "project-local-v1".into();
        artifact["typescriptRuntimeLayout"] = "local".into();
        let contract = serde_json::to_string(&artifact)?;
        let sources = super::http_artifact_sources(&contract, service)?;
        for (name, source) in sources.typescript_files {
            super::write_output(&paths.typescript.join(name), &source)?;
        }
        super::write_output(&paths.typescript.join("contract.json"), &contract)?;
        super::write_output(
            &paths.typescript.join("openapi.json"),
            &crate::openapi::generate_portable_openapi_json(registry)?,
        )?;
    } else {
        typescript_source(registry, service, &paths.typescript, Some(client_name))?;
    }
    if !local && std::env::var_os("FORKLAUNCH_TYPESCRIPT_SDK_DIR").is_none() {
        crate::build::write_output(
            &paths.typescript.join(".forklaunch-generated"),
            "forklaunch.sdk.http.v1\n",
        )?;
        let package = aggregate.join("typescript");
        crate::build::service_package::refresh(&package)?;
        for file in ["index.ts", "package.json", ".forklaunch-sdk-package.json"] {
            println!("cargo:rerun-if-changed={}", package.join(file).display());
        }
    }

    if !local && std::env::var_os("FORKLAUNCH_TYPESCRIPT_SDK_DIR").is_some() {
        crate::build::service_package::http(&paths.typescript)?;
    }

    rust_http_module(
        registry,
        &format!("{service}-sdk"),
        client_name,
        &paths.rust,
    )?;
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builtin_sdk_outputs_follow_the_application_modules_directory() {
        let root = std::env::temp_dir().join(format!(
            "forklaunch-builtin-sdk-layout-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join(".forklaunch")).unwrap();
        for modules in ["src", "modules", "backend/domains"] {
            std::fs::write(
                root.join(".forklaunch/manifest.toml"),
                format!("modules_path = '{modules}'\n"),
            )
            .unwrap();
            let service = root.join(modules).join("catalog");
            std::fs::create_dir_all(&service).unwrap();
            let paths = service_sdks_at(
                &service,
                &ContractRegistry::new(),
                "catalog",
                "ForkLaunchClient",
            )
            .unwrap();
            let sdk = root.join(modules).join("client-sdk");
            assert_eq!(paths.typescript, sdk.join("typescript/catalog"));
            assert_eq!(paths.rust, sdk.join("rust/catalog.rs"));
            assert!(paths.rust.is_file());
            assert!(sdk.join("typescript/index.ts").is_file());
            assert!(sdk.join("typescript/package.json").is_file());
            std::fs::remove_dir_all(root.join(modules.split('/').next().unwrap())).unwrap();
        }
        for invalid in [
            "modules_path='../outside'",
            "modules_path='/tmp/outside'",
            "modules_path=''",
            "modules_path=42",
        ] {
            std::fs::write(root.join(".forklaunch/manifest.toml"), invalid).unwrap();
            assert!(
                service_sdks_at(
                    &root,
                    &ContractRegistry::new(),
                    "catalog",
                    "ForkLaunchClient"
                )
                .is_err()
            );
            assert!(!root.join("src").exists());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn builtin_sdk_output_rejects_an_external_modules_symlink() {
        let root = std::env::temp_dir().join(format!(
            "forklaunch-builtin-sdk-symlink-{}",
            std::process::id()
        ));
        let outside = root.with_extension("outside");
        std::fs::create_dir_all(root.join(".forklaunch")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("modules")).unwrap();
        std::fs::write(
            root.join(".forklaunch/manifest.toml"),
            "modules_path='modules'",
        )
        .unwrap();
        assert!(
            service_sdks_at(
                &root,
                &ContractRegistry::new(),
                "catalog",
                "ForkLaunchClient"
            )
            .is_err()
        );
        assert!(!outside.join("client-sdk").exists());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn sdk_and_service_emit_identical_local_artifacts() {
        let root =
            std::env::temp_dir().join(format!("forklaunch-local-artifacts-{}", std::process::id()));
        let service = root.join("src/api");
        std::fs::create_dir_all(root.join(".forklaunch")).unwrap();
        std::fs::write(root.join(".forklaunch/manifest.toml"), "app_name='fixture'").unwrap();
        std::fs::create_dir_all(service.join("sdk/rust")).unwrap();
        std::fs::write(
            service.join("sdk/rust/.forklaunch-sdk-local"),
            "forklaunch.sdk.local.v1\n",
        )
        .unwrap();
        let registry = ContractRegistry::new();
        let first = service_sdks_at(&service, &registry, "api", "Client").unwrap();
        let contract = std::fs::read(first.typescript.join("contract.json")).unwrap();
        let rust = std::fs::read(&first.rust).unwrap();
        let second = service_sdks_at(&service, &registry, "api", "ForkLaunchClient").unwrap();
        assert_eq!(
            contract,
            std::fs::read(second.typescript.join("contract.json")).unwrap()
        );
        assert_eq!(rust, std::fs::read(second.rust).unwrap());
        assert!(
            String::from_utf8(contract)
                .unwrap()
                .contains("\"rustClientName\":\"Client\"")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn relocated_service_finds_owning_application() {
        let root =
            std::env::temp_dir().join(format!("forklaunch-build-layout-{}", std::process::id()));
        let service = root.join("src/team/notes");
        std::fs::create_dir_all(&service).unwrap();
        assert!(application_root(&service).is_err());
        std::fs::create_dir_all(root.join(".forklaunch")).unwrap();
        std::fs::write(
            root.join(".forklaunch/manifest.toml"),
            "application = 'fixture'\n",
        )
        .unwrap();
        assert_eq!(application_root(&service).unwrap(), root);
        std::fs::remove_dir_all(root).unwrap();
    }
}

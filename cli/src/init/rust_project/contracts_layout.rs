//! Host-compilable contract ownership for ordinary native projects.
use super::*;

/// Resolve the declared contract package, retaining existing nested layouts.
pub(super) fn directory(dir: &Path) -> Option<PathBuf> {
    let cargo: Value = toml::from_str(&read_to_string(dir.join("Cargo.toml")).ok()?).ok()?;
    let path = cargo
        .get("build-dependencies")?
        .get("contracts")?
        .get("path")?
        .as_str()?;
    Some(dir.join(path))
}
pub(super) fn is_native_service(dir: &Path) -> bool {
    directory(dir).is_some_and(|path| path.join("Cargo.toml").is_file())
        && dir.join("api/routes").is_dir()
        // Built-in modules share this host/SDK boundary, but their extension
        // routers bind to the module adapter rather than an application AppState.
        && !dir.join("api/contracts/registry.rs").is_file()
}

pub(super) fn has_local_sdk(dir: &Path) -> bool {
    dir.join("sdk/rust/.forklaunch-sdk-local").is_file()
}
/// Match the outputs owned by core::build::service_sdks and its provenance receipt.
/// AsyncAPI lives beside them so every per-service artifact shares one folder.
pub(super) fn set_artifact_paths(metadata: &mut Map<String, Value>, name: &str) {
    for (key, file) in [
        ("contractArtifactPath", "contract.json"),
        ("openapiPath", "openapi.json"),
        ("asyncapiPath", "asyncapi.json"),
    ] {
        metadata.insert(
            key.into(),
            Value::String(
                if metadata.get("sdkLayout").and_then(Value::as_str) == Some("project-local-v1") {
                    format!(
                        "{}/sdk/typescript/{file}",
                        metadata
                            .get("cratePath")
                            .and_then(Value::as_str)
                            .unwrap_or(&format!("src/{name}"))
                    )
                } else {
                    format!("src/client-sdk/typescript/{name}/{file}")
                },
            ),
        );
    }
}
pub(super) fn source(dir: &Path, relative: &str) -> PathBuf {
    if directory(dir).is_some_and(|host| host == new_host_directory(dir)) {
        return dir.join(relative);
    }
    let host = directory(dir).filter(|path| path.join("lib.rs").is_file());
    match (
        host,
        relative.strip_prefix("api/contracts/"),
        relative.strip_prefix("domain/schemas/"),
    ) {
        (Some(host), Some(path), _) => host.join("endpoints").join(path),
        (Some(host), _, Some(path)) => host.join("schemas").join(path),
        _ => dir.join(relative),
    }
}
fn split_sibling(dir: &Path, name: &str, files: Vec<RenderedTemplate>) -> Vec<RenderedTemplate> {
    if dir.join("domain/Cargo.toml").is_file() {
        return files;
    }
    let host = dir.with_file_name(format!("{name}-contracts"));
    let mut output = Vec::new();
    for mut file in files {
        let relative = file
            .path
            .strip_prefix(dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        match relative.as_str() {
            "Cargo.toml" => {
                file.content = file.content.replace(
                    "path = \"domain\"",
                    &format!("path = \"../{name}-contracts\""),
                )
            }
            "domain/Cargo.toml" => {
                file.path = host.join("Cargo.toml");
                file.content = file
                    .content
                    .replace("path = \"../sdk.rs\"", "path = \"lib.rs\"")
                    .replace("../../../.forklaunch/", "../../.forklaunch/");
            }
            "domain/Makefile" => file.path = host.join("Makefile"),
            "sdk.rs" => {
                file.content = "//! Public contract exports.\npub use contracts::*;\n".into();
                output.push(RenderedTemplate { path: host.join("lib.rs"), content: "//! Wire declarations shared by runtime and host compiler.\npub mod schemas;\nmod endpoints;\npub use endpoints::*;\n".into(), context: None });
            }
            "build.rs" => {
                file.content = file.content.replacen("let evidence =", &format!("println!(\"cargo:rerun-if-changed=../{name}-contracts\");\n    let evidence ="), 1);
            }
            _ if relative.starts_with("api/contracts/") => {
                let name = relative.strip_prefix("api/contracts/").unwrap();
                let export = if name == "mod.rs" {
                    "pub use contracts::*;\n".into()
                } else {
                    format!("pub use contracts::{}::*;\n", name.trim_end_matches(".rs"))
                };
                output.push(RenderedTemplate {
                    path: file.path.clone(),
                    content: export,
                    context: None,
                });
                file.path = host.join("endpoints").join(name);
            }
            _ if relative.starts_with("domain/schemas/") => {
                let name = relative.strip_prefix("domain/schemas/").unwrap();
                output.push(RenderedTemplate {
                    path: file.path.clone(),
                    content: "pub use contracts::schemas::*;\n".into(),
                    context: None,
                });
                file.path = host.join("schemas").join(name);
            }
            _ => {}
        }
        output.push(file);
    }
    output
}

/// Generated private compilation boundary; application declarations remain local.
pub(super) fn new_host_directory(dir: &Path) -> PathBuf {
    dir.join(".forklaunch/contracts")
}

pub(super) fn split(dir: &Path, name: &str, files: Vec<RenderedTemplate>) -> Vec<RenderedTemplate> {
    if dir.join("domain/Cargo.toml").is_file()
        || dir
            .with_file_name(format!("{name}-contracts"))
            .join("Cargo.toml")
            .is_file()
    {
        return split_sibling(dir, name, files);
    }
    let host = new_host_directory(dir);
    let mut output = Vec::new();
    let mut inline = Vec::new();
    for mut file in files {
        let relative = file
            .path
            .strip_prefix(dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        match relative.as_str() {
            "Cargo.toml" => file.content = file.content.replace("path = \"domain\"", "path = \".forklaunch/contracts\""),
            "domain/Cargo.toml" => {
                file.path=host.join("Cargo.toml");
                file.content=file.content.replace("[package]", "[package]\npublish = false").replace("path = \"../sdk.rs\"", "path = \"lib.rs\"");
            }
            "domain/Makefile" | "sdk.rs" | "api/contracts/mod.rs" => continue,
            _ if relative.starts_with("api/contracts/") => {
                let module=Path::new(&relative).file_stem().unwrap().to_string_lossy().into_owned();
                inline.push((module, inline_contract(&file.content)));
                continue;
            }
            "build.rs" => file.content=file.content.replace("api/contracts", "api/controllers").replace("    println!(\"cargo:rerun-if-changed=sdk.rs\");\n", "").replacen("let evidence =", "println!(\"cargo:rerun-if-changed=.forklaunch/contracts\");\n    let evidence =",1),
            _ if relative.starts_with("api/routes/") => {
                file.content=file.content.replace("contracts::health::", "crate::api::controllers::health::contract::");
            }
            _ => {}
        }
        output.push(file);
    }
    for (module, declaration) in &inline {
        let file = output
            .iter_mut()
            .find(|file| file.path == dir.join(format!("api/controllers/{module}.rs")))
            .expect("controller for generated contract");
        file.content.push_str(&format!("\n{declaration}\n"));
    }
    output.push(RenderedTemplate {
        path: host.join("lib.rs"),
        content: host_source(
            name,
            &inline.into_iter().map(|(name, _)| name).collect::<Vec<_>>(),
        ),
        context: None,
    });
    output.extend(sdk_consumer::local_templates(dir, name, &host));
    output
}

/// Preserve ordinary Rust authoring while exporting only the explicit API module.
pub(super) fn inline_contract(source: &str) -> String {
    source
        .split("pub use operations")
        .next()
        .unwrap_or(source)
        .replace("mod operations", "pub mod contract")
        .replace("crate::schemas::", "crate::domain::schemas::")
}
pub(super) fn host_source(name: &str, modules: &[String]) -> String {
    let exports=modules.iter().map(|module|format!("forklaunch_core::contract::controller_contract!({module}, \"../../api/controllers/{module}.rs\");\n")).collect::<String>();
    let routes = modules
        .iter()
        .map(|module| {
            format!("routes.extend({module}::contract::registry().routes().iter().cloned());\n")
        })
        .collect::<String>();
    let bindings = modules
        .iter()
        .map(|module| format!("declarations.extend({module}::contract::binding_manifest()?);\n"))
        .collect::<String>();
    format!(
        "//! Generated private compiler host; edit controller contracts and domain schemas.\n#[path=\"../../domain/schemas/mod.rs\"]\npub mod schemas;\npub mod domain {{pub use crate::schemas;}}\n{exports}\npub fn routes()->Vec<forklaunch_core::contract::RouteContract>{{let mut routes=Vec::new();{routes}routes}}\npub fn registry()->forklaunch_core::contract::ContractRegistry{{let mut registry=forklaunch_core::contract::ContractRegistry::for_service(forklaunch_core::contract::ServiceContract::new(\"{name}\",env!(\"CARGO_PKG_VERSION\")));for route in routes(){{registry.register(route);}}registry}}\npub fn binding_manifest()->Result<Vec<forklaunch_core::contract::assembly::BindingDeclaration>,String>{{let mut declarations=Vec::new();{bindings}Ok(declarations)}}\n"
    )
}

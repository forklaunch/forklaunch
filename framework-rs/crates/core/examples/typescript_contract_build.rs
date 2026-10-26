//! Internal source-distributed compiler; never imports an application server.
use anyhow::{Context, Result, ensure};
use forklaunch_core::build::{
    export_typescript_openapi, export_typescript_openapi_with_runtime, http_sdk_file_plan,
    openapi_artifact_sources, write_generated_output,
};
use std::path::{Component, Path, PathBuf};

fn relative(root: &Path, value: &serde_json::Value, field: &str) -> Result<PathBuf> {
    let text = value[field]
        .as_str()
        .with_context(|| format!("Missing build {field}"))?;
    let path = Path::new(text);
    ensure!(
        !path.is_absolute()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "Build paths must stay within the application"
    );
    Ok(root.join(path))
}

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|arg| arg == "--project") {
        ensure!(arguments.len() == 3, "Expected --project ROOT CONFIG_FILE");
        let root = Path::new(&arguments[1]).canonicalize()?;
        let config: serde_json::Value = serde_json::from_slice(&std::fs::read(&arguments[2])?)?;
        let service = relative(&root, &config, "service")?;
        let validator = config["validator"].as_str().context("Missing validator")?;
        let name = config["name"].as_str().context("Missing service name")?;
        let runtime = config["runtime"].as_str().context("Missing runtime")?;
        let document = export_typescript_openapi_with_runtime(&service, validator, runtime)?;
        let mut compiled = openapi_artifact_sources(&document, name)?;
        let authored = config["typescriptOwned"].as_bool().unwrap_or(false);
        if authored {
            let mut contract: serde_json::Value = serde_json::from_str(&compiled.contract)?;
            contract["sdkLayout"] = "project-local-v1".into();
            contract["rustClientName"] = "Client".into();
            contract["typescriptRuntimeLayout"] = "local".into();
            compiled.contract = serde_json::to_string(&contract)?;
            compiled.sources =
                forklaunch_core::build::http_artifact_sources(&compiled.contract, name)?;
        }
        // The generated modern consumer uses the shared runtime for all body
        // transports, so changing content types never requires manifest edits.
        // Resolve all destinations and compile both languages before publishing.
        let mut files = http_sdk_file_plan(
            &compiled.sources,
            &relative(&root, &config, "typescript")?,
            &relative(&root, &config, "rust")?,
        )?;
        if authored {
            let rust = relative(&root, &config, "rust")?;
            let directory = rust.parent().context("Missing Rust SDK directory")?;
            files.retain(|path, _| path.starts_with(directory));
        }
        files.insert(relative(&root, &config, "contract")?, compiled.contract);
        files.insert(relative(&root, &config, "openapi")?, document);
        for (path, contents) in files {
            write_generated_output(path, contents)?;
        }
        return Ok(());
    }
    ensure!(
        arguments.len() == 4,
        "Expected SERVICE VALIDATOR SERVICE_NAME OUTPUT_DIRECTORY"
    );
    let validator = arguments[1].to_str().context("Validator must be UTF-8")?;
    let name = arguments[2]
        .to_str()
        .context("Service name must be UTF-8")?;
    let document = export_typescript_openapi(Path::new(&arguments[0]), validator)?;
    let compiled = openapi_artifact_sources(&document, name)?;
    let directory = Path::new(&arguments[3]);
    let mut contract: serde_json::Value = serde_json::from_str(&compiled.contract)?;
    contract["typescriptRuntimeLayout"] = "local".into();
    let contract = serde_json::to_string(&contract)?;
    let sources = forklaunch_core::build::http_artifact_sources(&contract, name)?;
    let mut files = http_sdk_file_plan(
        &sources,
        &directory.join("index.ts"),
        &directory.join("lib.rs"),
    )?;
    files.insert(directory.join("contract.json"), contract);
    files.insert(directory.join("openapi.json"), document);
    for (file, contents) in files {
        write_generated_output(file, contents)?;
    }
    Ok(())
}

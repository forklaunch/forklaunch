//! Parse contract-export inputs without importing or evaluating application code.
use anyhow::{Context, Result, ensure};
use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

/// Collect syntax trees used to verify explicit router/contract/schema imports.
/// No service modules are evaluated. The Node adapter receives parsed syntax,
/// so applications keep their normal TypeScript compiler version.
/// # Errors
/// Rejects symlinks, invalid TypeScript and inputs outside the service directory.
pub fn typescript_contract_inputs(service: &Path) -> Result<Value> {
    fn parse(file: &Path, root: &Path, files: &mut BTreeMap<String, Value>) -> Result<()> {
        ensure!(
            !fs::symlink_metadata(file)?.file_type().is_symlink(),
            "Contract source must not be a symlink"
        );
        let canonical = file.canonicalize()?;
        ensure!(
            canonical.starts_with(root),
            "Contract source must remain inside its service"
        );
        let text = fs::read_to_string(&canonical)?;
        ensure!(
            text.len() <= 4 * 1024 * 1024,
            "Contract source exceeds 4 MiB"
        );
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, &text, SourceType::ts()).parse();
        ensure!(
            !parsed.fatal_error && !parsed.diagnostics.has_errors(),
            "Invalid TypeScript syntax in {}",
            file.display()
        );
        files.insert(
            canonical.to_string_lossy().into_owned(),
            serde_json::from_str(&parsed.program.to_estree_json(true, false))?,
        );
        Ok(())
    }
    fn walk(directory: &Path, root: &Path, files: &mut BTreeMap<String, Value>) -> Result<()> {
        for entry in fs::read_dir(directory).with_context(|| {
            format!(
                "Missing explicit contract directory {}",
                directory.display()
            )
        })? {
            let entry = entry?;
            let kind = entry.file_type()?;
            ensure!(
                !kind.is_symlink(),
                "Contract source tree must not contain symlinks"
            );
            if kind.is_dir() {
                walk(&entry.path(), root, files)?;
            } else if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "ts")
            {
                parse(&entry.path(), root, files)?;
            }
        }
        Ok(())
    }
    let service = service.canonicalize()?;
    let mut files = BTreeMap::new();
    parse(&service.join("server.ts"), &service, &mut files)?;
    for directory in [
        "api/routes",
        "api/controllers",
        "api/contracts",
        "domain/schemas",
        "domain/enum",
    ] {
        let path = service.join(directory);
        if directory.starts_with("domain/") && !path.exists() {
            continue;
        }
        walk(&path, &service, &mut files)?;
    }
    Ok(json!({"files": files}))
}

/// Export admitted contract metadata using the application's installed validator.
/// This runs the normal `tsx` loader against explicit pure contract modules and
/// never imports the service entrypoint, controllers or dependency container.
/// # Errors
/// Rejects unsupported source graphs, missing Node/tsx tooling and failed exports.
pub fn export_typescript_openapi(service: &Path, validator: &str) -> Result<String> {
    export_typescript_openapi_with_runtime(service, validator, "node")
}

/// Export isolated contracts using Node's `tsx` loader or Bun's native TypeScript loader.
/// # Errors
/// Rejects unsupported runtimes, unsafe source graphs and failed exports.
pub fn export_typescript_openapi_with_runtime(
    service: &Path,
    validator: &str,
    runtime: &str,
) -> Result<String> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    ensure!(
        matches!(runtime, "node" | "bun"),
        "Unsupported TypeScript export runtime"
    );
    let inputs = serde_json::to_vec(&typescript_contract_inputs(service)?)?;
    let service = service.canonicalize()?;
    let mut command = Command::new(runtime);
    if runtime == "node" {
        command.args(["--import", "tsx", "--input-type=module"]);
    }
    let mut child = command
        .args([
            "--eval",
            include_str!("runtime/typescript_source.mjs"),
            "--",
        ])
        .arg(&service)
        .arg(validator)
        .current_dir(&service)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start isolated TypeScript export")?;
    child
        .stdin
        .take()
        .context("Missing export stdin")?
        .write_all(&inputs)?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Isolated contract export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document =
        String::from_utf8(output.stdout).context("Contract export must be UTF-8 JSON")?;
    let _: Value = serde_json::from_str(&document)
        .context("Contract export must produce exactly one JSON document")?;
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pure_contracts_do_not_require_empty_domain_directories() {
        let root =
            std::env::temp_dir().join(format!("pure-contract-inputs-{}", std::process::id()));
        for directory in ["api/routes", "api/controllers", "api/contracts"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(root.join("server.ts"), "export {};\n").unwrap();
        let inputs = typescript_contract_inputs(&root).unwrap();
        assert_eq!(inputs["files"].as_object().unwrap().len(), 1);
        fs::remove_dir(root.join("api/contracts")).unwrap();
        assert!(typescript_contract_inputs(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

//! Compile portable contracts with the same compiler linked into Cargo build helpers.
//! Distributed CLI binaries emit both SDK languages without a local Rust toolchain.
use anyhow::{Result, bail};
use serde_json::Value;

#[derive(Clone, Debug)]
pub(super) struct CompiledSdk {
    pub typescript: String,
    pub rust: String,
    pub typescript_runtime_shared: bool,
    pub typescript_files: std::collections::BTreeMap<String, String>,
    pub rust_files: std::collections::BTreeMap<String, String>,
    pub rust_module: bool,
    pub request_multipart: bool,
    pub response_multipart: bool,
}
pub(super) fn requested(artifact: &Value) -> Result<bool> {
    match artifact.get("sdkCompiler") {
        None => Ok(false),
        Some(value) if value.as_str() == Some("forklaunch.sdk.http.v1") => Ok(true),
        Some(_) => bail!("Unsupported contract SDK compiler version"),
    }
}
pub(super) fn compile(service: &str, artifact: &Value) -> Result<CompiledSdk> {
    anyhow::ensure!(
        !service.is_empty()
            && service
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
        "Invalid SDK service name"
    );
    let compiled =
        forklaunch_core::build::http_artifact_sources(&serde_json::to_string(artifact)?, service)?;
    let rust_module = match artifact.get("rustOutputLayout") {
        None => false,
        Some(value) if value.as_str() == Some("service-module") && artifact.get("rustClientName").and_then(Value::as_str).is_some() => true,
        Some(_) => bail!("Unsupported Rust SDK layout or missing client name; rebuild the service contract"),
    };
    Ok(CompiledSdk {
        rust_module,
        typescript: compiled.typescript,
        rust: compiled.rust,
        typescript_files: compiled.typescript_files,
        typescript_runtime_shared: compiled.typescript_runtime_shared,
        rust_files: compiled.rust_files,
        request_multipart: compiled.request_multipart,
        response_multipart: compiled.response_multipart,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_contract_compiles_both_languages_in_memory() {
        let artifact = serde_json::json!({
            "formatVersion": "forklaunch.contract.v1",
            "sdkCompiler": "forklaunch.sdk.http.v1",
            "routes": [{
                "operationName": "readNote",
                "method": "GET",
                "path": "/notes/current",
                "request": {"params": [], "query": [], "headers": []},
                "responses": [{
                    "statusCode": 200,
                    "headers": [],
                    "body": {
                        "kind": "json",
                        "contentType": "application/json",
                        "schema": {"name": "Note", "jsonSchema": {
                            "type": "object",
                            "properties": {"title": {"type": "string", "minLength": 1}},
                            "required": ["title"],
                            "additionalProperties": false
                        }}
                    }
                }]
            }]
        });
        let compiled = compile("notes", &artifact).unwrap();
        assert!(compiled.typescript.contains("readNote"));
        assert!(compiled.rust.contains("read_note"));
        assert!(compiled.typescript.contains("minLength"));
        assert!(compiled.rust.contains("minLength"));
        assert_eq!(compiled.typescript_files.keys().map(String::as_str).collect::<Vec<_>>(), ["client.ts", "index.ts", "models.ts", "runtime.ts"]);
        assert_eq!(compiled.rust_files.keys().map(String::as_str).collect::<Vec<_>>(), ["client.rs", "lib.rs", "models.rs", "schemas.rs"]);
        assert!(!compiled.request_multipart);
        assert!(!compiled.response_multipart);
        let mut invalid = artifact;
        invalid["routes"][0]["responses"][0]["body"]["kind"] = "invented".into();
        assert!(compile("notes", &invalid).is_err());
    }

    #[test]
    fn only_explicit_supported_compiler_artifacts_are_accepted() {
        assert!(
            !requested(&serde_json::json!({"formatVersion":"forklaunch.contract.v1"})).unwrap()
        );
        assert!(requested(&serde_json::json!({"sdkCompiler":"forklaunch.sdk.http.v1"})).unwrap());
        assert!(requested(&serde_json::json!({"sdkCompiler":"unknown"})).is_err());
    }
}

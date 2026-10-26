//! The CLI and build scripts share this SDK compiler entrypoint.
use crate::build::{module_source, write_output};
use anyhow::{Context, Result};
use forklaunch_core::codegen::{SdkGenerationPlan, SdkTarget};
use std::path::Path;

/// Complete generated sources and their transport dependency requirements.
pub struct HttpSdkSources {
    pub typescript: String,
    pub rust: String,
    /// Service-relative sources; runtime.ts belongs once at the TypeScript package root.
    pub typescript_files: std::collections::BTreeMap<String, String>,
    /// Whether runtime.ts is owned by the parent package instead of the service directory.
    pub typescript_runtime_shared: bool,
    /// Crate-relative facade and implementation sources.
    pub rust_files: std::collections::BTreeMap<String, String>,
    pub request_multipart: bool,
    pub response_multipart: bool,
}

/// Resolve all package SDK source files before publishing any of them.
/// `lib.rs` denotes a standalone Rust crate; other Rust filenames are flat
/// service facades with implementation files in a same-named sibling directory.
/// # Errors
/// Rejects destinations without owning directories or valid Rust module names.
pub fn http_sdk_file_plan(
    sources: &HttpSdkSources,
    typescript: &Path,
    rust: &Path,
) -> Result<std::collections::BTreeMap<std::path::PathBuf, String>> {
    let mut files = std::collections::BTreeMap::new();
    let ts_directory = typescript
        .parent()
        .context("Missing TypeScript SDK directory")?;
    for (name, source) in &sources.typescript_files {
        let path = if name == "index.ts" {
            typescript.to_owned()
        } else if name == "runtime.ts" && sources.typescript_runtime_shared {
            ts_directory
                .parent()
                .context("Missing TypeScript package root")?
                .join(name)
        } else {
            ts_directory.join(name)
        };
        anyhow::ensure!(
            files.insert(path, source.clone()).is_none(),
            "Conflicting generated SDK paths"
        );
    }
    let standalone = rust.file_name().is_some_and(|name| name == "lib.rs");
    let rust_directory = if standalone {
        rust.parent()
            .context("Missing Rust SDK directory")?
            .to_owned()
    } else {
        rust.with_file_name(rust.file_stem().context("Missing Rust SDK file stem")?)
    };
    for (name, source) in &sources.rust_files {
        let (path, contents) = if name == "lib.rs" {
            (
                rust.to_owned(),
                if standalone {
                    source.clone()
                } else {
                    super::rust_http_facade(rust, &sources.rust_files)?
                },
            )
        } else {
            (rust_directory.join(name), source.clone())
        };
        anyhow::ensure!(
            files.insert(path, contents).is_none(),
            "Conflicting generated SDK paths"
        );
    }
    Ok(files)
}

/// Portable normalized contract and generated SDKs from a TypeScript/OpenAPI export.
pub struct ImportedHttpSdkSources {
    /// Canonical contract JSON, suitable for the same build and CLI compiler.
    pub contract: String,
    /// Generated TypeScript and Rust source with transport requirements.
    pub sources: HttpSdkSources,
}

/// Compile a side-effect-free TypeScript/OpenAPI export for both SDK languages.
/// # Errors
/// Rejects invalid JSON and unsupported schema, transport or naming semantics.
pub fn openapi_artifact_sources(source: &str, service: &str) -> Result<ImportedHttpSdkSources> {
    let document = serde_json::from_str(source)?;
    let mut contract = super::imports::contract_from_openapi(&document)?;
    name_imported_schemas(&mut contract)?;
    contract["formatVersion"] = serde_json::json!("forklaunch.contract.v1");
    contract["sdkCompiler"] = serde_json::json!("forklaunch.sdk.http.v1");
    let contract = serde_json::to_string_pretty(&contract)?;
    let sources = http_artifact_sources(&contract, service)?;
    Ok(ImportedHttpSdkSources { contract, sources })
}

// Legacy OpenAPI inputs carry inline schemas without Rust type names. Assign
// deterministic names at the canonical compiler boundary, preserving schema data.
fn name_imported_schemas(contract: &mut serde_json::Value) -> Result<()> {
    use anyhow::Context as _;
    use convert_case::{Case, Casing as _};
    fn name_route(route: &mut serde_json::Value, name: &str) -> Result<()> {
        for section in ["params", "query", "headers"] {
            for field in route["request"][section]
                .as_array_mut()
                .context("Imported fields missing")?
            {
                let field_name = field["name"]
                    .as_str()
                    .context("Imported field name missing")?
                    .to_case(Case::Pascal);
                field["schema"]["name"] =
                    format!("{name}{}{field_name}", section.to_case(Case::Pascal)).into();
            }
        }
        if let Some(body) = route["request"].get_mut("body") {
            body["schema"]["name"] = format!("{name}Request").into();
        }
        for response in route["responses"]
            .as_array_mut()
            .context("Imported responses missing")?
        {
            let status = response["statusCode"]
                .as_u64()
                .context("Imported status missing")?;
            for field in response["headers"]
                .as_array_mut()
                .context("Imported response headers missing")?
            {
                let field_name = field["name"]
                    .as_str()
                    .context("Imported header name missing")?
                    .to_case(Case::Pascal);
                field["schema"]["name"] = format!("{name}Response{status}{field_name}").into();
            }
            if response["body"].is_null() {
                response
                    .as_object_mut()
                    .context("Imported response missing")?
                    .remove("body");
            } else {
                response["body"]["schema"]["name"] = format!("{name}Response{status}").into();
            }
        }
        Ok(())
    }
    for route in contract["routes"]
        .as_array_mut()
        .context("Imported routes missing")?
    {
        let name = route["operationName"]
            .as_str()
            .context("Imported operation name missing")?
            .to_case(Case::Pascal);
        name_route(route, &name)?;
        if let Some(versions) = route
            .get_mut("versions")
            .and_then(serde_json::Value::as_array_mut)
        {
            for version in versions {
                let label = version["version"]
                    .as_str()
                    .context("Imported version name missing")?
                    .to_case(Case::Pascal);
                name_route(version, &format!("{name}Version{label}"))?;
            }
        }
    }
    Ok(())
}
/// Compile an exported contract in memory, without Cargo, application code or filesystem writes.
/// # Errors
/// Rejects unsupported artifact, schema, media and SDK naming semantics.
pub fn http_artifact_sources(source: &str, service: &str) -> Result<HttpSdkSources> {
    let ts_plan = SdkGenerationPlan::from_artifact_json(SdkTarget::TypeScript, source)?;
    let rust_plan = SdkGenerationPlan::from_artifact_json(SdkTarget::Rust, source)?;
    let ts = forklaunch_core::typescript::generate_typescript_http_sdk_source(
        &ts_plan,
        &forklaunch_core::typescript::TypeScriptHttpSdkOptions::new(service),
    )?;
    let artifact: serde_json::Value = serde_json::from_str(source)?;
    let mut rust_options = forklaunch_core::rust_sdk::RustSdkOptions::new(format!("{service}-sdk"));
    if let Some(name) = artifact.get("rustClientName") {
        rust_options = rust_options.with_client_struct(
            name.as_str()
                .context("Rust SDK client name must be a string")?,
        );
    }
    let rust = forklaunch_core::rust_sdk::generate_rust_http_sdk_source(&rust_plan, &rust_options)?;
    let typescript_runtime_shared = match artifact
        .get("typescriptRuntimeLayout")
        .map(|value| {
            value
                .as_str()
                .context("TypeScript runtime layout must be a string")
        })
        .transpose()?
    {
        None | Some("shared-parent") => true,
        Some("local") => false,
        Some(_) => anyhow::bail!("Unsupported TypeScript runtime layout"),
    };
    let typescript_files = if artifact
        .get("sdkLayout")
        .and_then(serde_json::Value::as_str)
        == Some("project-local-v1")
    {
        std::collections::BTreeMap::from([
            (
                "index.ts".to_owned(),
                "export type * from './sdk.js';\n".to_owned(),
            ),
            (
                "sdk.ts".to_owned(),
                crate::typescript::generate_typescript_service_declarations(
                    &ts_plan,
                    &crate::typescript::TypeScriptHttpSdkOptions::new(service),
                )?,
            ),
        ])
    } else {
        crate::typescript::generate_typescript_http_sdk_files_with_runtime(
            &ts_plan,
            &crate::typescript::TypeScriptHttpSdkOptions::new(service),
            if typescript_runtime_shared {
                "../runtime"
            } else {
                "./runtime"
            },
        )?
    };
    let rust_files = crate::rust_sdk::generate_rust_http_sdk_files(&rust_plan, &rust_options)?;
    Ok(HttpSdkSources {
        typescript_files,
        typescript_runtime_shared,
        rust_files,
        typescript: ts,
        rust: module_source(&rust),
        request_multipart: rust_plan.operations.iter().any(|operation| {
            operation.request_body_kind == Some(forklaunch_core::codegen::BodyKind::Multipart)
        }),
        response_multipart: rust_plan.operations.iter().any(|operation| {
            operation.responses.iter().any(|response| {
                response.body_kind == Some(forklaunch_core::codegen::BodyKind::Multipart)
            })
        }),
    })
}

/// Generate both HTTP SDK languages from an exported contract without loading application code.
/// # Errors
/// Returns artifact/schema/media validation and filesystem errors.
pub fn http_artifact_sdk(source: &str, service: &str, output: impl AsRef<Path>) -> Result<()> {
    let compiled = http_artifact_sources(source, service)?;
    let output = output.as_ref();
    let requirements = serde_json::json!({
        "requestMultipart": compiled.request_multipart,
        "responseMultipart": compiled.response_multipart,
    });
    write_output(&output.join("requirements.json"), &requirements.to_string())?;
    // Compatibility files remain for callers explicitly requesting standalone source.
    write_output(&output.join("index.ts"), &compiled.typescript)?;
    write_output(&output.join("lib.rs"), &compiled.rust)?;
    for (name, contents) in &compiled.typescript_files {
        let directory = if name == "runtime.ts" && compiled.typescript_runtime_shared {
            output.join("typescript")
        } else {
            output.join("typescript").join(service)
        };
        write_output(&directory.join(name), contents)?;
    }
    for (name, contents) in &compiled.rust_files {
        write_output(&output.join("rust").join(name), contents)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use forklaunch_core::contract::{
        BodyContract, ContractRegistry, RouteContract, ServiceContract,
    };
    #[test]
    fn imported_openapi_compiles_inline_schemas_and_optional_requests() {
        let source = serde_json::json!({"openapi":"3.1.0", "paths":{"/notes/{id}":{"post":{
            "operationId":"notes.create", "parameters":[{"name":"id","in":"path","required":true,"schema":{"type":"string"}}],
            "requestBody":{"required":false,"content":{"application/json":{"schema":{"type":"object","properties":{"title":{"type":"string"}},"required":["title"],"additionalProperties":false}}}},
            "responses":{"201":{"description":"Created","headers":{"x-revision":{"required":true,"schema":{"type":"string"}}},"content":{"application/json":{"schema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"],"additionalProperties":false}}}},"204":{"description":"Empty"}}
        }}}});
        let compiled = openapi_artifact_sources(&source.to_string(), "notes").unwrap();
        let contract: serde_json::Value = serde_json::from_str(&compiled.contract).unwrap();
        assert_eq!(contract["routes"][0]["request"]["bodyOptional"], true);
        assert_eq!(
            contract["routes"][0]["responses"][0]["headers"][0]["name"],
            "x-revision"
        );
        assert!(compiled.sources.typescript.contains("notesCreate"));
        assert!(compiled.sources.rust.contains("notes_create"));
        assert!(compiled.sources.typescript.contains("x-revision"));
    }

    #[test]
    fn imported_named_nullable_and_union_payloads_keep_distinct_identities() {
        use serde_json::json;
        let person = json!({"type":"object","properties":{"name":{"type":"string","minLength":1}},
            "required":["name"],"additionalProperties":false});
        let variants = [
            json!({"type":["object","null"],"properties":{"name":{"type":"string","minLength":1}},
                "required":["name"],"additionalProperties":false}),
            json!({"type":"object","nullable":true,"properties":{"name":{"type":"string","minLength":1}},
                "required":["name"],"additionalProperties":false}),
            json!({"anyOf":[{"$ref":"#/components/schemas/Person"},{"type":"null"}]}),
            json!({"type":["object","string"],"properties":{"name":{"type":"string","minLength":1}},
                "required":["name"],"additionalProperties":false}),
        ];
        for payload in variants {
            let nullable = payload["nullable"] == true
                || payload.get("anyOf").is_some()
                || payload["type"]
                    .as_array()
                    .is_some_and(|types| types.iter().any(|kind| kind == "null"));
            let document = json!({"openapi":"3.1.0","components":{"schemas":{"Nullable":payload,"Person":person}},
            "paths":{
                "/nullable":{"post":{"operationId":"writeNullable",
                    "requestBody":{"required":true,"content":{"application/json":{"schema":{"$ref":"#/components/schemas/Nullable"}}}},
                    "responses":{"200":{"description":"ok","content":{"application/json":{"schema":{"$ref":"#/components/schemas/Nullable"}}}}}}},
                "/person":{"get":{"operationId":"readPerson",
                    "responses":{"200":{"description":"ok","content":{"application/json":{"schema":{"$ref":"#/components/schemas/Person"}}}}}}}
            }});
            let original = document.clone();
            let compiled = openapi_artifact_sources(&document.to_string(), "identity").unwrap();
            assert_eq!(
                document, original,
                "Import must preserve the source schema graph"
            );
            assert_eq!(
                compiled.sources.rust.matches("pub struct Person {").count(),
                1
            );
            assert!(
                !compiled
                    .sources
                    .rust
                    .contains("pub type NullableValue = Nullable;")
            );
            let contract: serde_json::Value = serde_json::from_str(&compiled.contract).unwrap();
            let plan = crate::codegen::SdkGenerationPlan::from_artifact_json(
                crate::codegen::SdkTarget::Rust,
                &compiled.contract,
            )
            .unwrap();
            assert_eq!(plan.operations.len(), 2);
            let response = &contract["routes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|route| route["operationName"] == "writeNullable")
                .unwrap()["responses"][0]["body"]["schema"]["jsonSchema"];
            let validator = crate::contract::schema_validation::compile_checked(response).unwrap();
            assert!(validator(&json!({"name":"Ada"})).is_ok());
            assert!(validator(&json!({"name":""})).is_err());
            assert!(validator(&json!({"name":"Ada","extra":true})).is_err());
            assert_eq!(validator(&serde_json::Value::Null).is_ok(), nullable);
            assert_eq!(validator(&json!("scalar")).is_ok(), !nullable);
        }
    }

    #[test]
    fn service_local_artifact_emits_types_without_a_copied_transport() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("notes", "1"));
        registry
            .register(RouteContract::get("Health", "Health", "/health").with_response(204, None));
        let mut artifact: serde_json::Value =
            serde_json::from_str(&registry.into_artifact().to_json()).unwrap();
        artifact["sdkLayout"] = "project-local-v1".into();
        artifact["typescriptRuntimeLayout"] = "local".into();
        artifact["rustClientName"] = "NotesClient".into();
        let compiled = http_artifact_sources(&artifact.to_string(), "notes").unwrap();
        assert_eq!(
            compiled
                .typescript_files
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["index.ts", "sdk.ts"]
        );
        assert_eq!(
            compiled.typescript_files["index.ts"],
            "export type * from './sdk.js';\n"
        );
        assert!(compiled.typescript_files["sdk.ts"].contains("NotesSdkClient"));
        assert!(!compiled.typescript_files["sdk.ts"].contains("function createForkLaunchSdk"));
        assert!(!compiled.typescript_runtime_shared);
        assert!(compiled.rust_files["client.rs"].contains("NotesClient"));
    }

    #[test]
    fn custom_client_metadata_matches_host_compiled_module() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("notes", "1"));
        registry
            .register(RouteContract::get("Health", "Health", "/health").with_response(204, None));
        let root = std::env::temp_dir().join(format!("custom-client-sdk-{}", std::process::id()));
        crate::build::typescript_source(
            &registry,
            "notes",
            &root.join("typescript/notes"),
            Some("NotesClient"),
        )
        .unwrap();
        crate::build::rust_http_module(
            &registry,
            "notes-sdk",
            "NotesClient",
            root.join("notes.rs"),
        )
        .unwrap();
        let artifact =
            std::fs::read_to_string(root.join("typescript/notes/contract.json")).unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&artifact).unwrap();
        assert_eq!(metadata["rustOutputLayout"], "service-module");
        let compiled = http_artifact_sources(&artifact, "notes").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("notes.rs")).unwrap(),
            crate::build::rust_http_facade(&root.join("notes.rs"), &compiled.rust_files).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn artifact_sdk_matches_host_compiled_sdk() {
        let mut registry = ContractRegistry::for_service(ServiceContract::new("notes", "1"));
        registry.register(
            RouteContract::get("Health", "Health", "/health")
                .with_response(200, Some(BodyContract::json("Health"))),
        );
        let root = std::env::temp_dir().join(format!("artifact-sdk-{}", std::process::id()));
        crate::build::typescript(&registry, "notes", root.join("build")).unwrap();
        crate::build::rust_http_module(
            &registry,
            "notes-sdk",
            "ForkLaunchClient",
            root.join("build/lib.rs"),
        )
        .unwrap();
        http_artifact_sdk(
            &registry.into_artifact().to_json(),
            "notes",
            root.join("artifact"),
        )
        .unwrap();
        for file in ["index.ts", "client.ts", "models.ts"] {
            let actual = std::fs::read_to_string(root.join("build").join(file)).unwrap();
            let shared =
                std::fs::read_to_string(root.join("artifact/typescript/notes").join(file)).unwrap();
            assert_eq!(
                actual,
                shared.replace("from '../runtime'", "from './runtime'")
            );
        }
        assert_eq!(
            std::fs::read(root.join("build/runtime.ts")).unwrap(),
            std::fs::read(root.join("artifact/typescript/runtime.ts")).unwrap()
        );
        for file in ["models.rs", "client.rs", "schemas.rs"] {
            assert_eq!(
                std::fs::read(root.join("build/lib").join(file)).unwrap(),
                std::fs::read(root.join("artifact/rust").join(file)).unwrap()
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

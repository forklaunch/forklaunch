use super::*;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    create_dir_all(root.path().join(".forklaunch/contracts/native-api")).unwrap();
    create_dir_all(root.path().join("artifacts/typescript")).unwrap();
    create_dir_all(root.path().join(".forklaunch/openapi/legacy-api")).unwrap();
    write(
        root.path().join(".forklaunch/manifest.toml"),
        r#"
name = "mixed-sdk"
language = "mixed"
[[projects]]
name = "native-api"
type = "Service"
routers = ["health"]
[projects.metadata]
language = "rust"
[[projects]]
name = "typescript-api"
type = "Service"
[projects.metadata]
language = "typescript"
openapiPath = "artifacts/typescript/spec.json"
[[projects]]
name = "legacy-api"
type = "Service"
[[projects]]
name = "helpers"
type = "Library"
[[projects]]
name = "minio"
type = "Service"
"#,
    )
    .unwrap();
    // Applications select their framework crates; nothing is vendored.
    let manifest_path = root.path().join(".forklaunch/manifest.toml");
    let manifest_source = read_to_string(&manifest_path).unwrap();
    write(&manifest_path, format!("{manifest_source}\n[framework]\nversion = \"0.1.2\"\nblueprint_version = \"0.1.0\"\n")).unwrap();
    write(root.path().join(".forklaunch/contracts/native-api/contract.json"), json!({
        "routes":[{"method":"GET","path":"/native","operationName":"getNative","responses":[{"statusCode":200,"body":{"schema":{"jsonSchema":{"type":"object","properties":{"origin":{"type":"string"}},"required":["origin"]}}}}]}]
    }).to_string()).unwrap();
    write(root.path().join("artifacts/typescript/spec.json"), json!({
        "openapi":"3.1.0","info":{"title":"TypeScript service","version":"1"},
        "paths":{"/profiles/{id}":{"get":{"operationId":"getProfile","parameters":[{"name":"id","in":"path","required":true,"schema":{"type":"string"}}],"responses":{"200":{"description":"Profile","content":{"application/json":{"schema":{"$ref":"#/components/schemas/Profile"}}}}}}}},
        "components":{"schemas":{"Profile":{"type":"object","properties":{"profileName":{"type":"string"}},"required":["profileName"]}}}
    }).to_string()).unwrap();
    write(
        root.path()
            .join(".forklaunch/openapi/legacy-api/openapi.json"),
        json!({
            "openapi":"3.1.0","info":{"title":"Legacy service","version":"1"},"paths":{}
        })
        .to_string(),
    )
    .unwrap();
    root
}

fn generate(root: &Path, check: bool) -> Result<()> {
    generate_or_check_sdk_outputs(
        root,
        "typescript",
        "contract",
        None,
        check,
        &mut StandardStream::stdout(ColorChoice::Never),
    )
}

#[test]
fn mixed_contract_source_imports_typescript_openapi_and_legacy_services() {
    let root = fixture();
    let manifest = read_manifest(root.path()).unwrap();
    let services = http_sdk_services_from_contracts(root.path(), &manifest, None).unwrap();
    assert_eq!(
        services
            .iter()
            .map(|service| service.name.as_str())
            .collect::<Vec<_>>(),
        ["native-api", "typescript-api", "legacy-api"]
    );
    let source = typescript_sdk(&services).unwrap();
    assert!(source.contains("getNative:"));
    assert!(source.contains("getProfile:"));
    assert!(source.contains("profileName: string"));
    assert!(!source.contains("helpers:"));
    assert!(!source.contains("minio:"));
    for service in &services {
        rust_sdk(service).unwrap();
    }
    let selected =
        http_sdk_services_from_contracts(root.path(), &manifest, Some("typescript-api")).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].name, "typescript-api");
}

#[test]
fn lifecycle_preserves_mixed_clients_and_check_is_read_only() {
    let root = fixture();
    generate(root.path(), false).unwrap();
    let path = root.path().join("sdks/typescript/src/index.ts");
    let before = read_to_string(&path).unwrap();
    let state = root.path().join(crate::core::forklaunch_state::MANIFEST);
    let tracked = std::fs::read(&state).unwrap();
    generate(root.path(), true).unwrap();
    assert_eq!(std::fs::read(&state).unwrap(), tracked);
    regenerate_sdk_outputs_if_present(root.path(), &mut StandardStream::stdout(ColorChoice::Never))
        .unwrap();
    assert_eq!(read_to_string(&path).unwrap(), before);
    let services = crate::sdk::http_sources::read(root.path()).unwrap();
    assert_eq!(
        services,
        BTreeSet::from(["typescript-api".into(), "legacy-api".into()])
    );
}

#[test]
fn missing_tracked_typescript_export_does_not_remove_its_sdk() {
    let root = fixture();
    generate(root.path(), false).unwrap();
    let path = root.path().join("sdks/typescript/src/index.ts");
    let before = std::fs::read(&path).unwrap();
    std::fs::remove_file(root.path().join("artifacts/typescript/spec.json")).unwrap();
    let error = regenerate_sdk_outputs_if_present(
        root.path(),
        &mut StandardStream::stdout(ColorChoice::Never),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("Restore the TypeScript OpenAPI export"));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn new_unexported_typescript_service_does_not_break_native_lifecycle() {
    let root = fixture();
    std::fs::remove_file(root.path().join("artifacts/typescript/spec.json")).unwrap();
    let services =
        rust_sdk_services_for_lifecycle(root.path(), &read_manifest(root.path()).unwrap()).unwrap();
    assert_eq!(services.len(), 2);
    assert!(
        services
            .iter()
            .all(|service| service.name != "typescript-api")
    );
    assert!(
        generate(root.path(), false).is_err(),
        "explicit generation requires all selected exports"
    );
    assert!(!root.path().join("sdks/typescript/src/index.ts").exists());
}

#[cfg(unix)]
#[test]
fn mixed_artifacts_and_tracking_reject_external_symlinks() {
    let root = fixture();
    let outside = tempfile::tempdir().unwrap();
    let spec = root.path().join("artifacts/typescript/spec.json");
    std::fs::rename(&spec, outside.path().join("spec.json")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("spec.json"), &spec).unwrap();
    assert!(generate(root.path(), false).is_err());
    std::fs::remove_file(&spec).unwrap();
    std::fs::rename(outside.path().join("spec.json"), &spec).unwrap();
    assert!(!root.path().join("sdks/typescript/src/index.ts").exists());
}

#[test]
fn typescript_only_artifact_emits_both_sdk_languages() {
    let root = fixture();
    let mut manifest = read_manifest(root.path()).unwrap();
    manifest["language"] = Value::String("typescript".into());
    manifest["projects"]
        .as_array_mut()
        .unwrap()
        .retain(|project| project.get("name").and_then(Value::as_str) == Some("typescript-api"));
    write(
        root.path().join(".forklaunch/manifest.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    assert!(!rust_project::is_rust_app_root(root.path()).unwrap());
    let original_manifest = read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap();
    generate_or_check_sdk_outputs(
        root.path(),
        "all",
        "openapi",
        None,
        false,
        &mut StandardStream::stdout(ColorChoice::Never),
    )
    .unwrap();
    let typescript = read_to_string(root.path().join("sdks/typescript/src/index.ts")).unwrap();
    let rust = read_to_string(root.path().join("sdks/rust/typescript-api-sdk/src/lib.rs")).unwrap();
    assert!(typescript.contains("getProfile"));
    assert!(rust.contains("get_profile"));
    assert!(typescript.contains("profileName"));
    assert!(rust.contains("profile_name"));
    assert_eq!(
        original_manifest,
        read_to_string(root.path().join(".forklaunch/manifest.toml")).unwrap()
    );
    assert!(!root.path().join("Dockerfile.rust").exists());
    generate_or_check_sdk_outputs(
        root.path(),
        "all",
        "openapi",
        None,
        true,
        &mut StandardStream::stdout(ColorChoice::Never),
    )
    .unwrap();
}

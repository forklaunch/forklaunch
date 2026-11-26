//! Reproducible evidence for a declared build input scope and generated outputs.
//! This is freshness evidence, not an attestation that arbitrary application
//! handlers implement a contract or execute its authorization policy.
use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
mod bindings;
pub use bindings::BindingManifest;
mod asset_manifest;
mod assets;
pub use asset_manifest::AssetManifest;
mod service;
pub use assets::GeneratedAssets;
pub use service::{ServiceBuild, contract_directory};

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read as _,
    path::{Component, Path, PathBuf},
};

const FORMAT: &str = "forklaunch.build.provenance.v1";
const MAX_FILES: usize = 10_000;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// An explicit source/configuration scope. Directory membership is included,
/// so adding a new source invalidates the snapshot as well as changing a file.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputScope {
    pub path: String,
    pub optional: bool,
}
impl InputScope {
    #[must_use]
    pub fn required(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            optional: false,
        }
    }
    #[must_use]
    pub fn optional(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            optional: true,
        }
    }
}

/// Caller-selected target and configuration. These describe this one export,
/// never an inferred feature/target matrix.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildContext {
    pub host: String,
    pub target: String,
    pub features: Vec<String>,
    pub configuration: BTreeMap<String, String>,
}
impl BuildContext {
    /// Read only Cargo's public target and feature declarations, never secrets.
    /// # Errors
    /// Rejects calls outside Cargo without an explicit host/target.
    pub fn cargo() -> Result<Self> {
        let mut features = std::env::vars_os()
            .filter_map(|(name, _)| {
                name.to_str()?
                    .strip_prefix("CARGO_FEATURE_")
                    .map(str::to_owned)
            })
            .collect::<Vec<_>>();
        features.sort();
        Ok(Self {
            host: std::env::var("HOST").context("Cargo HOST is required")?,
            target: std::env::var("TARGET").context("Cargo TARGET is required")?,
            features,
            configuration: BTreeMap::new(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileDigest {
    pub bytes: u64,
    pub sha256: String,
}

/// Observed inputs immediately around export. Rust has already compiled a build
/// script at this point; this does not claim a compiler-internal source snapshot.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputSnapshot {
    pub scopes: Vec<InputScope>,
    pub files: BTreeMap<String, FileDigest>,
    pub absent: BTreeSet<String>,
}
impl InputSnapshot {
    /// Capture explicitly declared source/config scopes beneath an owning root.
    /// # Errors
    /// Rejects unsafe/symlink paths, missing required inputs and resource limits.
    pub fn capture(root: &Path, scopes: &[InputScope]) -> Result<Self> {
        let root = root.canonicalize()?;
        let mut snapshot = Self {
            scopes: scopes.to_vec(),
            files: BTreeMap::new(),
            absent: BTreeSet::new(),
        };
        snapshot.scopes.sort_by(|a, b| a.path.cmp(&b.path));
        ensure!(
            snapshot.scopes.len() <= MAX_FILES,
            "Too many declared input scopes"
        );
        ensure!(
            !snapshot
                .scopes
                .windows(2)
                .any(|pair| pair[0].path == pair[1].path),
            "Duplicate input scope"
        );
        let mut budget = MAX_BYTES;
        let mut nodes = 100_000;
        for scope in &snapshot.scopes {
            let path = checked_path(&root, &scope.path)?;
            match fs::symlink_metadata(&path) {
                Ok(_) => capture_path(
                    &root,
                    &path,
                    &mut snapshot.files,
                    &mut budget,
                    &mut nodes,
                    0,
                )?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && scope.optional => {
                    snapshot.absent.insert(scope.path.clone());
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("Missing declared input {}", scope.path));
                }
            }
        }
        Ok(snapshot)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompilerIdentity {
    pub package: String,
    pub version: String,
    pub protocol: String,
}
impl CompilerIdentity {
    fn current() -> Self {
        Self {
            package: "forklaunch-core".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: "forklaunch.sdk.http.v1".into(),
        }
    }
}

/// A graph compiler receipt, with intentionally separate unverified policies.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphReceipt {
    pub canonical_sha256: String,
    pub operations: usize,
    pub sdk_targets: Vec<String>,
    pub bindings: String,
    pub runtime_security: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HttpOutputs {
    pub contract: String,
    pub typescript: String,
    pub rust: String,
    pub additional: Vec<String>,
}

/// Versioned build-owned freshness evidence. Paths are portable root-relative
/// names; no machine paths, source contents or environment secrets are recorded.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format_version: String,
    pub compiler: CompilerIdentity,
    pub output_root: String,
    pub context: BuildContext,
    pub service: String,
    pub inputs: InputSnapshot,
    pub output_paths: HttpOutputs,
    pub outputs: BTreeMap<String, FileDigest>,
    pub graph: GraphReceipt,
}

/// Validate an exported HTTP graph and both emitted SDKs, then record exact
/// outputs and ensure declared inputs stayed unchanged across generation.
/// `root` is the owning application; paths must remain inside it.
/// # Errors
/// Rejects stale inputs, invalid contracts, compiler/output mismatch and unsafe I/O.
pub fn record_http(
    root: &Path,
    inputs: InputSnapshot,
    context: BuildContext,
    service: &str,
    paths: HttpOutputs,
) -> Result<Manifest> {
    record_http_roots(root, root, inputs, context, service, paths)
}

/// Record an explicitly selected external SDK output root without recording
/// machine-specific absolute paths in the manifest.
/// # Errors
/// Rejects unsafe roots, stale sources and compiler/output mismatches.
pub fn record_http_roots(
    root: &Path,
    output_root: &Path,
    inputs: InputSnapshot,
    context: BuildContext,
    service: &str,
    paths: HttpOutputs,
) -> Result<Manifest> {
    validate_context(&context)?;
    ensure!(
        !inputs.files.is_empty(),
        "Build provenance requires declared input files"
    );
    ensure!(
        InputSnapshot::capture(root, &inputs.scopes)? == inputs,
        "Declared build inputs changed during export"
    );
    let root = root.canonicalize()?;
    let output_root = output_root.canonicalize()?;
    let (outputs, graph) = check_outputs(&output_root, service, &paths)?;
    ensure!(
        InputSnapshot::capture(&root, &inputs.scopes)? == inputs,
        "Declared build inputs changed during export"
    );
    Ok(Manifest {
        format_version: FORMAT.into(),
        compiler: CompilerIdentity::current(),
        output_root: if output_root == root {
            "application"
        } else {
            "explicit"
        }
        .into(),
        context,
        service: service.into(),
        inputs,
        output_paths: paths,
        outputs,
        graph,
    })
}

impl Manifest {
    /// Verify exactly this declared scope/configuration using the current
    /// compiler; do not infer unrecorded bindings, targets or runtime policies.
    /// # Errors
    /// Rejects stale/malformed manifests, source/output changes or graph mismatch.
    pub fn verify(&self, root: &Path, expected_context: &BuildContext) -> Result<()> {
        ensure!(
            self.output_root == "application",
            "Provenance requires an explicit external output root"
        );
        self.verify_with_output_root(root, root, expected_context)
    }

    /// Verify with an explicitly authorized output root for external layouts.
    /// # Errors
    /// Rejects mismatched roots, contexts, sources, outputs or compiler receipts.
    pub fn verify_with_output_root(
        &self,
        root: &Path,
        output_root: &Path,
        expected_context: &BuildContext,
    ) -> Result<()> {
        ensure!(
            self.format_version == FORMAT,
            "Unsupported provenance format"
        );
        ensure!(
            self.compiler == CompilerIdentity::current(),
            "Provenance compiler identity changed"
        );
        validate_context(expected_context)?;
        ensure!(
            &self.context == expected_context,
            "Build target/features/configuration changed"
        );
        ensure!(
            InputSnapshot::capture(root, &self.inputs.scopes)? == self.inputs,
            "Declared build inputs are stale"
        );
        let root = root.canonicalize()?;
        let output_root = output_root.canonicalize()?;
        ensure!(
            self.output_root
                == if root == output_root {
                    "application"
                } else {
                    "explicit"
                },
            "Provenance output root layout changed"
        );
        let (outputs, graph) = check_outputs(&output_root, &self.service, &self.output_paths)?;
        ensure!(outputs == self.outputs, "Generated output digest mismatch");
        ensure!(graph == self.graph, "Contract graph receipt mismatch");
        ensure!(
            InputSnapshot::capture(&root, &self.inputs.scopes)? == self.inputs,
            "Declared build inputs changed during verification"
        );
        Ok(())
    }
    /// Require the generated-service input declaration as well as verifying its
    /// recorded scope. This does not establish a complete transitive source graph.
    /// # Errors
    /// Rejects omitted conventional inputs or stale application-local outputs.
    pub fn verify_service_scope(
        &self,
        manifest_dir: &Path,
        expected_context: &BuildContext,
    ) -> Result<()> {
        let expected = ServiceBuild::for_manifest(manifest_dir, expected_context.clone(), &[])?;
        expected.check_scopes(&self.inputs.scopes)?;
        self.verify(
            &crate::build::application_root(manifest_dir)?,
            expected_context,
        )
    }

    /// Atomically write the evidence beside generated artifacts.
    /// # Errors
    /// Rejects unsafe output destinations and serialization/I/O errors.
    pub fn write(&self, path: &Path) -> Result<()> {
        super::write_generated_output(path, serde_json::to_vec_pretty(self)?)
    }
}

fn validate_context(context: &BuildContext) -> Result<()> {
    ensure!(
        !context.host.is_empty() && !context.target.is_empty(),
        "Explicit build host and target are required"
    );
    ensure!(
        context.features.windows(2).all(|pair| pair[0] < pair[1]),
        "Build features must be sorted and unique"
    );
    Ok(())
}

fn check_sdk_files(
    root: &Path,
    paths: &HttpOutputs,
    compiled: &super::HttpSdkSources,
    typescript: &[u8],
    rust: &[u8],
    outputs: &mut BTreeMap<String, FileDigest>,
) -> Result<()> {
    let mut expected = BTreeMap::new();
    if typescript != compiled.typescript.as_bytes() {
        ensure!(
            typescript == compiled.typescript_files["index.ts"].as_bytes(),
            "TypeScript SDK differs from compiled contract"
        );
        let directory = Path::new(&paths.typescript)
            .parent()
            .context("Missing TypeScript SDK directory")?;
        for (name, source) in &compiled.typescript_files {
            if name == "index.ts" {
                continue;
            }
            let parent = if name == "runtime.ts" && compiled.typescript_runtime_shared {
                directory
                    .parent()
                    .context("Missing shared runtime directory")?
            } else {
                directory
            };
            expected.insert(
                parent.join(name).to_string_lossy().into_owned(),
                source.as_str(),
            );
        }
    }
    if rust != compiled.rust.as_bytes() {
        let path = Path::new(&paths.rust);
        ensure!(
            rust == super::rust_http_facade(path, &compiled.rust_files)?.as_bytes(),
            "Rust SDK differs from compiled contract"
        );
        let directory =
            path.with_file_name(path.file_stem().context("Missing Rust SDK module name")?);
        for (name, source) in &compiled.rust_files {
            if name != "lib.rs" {
                expected.insert(
                    directory.join(name).to_string_lossy().into_owned(),
                    source.as_str(),
                );
            }
        }
    }
    for (path, source) in expected {
        let bytes = read(root, &path)?;
        ensure!(
            bytes == source.as_bytes(),
            "SDK implementation differs from compiled contract: {path}"
        );
        ensure!(
            outputs.insert(path, digest(&bytes)).is_none(),
            "Duplicate generated SDK implementation path"
        );
    }
    Ok(())
}

fn check_outputs(
    root: &Path,
    service: &str,
    paths: &HttpOutputs,
) -> Result<(BTreeMap<String, FileDigest>, GraphReceipt)> {
    ensure!(
        paths.additional.len() <= MAX_FILES - 3,
        "Too many generated output paths"
    );
    let source = read(root, &paths.contract)?;
    let source_text = std::str::from_utf8(&source).context("Contract must be UTF-8")?;
    let compiled = super::http_artifact_sources(source_text, service)?;
    let typescript = read(root, &paths.typescript)?;
    let rust = read(root, &paths.rust)?;
    let mut outputs = BTreeMap::new();
    check_sdk_files(root, paths, &compiled, &typescript, &rust, &mut outputs)?;
    for (path, bytes) in [
        (&paths.contract, source.as_slice()),
        (&paths.typescript, typescript.as_slice()),
        (&paths.rust, rust.as_slice()),
    ] {
        ensure!(
            outputs.insert(path.clone(), digest(bytes)).is_none(),
            "Duplicate generated output path"
        );
    }
    for path in &paths.additional {
        ensure!(
            outputs
                .insert(path.clone(), digest(&read(root, path)?))
                .is_none(),
            "Duplicate generated output path"
        );
    }
    ensure!(
        outputs.len() <= MAX_FILES
            && outputs.values().map(|file| file.bytes).sum::<u64>() <= MAX_BYTES,
        "Generated evidence exceeds limits"
    );
    let plan = crate::codegen::SdkGenerationPlan::from_artifact_json(
        crate::codegen::SdkTarget::Rust,
        source_text,
    )?;
    let canonical = canonical_json(serde_json::from_slice(&source)?);
    let graph = GraphReceipt {
        canonical_sha256: digest(&serde_json::to_vec(&canonical)?).sha256,
        operations: plan.operations.len(),
        sdk_targets: vec!["rust".into(), "typescript".into()],
        bindings: "not-checked".into(),
        runtime_security: "not-checked".into(),
    };
    Ok((outputs, graph))
}
fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(canonical_json).collect())
        }
        value => value,
    }
}
fn digest(bytes: &[u8]) -> FileDigest {
    FileDigest {
        bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes)
            .iter()
            .fold(String::new(), |mut hex, byte| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02x}");
                hex
            }),
    }
}
fn read(root: &Path, relative: &str) -> Result<Vec<u8>> {
    let path = checked_path(root, relative)?;
    ensure!(
        fs::symlink_metadata(&path)?.is_file(),
        "Evidence file must be regular"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "Evidence file exceeds 64 MiB"
    );
    Ok(bytes)
}
fn checked_path(root: &Path, relative: &str) -> Result<PathBuf> {
    ensure!(
        relative.len() <= 4096 && !relative.contains('\\') && relative.split('/').count() <= 64,
        "Evidence path exceeds portable limits"
    );
    let relative = Path::new(relative);
    ensure!(!relative.as_os_str().is_empty(), "Evidence path is empty");
    let mut path = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("Evidence paths must be root-relative without traversal");
        };
        path.push(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) => ensure!(
                !metadata.file_type().is_symlink(),
                "Evidence paths must not contain symlinks"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}
fn capture_path(
    root: &Path,
    path: &Path,
    files: &mut BTreeMap<String, FileDigest>,
    budget: &mut u64,
    nodes: &mut usize,
    depth: usize,
) -> Result<()> {
    ensure!(
        depth <= 64 && *nodes > 0,
        "Declared input tree exceeds limits"
    );
    *nodes -= 1;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Declared inputs must not contain symlinks"
    );
    if metadata.is_dir() {
        for child in fs::read_dir(path)? {
            capture_path(root, &child?.path(), files, budget, nodes, depth + 1)?;
        }
        return Ok(());
    }
    ensure!(
        metadata.is_file(),
        "Declared input must be a regular file or directory"
    );
    let relative = path
        .strip_prefix(root)?
        .to_str()
        .context("Evidence paths must be UTF-8")?
        .replace('\\', "/");
    if files.contains_key(&relative) {
        return Ok(());
    }
    ensure!(files.len() < MAX_FILES, "Declared inputs exceed file limit");
    ensure!(metadata.len() <= *budget, "Declared inputs exceed 64 MiB");
    let bytes = read(root, &relative)?;
    *budget = budget
        .checked_sub(bytes.len() as u64)
        .context("Declared inputs exceed 64 MiB")?;
    files.insert(relative, digest(&bytes));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        root: PathBuf,
        context: BuildContext,
        inputs: InputSnapshot,
        paths: HttpOutputs,
    }
    impl Fixture {
        fn new() -> Self {
            static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let root = std::env::temp_dir().join(format!(
                "provenance-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("source")).unwrap();
            fs::create_dir_all(root.join("sdk")).unwrap();
            fs::write(
                root.join("source/contract.rs"),
                "struct Item { name: String }",
            )
            .unwrap();
            fs::write(root.join("config.json"), "{\"target\":\"node\"}").unwrap();
            let context = BuildContext {
                host: "host-fixture".into(),
                target: "target-fixture".into(),
                features: vec!["json".into()],
                configuration: BTreeMap::new(),
            };
            let inputs = InputSnapshot::capture(
                &root,
                &[
                    InputScope::required("source"),
                    InputScope::required("config.json"),
                    InputScope::optional("Cargo.lock"),
                ],
            )
            .unwrap();
            let mut registry = crate::contract::ContractRegistry::default();
            registry.register(
                crate::contract::RouteContract::new(
                    "get",
                    "Get",
                    crate::contract::HttpMethod::Get,
                    "/item",
                )
                .with_response(200, Some(crate::contract::BodyContract::json("String"))),
            );
            let mut artifact: serde_json::Value =
                serde_json::from_str(&registry.into_artifact().to_json()).unwrap();
            artifact["rustClientName"] = "ItemsClient".into();
            artifact["rustOutputLayout"] = "service-module".into();
            let source = artifact.to_string();
            let compiled = super::super::http_artifact_sources(&source, "items").unwrap();
            fs::write(root.join("sdk/contract.json"), source).unwrap();
            fs::write(root.join("sdk/index.ts"), compiled.typescript).unwrap();
            fs::write(root.join("sdk/client.rs"), compiled.rust).unwrap();
            let paths = HttpOutputs {
                contract: "sdk/contract.json".into(),
                typescript: "sdk/index.ts".into(),
                rust: "sdk/client.rs".into(),
                additional: Vec::new(),
            };
            Self {
                root,
                context,
                inputs,
                paths,
            }
        }
        fn record(&self) -> Manifest {
            record_http(
                &self.root,
                self.inputs.clone(),
                self.context.clone(),
                "items",
                self.paths.clone(),
            )
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn records_real_compiler_outputs_and_keeps_unverified_policies_explicit() {
        let fixture = Fixture::new();
        let manifest = fixture.record();
        manifest.verify(&fixture.root, &fixture.context).unwrap();
        assert_eq!(manifest.graph.operations, 1);
        assert_eq!(manifest.graph.bindings, "not-checked");
        assert_eq!(manifest.graph.runtime_security, "not-checked");
        let encoded = serde_json::to_string(&manifest).unwrap();
        assert!(!encoded.contains(fixture.root.to_str().unwrap()));
        let decoded: Manifest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(manifest, decoded);
        manifest
            .write(&fixture.root.join("sdk/provenance.json"))
            .unwrap();
    }
    #[test]
    fn split_sources_are_all_compiler_checked_and_fingerprinted() {
        let fixture = Fixture::new();
        let contract = fs::read_to_string(fixture.root.join("sdk/contract.json")).unwrap();
        let compiled = super::super::http_artifact_sources(&contract, "items").unwrap();
        for (name, source) in &compiled.typescript_files {
            let parent = if name == "runtime.ts" {
                fixture.root.clone()
            } else {
                fixture.root.join("sdk")
            };
            fs::write(parent.join(name), source).unwrap();
        }
        super::super::write_rust_http_files(
            &fixture.root.join("sdk/client.rs"),
            &compiled.rust_files,
        )
        .unwrap();
        let manifest = fixture.record();
        for path in [
            "runtime.ts",
            "sdk/models.ts",
            "sdk/client.ts",
            "sdk/client/models.rs",
            "sdk/client/client.rs",
            "sdk/client/schemas.rs",
        ] {
            assert!(manifest.outputs.contains_key(path));
            let target = fixture.root.join(path);
            let saved = fs::read(&target).unwrap();
            fs::write(&target, "altered implementation").unwrap();
            assert!(manifest.verify(&fixture.root, &fixture.context).is_err());
            assert!(check_outputs(&fixture.root, "items", &fixture.paths).is_err());
            fs::write(target, saved).unwrap();
        }
        manifest.verify(&fixture.root, &fixture.context).unwrap();
    }
    #[test]
    fn detects_source_membership_configuration_and_optional_input_changes() {
        let fixture = Fixture::new();
        let manifest = fixture.record();
        fs::write(fixture.root.join("source/added.rs"), "struct Added;").unwrap();
        assert!(manifest.verify(&fixture.root, &fixture.context).is_err());
        fs::remove_file(fixture.root.join("source/added.rs")).unwrap();
        fs::write(fixture.root.join("config.json"), "{}").unwrap();
        assert!(manifest.verify(&fixture.root, &fixture.context).is_err());
        fs::write(fixture.root.join("config.json"), "{\"target\":\"node\"}").unwrap();
        fs::write(fixture.root.join("Cargo.lock"), "new lockfile").unwrap();
        assert!(manifest.verify(&fixture.root, &fixture.context).is_err());
    }
    #[test]
    fn rejects_stale_forged_sdk_graph_and_build_context() {
        let fixture = Fixture::new();
        let mut manifest = fixture.record();
        let mut context = fixture.context.clone();
        context.target = "other".into();
        assert!(manifest.verify(&fixture.root, &context).is_err());
        manifest.graph.bindings = "verified".into();
        assert!(manifest.verify(&fixture.root, &fixture.context).is_err());
        manifest.graph.bindings = "not-checked".into();
        fs::write(fixture.root.join("sdk/index.ts"), "export {};").unwrap();
        manifest
            .outputs
            .insert("sdk/index.ts".into(), digest(b"export {};"));
        assert!(
            manifest
                .verify(&fixture.root, &fixture.context)
                .unwrap_err()
                .to_string()
                .contains("differs from compiled contract")
        );
        assert!(
            record_http(
                &fixture.root,
                fixture.inputs.clone(),
                fixture.context.clone(),
                "items",
                fixture.paths.clone()
            )
            .is_err()
        );
    }
    #[test]
    fn rejects_source_changes_between_snapshot_and_record() {
        let fixture = Fixture::new();
        fs::write(fixture.root.join("source/contract.rs"), "changed").unwrap();
        assert!(
            record_http(
                &fixture.root,
                fixture.inputs.clone(),
                fixture.context.clone(),
                "items",
                fixture.paths.clone()
            )
            .unwrap_err()
            .to_string()
            .contains("changed during export")
        );
    }
    #[test]
    fn rejects_traversal_symlinks_and_unbounded_inputs() {
        let fixture = Fixture::new();
        assert!(
            InputSnapshot::capture(&fixture.root, &[InputScope::optional("../elsewhere")]).is_err()
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                fixture.root.join("source/contract.rs"),
                fixture.root.join("source/link.rs"),
            )
            .unwrap();
            assert!(
                InputSnapshot::capture(&fixture.root, &[InputScope::required("source")]).is_err()
            );
            fs::remove_file(fixture.root.join("source/link.rs")).unwrap();
        }
        fs::File::create(fixture.root.join("source/large.rs"))
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(InputSnapshot::capture(&fixture.root, &[InputScope::required("source")]).is_err());
    }
    #[test]
    fn canonical_graph_hash_ignores_object_order_only() {
        let left: serde_json::Value =
            serde_json::from_str(r#"{"b":{"d":1,"c":2},"a":[1,2]}"#).unwrap();
        let right: serde_json::Value =
            serde_json::from_str(r#"{"a":[1,2],"b":{"c":2,"d":1}}"#).unwrap();
        assert_eq!(
            serde_json::to_vec(&canonical_json(left)).unwrap(),
            serde_json::to_vec(&canonical_json(right)).unwrap()
        );
    }
    #[test]
    fn generated_service_adapter_checks_scope_and_custom_client_outputs() {
        let fixture = Fixture::new();
        let directory = fixture.root.join("src/items");
        fs::create_dir_all(&directory).unwrap();
        fs::create_dir_all(fixture.root.join(".forklaunch")).unwrap();
        fs::write(
            fixture.root.join(".forklaunch/manifest.toml"),
            "application='fixture'",
        )
        .unwrap();
        fs::write(
            directory.join("Cargo.toml"),
            "[package]\nname='items'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(directory.join("build.rs"), "fn main() {}\n").unwrap();
        fs::write(fixture.root.join("sdk/openapi.json"), "{}").unwrap();
        // TS shared-core tooling uses local symlinks. Only native child packages
        // are part of a Rust service's shared-source scope.
        fs::create_dir_all(fixture.root.join("src/core/security")).unwrap();
        fs::write(fixture.root.join("src/core/package.json"), "{}").unwrap();
        fs::write(
            fixture.root.join("src/core/security/Cargo.toml"),
            "[package]\nname='security'\nversion='0.1.0'",
        )
        .unwrap();
        fs::write(
            fixture.root.join("src/core/security/lib.rs"),
            "pub struct Policy;",
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("../../sdk", fixture.root.join("src/core/node_modules"))
            .unwrap();
        let build =
            ServiceBuild::for_manifest(&directory, fixture.context.clone(), &fixture.inputs.scopes)
                .unwrap();
        let paths = crate::build::ServiceSdkPaths {
            typescript: fixture.root.join("sdk"),
            rust: fixture.root.join("sdk/client.rs"),
        };
        let manifest = build.record("items", &paths).unwrap();
        assert!(
            manifest
                .inputs
                .files
                .contains_key("src/core/security/lib.rs")
        );
        assert!(
            !manifest
                .inputs
                .files
                .keys()
                .any(|name| name.contains("node_modules"))
        );

        manifest
            .verify_service_scope(&directory, &fixture.context)
            .unwrap();
        let mut forged = manifest.clone();
        let scopes = forged
            .inputs
            .scopes
            .iter()
            .filter(|scope| scope.path != "src/items/api")
            .cloned()
            .collect::<Vec<_>>();
        forged.inputs = InputSnapshot::capture(&fixture.root, &scopes).unwrap();
        assert!(
            forged
                .verify_service_scope(&directory, &fixture.context)
                .unwrap_err()
                .to_string()
                .contains("omits conventional input")
        );
        assert!(paths.typescript.join("provenance.json").is_file());
    }
    fn emit_channels(output: &Path) {
        let event = crate::build::ChannelEvent {
            name: "Changed",
            purpose: "changed-v1",
            schema: serde_json::json!({"type":"object", "required":["tenant","resource","operation","revision"],
                "properties":{"tenant":{"type":"string"},"resource":{"type":"string"},
                    "operation":{"type":"string"},"revision":{"type":"integer","minimum":1,"maximum":9_007_199_254_740_991_u64}}}),
            context: crate::build::ChannelContextFields {
                tenant: "tenant",
                resource: "resource",
                operation: "operation",
                revision: "revision",
            },
            wire: crate::build::ChannelWire::SealedEvent,
        };
        crate::build::channel_events("items", &[event], "// shared event DTOs", &[], output)
            .unwrap();
    }
    #[test]
    fn service_extensions_track_emitted_socket_channel_files_and_inputs() {
        let fixture = Fixture::new();
        let directory = fixture.root.join("src/items");
        fs::create_dir_all(&directory).unwrap();
        fs::create_dir_all(fixture.root.join(".forklaunch")).unwrap();
        fs::write(
            fixture.root.join(".forklaunch/manifest.toml"),
            "application='fixture'",
        )
        .unwrap();
        fs::write(
            directory.join("Cargo.toml"),
            "[package]\nname='items'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(directory.join("build.rs"), "fn main() {}\n").unwrap();
        fs::write(fixture.root.join("sdk/openapi.json"), "{}").unwrap();
        fs::write(fixture.root.join("events.rs"), "// shared event DTOs").unwrap();
        let typescript_socket = fixture.root.join("sdk/presence.ts");
        let socket_contract = fixture.root.join("sdk/socket-contract.json");
        let rust_socket = fixture.root.join("sdk/presence.rs");
        let channels = fixture.root.join("events");
        let build =
            ServiceBuild::for_manifest(&directory, fixture.context.clone(), &fixture.inputs.scopes)
                .unwrap()
                .include_assets(&[
                    GeneratedAssets::socket(&typescript_socket, &socket_contract, &rust_socket)
                        .unwrap(),
                    GeneratedAssets::channels(&channels)
                        .with_inputs([InputScope::required("events.rs")]),
                ])
                .unwrap();
        let socket = crate::ws::SocketContract::new("/presence");
        crate::build::typescript_websocket(
            &socket,
            "PresenceClient",
            &typescript_socket,
            &socket_contract,
        )
        .unwrap();
        crate::build::rust_websocket_module(&socket, "presence", "PresenceClient", &rust_socket)
            .unwrap();
        emit_channels(&channels);
        let asset_build =
            ServiceBuild::for_manifest(&directory, fixture.context.clone(), &fixture.inputs.scopes)
                .unwrap()
                .include_assets(&[GeneratedAssets::channels(&channels)])
                .unwrap();
        let asset_receipt = asset_build
            .record_assets("items", &channels.join("provenance.json"))
            .unwrap();
        asset_receipt
            .verify_service_scope(&directory, &fixture.context)
            .unwrap();
        fs::write(channels.join("rust/events.rs"), "changed").unwrap();
        assert!(
            asset_receipt
                .verify_service_scope(&directory, &fixture.context)
                .is_err()
        );
        emit_channels(&channels);
        let paths = crate::build::ServiceSdkPaths {
            typescript: fixture.root.join("sdk"),
            rust: fixture.root.join("sdk/client.rs"),
        };
        let manifest = build.record("items", &paths).unwrap();
        manifest
            .verify_service_scope(&directory, &fixture.context)
            .unwrap();
        assert_eq!(manifest.output_paths.additional.len(), 13);
        // Every declared extension output is checked by the existing analyzer seam.
        for name in manifest.output_paths.additional.iter().skip(1) {
            let path = fixture.root.join(name);
            let original = fs::read(&path).unwrap();
            fs::write(&path, "tampered").unwrap();
            assert!(
                manifest
                    .verify_service_scope(&directory, &fixture.context)
                    .is_err(),
                "{name}"
            );
            fs::remove_file(&path).unwrap();
            assert!(
                manifest
                    .verify_service_scope(&directory, &fixture.context)
                    .is_err(),
                "{name}"
            );
            fs::write(&path, original).unwrap();
        }
        fs::write(fixture.root.join("events.rs"), "// changed DTO").unwrap();
        assert!(
            manifest
                .verify_service_scope(&directory, &fixture.context)
                .is_err()
        );
    }
    #[test]
    fn external_sdk_overrides_require_explicit_output_root_at_verification() {
        let fixture = Fixture::new();
        let external = Fixture::new();
        let manifest = record_http_roots(
            &fixture.root,
            &external.root,
            fixture.inputs.clone(),
            fixture.context.clone(),
            "items",
            fixture.paths.clone(),
        )
        .unwrap();
        assert_eq!(manifest.output_root, "explicit");
        assert!(manifest.verify(&fixture.root, &fixture.context).is_err());
        manifest
            .verify_with_output_root(&fixture.root, &external.root, &fixture.context)
            .unwrap();
    }
}

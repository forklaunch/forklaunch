//! Generated Rust build-hook adapter for explicitly declared conventional inputs.
use super::{BuildContext, HttpOutputs, InputScope, InputSnapshot, Manifest, record_http_roots};
use anyhow::{Context as _, Result};
use std::path::{Path, PathBuf};

/// One generated service export. Conventional contract/source directories and
/// workspace configuration are declared here; callers add nonstandard contract
/// dependency scopes explicitly. This is not a complete Cargo dependency graph.
pub struct ServiceBuild {
    extra_outputs: Vec<PathBuf>,
    declarations: Option<Vec<crate::contract::assembly::BindingDeclaration>>,
    root: PathBuf,
    manifest_directory: PathBuf,
    inputs: InputSnapshot,
    context: BuildContext,
}
impl ServiceBuild {
    /// Capture generated-service inputs before SDK generation and register Cargo
    /// watches for membership changes, including currently absent optional inputs.
    /// Additional scopes are relative to the application root.
    /// # Errors
    /// Rejects invalid application layout, target context and declared input scopes.
    pub fn capture(additional: &[InputScope]) -> Result<Self> {
        let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
        let result = Self::for_manifest(&manifest, BuildContext::cargo()?, additional)?;
        // Cargo treats a watched path that does not exist as always changed, so
        // absent optional inputs would rerun every build script on every build.
        for scope in &result.inputs.scopes {
            let path = result.root.join(&scope.path);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
        Ok(result)
    }

    /// Explicit-context form for tools that already know the chosen build matrix.
    /// # Errors
    /// Rejects paths outside the owning application and missing required sources.
    pub fn for_manifest(
        manifest: &Path,
        context: BuildContext,
        additional: &[InputScope],
    ) -> Result<Self> {
        let root = crate::build::application_root(manifest)?.canonicalize()?;
        let manifest = manifest.canonicalize()?;
        let service = manifest.strip_prefix(&root)?;
        let mut scopes = vec![InputScope::required(".forklaunch/manifest.toml")];
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain",
            "rust-toolchain.toml",
            ".cargo/config.toml",
        ] {
            scopes.push(InputScope::optional(path));
        }
        scopes.extend(shared_scopes(&root)?);
        for path in ["Cargo.toml", "build.rs"] {
            scopes.push(InputScope::required(relative(&service.join(path))?));
        }
        for path in [
            "api",
            "domain",
            "persistence",
            "registrations",
            "registrations.rs",
            "build_assets.rs",
            "sdk.rs",
            "server.rs",
            "application.rs",
            "bootstrapper.rs",
            "index.rs",
            "lib.rs",
            "main.rs",
            "worker.rs",
            "consumer",
            "signals.rs",
        ] {
            scopes.push(InputScope::optional(relative(&service.join(path))?));
        }
        if let Some(contracts) = contract_directory(&manifest)? {
            scopes.push(InputScope::required(relative(
                contracts.strip_prefix(&root)?,
            )?));
        }
        scopes.extend_from_slice(additional);
        // A service at the application root can overlap workspace configuration.
        scopes.sort_by(|a, b| a.path.cmp(&b.path));
        let mut unique: Vec<InputScope> = Vec::new();
        for scope in scopes {
            if let Some(previous) = unique
                .last_mut()
                .filter(|previous| previous.path == scope.path)
            {
                previous.optional &= scope.optional;
            } else {
                unique.push(scope);
            }
        }
        let inputs = InputSnapshot::capture(&root, &unique)?;
        super::validate_context(&context)?;
        Ok(Self {
            extra_outputs: Vec::new(),
            declarations: None,
            root,
            manifest_directory: manifest,
            inputs,
            context,
        })
    }

    /// Include explicitly owned socket/channel assets before running build extensions.
    /// Their files use the existing additional-output digest verification; this does
    /// not claim socket/channel compiler equivalence or runtime security execution.
    /// # Errors
    /// Rejects changed existing inputs, invalid additional scopes or excess outputs.
    pub fn include_assets(mut self, assets: &[super::GeneratedAssets]) -> Result<Self> {
        anyhow::ensure!(
            InputSnapshot::capture(&self.root, &self.inputs.scopes)? == self.inputs,
            "Declared build inputs changed before extension capture"
        );
        let mut scopes = self.inputs.scopes.clone();
        for asset in assets {
            scopes.extend_from_slice(&asset.inputs);
            self.extra_outputs.extend_from_slice(&asset.outputs);
        }
        anyhow::ensure!(
            self.extra_outputs.len() <= super::MAX_FILES - 4,
            "Too many extension outputs"
        );
        scopes.sort_by(|a, b| a.path.cmp(&b.path));
        let mut unique: Vec<InputScope> = Vec::new();
        for scope in scopes {
            if let Some(previous) = unique
                .last_mut()
                .filter(|previous| previous.path == scope.path)
            {
                previous.optional &= scope.optional;
            } else {
                unique.push(scope);
            }
        }
        self.inputs = InputSnapshot::capture(&self.root, &unique)?;
        for scope in &self.inputs.scopes {
            let path = self.root.join(&scope.path);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
        Ok(self)
    }

    /// Declare compiler-generated static binding groups for source assembly analysis.
    /// This does not claim that runtime handlers have executed.
    #[must_use]
    pub fn with_bindings(
        mut self,
        declarations: Vec<crate::contract::assembly::BindingDeclaration>,
    ) -> Self {
        self.declarations = Some(declarations);
        self
    }

    /// Add public compile configuration, not credentials or runtime key material.
    #[must_use]
    pub fn context_mut(&mut self) -> &mut BuildContext {
        &mut self.context
    }

    /// Check and record the HTTP contract, both SDK modules and `OpenAPI` after all
    /// service build extensions finish. Explicitly declared extension files receive
    /// digest freshness checks; their semantic/runtime policies remain separate.
    /// # Errors
    /// Rejects changed inputs, mismatched SDKs and unavailable output roots.
    pub fn record(
        mut self,
        service: &str,
        paths: &crate::build::ServiceSdkPaths,
    ) -> Result<Manifest> {
        if let Some(mut declarations) = self.declarations.take() {
            for declaration in &mut declarations {
                let source = Path::new(&declaration.source);
                let contract_directory = contract_directory(&self.manifest_directory)?;
                let candidates = [
                    self.manifest_directory.join(source),
                    self.manifest_directory.join("domain").join(source),
                    self.root.join(source),
                    contract_directory
                        .as_ref()
                        .unwrap_or(&self.manifest_directory)
                        .join(source),
                ];
                let source = candidates
                    .iter()
                    .find_map(|path| path.canonicalize().ok())
                    .context("Binding source file is unavailable")?;
                anyhow::ensure!(
                    source.starts_with(&self.manifest_directory)
                        || contract_directory
                            .as_ref()
                            .is_some_and(|dir| source.starts_with(dir)),
                    "Binding source leaves owning service and declared contract crate"
                );
                declaration.source = if let Ok(path) = source.strip_prefix(&self.manifest_directory)
                {
                    relative(path)?
                } else {
                    format!(
                        "../{}",
                        relative(
                            source.strip_prefix(
                                self.manifest_directory
                                    .parent()
                                    .context("Service parent missing")?
                            )?
                        )?
                    )
                };
            }
            let manifest = super::BindingManifest {
                format_version: "forklaunch.bindings.v1".into(),
                declarations,
            };
            let artifact =
                serde_json::from_slice(&std::fs::read(paths.typescript.join("contract.json"))?)?;
            manifest.verify_contract(&artifact)?;
            let path = paths.typescript.join("bindings.json");
            crate::build::write_generated_output(&path, &serde_json::to_string_pretty(&manifest)?)?;
            self.extra_outputs.push(path);
        }
        let files = [
            paths.typescript.join("contract.json"),
            paths.typescript.join("index.ts"),
            paths.rust.clone(),
            paths.typescript.join("openapi.json"),
        ]
        .into_iter()
        .chain(self.extra_outputs)
        .map(|path| path.canonicalize())
        .collect::<std::io::Result<Vec<_>>>()?;
        let output_root = if files.iter().all(|path| path.starts_with(&self.root)) {
            self.root.clone()
        } else {
            files[0]
                .ancestors()
                .find(|parent| files.iter().all(|path| path.starts_with(parent)))
                .context("SDK output paths have no common root")?
                .to_path_buf()
        };
        let names = files
            .iter()
            .map(|path| relative(path.strip_prefix(&output_root)?))
            .collect::<Result<Vec<_>>>()?;
        let outputs = HttpOutputs {
            contract: names[0].clone(),
            typescript: names[1].clone(),
            rust: names[2].clone(),
            additional: names[3..].to_vec(),
        };
        let manifest = record_http_roots(
            &self.root,
            &output_root,
            self.inputs,
            self.context,
            service,
            outputs,
        )?;
        let destination = paths.typescript.join("provenance.json");
        manifest.write(&destination)?;
        println!("cargo:rerun-if-changed={}", destination.display());
        Ok(manifest)
    }
    /// Record explicitly included worker/channel assets without inventing an HTTP graph.
    /// Paths must remain inside the application; semantic validation remains the exporter’s responsibility.
    /// # Errors
    /// Rejects changed inputs, missing/unsafe outputs or invalid receipt paths.
    pub fn record_assets(self, service: &str, destination: &Path) -> Result<super::AssetManifest> {
        super::asset_manifest::record(
            &self.root,
            self.inputs,
            self.context,
            service,
            &self.extra_outputs,
            destination,
        )
    }

    pub(super) fn check_scopes(&self, declared: &[InputScope]) -> Result<()> {
        for expected in &self.inputs.scopes {
            anyhow::ensure!(
                declared
                    .iter()
                    .any(|scope| scope.path == expected.path
                        && (expected.optional || !scope.optional)),
                "Provenance omits conventional input scope {}",
                expected.path
            );
        }
        Ok(())
    }
}
// Shared Rust libraries can sit beneath a TypeScript core project. Track their
// Cargo boundaries, not the TS package's formatter links or dependency install.
fn shared_scopes(root: &Path) -> Result<Vec<InputScope>> {
    let mut scopes = Vec::new();
    for path in ["src/core", "src/domain"] {
        let directory = root.join(path);
        if !directory.join("package.json").is_file() {
            scopes.push(InputScope::optional(path));
            continue;
        }
        scopes.push(InputScope::optional(format!("{path}/Cargo.toml")));
        anyhow::ensure!(
            !directory.join("Cargo.toml").is_file(),
            "Shared Rust code must use a distinct Cargo package beneath the TypeScript core project"
        );
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.path().join("Cargo.toml").is_file() {
                scopes.push(InputScope::required(relative(
                    entry.path().strip_prefix(root)?,
                )?));
            }
        }
    }
    Ok(scopes)
}

fn relative(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .context("Build input paths must be UTF-8")?
        .replace('\\', "/"))
}

/// Resolve the ordinary service's host contract crate within its application.
/// The private service-local host and legacy nested/sibling packages are supported.
/// # Errors
/// Rejects mismatched build/runtime declarations and paths outside the owning project tree.
pub fn contract_directory(manifest: &Path) -> Result<Option<PathBuf>> {
    let manifest = manifest.canonicalize()?;
    let cargo: toml::Value =
        toml::from_str(&std::fs::read_to_string(manifest.join("Cargo.toml"))?)?;
    let Some(dependency) = cargo
        .get("build-dependencies")
        .and_then(|value| value.get("contracts"))
    else {
        return Ok(None);
    };
    let Some(path) = dependency.get("path").and_then(toml::Value::as_str) else {
        return Ok(None);
    };
    anyhow::ensure!(
        cargo
            .get("dependencies")
            .and_then(|value| value.get("contracts"))
            .and_then(|value| value.get("path"))
            .and_then(toml::Value::as_str)
            == Some(path),
        "Runtime and build contract packages differ"
    );
    let directory = manifest.join(path).canonicalize()?;
    let root = crate::build::application_root(&manifest)?.canonicalize()?;
    anyhow::ensure!(
        directory.starts_with(&root)
            && (directory.starts_with(&manifest) || directory.parent() == manifest.parent()),
        "Contract package must be owned by the service or a sibling within its application"
    );
    anyhow::ensure!(
        directory.join("Cargo.toml").is_file(),
        "Contract package manifest missing"
    );
    Ok(Some(directory))
}

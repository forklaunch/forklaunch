//! Freshness-only receipt for worker-owned assets without an HTTP contract.
use super::{BuildContext, CompilerIdentity, InputScope, InputSnapshot, ServiceBuild};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Declared-source and output digest evidence; no semantic/security attestation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssetManifest {
    pub format_version: String,
    pub compiler: CompilerIdentity,
    pub context: BuildContext,
    pub service: String,
    pub inputs: InputSnapshot,
    pub outputs: InputSnapshot,
}
impl AssetManifest {
    /// Verify conventional worker scopes plus all explicitly declared asset files.
    /// The caller supplies the context it intends to verify, not a feature matrix.
    /// # Errors
    /// Rejects altered scopes, input/output bytes, compiler or context.
    pub fn verify_service_scope(&self, manifest: &Path, context: &BuildContext) -> Result<()> {
        ensure!(
            self.format_version == "forklaunch.build.assets.v1",
            "Unsupported asset provenance format"
        );
        ensure!(
            self.compiler == CompilerIdentity::current(),
            "Asset compiler identity changed"
        );
        ensure!(&self.context == context, "Asset build context changed");
        let build = ServiceBuild::for_manifest(manifest, context.clone(), &[])?;
        build.check_scopes(&self.inputs.scopes)?;
        let root = crate::build::application_root(manifest)?;
        ensure!(
            InputSnapshot::capture(&root, &self.inputs.scopes)? == self.inputs,
            "Asset inputs changed"
        );
        ensure!(
            !self.outputs.scopes.is_empty()
                && self.outputs.scopes.iter().all(|scope| !scope.optional),
            "Asset outputs must be required"
        );
        ensure!(
            InputSnapshot::capture(&root, &self.outputs.scopes)? == self.outputs,
            "Asset outputs changed"
        );
        Ok(())
    }
}

pub(super) fn record(
    root: &Path,
    inputs: InputSnapshot,
    context: BuildContext,
    service: &str,
    outputs: &[std::path::PathBuf],
    destination: &Path,
) -> Result<AssetManifest> {
    super::validate_context(&context)?;
    ensure!(
        !service.is_empty() && !outputs.is_empty(),
        "Asset owner and outputs are required"
    );
    ensure!(
        InputSnapshot::capture(root, &inputs.scopes)? == inputs,
        "Asset inputs changed during export"
    );
    let scopes = outputs
        .iter()
        .map(|path| {
            let absolute = if path.is_absolute() {
                path.clone()
            } else {
                std::env::current_dir()?.join(path)
            };
            // Permit the platform's /var -> /private/var root alias while retaining
            // descendant spellings so checked_path still rejects output symlinks.
            let owner = absolute
                .ancestors()
                .find(|ancestor| ancestor.canonicalize().is_ok_and(|path| path == root))
                .ok_or_else(|| anyhow::anyhow!("Asset output is outside application"))?;
            let relative = absolute.strip_prefix(owner)?;
            Ok(InputScope::required(
                relative
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("Non-UTF8 asset path"))?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let outputs = InputSnapshot::capture(root, &scopes)?;
    let result = AssetManifest {
        format_version: "forklaunch.build.assets.v1".into(),
        compiler: CompilerIdentity::current(),
        context,
        service: service.into(),
        inputs,
        outputs,
    };
    ensure!(
        InputSnapshot::capture(root, &result.inputs.scopes)? == result.inputs,
        "Asset inputs changed during export"
    );
    crate::build::write_generated_output(destination, serde_json::to_vec_pretty(&result)?)?;
    println!("cargo:rerun-if-changed={}", destination.display());
    Ok(result)
}

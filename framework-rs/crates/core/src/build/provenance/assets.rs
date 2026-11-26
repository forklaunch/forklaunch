//! Explicit ownership of generated extension files; never directory discovery.
use super::InputScope;
use anyhow::{Context as _, Result};
use std::path::{Path, PathBuf};

/// Files emitted by a build extension and extra application-relative source scopes.
/// These declarations establish freshness, not compiler equivalence or runtime
/// authorization/encryption. Declare them before running the extension.
pub struct GeneratedAssets {
    pub(super) outputs: Vec<PathBuf>,
    pub(super) inputs: Vec<InputScope>,
}
impl GeneratedAssets {
    /// Outputs of the shared TypeScript/browser and Rust socket exporters.
    /// # Errors
    /// Rejects a TypeScript SDK output without a file stem.
    pub fn socket(
        typescript: impl AsRef<Path>,
        contract: impl Into<PathBuf>,
        rust: impl Into<PathBuf>,
    ) -> Result<Self> {
        let typescript = typescript.as_ref();
        let stem = typescript
            .file_stem()
            .context("Socket SDK requires a file stem")?
            .to_string_lossy();
        Ok(Self {
            outputs: vec![
                typescript.to_owned(),
                typescript.with_file_name(format!("{stem}.browser.ts")),
                contract.into(),
                rust.into(),
            ],
            inputs: Vec::new(),
        })
    }

    /// All service-owned files emitted by [`crate::build::channel_events`].
    #[must_use]
    pub fn channels(directory: impl AsRef<Path>) -> Self {
        let directory = directory.as_ref();
        Self {
            outputs: [
                "event-contract.json",
                "typescript/events.ts",
                "typescript/encrypted-channel.mjs",
                "typescript/encrypted-channel.d.mts",
                "typescript/package.json",
                "typescript/tsconfig.json",
                "rust/events.rs",
                "rust/README.md",
            ]
            .into_iter()
            .map(|name| directory.join(name))
            .collect(),
            inputs: Vec::new(),
        }
    }

    /// Exact service-owned outputs emitted by [`crate::build::worker_events`].
    #[must_use]
    pub fn worker_events(directory: impl AsRef<Path>) -> Self {
        let directory = directory.as_ref();
        Self {
            outputs: [
                "event-contract.json",
                "typescript/events.ts",
                "typescript/package.json",
                "typescript/tsconfig.json",
                "typescript/encrypted-worker.mjs",
                "typescript/encrypted-worker.d.mts",
                "typescript/encrypted-channel.mjs",
                "typescript/encrypted-channel.d.mts",
                "rust/events.rs",
            ]
            .into_iter()
            .map(|name| directory.join(name))
            .collect(),
            inputs: Vec::new(),
        }
    }

    /// Declare nonstandard contract dependencies relative to the application root.
    /// Existing conventional service `api`, `domain` and `build_assets.rs` inputs
    /// are already covered. Never add generated output directories as inputs.
    #[must_use]
    pub fn with_inputs(mut self, inputs: impl IntoIterator<Item = InputScope>) -> Self {
        self.inputs.extend(inputs);
        self
    }
}

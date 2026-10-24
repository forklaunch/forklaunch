//! Resolve generated outputs without escaping the application through paths or symlinks.
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};

pub(crate) fn output_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        bail!("Invalid application output path");
    }
    let relative: PathBuf = relative
        .components()
        .filter(|component| !matches!(component, Component::CurDir))
        .collect();
    if relative.as_os_str().is_empty() {
        bail!("Application output path must name a file");
    }
    let root = root.canonicalize()?;
    let destination = root.join(relative);
    let mut existing = destination.clone();
    while !existing.exists() {
        if fs::symlink_metadata(&existing).is_ok() {
            bail!("Application output contains a dangling symlink");
        }
        existing = existing
            .parent()
            .context("Application output parent missing")?
            .into();
    }
    if !existing.canonicalize()?.starts_with(&root) {
        bail!("Output must stay inside the application");
    }
    Ok(destination)
}

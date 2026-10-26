use anyhow::Result;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn sdk_base(root: &Path) -> Result<PathBuf> {
    let manifest = root.join(".forklaunch/manifest.toml");
    let native = if manifest.is_file() {
        let manifest: toml::Value = toml::from_str(&fs::read_to_string(manifest)?)?;
        manifest.get("modules_path").and_then(toml::Value::as_str) == Some("src")
    } else {
        false
    };
    Ok(PathBuf::from(if native {
        "src/client-sdk"
    } else {
        "sdks"
    }))
}

pub(super) fn native(root: &Path) -> Result<bool> {
    Ok(sdk_base(root)? == Path::new("src/client-sdk"))
}

pub(super) fn typescript(root: &Path) -> Result<PathBuf> {
    Ok(sdk_base(root)?.join("typescript"))
}

pub(super) fn rust(root: &Path) -> Result<PathBuf> {
    Ok(sdk_base(root)?.join("rust"))
}

pub(super) fn source(root: &Path, directory: &Path) -> Result<PathBuf> {
    Ok(if native(root)? {
        directory.to_path_buf()
    } else {
        directory.join("src")
    })
}

pub(super) fn typescript_authored(root: &Path, service: &str) -> Result<bool> {
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?;
    Ok(manifest.get("projects").and_then(toml::Value::as_array).and_then(|projects| projects.iter().find(|project| project.get("name").and_then(toml::Value::as_str) == Some(service)))
        .and_then(|project| project.get("metadata")).is_some_and(|metadata| metadata.get("sdkLayout").and_then(toml::Value::as_str) == Some("project-local-v1") && metadata.get("language").and_then(toml::Value::as_str) != Some("rust")))
}

/// Resolve explicitly generated service-owned SDK output without redirecting
/// older applications or custom SDK layouts.
pub(super) fn local_sdk(root: &Path, service: &str, language: &str) -> Result<Option<PathBuf>> {
    let manifest = root.join(".forklaunch/manifest.toml");
    if !manifest.is_file() { return Ok(None); }
    let manifest: toml::Value = toml::from_str(&fs::read_to_string(manifest)?)?;
    let project = manifest.get("projects").and_then(toml::Value::as_array).and_then(|projects| projects.iter().find(|project| project.get("name").and_then(toml::Value::as_str) == Some(service)));
    let Some(metadata) = project.and_then(|project| project.get("metadata")) else { return Ok(None); };
    if metadata.get("sdkLayout").and_then(toml::Value::as_str) != Some("project-local-v1") { return Ok(None); }
    let authored = metadata.get("language").and_then(toml::Value::as_str) != Some("rust");
    let path = metadata.get("cratePath").or_else(|| metadata.get("staticPath")).and_then(toml::Value::as_str).ok_or_else(|| anyhow::anyhow!("Service-local SDK requires project path metadata"))?;
    let relative = if authored && language == "typescript" { Path::new(path).join("sdk") } else { Path::new(path).join("sdk").join(language) };
    Ok(Some(crate::core::application_output::output_path(root, &relative)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_generation_receipts_do_not_redirect_sdk_output() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::write(root.path().join(".forklaunch/manifest.toml"), "language = 'rust'\nmodules_path = 'src'\n").unwrap();
        fs::write(root.path().join("generation.json"), "application-owned receipt; not compiler configuration").unwrap();
        assert_eq!(typescript(root.path()).unwrap(), Path::new("src/client-sdk/typescript"));
    }

    #[test]
    fn source_root_layout_is_independent_of_application_language() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        for (language, modules, expected) in [
            ("language = 'typescript'", "src", "src/client-sdk"),
            ("", "src", "src/client-sdk"),
            ("language = 'rust'", "src", "src/client-sdk"),
            ("language = 'mixed'", "src", "src/client-sdk"),
            ("applicationLanguage = 'mixed'", "src", "src/client-sdk"),
            ("language = 'mixed'", "crates", "sdks"),
        ] {
            fs::write(
                root.path().join(".forklaunch/manifest.toml"),
                format!("{language}\nmodules_path = '{modules}'\n"),
            )
            .unwrap();
            assert_eq!(sdk_base(root.path()).unwrap(), Path::new(expected));
        }
    }
}

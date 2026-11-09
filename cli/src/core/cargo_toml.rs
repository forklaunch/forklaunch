//! Dependency versions for generated Cargo.toml files.
//!
//! Templates and generators name third-party crates without a version, e.g.
//! `tokio = { features = ["net"] }` or `serde_json = {}`. [`fill`] takes the
//! version from `cargo_toml_constants.rs`, the single place they are kept.
use anyhow::{Result, bail};

mod cargo_toml_constants;
pub(crate) use cargo_toml_constants::*;

const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

/// The registry version for `name`, the dependency's key in Cargo.toml.
pub(crate) fn version(name: &str) -> Option<&'static str> {
    CRATE_VERSIONS
        .iter()
        .find(|(crate_name, _)| *crate_name == name)
        .map(|(_, version)| *version)
}

/// Give every registry dependency without a version, path, git or workspace
/// source its registry version, keeping the manifest's formatting.
pub(crate) fn fill(source: &str) -> Result<String> {
    // An unparseable manifest is the application's to fix; Cargo reports it.
    let Ok(mut document) = source.parse::<toml_edit::DocumentMut>() else { return Ok(source.into()) };
    let mut changed = false;
    let mut tables: Vec<&mut toml_edit::Item> = Vec::new();
    let root = document.as_table_mut();
    let (workspace, rest): (Vec<_>, Vec<_>) =
        root.iter_mut().partition(|(key, _)| key.get() == "workspace");
    for (key, item) in rest {
        if DEPENDENCY_TABLES.contains(&key.get()) {
            tables.push(item);
        } else if key.get() == "target" {
            for (_, target) in item.as_table_like_mut().into_iter().flat_map(|table| table.iter_mut()) {
                for (key, item) in target.as_table_like_mut().into_iter().flat_map(|table| table.iter_mut()) {
                    if DEPENDENCY_TABLES.contains(&key.get()) {
                        tables.push(item);
                    }
                }
            }
        }
    }
    for (_, item) in workspace {
        if let Some(item) = item.as_table_like_mut().and_then(|table| table.get_mut("dependencies")) {
            tables.push(item);
        }
    }
    for table in tables {
        let Some(table) = table.as_table_like_mut() else { continue };
        for (name, dependency) in table.iter_mut() {
            changed |= fill_dependency(name.get(), dependency)?;
        }
    }
    Ok(if changed { document.to_string() } else { source.into() })
}

fn fill_dependency(name: &str, dependency: &mut toml_edit::Item) -> Result<bool> {
    let Some(spec) = dependency.as_table_like() else { return Ok(false) };
    if ["version", "path", "git", "workspace"].iter().any(|key| spec.contains_key(key)) {
        return Ok(false);
    }
    let Some(version) = version(name) else {
        bail!("Cargo dependency `{name}` has no version and the CLI has no default for it");
    };
    if spec.is_empty() {
        *dependency = toml_edit::value(version);
        return Ok(true);
    }
    let mut inline = toml_edit::InlineTable::new();
    inline.insert("version", version.into());
    for (key, value) in spec.iter() {
        if let Some(value) = value.as_value() {
            inline.insert(key, value.clone());
        }
    }
    *dependency = toml_edit::value(inline);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versionless_dependencies_take_registry_versions() {
        let source = "[dependencies]\nserde_json = {}\ntokio = { features = [\"net\"] }\nlocal = { path = \"../local\" }\nuuid = \"1\"\n\n[target.'cfg(unix)'.dev-dependencies]\naxum = {}\n";
        let filled = fill(source).unwrap();
        let manifest: toml::Value = toml::from_str(&filled).unwrap();
        assert_eq!(manifest["dependencies"]["serde_json"].as_str(), version("serde_json"));
        assert_eq!(manifest["dependencies"]["tokio"]["version"].as_str(), version("tokio"));
        assert_eq!(manifest["dependencies"]["tokio"]["features"][0].as_str(), Some("net"));
        assert!(manifest["dependencies"]["local"].get("version").is_none());
        assert_eq!(manifest["target"]["cfg(unix)"]["dev-dependencies"]["axum"].as_str(), version("axum"));
        assert!(filled.contains("tokio = { version = "), "version leads the inline table: {filled}");
        assert_eq!(fill(&filled).unwrap(), filled);
    }

    #[test]
    fn unknown_versionless_dependency_is_an_error() {
        let error = fill("[dependencies]\nleft-pad = {}\n").unwrap_err();
        assert!(error.to_string().contains("left-pad"));
    }

    #[test]
    fn templates_leave_versions_to_the_registry() {
        let templates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/templates/rust");
        let mut checked = 0;
        for kind in ["application", "billing", "cac", "ecommerce", "iam", "project"] {
            let path = templates.join(kind).join("Cargo.toml.template");
            let source = std::fs::read_to_string(&path).unwrap();
            let mut section = String::new();
            for line in source.lines().map(str::trim) {
                if line.starts_with('[') {
                    section = line.trim_matches(['[', ']']).to_owned();
                    continue;
                }
                let Some((name, spec)) = line.split_once('=') else { continue };
                let (name, spec) = (name.trim(), spec.trim());
                if !DEPENDENCY_TABLES.iter().any(|table| section.ends_with(table)) || spec.contains("path") {
                    continue;
                }
                assert!(!spec.starts_with('"') && !spec.contains("version"), "{name} in {} pins its own version", path.display());
                assert!(version(name).is_some(), "{name} in {} has no registry version", path.display());
                checked += 1;
            }
        }
        assert!(checked > 0, "No template dependencies were checked");
    }
}

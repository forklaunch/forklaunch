//! Static captures use library registration without package or Cargo artifacts.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use toml_edit::{DocumentMut, Table, value};

use super::{
    application_output::output_path,
    manifest::{ProjectEntry, ProjectType},
};

pub(crate) fn is_capture(project: &ProjectEntry) -> bool {
    project.r#type == ProjectType::Library && project.variant.as_deref() == Some("storefront")
}

pub(crate) fn registered(root: &Path, name: &str) -> Result<bool> {
    let manifest: super::manifest::application::ApplicationManifestData =
        toml::from_str(&fs::read_to_string(root.join(".forklaunch/manifest.toml"))?)?;
    Ok(manifest
        .projects
        .iter()
        .any(|project| project.name == name && is_capture(project)))
}

fn load(root: &Path, name: &str) -> Result<(DocumentMut, usize, PathBuf, PathBuf)> {
    let manifest_path = output_path(root, Path::new(".forklaunch/manifest.toml"))?;
    let document: DocumentMut = fs::read_to_string(&manifest_path)?.parse()?;
    let projects = document
        .get("projects")
        .and_then(|item| item.as_array_of_tables())
        .context("Projects missing")?;
    let index = projects
        .iter()
        .position(|project| project.get("name").and_then(|item| item.as_str()) == Some(name))
        .context("Storefront registration missing")?;
    let project = projects.get(index).unwrap();
    if project.get("variant").and_then(|item| item.as_str()) != Some("storefront")
        || project.get("type").and_then(|item| item.as_str()) != Some("Library")
    {
        bail!("Project is not a captured storefront");
    }
    let relative = project
        .get("metadata")
        .and_then(|metadata| {
            metadata
                .get("staticPath")
                .or_else(|| metadata.get("static_path"))
        })
        .and_then(|item| item.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(document["modules_path"].as_str().unwrap_or("modules")).join(name)
        });
    let directory = output_path(root, &relative)?;
    if !directory.is_dir() || fs::symlink_metadata(&directory)?.file_type().is_symlink() {
        bail!(
            "Storefront must be an existing directory, not a symlink: {}",
            directory.display()
        );
    }
    if manifest_path.starts_with(&directory) {
        bail!("Storefront directory contains application metadata");
    }
    for (other_index, other) in projects.iter().enumerate() {
        if other_index == index {
            continue;
        }
        let name = other
            .get("name")
            .and_then(|item| item.as_str())
            .context("Project name missing")?;
        let metadata = other.get("metadata");
        let relative = metadata
            .and_then(|metadata| {
                metadata
                    .get("staticPath")
                    .or_else(|| metadata.get("static_path"))
                    .or_else(|| metadata.get("cratePath"))
                    .or_else(|| metadata.get("crate_path"))
            })
            .and_then(|item| item.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(document["modules_path"].as_str().unwrap_or("modules")).join(name)
            });
        if output_path(root, &relative)?.starts_with(&directory) {
            bail!("Storefront directory contains another registered project");
        }
    }
    Ok((document, index, directory, manifest_path))
}

fn publish(path: &Path, document: &DocumentMut) -> Result<()> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().context("Manifest parent missing")?)?;
    temporary
        .as_file_mut()
        .set_permissions(fs::metadata(path)?.permissions())?;
    temporary.write_all(document.to_string().as_bytes())?;
    temporary.persist(path)?;
    Ok(())
}

fn topology(document: &mut DocumentMut, old: &str, new: Option<&str>) {
    if let Some(table) = document
        .get_mut("project_peer_topology")
        .and_then(|item| item.as_table_mut())
    {
        if let Some(entry) = table.remove(old)
            && let Some(new) = new
        {
            table.insert(new, entry);
        }
        for (_, item) in table.iter_mut() {
            if let Some(names) = item.as_array_mut() {
                for index in (0..names.len()).rev() {
                    if names.get(index).and_then(|name| name.as_str()) == Some(old) {
                        if let Some(new) = new {
                            names.replace(index, new);
                        } else {
                            names.remove(index);
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn change(
    root: &Path,
    name: &str,
    renamed: Option<&str>,
    description: Option<&str>,
    dryrun: bool,
) -> Result<()> {
    let (mut document, index, old, manifest) = load(root, name)?;
    let renamed = renamed.unwrap_or(name);
    if !super::name::validate_name(renamed) {
        bail!("Invalid storefront name");
    }
    if renamed != name
        && document
            .get("project_peer_topology")
            .and_then(|item| item.get(renamed))
            .is_some()
    {
        bail!("Storefront name conflicts with an existing topology entry");
    }
    let projects = document["projects"].as_array_of_tables_mut().unwrap();
    if renamed != name
        && projects
            .iter()
            .any(|project| project["name"].as_str() == Some(renamed))
    {
        bail!("Project name is already registered");
    }
    let next = old.with_file_name(renamed);
    output_path(root, next.strip_prefix(root.canonicalize()?)?)?;
    if renamed != name && next.try_exists()? {
        bail!("Storefront destination already exists");
    }
    let project = projects.get_mut(index).unwrap();
    project["name"] = value(renamed);
    if let Some(description) = description {
        project["description"] = value(description);
    }
    let metadata = project
        .entry("metadata")
        .or_insert(toml_edit::Item::Table(Table::new()));
    metadata
        .as_table_mut()
        .context("Project metadata must be a table")?
        .remove("static_path");
    metadata["staticPath"] = value(
        next.strip_prefix(root.canonicalize()?)?
            .to_string_lossy()
            .as_ref(),
    );
    if renamed != name {
        topology(&mut document, name, Some(renamed));
    }
    if dryrun {
        return Ok(());
    }
    if renamed != name {
        fs::rename(&old, &next)?;
    }
    if let Err(error) = publish(&manifest, &document) {
        if renamed != name {
            fs::rename(&next, &old)
                .context("Manifest write failed and storefront rename rollback failed")?;
        }
        return Err(error);
    }
    Ok(())
}

pub(crate) fn delete(root: &Path, name: &str) -> Result<()> {
    let (mut document, index, directory, manifest) = load(root, name)?;
    document["projects"]
        .as_array_of_tables_mut()
        .unwrap()
        .remove(index);
    topology(&mut document, name, None);
    let holding = tempfile::Builder::new()
        .prefix(".forklaunch-storefront-delete-")
        .tempdir_in(directory.parent().context("Storefront parent missing")?)?;
    let staged = holding.path().join("capture");
    fs::rename(&directory, &staged)?;
    if let Err(error) = publish(&manifest, &document) {
        // Keep the capture recoverable even if rollback itself fails.
        let retained = holding.keep();
        fs::rename(&staged, &directory).with_context(|| {
            format!(
                "Manifest write failed; recover the storefront from {}",
                staged.display()
            )
        })?;
        fs::remove_dir(retained)?;
        return Err(error);
    }
    holding.close()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join(".forklaunch")).unwrap();
        fs::create_dir_all(root.path().join("crates/shop/assets")).unwrap();
        fs::write(
            root.path().join("crates/shop/assets/picture.bin"),
            [0, 255, 7],
        )
        .unwrap();
        fs::write(
            root.path().join(".forklaunch/manifest.toml"),
            r#"# retain application comments
modules_path = "modules"
[project_peer_topology]
app = ["shop", "api"]
[[projects]]
name = "shop"
type = "Library"
variant = "storefront"
description = "Captured site"
[projects.metadata]
staticPath = "crates/shop"
[[projects]]
name = "api"
type = "Service"
"#,
        )
        .unwrap();
        root
    }

    #[test]
    fn static_lifecycle_preserves_assets_and_application_artifacts() {
        let root = fixture();
        let manifest = root.path().join(".forklaunch/manifest.toml");
        let original = fs::read(&manifest).unwrap();
        change(
            root.path(),
            "shop",
            Some("boutique"),
            Some("New description"),
            true,
        )
        .unwrap();
        assert_eq!(fs::read(&manifest).unwrap(), original);
        assert!(change(root.path(), "shop", Some("api"), None, false).is_err());
        fs::create_dir_all(root.path().join("crates/occupied")).unwrap();
        assert!(change(root.path(), "shop", Some("occupied"), None, false).is_err());
        change(
            root.path(),
            "shop",
            Some("boutique"),
            Some("New description"),
            false,
        )
        .unwrap();
        assert_eq!(
            fs::read(root.path().join("crates/boutique/assets/picture.bin")).unwrap(),
            [0, 255, 7]
        );
        let source = fs::read_to_string(&manifest).unwrap();
        assert!(source.starts_with("# retain application comments"));
        let parsed: toml::Value = toml::from_str(&source).unwrap();
        assert_eq!(
            parsed["projects"][0]["metadata"]["staticPath"].as_str(),
            Some("crates/boutique")
        );
        assert_eq!(
            parsed["project_peer_topology"]["app"][0].as_str(),
            Some("boutique")
        );
        delete(root.path(), "boutique").unwrap();
        assert!(!root.path().join("crates/boutique").exists());
        assert!(root.path().join("crates/occupied").exists());
        let parsed: toml::Value = toml::from_str(&fs::read_to_string(manifest).unwrap()).unwrap();
        assert_eq!(parsed["projects"].as_array().unwrap().len(), 1);
        assert_eq!(
            parsed["project_peer_topology"]["app"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(!root.path().join("package.json").exists());
        assert!(!root.path().join("Cargo.toml").exists());
    }

    #[test]
    fn rejects_capture_paths_that_escape_or_own_application_metadata() {
        for path in ["../outside", ".forklaunch", "modules"] {
            let root = fixture();
            fs::create_dir_all(root.path().join("modules/api")).unwrap();
            let manifest = root.path().join(".forklaunch/manifest.toml");
            fs::write(
                &manifest,
                fs::read_to_string(&manifest)
                    .unwrap()
                    .replace("crates/shop", path),
            )
            .unwrap();
            let before = fs::read(&manifest).unwrap();
            assert!(delete(root.path(), "shop").is_err(), "accepted {path}");
            assert_eq!(fs::read(manifest).unwrap(), before);
            assert!(root.path().join("crates/shop/assets/picture.bin").exists());
        }
    }
}

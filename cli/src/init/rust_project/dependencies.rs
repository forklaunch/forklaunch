//! Explicit dependency source selection.
use super::*;

use crate::core::{
    cargo_toml::{self, BLUEPRINT_VERSION as BLUEPRINT_RELEASE_VERSION, FRAMEWORK_VERSION as RELEASE_VERSION},
    forklaunch_state,
};

/// The `[framework]` selection for a new application: published crates by
/// default, or an external framework-rs checkout referenced in place.
pub(crate) fn configuration(matches: &ArgMatches) -> Result<toml::Table> {
    let selected_version = matches
        .try_get_one::<String>("framework-version")
        .ok()
        .flatten();
    let path = matches
        .try_get_one::<String>("framework-path")
        .ok()
        .flatten();
    let version = selected_version.map(String::as_str)
        .or_else(|| if path.is_none() { Some(RELEASE_VERSION) } else { None });
    let mut config = toml::Table::new();
    if let Some(version) = version {
        // Accept exact releases only; arbitrary TOML or broad ranges cannot leak
        // into generated dependencies or silently select a different release.
        anyhow::ensure!(
            valid_version(version),
            "--framework-version requires an exact numeric MAJOR.MINOR.PATCH version"
        );
        config.insert("version".into(), version.into());
    }
    // Module and worker crates come from the registry too; nothing is copied.
    // A local framework checkout uses its sibling blueprint-rs when present.
    config.insert("blueprint_version".into(), BLUEPRINT_RELEASE_VERSION.into());
    if let Some(path) = path {
        let path =
            std::fs::canonicalize(path).context("Framework override directory does not exist")?;
        anyhow::ensure!(
            path.join("crates/axum/Cargo.toml").is_file(),
            "--framework-path must identify the framework-rs workspace"
        );
        config.insert("path".into(), path.to_string_lossy().into_owned().into());
    }
    Ok(config)
}

/// The registry selection written when an application is generated without
/// explicit options.
fn default_configuration() -> toml::Table {
    let mut config = toml::Table::new();
    config.insert("version".into(), RELEASE_VERSION.into());
    config.insert("blueprint_version".into(), BLUEPRINT_RELEASE_VERSION.into());
    config
}

/// Switch an existing application between published crates and a local
/// framework checkout, rewriting manifests and the generated Docker inputs.
pub(crate) fn change(root: &Path, matches: &ArgMatches) -> Result<String> {
    configure(root, matches)?;
    super::language_images::reconcile(root)?;
    Ok(match forklaunch_state::framework(root).and_then(|config| config.get("path").and_then(Value::as_str).map(str::to_owned)) {
        Some(path) => format!("local framework checkout {path}"),
        None => format!("published framework {}", forklaunch_state::framework(root).and_then(|c| c.get("version").and_then(Value::as_str).map(str::to_owned)).unwrap_or_default()),
    })
}

/// Record the selection and rewrite already generated manifests to it.
pub(crate) fn configure(root: &Path, matches: &ArgMatches) -> Result<()> {
    let config = configuration(matches)?;
    forklaunch_state::write_framework(root, &config)?;
    rewrite_tree(root, root)
}

/// Select the published release when the application has no selection, and
/// rewrite generated manifests to the selection.
pub(super) fn prepare(root: &Path) -> Result<()> {
    if !configured(root) {
        forklaunch_state::write_framework(root, &default_configuration())?;
    }
    rewrite_tree(root, root)
}

fn valid_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        })
}

pub(super) fn rewrite_tree(root: &Path, directory: &Path) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            if ["vendor", "target", "node_modules", ".git"]
                .iter()
                .any(|name| path.file_name().is_some_and(|file| file == *name))
            {
                continue;
            }
            rewrite_tree(root, &path)?;
        } else if path.file_name().is_some_and(|name| name == "Cargo.toml") {
            let source = read_to_string(&path)?;
            if let Some(updated) = rewrite_in_place(root, &source)? {
                write(&path, updated)?;
            }
        }
    }
    Ok(())
}

pub(super) fn configured(root: &Path) -> bool {
    forklaunch_state::has_framework(root)
}

pub(crate) fn render_for_path(path: &Path, source: &str) -> Result<String> {
    if path.file_name().is_none_or(|name| name != "Cargo.toml") {
        return Ok(source.into());
    }
    let source = cargo_toml::fill(source)?;
    let root = path.ancestors().find(|root| configured(root));
    match root {
        Some(root) => render(root, &source),
        None => Ok(source),
    }
}

fn blueprint_suffix(path: &str) -> Option<&str> {
    [".forklaunch/blueprint-rs/crates/", "blueprint-rs/crates/"]
        .into_iter()
        .find_map(|prefix| path.split_once(prefix).map(|(_, suffix)| suffix))
}

fn local_blueprint(framework: &str) -> PathBuf {
    Path::new(framework).parent().unwrap_or(Path::new(framework)).join("blueprint-rs/crates")
}

fn framework_suffix(path: &str) -> Option<&str> {
    path.split_once("framework-rs/").map(|(_, suffix)| suffix)
}

fn render(root: &Path, source: &str) -> Result<String> {
    Ok(rewrite_in_place(root, source)?.unwrap_or_else(|| source.into()))
}

/// Rewrite an existing manifest's framework dependencies, keeping its comments
/// and formatting. `None` when nothing refers to framework sources.
fn rewrite_in_place(root: &Path, source: &str) -> Result<Option<String>> {
    let config = forklaunch_state::framework(root).context("Framework dependency selection missing")?;
    // An unparseable manifest is the application's to fix; Cargo reports it.
    let Ok(mut document) = source.parse::<toml_edit::DocumentMut>() else { return Ok(None) };
    let mut changed = false;
    rewrite_item(document.as_item_mut(), &config, &mut changed);
    // Published crates switched to a local checkout: registry specs become paths.
    if let Some(path) = config.get("path").and_then(Value::as_str) {
        let index = local_crates(Path::new(path));
        localize_item(document.as_item_mut(), &index, &mut changed);
    }
    Ok(changed.then(|| document.to_string()))
}

/// Package name → crate directory for a local framework checkout and its
/// sibling blueprint-rs.
fn local_crates(framework: &Path) -> std::collections::BTreeMap<String, PathBuf> {
    fn walk(directory: &Path, index: &mut std::collections::BTreeMap<String, PathBuf>, depth: usize) {
        let Ok(entries) = std::fs::read_dir(directory) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() || path.file_name().is_some_and(|name| name == "target" || name == "node_modules") {
                continue;
            }
            if let Ok(source) = read_to_string(path.join("Cargo.toml"))
                && let Ok(manifest) = toml::from_str::<Value>(&source)
                && let Some(name) = manifest.get("package").and_then(|package| package.get("name")).and_then(Value::as_str)
            {
                index.insert(name.to_owned(), path.clone());
            }
            if depth < 4 {
                walk(&path, index, depth + 1);
            }
        }
    }
    let mut index = std::collections::BTreeMap::new();
    walk(&framework.join("crates"), &mut index, 0);
    let blueprint = local_blueprint(&framework.to_string_lossy());
    if blueprint.is_dir() {
        walk(&blueprint, &mut index, 0);
    }
    index
}

fn localize_item(item: &mut toml_edit::Item, index: &std::collections::BTreeMap<String, PathBuf>, changed: &mut bool) {
    let Some(table) = item.as_table_like_mut() else { return };
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_owned()).collect();
    for key in keys {
        let Some(child) = table.get_mut(&key) else { continue };
        let Some(spec) = child.as_table_like_mut() else { continue };
        let package = spec.get("package").and_then(toml_edit::Item::as_str).unwrap_or(&key).to_owned();
        if spec.contains_key("version") && !spec.contains_key("path") && package.starts_with("forklaunch-")
            && let Some(directory) = index.get(&package)
        {
            spec.remove("version");
            spec.insert("path", toml_edit::value(directory.to_string_lossy().into_owned()));
            *changed = true;
            if let Some(inline) = child.as_inline_table_mut() {
                // Keep inline specs readable: the path leads, as in templates.
                let path = inline.remove("path").expect("inserted above");
                let mut ordered = toml_edit::InlineTable::new();
                ordered.insert("path", path);
                ordered.extend(inline.iter().map(|(key, value)| (key.to_owned(), value.clone())));
                ordered.fmt();
                *inline = ordered;
            }
            continue;
        }
        localize_item(child, index, changed);
    }
}

fn rewrite_item(item: &mut toml_edit::Item, config: &Value, changed: &mut bool) {
    let Some(table) = item.as_table_like_mut() else { return };
    if let Some(path) = table.get("path").and_then(toml_edit::Item::as_str).map(str::to_owned) {
        let mut parsed = toml::Table::new();
        parsed.insert("path".into(), path.clone().into());
        let mut value = Value::Table(parsed);
        rewrite_value(&mut value, config);
        match (value.get("path").and_then(Value::as_str), value.get("version").and_then(Value::as_str)) {
            (Some(next), _) if next != path => {
                table.insert("path", toml_edit::value(next));
                *changed = true;
            }
            (None, Some(version)) => {
                table.remove("path");
                table.insert("version", toml_edit::value(version));
                *changed = true;
                if let Some(inline) = item.as_inline_table_mut() {
                    // Keep inline specs readable: the version leads, as in templates.
                    let version = inline.remove("version").unwrap_or_else(|| version.into());
                    let mut ordered = toml_edit::InlineTable::new();
                    ordered.insert("version", version);
                    ordered.extend(inline.iter().map(|(key, value)| (key.to_owned(), value.clone())));
                    ordered.fmt();
                    *inline = ordered;
                }
                return;
            }
            _ => {}
        }
    }
    for (_, child) in table.iter_mut() {
        rewrite_item(child, config, changed);
    }
}

fn rewrite_value(value: &mut Value, config: &Value) {
    if let Some(table) = value.as_table_mut() {
        // All framework path dependencies are explicit inline/table specs.
        // Features, aliases, optional flags and default-features are preserved.
        if let Some(path) = table.get("path").and_then(Value::as_str).map(str::to_owned) {
            if let Some(suffix) = blueprint_suffix(&path) {
                let local = config.get("path").and_then(Value::as_str).map(local_blueprint).filter(|path| path.is_dir());
                if let Some(local) = local {
                    table.insert("path".into(), local.join(suffix).to_string_lossy().into_owned().into());
                } else if let Some(version) = config.get("blueprint_version").and_then(Value::as_str) {
                    table.remove("path");
                    table.insert("version".into(), format!("={version}").into());
                }
            } else if let Some(suffix) = framework_suffix(&path) {
                if let Some(local) = config.get("path").and_then(Value::as_str) {
                    table.insert(
                        "path".into(),
                        Path::new(local).join(suffix).to_string_lossy().into_owned().into(),
                    );
                } else if let Some(version) = config.get("version").and_then(Value::as_str) {
                    table.remove("path");
                    table.insert("version".into(), format!("={version}").into());
                }
            }
        }
        for (_, value) in table.iter_mut() {
            rewrite_value(value, config);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_mode_preserves_features_and_aliases() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(root.path().join(".forklaunch/manifest.toml"), "app_name = 'demo'\n\n[framework]\nversion = '0.1.0'\n").unwrap();
        let source = "[dependencies]\nframework = { package='forklaunch-axum',path='../../.forklaunch/framework-rs/crates/axum', default-features=false,features=['native'] }\nmodels = { path='../models' }\n";
        let output: Value = toml::from_str(&render(root.path(), source).unwrap()).unwrap();
        assert_eq!(
            output["dependencies"]["framework"]["version"].as_str(),
            Some("=0.1.0")
        );
        assert_eq!(
            output["dependencies"]["framework"]["features"][0].as_str(),
            Some("native")
        );
        assert_eq!(
            output["dependencies"]["framework"]["default-features"].as_bool(),
            Some(false)
        );
        assert_eq!(
            output["dependencies"]["models"]["path"].as_str(),
            Some("../models")
        );
        assert!(output.get("patch").is_none());
        assert!(output["dependencies"]["framework"].get("path").is_none());
    }
    #[test]
    fn existing_manifests_keep_comments_and_untouched_files_are_not_rewritten() {
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        write(root.path().join(".forklaunch/manifest.toml"), "[framework]\nversion = '0.1.2'\nblueprint_version = '0.1.0'\n").unwrap();
        let source = "# owned\n[dependencies]\n# framework\ncore = { package = 'forklaunch-core', path = '../../.forklaunch/framework-rs/crates/core', features = ['build'] }\nworker = { path = '../../.forklaunch/blueprint-rs/crates/interfaces/worker-interface' }\nlocal = { path = '../local' }\n";
        let updated = rewrite_in_place(root.path(), source).unwrap().unwrap();
        assert!(updated.starts_with("# owned\n") && updated.contains("# framework\n"), "{updated}");
        let value: Value = toml::from_str(&updated).unwrap();
        assert_eq!(value["dependencies"]["core"]["version"].as_str(), Some("=0.1.2"));
        assert_eq!(value["dependencies"]["core"]["features"][0].as_str(), Some("build"));
        assert_eq!(value["dependencies"]["worker"]["version"].as_str(), Some("=0.1.0"));
        assert_eq!(value["dependencies"]["local"]["path"].as_str(), Some("../local"));
        assert!(rewrite_in_place(root.path(), "# mine\n[dependencies]\nlocal = { path = '../local' }\n").unwrap().is_none());
    }

    #[test]
    fn published_crates_switch_to_a_local_checkout_by_package_name() {
        let checkout = tempfile::tempdir().unwrap();
        for (path, name) in [("framework-rs/crates/core", "forklaunch-core"), ("blueprint-rs/crates/interfaces/worker-interface", "forklaunch-worker-interface")] {
            create_dir_all(checkout.path().join(path)).unwrap();
            write(checkout.path().join(path).join("Cargo.toml"), format!("[package]\nname = '{name}'\n")).unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        create_dir_all(root.path().join(".forklaunch")).unwrap();
        let framework = checkout.path().join("framework-rs");
        write(root.path().join(".forklaunch/manifest.toml"), format!("[framework]\npath = '{}'\n", framework.display())).unwrap();
        let source = "[dependencies]\ncore = { package = 'forklaunch-core', version = '=0.2.0', features = ['build'] }\nforklaunch-worker-interface = { version = '=0.2.0' }\nserde = { version = '1' }\n";
        let value: Value = toml::from_str(&rewrite_in_place(root.path(), source).unwrap().unwrap()).unwrap();
        assert_eq!(value["dependencies"]["core"]["path"].as_str(), Some(framework.join("crates/core").to_str().unwrap()));
        assert_eq!(value["dependencies"]["core"]["features"][0].as_str(), Some("build"));
        assert!(value["dependencies"]["core"].get("version").is_none());
        assert!(value["dependencies"]["forklaunch-worker-interface"]["path"].as_str().unwrap().ends_with("blueprint-rs/crates/interfaces/worker-interface"));
        assert_eq!(value["dependencies"]["serde"]["version"].as_str(), Some("1"));
        // Switching back and forth leaves an inline spec the way a template writes it.
        let inline = "[dependencies]\nforklaunch-core = { version = \"=0.2.0\", features = [\"build\"] }\n";
        let local = rewrite_in_place(root.path(), inline).unwrap().unwrap();
        assert_eq!(local, format!("[dependencies]\nforklaunch-core = {{ path = \"{}\", features = [\"build\"] }}\n", framework.join("crates/core").display()));
    }

    #[test]
    fn local_override_is_explicit_and_exact_versions_are_required() {
        assert!(valid_version("0.1.0"));
        for invalid in ["*", "^0.1", "0.1", "01.2.3", "0.1.0\n"] {
            assert!(!valid_version(invalid));
        }
        let mut value: Value = toml::from_str("[dependencies]\nworker = {package='forklaunch-worker-interface',path='../../.forklaunch/blueprint-rs/crates/interfaces/worker-interface',features=['apalis']}\n").unwrap();
        let config: Value = toml::from_str("path='/tmp/framework-rs'").unwrap();
        rewrite_value(&mut value, &config);
        assert_eq!(
            value["dependencies"]["worker"]["path"].as_str(),
            Some("../../.forklaunch/blueprint-rs/crates/interfaces/worker-interface")
        );
        let mut framework: Value = toml::from_str("[dependencies]\ncore = {package='forklaunch-core',path='../../.forklaunch/framework-rs/crates/core',features=['typed-schema']}\n").unwrap();
        rewrite_value(&mut framework, &config);
        assert_eq!(framework["dependencies"]["core"]["path"].as_str(), Some("/tmp/framework-rs/crates/core"));
    }
}

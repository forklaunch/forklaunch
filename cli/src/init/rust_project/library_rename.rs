//! Plan registered library reference changes before moving any application files.
use super::*;

pub(super) fn plan(root: &Path, old_path: &str, new_path: &str, old: &str, new: &str) -> Result<Vec<(PathBuf, String)>> {
    let workspace: Value = toml::from_str(&read_to_string(root.join("Cargo.toml"))?)?;
    let members = workspace["workspace"]["members"].as_array().context("Expected workspace members")?;
    let old_dir = normalize_path(root.join(old_path));
    let new_dir = normalize_path(root.join(new_path));
    let mut edits = std::collections::BTreeMap::new();
    // Workspace dependencies resolve relative to the root; member dependencies
    // resolve relative to that member, including private contract/SDK packages.
    for dir in std::iter::once(root.to_path_buf()).chain(members.iter().filter_map(Value::as_str).map(|member| root.join(member))) {
        let path = dir.join("Cargo.toml");
        let mut cargo: Value = toml::from_str(&read_to_string(&path).with_context(|| format!("Read library consumer {}", path.display()))?)?;
        let mut namespace = false;
        if update(&mut cargo, &dir, &old_dir, &new_dir, old, new, &mut namespace)? {
            let output = path.strip_prefix(&old_dir).map(|tail| new_dir.join(tail)).unwrap_or(path);
            edits.insert(output, toml::to_string_pretty(&cargo)?);
            if namespace && dir != root {
                sources(&dir, &old_dir, &new_dir, old, new, &mut edits)?;
            }
        }
    }
    Ok(edits.into_iter().collect())
}

fn update(value: &mut Value, dir: &Path, old_dir: &Path, new_dir: &Path, old: &str, new: &str, namespace: &mut bool) -> Result<bool> {
    let Some(table) = value.as_table_mut() else { return Ok(false); };
    let mut changed = false;
    for (section, value) in table.iter_mut() {
        if matches!(section.as_str(), "dependencies" | "dev-dependencies" | "build-dependencies") {
            let Some(deps) = value.as_table_mut() else { continue; };
            let keys: Vec<_> = deps.iter().filter(|(_, dep)| dependency_path(dep).is_some_and(|path| normalize_path(dir.join(path)) == old_dir)).map(|(name, _)| name.clone()).collect();
            for key in keys {
                let renamed = key == old;
                anyhow::ensure!(!renamed || !deps.contains_key(new), "Library rename would overwrite dependency {new} in {}; choose an explicit alias first", dir.display());
                let mut dep = deps.remove(&key).context("Expected dependency")?;
                let dep_table = dep.as_table_mut().context("Expected path dependency")?;
                let path = pathdiff::diff_paths(new_dir, dir).context("Library relative path")?;
                dep_table.insert("path".into(), Value::String(path.to_string_lossy().into_owned()));
                if !renamed || dep_table.contains_key("package") {
                    dep_table.insert("package".into(), Value::String(new.into()));
                }
                deps.insert(if renamed { new.into() } else { key }, dep);
                *namespace |= renamed;
                changed = true;
            }
        } else {
            changed |= update(value, dir, old_dir, new_dir, old, new, namespace)?;
        }
    }
    Ok(changed)
}

fn sources(dir: &Path, old_dir: &Path, new_dir: &Path, old: &str, new: &str, edits: &mut std::collections::BTreeMap<PathBuf, String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_symlink() { continue; }
        if path.is_dir() {
            if !matches!(entry.file_name().to_str(), Some("target" | "node_modules" | ".git")) {
                sources(&path, old_dir, new_dir, old, new, edits)?;
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let original = read_to_string(&path)?;
            let changed = native_layout::replace_namespace(&original, &old.to_case(Case::Snake), &new.to_case(Case::Snake));
            if changed != original {
                let output = path.strip_prefix(old_dir).map(|tail| new_dir.join(tail)).unwrap_or(path);
                edits.insert(output, changed);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alias_and_dependency_flags_survive_library_rename() {
        let mut value: Value = toml::from_str("[dependencies]\nshared={path='../core/shared', default-features=false, features=['custom']}\n[build-dependencies]\nmodels={package='shared', path='../core/shared', optional=true}\n").unwrap();
        let mut namespace = false;
        assert!(update(&mut value, Path::new("/app/src/api"), Path::new("/app/src/core/shared"), Path::new("/app/src/core/common-core"), "shared", "common-core", &mut namespace).unwrap());
        assert!(namespace);
        assert!(value["dependencies"].get("shared").is_none());
        assert_eq!(value["dependencies"]["common-core"]["path"].as_str(), Some("../core/common-core"));
        assert_eq!(value["dependencies"]["common-core"]["default-features"].as_bool(), Some(false));
        assert_eq!(value["dependencies"]["common-core"]["features"][0].as_str(), Some("custom"));
        assert_eq!(value["build-dependencies"]["models"]["package"].as_str(), Some("common-core"));
        assert_eq!(value["build-dependencies"]["models"]["optional"].as_bool(), Some(true));
    }
}

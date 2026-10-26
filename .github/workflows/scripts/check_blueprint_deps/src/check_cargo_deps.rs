//! Keeps the CLI's Cargo version registry in step with framework-rs and
//! blueprint-rs, the way `check_blueprint_deps` does for package.json.
use std::collections::BTreeMap;
use std::fs::{read_to_string, write};
use std::path::Path;

const CONSTANTS: &str = "../../../../cli/src/core/cargo_toml/cargo_toml_constants.rs";
const FRAMEWORK: &str = "../../../../framework-rs";
const BLUEPRINT: &str = "../../../../blueprint-rs";

pub fn verify_crate_versions(fix_mode: bool) -> Result<(), String> {
    let framework_version = workspace_version(FRAMEWORK)?;
    let blueprint_version = workspace_version(BLUEPRINT)?;
    let mut requirements: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut errors = Vec::new();
    for workspace in [FRAMEWORK, BLUEPRINT] {
        for manifest in manifests(workspace)? {
            for (name, dependency) in dependencies(&manifest.1) {
                let path = dependency.get("path").and_then(toml::Value::as_str).unwrap_or_default();
                let version = dependency.as_str().or_else(|| dependency.get("version")?.as_str());
                if workspace == BLUEPRINT && path.contains("framework-rs/") {
                    let pinned = format!("={framework_version}");
                    if version != Some(pinned.as_str()) {
                        errors.push(format!("{}: {name} must pin the framework release {pinned}", manifest.0));
                    }
                }
                if let Some(version) = version.filter(|_| !name.starts_with("forklaunch-")) {
                    requirements.entry(name).or_default().push(version.to_owned());
                }
            }
        }
    }

    let contents =
        read_to_string(CONSTANTS).map_err(|e| format!("Failed to read {CONSTANTS}: {e}"))?;
    let mut updated = Vec::new();
    for (number, line) in contents.lines().enumerate() {
        let location = format!("{CONSTANTS}:{}", number + 1);
        let mut expected = None;
        let mut current = None;
        if let Some(version) = constant(line, "FRAMEWORK_VERSION") {
            current = Some(version);
            expected = Some((format!("framework-rs {framework_version}"), framework_version.clone()));
        } else if let Some(version) = constant(line, "BLUEPRINT_VERSION") {
            current = Some(version);
            expected = Some((format!("blueprint-rs {blueprint_version}"), blueprint_version.clone()));
        } else if let Some((name, version)) = registry_entry(line) {
            current = Some(version);
            match requirements.get(name).and_then(|versions| newest(versions)) {
                Some(newest) => {
                    let wanted = if version.starts_with('=') { newest.to_owned() } else { range(newest) };
                    let matches = if version.starts_with('=') { version == wanted } else { range(version) == wanted };
                    if !matches {
                        expected = Some((format!("{name} {newest} in the framework workspaces"), wanted));
                    }
                }
                None => println!("Note: {name} is not used by framework-rs or blueprint-rs; {location} is not checked"),
            }
        }
        match (current, expected) {
            (Some(current), Some((source, wanted))) if current != wanted => {
                if fix_mode {
                    println!("Updating {location} from {current} to {wanted} ({source})");
                    updated.push(line.replacen(&format!("\"{current}\""), &format!("\"{wanted}\""), 1));
                    continue;
                }
                errors.push(format!("{location} has {current}, but {source} needs {wanted}"));
            }
            _ => {}
        }
        updated.push(line.to_owned());
    }

    if fix_mode {
        let mut output = updated.join("\n");
        output.push('\n');
        if output != contents {
            write(CONSTANTS, output).map_err(|e| format!("Failed to write {CONSTANTS}: {e}"))?;
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
}

fn workspace_version(workspace: &str) -> Result<String, String> {
    let path = format!("{workspace}/Cargo.toml");
    let manifest = parse(Path::new(&path))?;
    manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("package")?.get("version")?.as_str())
        .map(str::to_owned)
        .ok_or(format!("{path} has no workspace.package.version"))
}

fn manifests(workspace: &str) -> Result<Vec<(String, toml::Value)>, String> {
    walkdir::WalkDir::new(workspace)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != "target")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name() == "Cargo.toml")
        .map(|entry| Ok((entry.path().display().to_string(), parse(entry.path())?)))
        .collect()
}

fn parse(path: &Path) -> Result<toml::Value, String> {
    let source = read_to_string(path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    toml::from_str(&source).map_err(|e| format!("Failed to parse {}: {e}", path.display()))
}

/// Every dependency in a manifest, including target-specific and workspace ones.
fn dependencies(manifest: &toml::Value) -> Vec<(String, toml::Value)> {
    let mut tables = Vec::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        tables.extend(manifest.get(section));
        for target in manifest.get("target").and_then(toml::Value::as_table).into_iter().flat_map(|t| t.values()) {
            tables.extend(target.get(section));
        }
    }
    tables.extend(manifest.get("workspace").and_then(|workspace| workspace.get("dependencies")));
    tables
        .into_iter()
        .filter_map(toml::Value::as_table)
        .flat_map(|table| table.iter().map(|(name, value)| (name.clone(), value.clone())))
        .collect()
}

fn constant<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let rest = line.trim().strip_prefix(&format!("pub(crate) const {name}: &str = \""))?;
    rest.strip_suffix("\";")
}

fn registry_entry(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim().strip_prefix("(\"")?.strip_suffix("\"),")?;
    rest.split_once("\", \"")
}

fn numbers(version: &str) -> Vec<u64> {
    version
        .trim_start_matches(['=', '^', '~'])
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// The compatibility range a requirement selects: the major version, or
/// `0.minor` below 1.0.
fn range(version: &str) -> String {
    let numbers = numbers(version);
    match numbers.as_slice() {
        [0, minor, ..] => format!("0.{minor}"),
        [major, ..] => major.to_string(),
        [] => version.to_owned(),
    }
}

fn newest(versions: &[String]) -> Option<&str> {
    versions.iter().max_by_key(|version| numbers(version)).map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_follow_cargo_caret_compatibility() {
        assert_eq!(range("0.8.4"), "0.8");
        assert_eq!(range("=1.0.151"), "1");
        assert_eq!(range("0.13"), "0.13");
        assert_eq!(newest(&["0.12".into(), "0.13".into(), "0.9".into()]), Some("0.13"));
        assert_eq!(newest(&["=1.0.151".into(), "1".into()]), Some("=1.0.151"));
    }

    #[test]
    fn registry_lines_parse() {
        assert_eq!(registry_entry("    (\"axum\", \"0.8\"),"), Some(("axum", "0.8")));
        assert_eq!(constant("pub(crate) const FRAMEWORK_VERSION: &str = \"0.1.3\";", "FRAMEWORK_VERSION"), Some("0.1.3"));
    }
}

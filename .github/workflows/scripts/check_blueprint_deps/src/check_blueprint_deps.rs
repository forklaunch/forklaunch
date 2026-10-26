use serde_json::Value;
use std::collections::HashMap;
use std::fs::{read_to_string, write};
use std::path::{Path, PathBuf};

pub fn verify_package_versions(
    package_dirs: &[&str],
    implementation_refs: &Vec<PathBuf>,
    fix_mode: bool,
) -> Result<(), String> {
    // Collect actual versions from package.json files
    let mut actual_versions: HashMap<String, String> = HashMap::new();

    // Scan all package.json files to collect actual versions
    for implementation in implementation_refs {
        for entry in walkdir::WalkDir::new(implementation)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() == "package.json")
        {
            collect_versions_from_package_json(entry.path(), &mut actual_versions)?;
        }
    }

    for dir in package_dirs {
        let path = Path::new(dir).join("package.json");
        collect_versions_from_package_json(&path, &mut actual_versions)?;
    }

    if fix_mode {
        update_constants_file(&actual_versions)?;
    } else {
        // Check mode - compare with constants file
        check_against_constants_file(&actual_versions)?;
    }

    Ok(())
}

fn collect_versions_from_package_json(
    path: &Path,
    actual_versions: &mut HashMap<String, String>,
) -> Result<(), String> {
    let package_json_string =
        read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let package_json: Value = serde_json::from_str(&package_json_string)
        .map_err(|e| format!("Failed to parse package.json for {}: {}", path.display(), e))?;

    // Get package name and version
    if let Some(name) = package_json["name"].as_str() {
        if let Some(version) = package_json["version"].as_str() {
            actual_versions.insert(name.to_string(), version.to_string());
        }
    }

    // Collect versions from dependencies
    for section in &["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(deps) = package_json[section].as_object() {
            for (pkg, ver) in deps {
                if let Some(version) = ver.as_str() {
                    // Skip workspace dependencies and special formats
                    if !version.contains("workspace:") && !version.contains("*") {
                        actual_versions.insert(pkg.clone(), version.to_string());
                    }
                }
            }
        }
    }

    Ok(())
}

fn check_against_constants_file(actual_versions: &HashMap<String, String>) -> Result<(), String> {
    let constants_path = "../../../../cli/src/core/package_json/package_json_constants.rs";
    let contents = read_to_string(constants_path)
        .map_err(|e| format!("Failed to read constants file: {}", e))?;

    let mut last_comment = String::new();
    let mut line_number = 0;

    for line in contents.lines() {
        line_number += 1;
        if line.trim().starts_with("//") {
            last_comment = line.trim().to_string();
        } else if line.contains("_VERSION: &str =") {
            let version = line
                .split('=')
                .nth(1)
                .ok_or("Invalid version constant format")?
                .trim()
                .trim_matches(';')
                .trim_matches('"');

            if !last_comment.is_empty() {
                let packages = last_comment
                    .trim_start_matches("//")
                    .trim()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());

                for package in packages {
                    if let Some(actual_version) = actual_versions.get(&package) {
                        // Skip verification for workspace: values
                        if version.starts_with("workspace:") {
                            continue;
                        }

                        // Strip leading characters for comparison
                        let constants_version_stripped =
                            if version.starts_with('^') || version.starts_with('~') {
                                &version[1..]
                            } else {
                                version
                            };

                        let actual_version_stripped =
                            if actual_version.starts_with('^') || actual_version.starts_with('~') {
                                &actual_version[1..]
                            } else {
                                actual_version
                            };

                        if constants_version_stripped != actual_version_stripped {
                            return Err(format!(
                                r#"Version mismatch for {}: constants file has {}, but package.json has {} 
constants file: {}:{}"#,
                                package, version, actual_version, constants_path, line_number
                            ));
                        }
                    }
                }
            }
        }

        if !line.trim().starts_with("//") {
            last_comment.clear();
        }
    }

    Ok(())
}

fn update_constants_file(actual_versions: &HashMap<String, String>) -> Result<(), String> {
    let constants_path = "../../../../cli/src/core/package_json/package_json_constants.rs";
    let contents = read_to_string(constants_path)
        .map_err(|e| format!("Failed to read constants file: {}", e))?;

    let mut updated_lines: Vec<String> = Vec::new();
    let mut last_comment = String::new();
    let mut updated_count = 0;

    for line in contents.lines() {
        if line.trim().starts_with("//") {
            last_comment = line.trim().to_string();
            updated_lines.push(line.to_string());
        } else if line.contains("_VERSION: &str =") {
            let version = line
                .split('=')
                .nth(1)
                .ok_or("Invalid version constant format")?
                .trim()
                .trim_matches(';')
                .trim_matches('"');

            if !last_comment.is_empty() {
                let packages = last_comment
                    .trim_start_matches("//")
                    .trim()
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());

                let mut should_update = false;
                let mut new_version = version.to_string();

                for package in packages {
                    if let Some(actual_version) = actual_versions.get(&package) {
                        // Skip updates if the current version starts with workspace:
                        if version.starts_with("workspace:") {
                            println!(
                                "Skipping update for {} - preserving workspace:{}",
                                package,
                                &version[9..]
                            );
                            continue;
                        }

                        if version.trim_start_matches(['^', '~'])
                            != actual_version.trim_start_matches(['^', '~'])
                        {
                            should_update = true;
                            // Preserve the leading character from the original constant
                            let leading_char = version.chars().next().unwrap_or(' ');
                            let actual_version_without_prefix = if actual_version.starts_with('^')
                                || actual_version.starts_with('~')
                            {
                                &actual_version[1..]
                            } else {
                                actual_version
                            };

                            if leading_char == '^' || leading_char == '~' {
                                new_version =
                                    format!("{}{}", leading_char, actual_version_without_prefix);
                            } else {
                                new_version = actual_version.clone();
                            }

                            println!(
                                "Updating {} version from {} to {}",
                                package, version, new_version
                            );
                            break;
                        }
                    }
                }

                if should_update {
                    // Reconstruct the line with the new version
                    let parts: Vec<&str> = line.split('=').collect();
                    if parts.len() >= 2 {
                        let new_line = format!("{} = \"{}\";", parts[0].trim(), new_version);
                        updated_lines.push(new_line);
                        updated_count += 1;
                    } else {
                        updated_lines.push(line.to_string());
                    }
                } else {
                    updated_lines.push(line.to_string());
                }
            } else {
                updated_lines.push(line.to_string());
            }
        } else {
            updated_lines.push(line.to_string());
        }

        if !line.trim().starts_with("//") {
            last_comment.clear();
        }
    }

    if updated_count > 0 {
        let mut updated_content = updated_lines.join("\n");
        updated_content.push('\n');
        write(constants_path, updated_content)
            .map_err(|e| format!("Failed to write updated constants file: {}", e))?;
        println!(
            "Updated {} version constants in {}",
            updated_count, constants_path
        );
    } else {
        println!("No version updates needed");
    }

    Ok(())
}

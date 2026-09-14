use std::{
    collections::{BTreeMap, BTreeSet},
    io::{IsTerminal, Write},
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Confirm, theme::ColorfulTheme};
use serde::Deserialize;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::CliCommand;
use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        env::{EnvFileItem, parse_env_file_items, parse_env_items_from_str},
        hmac::AuthMode,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// Reconstruct env file content from parsed items for pushing to the platform.
/// Section headers are preserved as-is; key-value pairs are formatted as `KEY=value`
/// (multiline values are double-quoted).
pub(crate) fn reconstruct_env_content(items: Vec<EnvFileItem>) -> String {
    items
        .into_iter()
        .map(|item| match item {
            EnvFileItem::SectionHeader(line) => format!("{}\n", line),
            EnvFileItem::KeyValue(key, value) => {
                if value.contains('\n') {
                    format!("{}=\"{}\"\n", key, value)
                } else {
                    format!("{}={}\n", key, value)
                }
            }
        })
        .collect::<String>()
}

/// Keys with a value, grouped by the section header they sit under. Headers
/// are normalised to their first word (`application`, or the service/worker
/// name) so a pulled `# svc (uuid)` and a hand-written `# svc` line up.
pub(crate) fn keys_by_section(items: &[EnvFileItem]) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current = String::from("application");
    for item in items {
        match item {
            EnvFileItem::SectionHeader(line) => {
                current = section_name(line);
            }
            EnvFileItem::KeyValue(k, v) => {
                if !v.trim().is_empty() {
                    out.entry(current.clone()).or_default().insert(k.clone());
                }
            }
        }
    }
    out
}

fn section_name(header: &str) -> String {
    header
        .trim_start_matches('#')
        .trim()
        .split(' ')
        .next()
        .unwrap_or("application")
        .to_string()
}

/// One key a `--replace` push is about to unset.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PlannedUnset {
    pub(crate) section: String,
    pub(crate) key: String,
    /// True when the same key has a value at application scope, which this
    /// unset will hide for the component: an unset at service/worker scope
    /// wins over the app-level value when the environment is resolved.
    pub(crate) masks_application_value: bool,
}

/// What a `--replace` push will unset: every key that currently has a value
/// in a section the local file contains, but which the local file does not
/// name. Sections the file does not mention are never touched.
pub(crate) fn plan_replace_unsets(
    current: &[EnvFileItem],
    pushed: &[EnvFileItem],
) -> Vec<PlannedUnset> {
    let current_by_section = keys_by_section(current);
    let pushed_by_section = keys_by_section(pushed);
    let app_keys = current_by_section
        .get("application")
        .cloned()
        .unwrap_or_default();

    // A section the file contains but with zero valued keys still counts as
    // pushed: the platform sees the header and clears the whole scope.
    let mut pushed_sections: BTreeSet<String> = pushed_by_section.keys().cloned().collect();
    for item in pushed {
        if let EnvFileItem::SectionHeader(line) = item {
            pushed_sections.insert(section_name(line));
        }
    }

    let mut out = Vec::new();
    for section in pushed_sections {
        let Some(current_keys) = current_by_section.get(&section) else {
            continue;
        };
        let pushed_keys = pushed_by_section
            .get(&section)
            .cloned()
            .unwrap_or_default();
        for key in current_keys.difference(&pushed_keys) {
            out.push(PlannedUnset {
                section: section.clone(),
                key: key.clone(),
                masks_application_value: section != "application" && app_keys.contains(key),
            });
        }
    }
    out
}

#[derive(Deserialize)]
struct PushResponse {
    #[serde(default, rename = "unsetKeys")]
    unset_keys: Vec<UnsetKey>,
}

#[derive(Deserialize)]
struct UnsetKey {
    key: String,
    scope: String,
}

#[derive(Debug)]
pub(crate) struct PushCommand;

impl PushCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for PushCommand {
    fn command(&self) -> Command {
        command(
            "push",
            "Push environment configuration to the forklaunch platform",
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("Region (e.g. us-east-1)"),
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment name (e.g. production, staging)"),
        )
        .arg(
            Arg::new("input")
                .short('i')
                .long("input")
                .required(false)
                .help("Input file path (defaults to <environment>.env)"),
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
        .arg(
            Arg::new("replace")
                .long("replace")
                .action(ArgAction::SetTrue)
                .help(
                    "Treat the file as the whole truth for every section it contains: keys on the platform that the file does not name are unset. Default is merge, which only changes the keys named in the file",
                ),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .short('y')
                .action(ArgAction::SetTrue)
                .help("Skip the confirmation that --replace asks for before unsetting keys"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;

        let region = matches
            .get_one::<String>("region")
            .expect("region is required");
        let environment = matches
            .get_one::<String>("environment")
            .expect("environment is required");

        let input = format!("{}.env", environment);
        let input = matches.get_one::<String>("input").unwrap_or(&input);

        let url = format!(
            "{}/config/push",
            get_platform_management_api_url()
        );

        let replace = matches.get_flag("replace");
        let skip_confirm = matches.get_flag("yes");

        let items = parse_env_file_items(std::path::Path::new(input))
            .with_context(|| format!("Failed to parse file {}. Please check file permissions.", input))?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        if replace {
            confirm_replace(&mut stdout, &app, region, environment, &items, skip_confirm)?;
        }

        let content = reconstruct_env_content(items);

        let body = serde_json::json!({
            "applicationId": app,
            "region": region,
            "environment": environment,
            "content": content,
            "mode": if replace { "replace" } else { "merge" }
        });

        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        match response.status() {
            reqwest::StatusCode::OK => {
                log_ok!(stdout, "Config pushed successfully for {} ({})", environment, region);
                let parsed: PushResponse = response
                    .json()
                    .unwrap_or(PushResponse { unset_keys: Vec::new() });
                if !parsed.unset_keys.is_empty() {
                    log_warn!(stdout, "Unset {} key(s):", parsed.unset_keys.len());
                    for u in parsed.unset_keys {
                        writeln!(stdout, "  - {} ({} scope)", u.key, u.scope)?;
                    }
                }
            }
            _ => {
                let err_text = response.text()?;
                log_error!(stdout, "Failed to push config: {}", err_text);
                anyhow::bail!("Failed to push config: {}", err_text);
            }
        }

        Ok(())
    }
}

/// Pull the current config, work out what a replace push would unset, show
/// it, and ask. Refuses without `--yes` when nobody can answer: a
/// non-interactive stdin, or machine auth (HMAC), which is how agents and CI
/// run. Prints nothing and returns Ok when there is nothing to unset.
fn confirm_replace(
    stdout: &mut StandardStream,
    app: &str,
    region: &str,
    environment: &str,
    items: &[EnvFileItem],
    skip_confirm: bool,
) -> Result<()> {
    let pull_url = format!(
        "{}/config/pull?applicationId={}&region={}&environment={}",
        get_platform_management_api_url(),
        app,
        region,
        environment
    );
    let response = http_client::get(&pull_url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if response.status() != reqwest::StatusCode::OK {
        let err_text = response.text()?;
        bail!("Failed to pull current config before --replace: {}", err_text);
    }
    let current = parse_env_items_from_str(&response.text()?);
    let planned = plan_replace_unsets(&current, items);

    if planned.is_empty() {
        return Ok(());
    }

    log_warn!(
        stdout,
        "--replace will unset {} key(s) that are not in the file:",
        planned.len()
    );
    for u in &planned {
        if u.masks_application_value {
            writeln!(
                stdout,
                "  - {} ({} scope) — this HIDES the application-level {} for this component",
                u.key, u.section, u.key
            )?;
        } else {
            writeln!(stdout, "  - {} ({} scope)", u.key, u.section)?;
        }
    }
    writeln!(stdout)?;

    if skip_confirm {
        return Ok(());
    }

    let machine = matches!(AuthMode::detect(), AuthMode::Hmac { .. });
    if machine || !std::io::stdin().is_terminal() {
        bail!(
            "--replace would unset {} key(s) and nobody is here to confirm. Re-run with --yes to proceed, or drop --replace to merge (only keys in the file change).",
            planned.len()
        );
    }

    let ok = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Unset these keys?")
        .default(false)
        .interact()
        .with_context(|| "Failed to read confirmation")?;
    if !ok {
        bail!("aborted, nothing was pushed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    /// Helper: parse an env file and reconstruct content, simulating the push path.
    fn parse_and_reconstruct(path: &std::path::Path) -> String {
        let items = parse_env_file_items(path).unwrap();
        reconstruct_env_content(items)
    }

    /// Helper: extract key names from reconstructed content (ignoring section headers).
    fn extract_keys(content: &str) -> Vec<String> {
        content
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .filter_map(|line| line.split_once('=').map(|(k, _)| k.to_string()))
            .collect()
    }

    #[test]
    fn test_push_content_only_includes_present_vars() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("production.env");

        // Initial file with 5 vars
        fs::write(
            &path,
            "# application\n\
             DB_HOST=db.example.com\n\
             DB_PORT=5432\n\
             GOOGLE_CLOUD_LOCATION=us-central1\n\
             GOOGLE_CLOUD_API_KEY=abc123\n\
             REDIS_URL=redis://localhost:6379\n",
        )
        .unwrap();

        let content = parse_and_reconstruct(&path);
        let keys = extract_keys(&content);
        assert_eq!(keys.len(), 5);
        assert!(keys.contains(&"GOOGLE_CLOUD_LOCATION".to_string()));
        assert!(keys.contains(&"GOOGLE_CLOUD_API_KEY".to_string()));

        // User deletes two vars from the env file (simulates deletion before re-push)
        fs::write(
            &path,
            "# application\n\
             DB_HOST=db.example.com\n\
             DB_PORT=5432\n\
             REDIS_URL=redis://localhost:6379\n",
        )
        .unwrap();

        let content = parse_and_reconstruct(&path);
        let keys = extract_keys(&content);

        // Deleted vars must NOT appear in push content
        assert_eq!(keys.len(), 3);
        assert!(!keys.contains(&"GOOGLE_CLOUD_LOCATION".to_string()));
        assert!(!keys.contains(&"GOOGLE_CLOUD_API_KEY".to_string()));

        // Remaining vars must still be present
        assert!(keys.contains(&"DB_HOST".to_string()));
        assert!(keys.contains(&"DB_PORT".to_string()));
        assert!(keys.contains(&"REDIS_URL".to_string()));
    }

    #[test]
    fn test_push_content_preserves_section_headers() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("staging.env");

        fs::write(
            &path,
            "# application\n\
             APP_NAME=myapp\n\
             # my-service (svc-123)\n\
             DB_HOST=db.example.com\n\
             # video-processor-worker (wkr-456)\n\
             QUEUE_URL=sqs://queue\n",
        )
        .unwrap();

        let content = parse_and_reconstruct(&path);

        // Section headers preserved exactly
        assert!(content.contains("# application\n"));
        assert!(content.contains("# my-service (svc-123)\n"));
        assert!(content.contains("# video-processor-worker (wkr-456)\n"));

        // All key-value pairs present
        assert!(content.contains("APP_NAME=myapp\n"));
        assert!(content.contains("DB_HOST=db.example.com\n"));
        assert!(content.contains("QUEUE_URL=sqs://queue\n"));
    }

    #[test]
    fn test_push_content_multiline_values_are_quoted() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("production.env");

        fs::write(
            &path,
            "SIMPLE=hello\n\
             CERT=\"-----BEGIN CERTIFICATE-----\n\
             abc123\n\
             -----END CERTIFICATE-----\"\n",
        )
        .unwrap();

        let content = parse_and_reconstruct(&path);

        // Simple value: no quotes
        assert!(content.contains("SIMPLE=hello\n"));

        // Multiline value: must be double-quoted in reconstructed output
        assert!(content.contains("CERT=\"-----BEGIN CERTIFICATE-----\nabc123\n-----END CERTIFICATE-----\"\n"));
    }

    #[test]
    fn test_push_content_empty_file_produces_empty_string() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("empty.env");

        fs::write(&path, "").unwrap();

        let content = parse_and_reconstruct(&path);
        assert!(content.is_empty());
    }

    #[test]
    fn test_push_content_nonexistent_file_produces_empty_string() {
        let content = parse_and_reconstruct(std::path::Path::new("/nonexistent/file.env"));
        assert!(content.is_empty());
    }

    #[test]
    fn test_push_re_adding_deleted_var_includes_it() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("production.env");

        // Step 1: file with 3 vars
        fs::write(
            &path,
            "# application\nA=1\nB=2\nC=3\n",
        )
        .unwrap();

        let keys = extract_keys(&parse_and_reconstruct(&path));
        assert_eq!(keys.len(), 3);

        // Step 2: user removes B
        fs::write(
            &path,
            "# application\nA=1\nC=3\n",
        )
        .unwrap();

        let keys = extract_keys(&parse_and_reconstruct(&path));
        assert_eq!(keys.len(), 2);
        assert!(!keys.contains(&"B".to_string()));

        // Step 3: user explicitly re-adds B (this is an intentional restore)
        fs::write(
            &path,
            "# application\nA=1\nB=new_value\nC=3\n",
        )
        .unwrap();

        let keys = extract_keys(&parse_and_reconstruct(&path));
        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"B".to_string()));

        let content = parse_and_reconstruct(&path);
        assert!(content.contains("B=new_value\n"));
    }

    #[test]
    fn test_push_json_body_structure() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("production.env");

        fs::write(
            &path,
            "# application\nDB_HOST=localhost\n",
        )
        .unwrap();

        let items = parse_env_file_items(&path).unwrap();
        let content = reconstruct_env_content(items);

        let body = serde_json::json!({
            "applicationId": "app-123",
            "region": "us-east-1",
            "environment": "production",
            "content": content
        });

        assert_eq!(body["applicationId"], "app-123");
        assert_eq!(body["region"], "us-east-1");
        assert_eq!(body["environment"], "production");

        let content_str = body["content"].as_str().unwrap();
        assert!(content_str.contains("# application\n"));
        assert!(content_str.contains("DB_HOST=localhost\n"));
    }

    fn items(content: &str) -> Vec<EnvFileItem> {
        parse_env_items_from_str(content)
    }

    #[test]
    fn replace_plan_unsets_only_keys_missing_from_pushed_sections() {
        let current = items(
            "# application\nDB_URL=x\nSTRIPE_KEY=y\n# api (uuid-1)\nSTRIPE_KEY=z\nLOG_LEVEL=debug\n# worker (uuid-2)\nQUEUE=q\n",
        );
        // The customer's file: app names DB_URL only, api names LOG_LEVEL only, no worker section.
        let pushed = items("# application\nDB_URL=x\n# api (uuid-1)\nLOG_LEVEL=info\n");

        let plan = plan_replace_unsets(&current, &pushed);
        assert_eq!(
            plan,
            vec![
                PlannedUnset {
                    section: "api".into(),
                    key: "STRIPE_KEY".into(),
                    masks_application_value: true
                },
                PlannedUnset {
                    section: "application".into(),
                    key: "STRIPE_KEY".into(),
                    masks_application_value: false
                },
            ]
        );
    }

    #[test]
    fn replace_plan_ignores_keys_already_unset_and_untouched_sections() {
        let current = items("# application\nA=1\nGONE=\n# worker (u)\nQ=1\n");
        let pushed = items("# application\nA=1\n");
        assert!(plan_replace_unsets(&current, &pushed).is_empty());
    }

    #[test]
    fn replace_plan_treats_an_empty_pushed_section_as_clearing_it() {
        let current = items("# application\nA=1\n# api (u)\nB=2\n");
        let pushed = items("# application\nA=1\n# api (u)\n");
        let plan = plan_replace_unsets(&current, &pushed);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].key, "B");
        assert_eq!(plan[0].section, "api");
    }
}

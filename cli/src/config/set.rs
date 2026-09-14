use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::{CliCommand, declared::confirm_if_undeclared, push::reconstruct_env_content};
use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        env::{EnvFileItem, parse_env_items_from_str},
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct SetCommand;

impl SetCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

/// Match a pulled section header (e.g. `# application` or
/// `# my-service (uuid)`) against a target scope name.
fn header_matches_scope(header: &str, scope: &str) -> bool {
    let name = header
        .trim_start_matches('#')
        .trim()
        .split(' ')
        .next()
        .unwrap_or("");
    name == scope
}

/// Drop valueless entries from a scope that is about to be pushed back, except
/// the one the operator named.
///
/// `config pull` emits two different things as a bare `KEY=` line: a variable
/// the release manifest declares but nobody has filled in yet, and a variable
/// already marked unset. Pushing the first kind back creates a record with
/// `isUnset = true` — the platform reads an empty pushed value as "the
/// operator asserts this key is deliberately absent", which is exactly what
/// makes the deploy gate stop demanding it. So a `config set` of one unrelated
/// key would quietly switch off the gate for every unfilled variable sharing
/// its scope.
///
/// Dropping them loses nothing. Only entries with no value are dropped, so
/// nothing that has a value to lose ever leaves the payload. A dropped key
/// with no record yet stays absent instead of gaining a tombstone, and a
/// dropped key that already has one is swept to unset by the push — the same
/// state an empty pushed value would have produced anyway.
fn drop_valueless_entries(items: Vec<EnvFileItem>, keep: &str) -> Vec<EnvFileItem> {
    items
        .into_iter()
        .filter(|item| match item {
            EnvFileItem::KeyValue(k, v) => k == keep || !v.trim().is_empty(),
            EnvFileItem::SectionHeader(_) => true,
        })
        .collect()
}

impl CliCommand for SetCommand {
    fn command(&self) -> Command {
        command(
            "set",
            "Set a single environment variable without touching the rest of the scope",
        )
        .arg(
            Arg::new("pair")
                .required(true)
                .help("KEY=VALUE to set (quote values containing spaces)"),
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
            Arg::new("service")
                .short('s')
                .long("service")
                .help("Scope to a specific service/worker (defaults to application scope)"),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .short('f')
                .help("Set the variable even when no component declares it")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (app_root, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let pair = matches.get_one::<String>("pair").expect("pair required");
        let Some((key, value)) = pair.split_once('=') else {
            bail!("Expected KEY=VALUE, got '{}'", pair);
        };
        let key = key.trim();
        if key.is_empty() {
            bail!("Variable name cannot be empty");
        }
        // The deploy env-editor incident class: a value that itself looks
        // like an assignment is almost always a paste error and produces
        // values like `S3_BUCKET=S3_BUCKET=real-value` server-side.
        if value
            .split_once('=')
            .is_some_and(|(maybe_key, _)| maybe_key.chars().all(|c| c.is_ascii_uppercase() || c == '_') && !maybe_key.is_empty())
        {
            bail!(
                "Value '{}' looks like another KEY=VALUE assignment — this is usually a paste error. Quote the value if it is intentional.",
                value
            );
        }

        let region = matches.get_one::<String>("region").expect("required");
        let environment = matches
            .get_one::<String>("environment")
            .expect("required");
        let scope = matches
            .get_one::<String>("service")
            .cloned()
            .unwrap_or_else(|| "application".to_string());

        // The push endpoint is authoritative per scope: any var present in
        // the pushed scope but missing from the payload is cleared. So a
        // single-var set must round-trip the FULL current scope.
        let pull_url = format!(
            "{}/config/pull?applicationId={}&region={}&environment={}",
            get_platform_management_api_url(),
            app,
            region,
            environment
        );
        let pull_response =
            http_client::get(&pull_url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !pull_response.status().is_success() {
            bail!(
                "Failed to pull current config: {}",
                pull_response.text().unwrap_or_default()
            );
        }
        let current = pull_response.text()?;

        // Checked before anything is written, because the damage in the
        // incident this guards against was done the moment the credential
        // landed under the wrong name.
        confirm_if_undeclared(
            &mut stdout,
            key,
            &app_root,
            &current,
            environment,
            region,
            matches.get_flag("force"),
        )?;

        let items = parse_env_items_from_str(&current);

        // Extract just the target scope's items, applying the set.
        let mut scoped: Vec<EnvFileItem> = Vec::new();
        let mut in_scope = scope == "application"; // leading section may be implicit
        let mut scope_seen = false;
        let mut replaced = false;
        for item in items {
            match item {
                EnvFileItem::SectionHeader(header) => {
                    in_scope = header_matches_scope(&header, &scope);
                    if in_scope {
                        scope_seen = true;
                        scoped.push(EnvFileItem::SectionHeader(header));
                    }
                }
                EnvFileItem::KeyValue(k, v) => {
                    if in_scope {
                        if k == key {
                            scoped.push(EnvFileItem::KeyValue(k, value.to_string()));
                            replaced = true;
                        } else {
                            scoped.push(EnvFileItem::KeyValue(k, v));
                        }
                    }
                }
            }
        }
        if !scope_seen && scope != "application" {
            bail!(
                "Scope '{}' not found in the current configuration. Known scopes appear as '# <name> (id)' section headers in `forklaunch config pull` output.",
                scope
            );
        }
        if scoped.is_empty() {
            scoped.push(EnvFileItem::SectionHeader(format!("# {}", scope)));
        }
        if !replaced {
            scoped.push(EnvFileItem::KeyValue(key.to_string(), value.to_string()));
        }

        let content = reconstruct_env_content(drop_valueless_entries(scoped, key));
        let push_url = format!("{}/config/push", get_platform_management_api_url());
        let body = serde_json::json!({
            "applicationId": app,
            "region": region,
            "environment": environment,
            "content": content
        });
        let response =
            http_client::post(&push_url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if response.status().is_success() {
            // "Added" versus "Updated" is the only signal that a name was
            // never seen before, and it was easy to miss as one word in one
            // line. Each outcome now says what it means on its own line.
            if replaced {
                log_ok!(
                    stdout,
                    "UPDATED {} in scope '{}' for {} ({})",
                    key,
                    scope,
                    environment,
                    region
                );
                log_info!(stdout, "The previous value of {} was replaced.", key);
            } else {
                log_ok!(
                    stdout,
                    "ADDED {} in scope '{}' for {} ({})",
                    key,
                    scope,
                    environment,
                    region
                );
                log_info!(
                    stdout,
                    "This created a new variable — scope '{}' had nothing named {} before.",
                    scope,
                    key
                );
            }
            log_info!(
                stdout,
                "Running tasks keep their existing environment — redeploy to apply."
            );
            Ok(())
        } else {
            let err_text = response.text()?;
            bail!("Failed to set variable: {}", err_text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(key: &str, value: &str) -> EnvFileItem {
        EnvFileItem::KeyValue(key.to_string(), value.to_string())
    }

    fn header(text: &str) -> EnvFileItem {
        EnvFileItem::SectionHeader(text.to_string())
    }

    fn keys(items: &[EnvFileItem]) -> Vec<String> {
        items
            .iter()
            .filter_map(|item| match item {
                EnvFileItem::KeyValue(k, _) => Some(k.clone()),
                EnvFileItem::SectionHeader(_) => None,
            })
            .collect()
    }

    #[test]
    fn test_header_matches_scope_application() {
        assert!(header_matches_scope("# application", "application"));
        assert!(!header_matches_scope("# payments (svc-1)", "application"));
    }

    #[test]
    fn test_header_matches_scope_named_component() {
        assert!(header_matches_scope("# payments (svc-1)", "payments"));
        assert!(!header_matches_scope("# payments-api (svc-1)", "payments"));
    }

    /// The whole point: a `config set` of one key must not sweep the unfilled
    /// required variables sharing its scope into unset tombstones.
    #[test]
    fn test_drop_valueless_entries_removes_unfilled_required_vars() {
        let scoped = vec![
            header("# application"),
            kv("DB_HOST", "db.example.com"),
            kv("ECS_AGENT_URI", ""),
            kv("STRIPE_API_KEY", ""),
        ];

        let kept = drop_valueless_entries(scoped, "DB_HOST");
        assert_eq!(keys(&kept), vec!["DB_HOST".to_string()]);
    }

    #[test]
    fn test_drop_valueless_entries_keeps_the_targeted_key() {
        // `config set KEY=` still reaches the platform as an empty value, so
        // its existing meaning is preserved exactly.
        let scoped = vec![header("# application"), kv("DB_HOST", "")];

        let kept = drop_valueless_entries(scoped, "DB_HOST");
        assert_eq!(keys(&kept), vec!["DB_HOST".to_string()]);
    }

    #[test]
    fn test_drop_valueless_entries_treats_whitespace_as_valueless() {
        let scoped = vec![kv("A", "   "), kv("B", "value")];
        let kept = drop_valueless_entries(scoped, "B");
        assert_eq!(keys(&kept), vec!["B".to_string()]);
    }

    #[test]
    fn test_drop_valueless_entries_preserves_section_headers() {
        let scoped = vec![header("# payments (svc-1)"), kv("A", "")];
        let kept = drop_valueless_entries(scoped, "OTHER");
        assert_eq!(kept.len(), 1);
        assert!(matches!(kept[0], EnvFileItem::SectionHeader(_)));
    }

    #[test]
    fn test_drop_valueless_entries_keeps_every_valued_key() {
        let scoped = vec![kv("A", "1"), kv("B", "2"), kv("C", "3")];
        let kept = drop_valueless_entries(scoped, "A");
        assert_eq!(keys(&kept), vec!["A", "B", "C"]);
    }
}

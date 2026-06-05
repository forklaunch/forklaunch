use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::{CliCommand, push::reconstruct_env_content};
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
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
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

        let content = reconstruct_env_content(scoped);
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
            log_ok!(
                stdout,
                "{} {} in scope '{}' for {} ({})",
                if replaced { "Updated" } else { "Added" },
                key,
                scope,
                environment,
                region
            );
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

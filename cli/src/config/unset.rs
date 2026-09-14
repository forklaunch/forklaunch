//! Mark a variable as deliberately absent.
//!
//! The platform treats an "unset" record as a tombstone: the operator
//! asserting that this application does not need the variable, which is what
//! stops the deploy gate demanding a value for it. `ECS_AGENT_URI` is the
//! motivating case — the ECS agent injects it per task at runtime, so it must
//! never be given a value, but the gate has no way to know that.
//!
//! Until now the only route to that state from the CLI was a whole-file
//! `config push`, because the push endpoint reads "key absent from the pushed
//! scope" as "unset". That is a sharp tool: it acts on every key the file
//! happens to omit, and it zeroes each one's stored value on the way. This
//! command instead names a single key against the per-component variable
//! endpoints the deploy flow already uses, which upsert only the keys they are
//! given and never sweep.

use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Confirm, theme::ColorfulTheme};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::CliCommand;
use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        env::{EnvFileItem, parse_env_items_from_str},
        http_client,
        manifest::{ProjectEntry, ProjectType},
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// Which platform endpoint a scope's variables live behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ScopeTarget {
    Application,
    Service(String),
    Worker(String),
}

/// What the pulled configuration says about a key inside one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyState {
    /// Not mentioned in this scope at all.
    Absent,
    /// Present with no value — either already unset, or declared by the
    /// release manifest and never filled in.
    Valueless,
    /// Present and holding a value, which unsetting will destroy.
    Valued,
}

#[derive(Debug)]
pub(crate) struct UnsetCommand;

impl UnsetCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

/// Pull the component id out of a `# name (id)` section header, when the
/// header names `scope`.
pub(crate) fn scope_id_from_header(header: &str, scope: &str) -> Option<String> {
    let body = header.trim_start_matches('#').trim();
    let (name, rest) = body.split_once(' ')?;
    if name != scope {
        return None;
    }
    let id = rest.trim().strip_prefix('(')?.strip_suffix(')')?.trim();
    if id.is_empty() { None } else { Some(id.to_string()) }
}

/// Find the platform id of a named scope in `config pull` output.
pub(crate) fn find_scope_id(content: &str, scope: &str) -> Option<String> {
    parse_env_items_from_str(content)
        .into_iter()
        .find_map(|item| match item {
            EnvFileItem::SectionHeader(header) => scope_id_from_header(&header, scope),
            EnvFileItem::KeyValue(_, _) => None,
        })
}

/// Decide whether a named scope is a service or a worker.
///
/// Pull output cannot answer this: a service section and a worker section are
/// both rendered as `# name (id)`, with nothing to tell them apart. The local
/// manifest can, because the platform names a worker component after its
/// project with a `-worker` suffix.
pub(crate) fn classify_scope(
    projects: &[ProjectEntry],
    scope: &str,
    scope_id: String,
) -> Option<ScopeTarget> {
    for project in projects {
        match project.r#type {
            ProjectType::Service if project.name == scope => {
                return Some(ScopeTarget::Service(scope_id));
            }
            ProjectType::Worker
                if project.name == scope || scope == format!("{}-worker", project.name) =>
            {
                return Some(ScopeTarget::Worker(scope_id));
            }
            _ => {}
        }
    }
    None
}

/// Report what the pulled configuration currently holds for `key` in `scope`.
pub(crate) fn key_state(content: &str, scope: &str, key: &str) -> KeyState {
    let mut in_scope = scope == "application";
    let mut state = KeyState::Absent;

    for item in parse_env_items_from_str(content) {
        match item {
            EnvFileItem::SectionHeader(header) => {
                let name = header
                    .trim_start_matches('#')
                    .trim()
                    .split(' ')
                    .next()
                    .unwrap_or("")
                    .to_string();
                in_scope = name == scope;
            }
            EnvFileItem::KeyValue(k, v) => {
                if in_scope && k == key {
                    // A later valued entry wins over an earlier blank one, so
                    // the destructive case is never under-reported.
                    if !v.trim().is_empty() {
                        return KeyState::Valued;
                    }
                    state = KeyState::Valueless;
                }
            }
        }
    }

    state
}

/// The endpoint and body for unsetting one key, given the resolved scope.
pub(crate) fn unset_request(
    target: &ScopeTarget,
    application_id: &str,
    environment: &str,
    region: &str,
    key: &str,
) -> (String, serde_json::Value) {
    let api = get_platform_management_api_url();
    match target {
        ScopeTarget::Application => (
            format!(
                "{}/applications/{}/environments/{}/variables",
                api, application_id, environment
            ),
            serde_json::json!({
                "region": region,
                "variables": [{
                    "key": key,
                    "value": "",
                    "source": "application",
                    "required": false,
                    "hasValue": false,
                    "isUnset": true
                }]
            }),
        ),
        ScopeTarget::Service(id) => (
            format!("{}/services/{}/environments/{}/variables", api, id, environment),
            serde_json::json!({
                "region": region,
                "variables": [{ "key": key, "value": "", "isUnset": true }]
            }),
        ),
        ScopeTarget::Worker(id) => (
            format!("{}/workers/{}/environments/{}/variables", api, id, environment),
            serde_json::json!({
                "region": region,
                "variables": [{ "key": key, "value": "", "isUnset": true }]
            }),
        ),
    }
}

impl CliCommand for UnsetCommand {
    fn command(&self) -> Command {
        command(
            "unset",
            "Mark a variable as deliberately absent so the deploy gate stops requiring it",
        )
        .arg(
            Arg::new("key")
                .required(true)
                .help("Name of the variable to mark unset"),
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
            Arg::new("yes")
                .long("yes")
                .short('y')
                .help("Skip the confirmation prompt (for CI/scripted use) — the stored value is destroyed")
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
        let (_app_root, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let key = matches.get_one::<String>("key").expect("key required");
        if let Some((name, _)) = key.split_once('=') {
            bail!(
                "Expected a variable name, got the assignment '{}'. Use `forklaunch config unset {}` to mark it unset, or `forklaunch config set` to give it a value.",
                key,
                name
            );
        }
        let key = key.trim();
        if key.is_empty() {
            bail!("Variable name cannot be empty");
        }

        let region = matches.get_one::<String>("region").expect("required");
        let environment = matches.get_one::<String>("environment").expect("required");
        let scope = matches
            .get_one::<String>("service")
            .cloned()
            .unwrap_or_else(|| "application".to_string());
        let skip_confirm = matches.get_flag("yes");

        // Read-only: used to resolve the component id behind a named scope and
        // to find out whether a value is about to be destroyed. Nothing from
        // this response is written back.
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

        let target = if scope == "application" {
            ScopeTarget::Application
        } else {
            let scope_id = find_scope_id(&current, &scope).ok_or_else(|| {
                anyhow::anyhow!(
                    "Scope '{}' not found in the current configuration. Known scopes appear as '# <name> (id)' section headers in `forklaunch config pull` output.",
                    scope
                )
            })?;
            classify_scope(&manifest.projects, &scope, scope_id).ok_or_else(|| {
                anyhow::anyhow!(
                    "Scope '{}' is not a service or worker in this application's manifest, so there is no way to tell which endpoint owns it.",
                    scope
                )
            })?
        };

        let state = key_state(&current, &scope, key);
        match state {
            KeyState::Valued => {
                log_warn!(
                    stdout,
                    "{} currently holds a value in scope '{}'. Marking it unset DESTROYS that value — the platform stores an empty string, and it cannot be recovered from here or from the dashboard.",
                    key,
                    scope
                );

                if !skip_confirm {
                    if !std::io::stdin().is_terminal() {
                        bail!(
                            "refusing to destroy the stored value of {} without confirmation — re-run with --yes if that is intended",
                            key
                        );
                    }

                    let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt(format!("Destroy the stored value of {} and mark it unset?", key))
                        .default(false)
                        .interact()?;

                    if !confirmed {
                        bail!("aborted — nothing was changed");
                    }
                }
            }
            KeyState::Valueless => {
                log_info!(
                    stdout,
                    "{} holds no value in scope '{}', so nothing is destroyed.",
                    key,
                    scope
                );
            }
            KeyState::Absent => {
                log_info!(
                    stdout,
                    "{} does not currently appear in scope '{}'. Marking it unset records that this application deliberately does not need it.",
                    key,
                    scope
                );
            }
        }

        let (url, body) = unset_request(&target, &app, environment, region, key);
        let response =
            http_client::put(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if !response.status().is_success() {
            let err_text = response.text()?;
            bail!("Failed to unset variable: {}", err_text);
        }

        log_ok!(
            stdout,
            "UNSET {} in scope '{}' for {} ({})",
            key,
            scope,
            environment,
            region
        );
        if state == KeyState::Valued {
            log_info!(stdout, "The stored value of {} was destroyed.", key);
        }
        log_info!(
            stdout,
            "The deploy gate will no longer require a value for {}.",
            key
        );
        log_info!(
            stdout,
            "Running tasks keep their existing environment — redeploy to apply."
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_id_from_header_matching_scope() {
        assert_eq!(
            scope_id_from_header("# payments (svc-123)", "payments"),
            Some("svc-123".to_string())
        );
    }

    #[test]
    fn test_scope_id_from_header_rejects_other_scopes() {
        assert_eq!(scope_id_from_header("# payments (svc-123)", "billing"), None);
        assert_eq!(scope_id_from_header("# application", "application"), None);
    }

    #[test]
    fn test_find_scope_id_across_sections() {
        let content = "# application\n\
                       DB_HOST=db\n\
                       # payments (svc-123)\n\
                       STRIPE_API_KEY=sk\n\
                       # mailer-worker (wkr-456)\n\
                       QUEUE_NAME=mail\n";

        assert_eq!(find_scope_id(content, "payments"), Some("svc-123".into()));
        assert_eq!(
            find_scope_id(content, "mailer-worker"),
            Some("wkr-456".into())
        );
        assert_eq!(find_scope_id(content, "nope"), None);
    }

    #[test]
    fn test_key_state_in_application_scope() {
        let content = "# application\nDB_HOST=db\nECS_AGENT_URI=\n";
        assert_eq!(key_state(content, "application", "DB_HOST"), KeyState::Valued);
        assert_eq!(
            key_state(content, "application", "ECS_AGENT_URI"),
            KeyState::Valueless
        );
        assert_eq!(
            key_state(content, "application", "NOT_THERE"),
            KeyState::Absent
        );
    }

    /// A key with a value under one component must not make it look valued
    /// under another — that would demand a confirmation for a no-op, or worse,
    /// hide one that is needed.
    #[test]
    fn test_key_state_is_scoped() {
        let content = "# application\nSHARED=\n# payments (svc-1)\nSHARED=real\n";
        assert_eq!(key_state(content, "application", "SHARED"), KeyState::Valueless);
        assert_eq!(key_state(content, "payments", "SHARED"), KeyState::Valued);
    }

    #[test]
    fn test_key_state_treats_whitespace_as_valueless() {
        let content = "# application\nBLANK=   \n";
        assert_eq!(key_state(content, "application", "BLANK"), KeyState::Valueless);
    }

    #[test]
    fn test_unset_request_targets_the_application_endpoint() {
        let (url, body) = unset_request(
            &ScopeTarget::Application,
            "app-1",
            "production",
            "us-east-1",
            "ECS_AGENT_URI",
        );

        assert!(url.ends_with("/applications/app-1/environments/production/variables"));
        assert_eq!(body["region"], "us-east-1");
        assert_eq!(body["variables"][0]["key"], "ECS_AGENT_URI");
        assert_eq!(body["variables"][0]["isUnset"], true);
        assert_eq!(body["variables"][0]["hasValue"], false);
        assert_eq!(body["variables"][0]["value"], "");
        // Exactly one key is named, so nothing else in the scope is touched.
        assert_eq!(body["variables"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_unset_request_targets_the_service_endpoint() {
        let (url, body) = unset_request(
            &ScopeTarget::Service("svc-9".into()),
            "app-1",
            "production",
            "us-east-1",
            "ECS_AGENT_URI",
        );

        assert!(url.ends_with("/services/svc-9/environments/production/variables"));
        assert_eq!(body["variables"][0]["isUnset"], true);
    }

    #[test]
    fn test_unset_request_targets_the_worker_endpoint() {
        let (url, _) = unset_request(
            &ScopeTarget::Worker("wkr-9".into()),
            "app-1",
            "production",
            "us-east-1",
            "ECS_AGENT_URI",
        );

        assert!(url.ends_with("/workers/wkr-9/environments/production/variables"));
    }

    fn project(name: &str, r#type: ProjectType) -> ProjectEntry {
        ProjectEntry {
            r#type,
            name: name.to_string(),
            description: String::new(),
            variant: None,
            resources: None,
            routers: None,
            metadata: None,
        }
    }

    #[test]
    fn test_classify_scope_finds_a_service() {
        let projects = vec![
            project("payments", ProjectType::Service),
            project("mailer", ProjectType::Worker),
        ];

        assert_eq!(
            classify_scope(&projects, "payments", "svc-1".into()),
            Some(ScopeTarget::Service("svc-1".into()))
        );
    }

    /// The platform names a worker component `<project>-worker`, which is what
    /// shows up in the pull section header.
    #[test]
    fn test_classify_scope_finds_a_worker_by_suffixed_name() {
        let projects = vec![project("mailer", ProjectType::Worker)];

        assert_eq!(
            classify_scope(&projects, "mailer-worker", "wkr-1".into()),
            Some(ScopeTarget::Worker("wkr-1".into()))
        );
        assert_eq!(
            classify_scope(&projects, "mailer", "wkr-1".into()),
            Some(ScopeTarget::Worker("wkr-1".into()))
        );
    }

    #[test]
    fn test_classify_scope_ignores_libraries_and_unknowns() {
        let projects = vec![
            project("shared", ProjectType::Library),
            project("payments", ProjectType::Service),
        ];

        assert_eq!(classify_scope(&projects, "shared", "x".into()), None);
        assert_eq!(classify_scope(&projects, "ghost", "x".into()), None);
    }

    fn unset_cmd() -> Command {
        UnsetCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn test_command_definition_is_valid() {
        unset_cmd().debug_assert();
    }

    #[test]
    fn test_key_region_and_environment_are_required() {
        assert!(unset_cmd().try_get_matches_from(["unset"]).is_err());
        assert!(
            unset_cmd()
                .try_get_matches_from(["unset", "ECS_AGENT_URI"])
                .is_err()
        );
    }

    /// `config unset` must take the same option shape as `config set`.
    #[test]
    fn test_accepts_the_same_option_shape_as_set() {
        let matches = unset_cmd()
            .try_get_matches_from([
                "unset",
                "ECS_AGENT_URI",
                "-r",
                "us-east-1",
                "-e",
                "production",
                "-s",
                "payments",
                "-y",
            ])
            .expect("should parse");

        assert_eq!(
            matches.get_one::<String>("key"),
            Some(&"ECS_AGENT_URI".to_string())
        );
        assert_eq!(
            matches.get_one::<String>("region"),
            Some(&"us-east-1".to_string())
        );
        assert_eq!(
            matches.get_one::<String>("environment"),
            Some(&"production".to_string())
        );
        assert_eq!(
            matches.get_one::<String>("service"),
            Some(&"payments".to_string())
        );
        assert!(matches.get_flag("yes"));
    }
}

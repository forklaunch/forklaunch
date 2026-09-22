//! Pre-flight for `deploy create`: say which application is about to be
//! deployed, and refuse the two ways a deploy lands on the wrong one.
//!
//! The application id comes from `.forklaunch/manifest.toml` in the current
//! directory and nothing used to echo it back. Run from the wrong checkout,
//! the CLI resolved a different application, found no `production` environment
//! on it, asked "production or development?" and would have created the
//! environment on that application (Main Street, Sept 2026).

use std::{
    io::{IsTerminal, Write},
    process::Command,
};

use anyhow::{Result, bail};
use dialoguer::{Confirm, theme::ColorfulTheme};
use serde::Deserialize;
use termcolor::{StandardStream, WriteColor};

use crate::{
    constants::get_platform_management_api_url,
    core::{hmac::AuthMode, http_client},
};

#[derive(Debug, Deserialize)]
struct ApplicationSummary {
    name: Option<String>,
    #[serde(rename = "gitRepository")]
    git_repository: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EnvironmentsResponse {
    environments: Vec<EnvironmentRow>,
}

#[derive(Debug, Deserialize)]
struct EnvironmentRow {
    name: String,
}

pub(crate) struct DeployTarget {
    pub(crate) application_name: String,
    pub(crate) repository: Option<String>,
    /// `None` when the platform could not be asked (HMAC auth has no session
    /// routes), so the caller cannot tell and must not refuse on it.
    pub(crate) environment_exists: Option<bool>,
    pub(crate) existing_environments: Vec<String>,
}

/// Same repository, whatever the spelling: `git@github.com:o/r.git`,
/// `https://github.com/o/r`, `ssh://git@github.com/o/r.git` and a trailing
/// slash all compare equal. Case-insensitive because GitHub is.
pub(crate) fn normalize_repository(url: &str) -> String {
    let mut s = url.trim().to_string();
    if let Some(rest) = s.strip_prefix("git@") {
        // git@host:owner/repo -> https://host/owner/repo
        s = format!("https://{}", rest.replacen(':', "/", 1));
    }
    if let Some(rest) = s.strip_prefix("ssh://git@") {
        s = format!("https://{rest}");
    }
    if let Some(rest) = s.strip_prefix("git://") {
        s = format!("https://{rest}");
    }
    if let Some(rest) = s.strip_prefix("http://") {
        s = format!("https://{rest}");
    }
    let s = s.trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    s.to_lowercase()
}

/// `git remote get-url origin` for the current directory, if this is a git
/// checkout with an origin. Any failure is "unknown", never an error: a
/// deploy from a tarball or a detached directory is legitimate.
pub(crate) fn local_origin_url() -> Option<String> {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if url.is_empty() { None } else { Some(url) }
}

pub(crate) fn resolve_target(
    auth_mode: &AuthMode,
    application_id: &str,
    environment: &str,
    manifest_repository: Option<&str>,
) -> DeployTarget {
    let mut target = DeployTarget {
        application_name: application_id.to_string(),
        repository: manifest_repository.map(str::to_string),
        environment_exists: None,
        existing_environments: Vec::new(),
    };
    if auth_mode.is_hmac() {
        return target;
    }
    let base = get_platform_management_api_url();

    if let Ok(resp) =
        http_client::get_with_auth(auth_mode, &format!("{base}/applications/{application_id}"))
    {
        if resp.status().is_success() {
            if let Ok(app) = resp.json::<ApplicationSummary>() {
                if let Some(name) = app.name.filter(|n| !n.is_empty()) {
                    target.application_name = name;
                }
                if let Some(repo) = app.git_repository.filter(|r| !r.is_empty()) {
                    target.repository = Some(repo);
                }
            }
        }
    }

    if let Ok(resp) = http_client::get_with_auth(
        auth_mode,
        &format!("{base}/applications/{application_id}/environments"),
    ) {
        if resp.status().is_success() {
            if let Ok(envs) = resp.json::<EnvironmentsResponse>() {
                let mut names: Vec<String> =
                    envs.environments.into_iter().map(|e| e.name).collect();
                names.sort();
                names.dedup();
                target.environment_exists =
                    Some(names.iter().any(|n| n.eq_ignore_ascii_case(environment)));
                target.existing_environments = names;
            }
        }
    }
    target
}

/// Decide whether the deploy may proceed. Pure so it can be tested; the
/// caller supplies the local origin and whether a person can be asked.
#[derive(Debug, PartialEq)]
pub(crate) enum Gate {
    Proceed,
    /// The checkout's origin is not the application's repository.
    RepositoryMismatch {
        local: String,
        application: String,
    },
    /// The environment does not exist and nothing authorised creating it.
    EnvironmentMissing,
    /// The environment does not exist; a person is present and can be asked.
    AskToCreateEnvironment,
}

pub(crate) fn gate(
    target: &DeployTarget,
    local_origin: Option<&str>,
    create_environment: bool,
    force: bool,
    interactive: bool,
) -> Gate {
    if !force {
        if let (Some(local), Some(app)) = (local_origin, target.repository.as_deref()) {
            if normalize_repository(local) != normalize_repository(app) {
                return Gate::RepositoryMismatch {
                    local: local.to_string(),
                    application: app.to_string(),
                };
            }
        }
    }
    match target.environment_exists {
        Some(false) if !create_environment && !force => {
            if interactive {
                Gate::AskToCreateEnvironment
            } else {
                Gate::EnvironmentMissing
            }
        }
        _ => Gate::Proceed,
    }
}

/// Echo the target and enforce the gate. Returns Ok(()) when the deploy may
/// continue.
pub(crate) fn confirm_target(
    target: &DeployTarget,
    release: &str,
    environment: &str,
    region: &str,
    create_environment: bool,
    force: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    match &target.repository {
        Some(repo) => log_info!(
            stdout,
            "Deploying {} ({}) release {} -> {}/{}",
            target.application_name,
            repo,
            release,
            environment,
            region
        ),
        None => log_info!(
            stdout,
            "Deploying {} release {} -> {}/{}",
            target.application_name,
            release,
            environment,
            region
        ),
    }

    let interactive = std::io::stdin().is_terminal();
    match gate(
        target,
        local_origin_url().as_deref(),
        create_environment,
        force,
        interactive,
    ) {
        Gate::Proceed => Ok(()),
        Gate::RepositoryMismatch { local, application } => bail!(
            "This checkout's origin is not the application's repository.\n  \
             here:        {local}\n  \
             {name}: {application}\n\
             You are probably in the wrong directory. Run from the application's checkout, \
             or pass --force to deploy {name} from here anyway.",
            name = target.application_name
        ),
        Gate::EnvironmentMissing => bail!(
            "{name} has no environment named \"{environment}\" (existing: {existing}).\n\
             A deploy would create it. If that is intended, pass --create-environment; \
             if you meant another application, run from its checkout.",
            name = target.application_name,
            existing = if target.existing_environments.is_empty() {
                "none".to_string()
            } else {
                target.existing_environments.join(", ")
            }
        ),
        Gate::AskToCreateEnvironment => {
            log_warn!(
                stdout,
                "{} has no environment named \"{}\" (existing: {}).",
                target.application_name,
                environment,
                if target.existing_environments.is_empty() {
                    "none".to_string()
                } else {
                    target.existing_environments.join(", ")
                }
            );
            let yes = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!(
                    "Create environment \"{}\" on {} and deploy to it?",
                    environment, target.application_name
                ))
                .default(false)
                .interact()?;
            if !yes {
                bail!("Deploy cancelled: environment \"{environment}\" was not created.");
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(repo: Option<&str>, exists: Option<bool>) -> DeployTarget {
        DeployTarget {
            application_name: "main-street".into(),
            repository: repo.map(str::to_string),
            environment_exists: exists,
            existing_environments: vec!["demo".into()],
        }
    }

    #[test]
    fn repository_spellings_compare_equal() {
        let https = normalize_repository("https://github.com/Main-Street-App/mainstreet-hq");
        for other in [
            "git@github.com:Main-Street-App/mainstreet-hq.git",
            "ssh://git@github.com/main-street-app/mainstreet-hq.git",
            "https://github.com/Main-Street-App/mainstreet-hq.git/",
            "http://github.com/Main-Street-App/mainstreet-hq",
        ] {
            assert_eq!(normalize_repository(other), https, "{other}");
        }
        assert_ne!(
            normalize_repository("git@github.com:Main-Street-App/mainstreet-locale.git"),
            https
        );
    }

    #[test]
    fn the_wrong_checkout_is_refused_before_anything_else() {
        // The Main Street case: deploy run from the locale repo against the hq app.
        let t = target(
            Some("https://github.com/Main-Street-App/mainstreet-hq"),
            Some(false),
        );
        let g = gate(
            &t,
            Some("git@github.com:Main-Street-App/mainstreet-locale.git"),
            false,
            false,
            true,
        );
        assert!(matches!(g, Gate::RepositoryMismatch { .. }));
        // --force deploys anyway (and skips the environment question too).
        assert_eq!(
            gate(
                &t,
                Some("git@github.com:Main-Street-App/mainstreet-locale.git"),
                false,
                true,
                true
            ),
            Gate::Proceed
        );
    }

    #[test]
    fn a_missing_environment_needs_a_flag_or_a_person() {
        let t = target(Some("https://github.com/o/r"), Some(false));
        assert_eq!(
            gate(&t, Some("https://github.com/o/r"), false, false, false),
            Gate::EnvironmentMissing
        );
        assert_eq!(
            gate(&t, Some("https://github.com/o/r"), false, false, true),
            Gate::AskToCreateEnvironment
        );
        assert_eq!(
            gate(&t, Some("https://github.com/o/r"), true, false, false),
            Gate::Proceed
        );
    }

    #[test]
    fn unknowns_never_block() {
        // No origin (tarball), no repo on the app, or HMAC (environments unknown).
        assert_eq!(
            gate(
                &target(Some("https://github.com/o/r"), Some(true)),
                None,
                false,
                false,
                false
            ),
            Gate::Proceed
        );
        assert_eq!(
            gate(
                &target(None, Some(true)),
                Some("https://github.com/o/r"),
                false,
                false,
                false
            ),
            Gate::Proceed
        );
        assert_eq!(
            gate(
                &target(Some("https://github.com/o/r"), None),
                Some("https://github.com/o/r"),
                false,
                false,
                false
            ),
            Gate::Proceed
        );
    }
}

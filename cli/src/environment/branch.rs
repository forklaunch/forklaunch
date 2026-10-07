use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use reqwest::Method;
use serde::Deserialize;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client::{self, make_authenticated_request},
        validate::{require_auth, require_integration, require_manifest},
    },
};

/// `environment branch` — the branch deployment matrix, which until now could
/// only be edited by clicking in the dashboard.
///
/// Precedence the platform applies (see forklaunch-platform #705/#878):
/// this tracked branch > the repository-level branchMapping > the suggested
/// default (`main` for production-named environments, else the env name).
#[derive(Debug)]
pub(crate) struct BranchCommand {
    set: SetCommand,
    clear: ClearCommand,
    list: ListCommand,
}

impl BranchCommand {
    pub(crate) fn new() -> Self {
        Self {
            set: SetCommand::new(),
            clear: ClearCommand::new(),
            list: ListCommand::new(),
        }
    }
}

impl CliCommand for BranchCommand {
    fn command(&self) -> Command {
        command(
            "branch",
            "Show or set the git branch each environment autodeploys from",
        )
        .subcommand(self.set.command())
        .subcommand(self.clear.command())
        .subcommand(self.list.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("set", sub_matches)) => self.set.handler(sub_matches),
            Some(("clear", sub_matches)) => self.clear.handler(sub_matches),
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            _ => self.list.handler(matches),
        }
    }
}

fn base_path_arg() -> Arg {
    Arg::new("base_path")
        .short('p')
        .long("path")
        .help("Path to application root (optional)")
}

fn environment_arg() -> Arg {
    Arg::new("environment")
        .long("environment")
        .short('e')
        .required(true)
        .help("Environment name")
}

fn patch_branch(matches: &ArgMatches, branch: Option<&str>) -> Result<()> {
    let _token = require_auth()?;
    let (_app_root, manifest) = require_manifest(matches)?;
    let application_id = require_integration(&manifest)?;
    let environment = matches
        .get_one::<String>("environment")
        .context("environment is required")?;

    let url = format!(
        "{}/applications/{}/environments/{}/cicd-branch",
        get_platform_management_api_url(),
        urlencoding::encode(&application_id),
        urlencoding::encode(environment)
    );
    // An omitted cicdBranch clears the override; a present one sets it.
    let body = match branch {
        Some(b) => serde_json::json!({ "cicdBranch": b }),
        None => serde_json::json!({}),
    };
    let response = make_authenticated_request(Method::PATCH, &url, Some(body))
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

    let status = response.status();
    if !status.is_success() {
        bail!(
            "Failed to update tracked branch ({}): {}",
            status,
            response.text().unwrap_or_default()
        );
    }

    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
    write!(stdout, "  Updated")?;
    stdout.reset()?;
    match branch {
        Some(b) => writeln!(stdout, "  {} now autodeploys from \"{}\"", environment, b)?,
        None => writeln!(
            stdout,
            "  {} now uses the suggested default branch",
            environment
        )?,
    }
    Ok(())
}

#[derive(Debug)]
struct SetCommand;
impl SetCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for SetCommand {
    fn command(&self) -> Command {
        command("set", "Set the branch an environment autodeploys from")
            .arg(environment_arg())
            .arg(
                Arg::new("branch")
                    .long("branch")
                    .short('b')
                    .required(true)
                    .help("Git branch name (e.g. main)"),
            )
            .arg(base_path_arg())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let branch = matches
            .get_one::<String>("branch")
            .context("branch is required")?;
        if branch.trim().is_empty() {
            bail!(
                "Branch cannot be empty. Use `environment branch clear` to fall back to the default."
            );
        }
        patch_branch(matches, Some(branch.trim()))
    }
}

#[derive(Debug)]
struct ClearCommand;
impl ClearCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for ClearCommand {
    fn command(&self) -> Command {
        command(
            "clear",
            "Clear an environment's tracked branch, falling back to the default",
        )
        .arg(environment_arg())
        .arg(base_path_arg())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        patch_branch(matches, None)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentRow {
    name: String,
    #[serde(default)]
    cicd_branch: Option<String>,
    #[serde(default)]
    region: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EnvironmentsResponse {
    environments: Vec<EnvironmentRow>,
}

/// The suggested default, mirroring `defaultCicdBranch` on the server.
fn suggested_default(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower == "production" || lower == "prod" {
        "main".to_string()
    } else {
        name.to_string()
    }
}

#[derive(Debug)]
struct ListCommand;
impl ListCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for ListCommand {
    fn command(&self) -> Command {
        command("list", "Show the branch each environment autodeploys from").arg(base_path_arg())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;

        let url = format!(
            "{}/applications/{}/environments",
            get_platform_management_api_url(),
            urlencoding::encode(&application_id)
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to list environments: {}",
                response.text().unwrap_or_default()
            );
        }
        let parsed: EnvironmentsResponse = response
            .json()
            .with_context(|| "Failed to parse environments response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        let mut seen: Vec<String> = Vec::new();
        for env in &parsed.environments {
            if seen.contains(&env.name) {
                continue;
            }
            seen.push(env.name.clone());
            let tracked = env.cicd_branch.as_deref().filter(|b| !b.trim().is_empty());
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
            write!(stdout, "  {:<16}", env.name)?;
            stdout.reset()?;
            match tracked {
                Some(b) => writeln!(stdout, "{}", b)?,
                None => writeln!(stdout, "{}  (default)", suggested_default(&env.name))?,
            }
            if let Some(region) = &env.region {
                writeln!(stdout, "  {:<16}{}", "", region)?;
            }
        }
        writeln!(stdout)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_definition_is_valid() {
        BranchCommand::new()
            .command()
            .version("0.0.0-test")
            .debug_assert();
    }

    #[test]
    fn production_defaults_to_main_everything_else_to_itself() {
        assert_eq!(suggested_default("production"), "main");
        assert_eq!(suggested_default("Prod"), "main");
        assert_eq!(suggested_default("staging"), "staging");
        assert_eq!(suggested_default("burlingame-locale"), "burlingame-locale");
    }

    #[test]
    fn set_requires_a_branch_and_clear_does_not() {
        let set = SetCommand::new().command().version("0.0.0-test");
        assert!(
            set.clone()
                .try_get_matches_from(["set", "-e", "prod"])
                .is_err()
        );
        assert!(
            set.try_get_matches_from(["set", "-e", "prod", "-b", "main"])
                .is_ok()
        );
        let clear = ClearCommand::new().command().version("0.0.0-test");
        assert!(clear.try_get_matches_from(["clear", "-e", "prod"]).is_ok());
    }
}

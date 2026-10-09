//! `forklaunch org pool remove-empty` — tear down an org pool nothing runs on.
//!
//! An org compute pool keeps costing money (hosts, load balancers, a
//! monitoring stack, and possibly shared Postgres/Redis) after the last
//! application leaves it. This asks the platform to remove it. The platform
//! refuses with 409 and the list of components still placed there if the
//! pool is not empty. The pool's shared Redis always goes; its shared
//! Postgres goes only if every app that used the pool was destroyed with its
//! data (`auto`), never with `--keep-data`, and regardless with
//! `--delete-data` (which needs the stronger confirmation).

use std::io::{IsTerminal, Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Input, theme::ColorfulTheme};
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

#[derive(Debug)]
pub(super) struct RemoveEmptyCommand;

impl RemoveEmptyCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

/// 200 response of `POST /compute-pools/remove-empty`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoveEmptyResponse {
    deployment_id: String,
    message: String,
    data_action: String,
}

/// 409 response: the pool still has components on it.
#[derive(Debug, Deserialize, Serialize)]
struct PoolNotEmpty {
    message: String,
    #[serde(default)]
    components: Vec<PlacedComponent>,
}

#[derive(Debug, Deserialize, Serialize)]
struct PlacedComponent {
    application: String,
    component: String,
    #[serde(rename = "type")]
    kind: String,
}

/// What happens to the pool's shared Postgres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataAction {
    /// Delete it only if no app kept its data (the platform decides).
    Auto,
    /// Keep it.
    Keep,
    /// Delete it even if an app kept data.
    Delete,
}

impl DataAction {
    fn as_str(self) -> &'static str {
        match self {
            DataAction::Auto => "auto",
            DataAction::Keep => "keep",
            DataAction::Delete => "delete",
        }
    }
}

fn confirm_token(environment: &str, region: &str, data_action: DataAction) -> String {
    match data_action {
        DataAction::Delete => format!("{}:{}:delete-data", environment, region),
        _ => format!("{}:{}", environment, region),
    }
}

fn build_remove_body(
    environment: &str,
    region: &str,
    data_action: DataAction,
) -> serde_json::Value {
    serde_json::json!({
        "environment": environment,
        "region": region,
        "dataAction": data_action.as_str(),
        "confirm": confirm_token(environment, region, data_action),
    })
}

fn confirmation_prompt(environment: &str, region: &str, data_action: DataAction) -> String {
    let data = match data_action {
        DataAction::Auto => {
            " and shared Redis, and its shared Postgres unless an app kept its data there"
        }
        DataAction::Keep => " and shared Redis, keeping its shared Postgres",
        DataAction::Delete => {
            " and shared Redis AND its shared Postgres, including data apps kept there"
        }
    };
    let expected = match data_action {
        DataAction::Delete => "the region followed by :delete-data",
        _ => "the region",
    };
    format!(
        "Remove the {}/{} org pool? This deletes its hosts, load balancers, monitoring{}. Type {} to confirm",
        environment, region, data, expected
    )
}

fn expected_typed(region: &str, data_action: DataAction) -> String {
    match data_action {
        DataAction::Delete => format!("{}:delete-data", region),
        _ => region.to_string(),
    }
}

fn format_not_empty(body: &PoolNotEmpty) -> String {
    let mut out = body.message.trim().to_string();
    for c in &body.components {
        out.push_str(&format!(
            "\n  {} / {} ({})",
            c.application, c.component, c.kind
        ));
    }
    out
}

fn data_action_text(data_action: &str) -> &'static str {
    match data_action {
        "deleted" => "Shared Postgres: deleted",
        "kept" => "Shared Postgres: kept",
        _ => "Shared Postgres: unchanged",
    }
}

impl CliCommand for RemoveEmptyCommand {
    fn command(&self) -> Command {
        command(
            "remove-empty",
            "Tear down an org compute pool that no application runs on",
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment of the pool"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("Region of the pool"),
        )
        .arg(
            Arg::new("keep_data")
                .long("keep-data")
                .action(ArgAction::SetTrue)
                .conflicts_with("delete_data")
                .help(
                    "Keep the pool's shared Postgres (default: delete it only if every app \
                     that used the pool was destroyed with its data)",
                ),
        )
        .arg(
            Arg::new("delete_data")
                .long("delete-data")
                .action(ArgAction::SetTrue)
                .help(
                    "Delete the pool's shared Postgres even if an app kept its data there \
                     (asks you to type <region>:delete-data)",
                ),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .action(ArgAction::SetTrue)
                .help("Skip the confirmation prompt (required when not on a terminal)"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Output raw JSON instead of formatted terminal output"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let environment = matches
            .get_one::<String>("environment")
            .context("--environment is required")?;
        let region = matches
            .get_one::<String>("region")
            .context("--region is required")?;
        let data_action = if matches.get_flag("delete_data") {
            DataAction::Delete
        } else if matches.get_flag("keep_data") {
            DataAction::Keep
        } else {
            DataAction::Auto
        };
        let json_output = matches.get_flag("json");

        if !matches.get_flag("yes") {
            if !std::io::stdin().is_terminal() {
                bail!(
                    "refusing to remove the {}/{} org pool without confirmation — stdin is not a terminal, so re-run with --yes",
                    environment,
                    region
                );
            }
            let typed: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(confirmation_prompt(environment, region, data_action))
                .allow_empty(true)
                .interact_text()?;
            if typed.trim() != expected_typed(region, data_action) {
                bail!("aborted — the confirmation did not match, nothing was removed");
            }
        }

        let response = http_client::post(
            &format!(
                "{}/compute-pools/remove-empty",
                get_platform_management_api_url()
            ),
            build_remove_body(environment, region, data_action),
        )
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let status = response.status();
        if status.as_u16() == 409 {
            let text = response.text().unwrap_or_default();
            match serde_json::from_str::<PoolNotEmpty>(&text) {
                Ok(body) => bail!("{}", format_not_empty(&body)),
                Err(_) => bail!("{}", text.trim()),
            }
        }
        if !status.is_success() {
            let detail = response.text().unwrap_or_default();
            if matches!(status.as_u16(), 400 | 403 | 404) && !detail.trim().is_empty() {
                bail!("{}", detail.trim());
            }
            bail!("Failed to remove the org pool ({}): {}", status, detail);
        }

        let result: RemoveEmptyResponse = response
            .json()
            .with_context(|| "Failed to parse remove-empty response")?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        log_ok!(stdout, "{}", result.message);
        writeln!(stdout, "  {}", data_action_text(&result.data_action))?;
        writeln!(
            stdout,
            "  Removal {} runs in the background and usually takes 10-20 minutes.",
            result.deployment_id
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        RemoveEmptyCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn environment_and_region_are_required() {
        assert!(cmd().try_get_matches_from(["remove-empty"]).is_err());
        assert!(
            cmd()
                .try_get_matches_from(["remove-empty", "-e", "prod"])
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(["remove-empty", "-r", "us-east-1"])
                .is_err()
        );
        let m = cmd()
            .try_get_matches_from(["remove-empty", "-e", "prod", "-r", "us-east-1"])
            .unwrap();
        assert!(!m.get_flag("keep_data"));
        assert!(!m.get_flag("delete_data"));
        assert!(!m.get_flag("yes"));
    }

    #[test]
    fn flags_parse() {
        let m = cmd()
            .try_get_matches_from([
                "remove-empty",
                "-e",
                "prod",
                "-r",
                "us-east-1",
                "--keep-data",
                "--yes",
                "--json",
            ])
            .unwrap();
        assert!(m.get_flag("keep_data") && m.get_flag("yes") && m.get_flag("json"));
    }

    #[test]
    fn keep_and_delete_data_are_mutually_exclusive() {
        assert!(
            cmd()
                .try_get_matches_from([
                    "remove-empty",
                    "-e",
                    "prod",
                    "-r",
                    "us-east-1",
                    "--keep-data",
                    "--delete-data",
                ])
                .is_err()
        );
    }

    #[test]
    fn body_carries_data_action_and_matching_confirm_token() {
        assert_eq!(
            build_remove_body("prod", "us-east-1", DataAction::Auto),
            serde_json::json!({
                "environment": "prod",
                "region": "us-east-1",
                "dataAction": "auto",
                "confirm": "prod:us-east-1",
            })
        );
        assert_eq!(
            build_remove_body("dev", "eu-west-1", DataAction::Keep)["dataAction"],
            "keep"
        );
        let delete = build_remove_body("dev", "eu-west-1", DataAction::Delete);
        assert_eq!(delete["dataAction"], "delete");
        assert_eq!(delete["confirm"], "dev:eu-west-1:delete-data");
    }

    #[test]
    fn prompt_and_typed_confirmation_follow_the_data_action() {
        assert!(
            confirmation_prompt("prod", "us-east-1", DataAction::Keep)
                .contains("keeping its shared Postgres")
        );
        assert!(
            confirmation_prompt("prod", "us-east-1", DataAction::Delete)
                .contains("including data apps kept there")
        );
        assert_eq!(expected_typed("us-east-1", DataAction::Auto), "us-east-1");
        assert_eq!(
            expected_typed("us-east-1", DataAction::Delete),
            "us-east-1:delete-data"
        );
    }

    #[test]
    fn success_response_deserializes() {
        let r: RemoveEmptyResponse = serde_json::from_str(
            r#"{"deploymentId":"dep-1","message":"Removing pool","dataAction":"kept"}"#,
        )
        .unwrap();
        assert_eq!(r.deployment_id, "dep-1");
        assert_eq!(data_action_text(&r.data_action), "Shared Postgres: kept");
        assert_eq!(data_action_text("deleted"), "Shared Postgres: deleted");
    }

    #[test]
    fn not_empty_response_lists_components() {
        let body: PoolNotEmpty = serde_json::from_str(
            r#"{"message":"The pool is not empty.","components":[
                {"application":"billing","component":"billing-service","type":"service"},
                {"application":"billing","component":"billing-worker","type":"worker"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            format_not_empty(&body),
            "The pool is not empty.\n  billing / billing-service (service)\n  billing / billing-worker (worker)"
        );

        let bare: PoolNotEmpty = serde_json::from_str(r#"{"message":"Not empty"}"#).unwrap();
        assert!(bare.components.is_empty());
        assert_eq!(format_not_empty(&bare), "Not empty");
    }
}

//! `forklaunch org pool remove-empty` — tear down an org pool nothing runs on.
//!
//! An org compute pool keeps costing money (hosts, load balancers, a
//! monitoring stack, and possibly shared Postgres/Redis) after the last
//! application leaves it. This asks the platform to remove it. The platform
//! refuses with 409 and the list of components still placed there if the
//! pool is not empty, and decides itself whether the shared databases are
//! safe to delete; `--keep-data` keeps them regardless.

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

fn build_remove_body(environment: &str, region: &str, keep_data: bool) -> serde_json::Value {
    serde_json::json!({
        "environment": environment,
        "region": region,
        "keepData": keep_data,
        "confirm": format!("{}:{}", environment, region),
    })
}

fn confirmation_prompt(environment: &str, region: &str, keep_data: bool) -> String {
    format!(
        "Remove the {}/{} org pool? This deletes its hosts, load balancers and monitoring{}. Type the region to confirm",
        environment,
        region,
        if keep_data {
            ""
        } else {
            " and its shared databases"
        }
    )
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
        "deleted" => "Shared databases: deleted",
        "kept" => "Shared databases: kept",
        _ => "Shared databases: unchanged",
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
                .help(
                    "Keep the pool's shared Postgres/Redis (default: delete them when the \
                     platform says it is safe)",
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
        let keep_data = matches.get_flag("keep_data");
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
                .with_prompt(confirmation_prompt(environment, region, keep_data))
                .allow_empty(true)
                .interact_text()?;
            if typed.trim() != region {
                bail!("aborted — the region did not match, nothing was removed");
            }
        }

        let response = http_client::post(
            &format!(
                "{}/compute-pools/remove-empty",
                get_platform_management_api_url()
            ),
            build_remove_body(environment, region, keep_data),
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
            "  Follow it: forklaunch deploy info --deployment {}",
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
    fn body_carries_confirm_token_and_keep_data() {
        assert_eq!(
            build_remove_body("prod", "us-east-1", false),
            serde_json::json!({
                "environment": "prod",
                "region": "us-east-1",
                "keepData": false,
                "confirm": "prod:us-east-1",
            })
        );
        assert_eq!(
            build_remove_body("dev", "eu-west-1", true)["keepData"],
            true
        );
    }

    #[test]
    fn prompt_mentions_databases_only_when_deleting_them() {
        let deleting = confirmation_prompt("prod", "us-east-1", false);
        assert_eq!(
            deleting,
            "Remove the prod/us-east-1 org pool? This deletes its hosts, load balancers and monitoring and its shared databases. Type the region to confirm"
        );
        let keeping = confirmation_prompt("prod", "us-east-1", true);
        assert!(!keeping.contains("databases"), "{keeping}");
        assert!(keeping.ends_with("monitoring. Type the region to confirm"));
    }

    #[test]
    fn success_response_deserializes() {
        let r: RemoveEmptyResponse = serde_json::from_str(
            r#"{"deploymentId":"dep-1","message":"Removing pool","dataAction":"kept"}"#,
        )
        .unwrap();
        assert_eq!(r.deployment_id, "dep-1");
        assert_eq!(data_action_text(&r.data_action), "Shared databases: kept");
        assert_eq!(data_action_text("deleted"), "Shared databases: deleted");
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

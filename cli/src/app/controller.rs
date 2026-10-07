use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{ColorChoice, StandardStream};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, http_client, validate::require_auth},
};

/// Get-by-id only — same caveat as `fl app route`: no "list all controllers
/// for an app" endpoint exists yet.
#[derive(Debug)]
pub(crate) struct ControllerCommand;

impl ControllerCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ControllerCommand {
    fn command(&self) -> Command {
        command("controller", "Get controller details by id, including its routes")
            .arg(Arg::new("id").required(true).help("The controller id"))
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let id = matches
            .get_one::<String>("id")
            .context("controller id is required")?;

        let url = format!(
            "{}/controllers/{}",
            get_platform_management_api_url(),
            urlencoding::encode(id)
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if response.status().as_u16() == 404 {
            bail!("Controller '{}' not found.", id);
        }
        if !response.status().is_success() {
            bail!(
                "Failed to get controller: {}",
                response.text().unwrap_or_default()
            );
        }

        let controller: ControllerDetail = response
            .json()
            .with_context(|| "Failed to parse controller response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        writeln!(stdout, "  {}", controller.name)?;
        writeln!(stdout, "  id:      {}", controller.id)?;
        writeln!(
            stdout,
            "  service: {}",
            controller.service_name.as_deref().unwrap_or("-")
        )?;
        if let Some(d) = &controller.description {
            writeln!(stdout, "  desc:    {}", d)?;
        }
        if !controller.routes.is_empty() {
            writeln!(stdout)?;
            writeln!(stdout, "  routes:")?;
            for r in &controller.routes {
                writeln!(stdout, "    {:<7} {}", r.method, r.path)?;
            }
        }
        writeln!(stdout)?;

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct RouteSummary {
    #[serde(default)]
    id: Option<String>,
    method: String,
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ControllerDetail {
    id: String,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    service_id: Option<String>,
    #[serde(default)]
    service_name: Option<String>,
    #[serde(default)]
    routes: Vec<RouteSummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controller_cmd() -> Command {
        ControllerCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        controller_cmd().debug_assert();
    }

    #[test]
    fn requires_id() {
        assert!(controller_cmd().try_get_matches_from(["controller"]).is_err());
        assert!(
            controller_cmd()
                .try_get_matches_from(["controller", "ctrl-1"])
                .is_ok()
        );
    }

    #[test]
    fn controller_detail_deserializes_with_routes() {
        let json = r#"{
            "id": "c1",
            "name": "OrdersController",
            "serviceId": "s1",
            "serviceName": "orders",
            "routes": [{"id": "r1", "method": "GET", "path": "/orders"}]
        }"#;
        let controller: ControllerDetail = serde_json::from_str(json).unwrap();
        assert_eq!(controller.name, "OrdersController");
        assert_eq!(controller.routes.len(), 1);
    }
}

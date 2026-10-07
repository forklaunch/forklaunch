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

/// Get-by-id only — there's no "list all routes for an app" endpoint on the
/// platform today, so this needs a route id up front (e.g. from
/// `fl app services` -> a service's controllers, or the dashboard).
#[derive(Debug)]
pub(crate) struct RouteCommand;

impl RouteCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for RouteCommand {
    fn command(&self) -> Command {
        command("route", "Get route details by id, including its controller and service")
            .arg(Arg::new("id").required(true).help("The route id"))
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let id = matches.get_one::<String>("id").context("route id is required")?;

        let url = format!(
            "{}/routes/{}",
            get_platform_management_api_url(),
            urlencoding::encode(id)
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if response.status().as_u16() == 404 {
            bail!("Route '{}' not found.", id);
        }
        if !response.status().is_success() {
            bail!(
                "Failed to get route: {}",
                response.text().unwrap_or_default()
            );
        }

        let route: RouteDetail = response
            .json()
            .with_context(|| "Failed to parse route response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        writeln!(stdout, "  {} {}", route.method, route.path)?;
        writeln!(stdout, "  id:          {}", route.id)?;
        writeln!(stdout, "  controller:  {}", route.controller_name.as_deref().unwrap_or(&route.controller_id))?;
        writeln!(stdout, "  service:     {}", route.service_name.as_deref().unwrap_or(&route.service_id))?;
        if let Some(d) = &route.description {
            writeln!(stdout, "  description: {}", d)?;
        }
        writeln!(stdout)?;

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteDetail {
    id: String,
    path: String,
    method: String,
    #[serde(default)]
    description: Option<String>,
    controller_id: String,
    #[serde(default)]
    controller_name: Option<String>,
    service_id: String,
    #[serde(default)]
    service_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route_cmd() -> Command {
        RouteCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        route_cmd().debug_assert();
    }

    #[test]
    fn requires_id() {
        assert!(route_cmd().try_get_matches_from(["route"]).is_err());
        assert!(route_cmd().try_get_matches_from(["route", "route-1"]).is_ok());
    }

    #[test]
    fn route_detail_deserializes() {
        let json = r#"{
            "id": "r1",
            "path": "/orders",
            "method": "GET",
            "controllerId": "c1",
            "controllerName": "OrdersController",
            "serviceId": "s1",
            "serviceName": "orders"
        }"#;
        let route: RouteDetail = serde_json::from_str(json).unwrap();
        assert_eq!(route.method, "GET");
        assert_eq!(route.controller_name.as_deref(), Some("OrdersController"));
    }
}

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_developer_tools_api_url},
    core::{command::command, http_client, validate::require_auth},
};

/// Fetches a *customer's own deployed service's* OpenAPI spec (e.g. their iam or
/// orders service), as captured in the release manifest — not this CLI's local
/// `openapi export`, and not ForkLaunch's own control-plane spec. Proxied through
/// `developer-tools`.
#[derive(Debug)]
pub(crate) struct FetchCommand;

impl FetchCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for FetchCommand {
    fn command(&self) -> Command {
        command(
            "fetch",
            "Fetch the OpenAPI spec for one of your own deployed services",
        )
        .arg(
            Arg::new("service")
                .long("service")
                .required(true)
                .help("The deployed service's id (not its name) — see `fl app services --json`"),
        )
        .arg(
            Arg::new("route_version")
                .long("route-version")
                .help("Route version (defaults to latest)"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("Write the spec to this file instead of stdout"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let service_id = matches
            .get_one::<String>("service")
            .context("--service is required")?;
        let version = matches.get_one::<String>("route_version");
        let output = matches.get_one::<String>("output");

        let mut url = format!(
            "{}/services/{}/openapi",
            get_developer_tools_api_url(),
            urlencoding::encode(service_id)
        );
        if let Some(v) = version {
            url.push_str(&format!("?version={}", urlencoding::encode(v)));
        }

        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if response.status().as_u16() == 404 {
            bail!(
                "Service '{}' not found, or has no OpenAPI spec in its release manifest. \
                 --service takes the service's id, not its name — run `fl app services --json` \
                 to look up the id.",
                service_id
            );
        }
        if !response.status().is_success() {
            bail!(
                "Failed to fetch OpenAPI spec: {}",
                response.text().unwrap_or_default()
            );
        }

        let body: serde_json::Value =
            response.json().with_context(|| "Failed to parse OpenAPI spec response")?;
        let spec = body.get("spec").unwrap_or(&body);
        let pretty = serde_json::to_string_pretty(spec)?;

        if let Some(path) = output {
            std::fs::write(path, &pretty)
                .with_context(|| format!("Failed to write spec to {}", path))?;
            println!("OpenAPI spec written to {}", path);
        } else {
            println!("{}", pretty);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fetch_cmd() -> Command {
        FetchCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        fetch_cmd().debug_assert();
    }

    #[test]
    fn requires_service() {
        assert!(fetch_cmd().try_get_matches_from(["fetch"]).is_err());
        assert!(
            fetch_cmd()
                .try_get_matches_from(["fetch", "--service", "svc-1"])
                .is_ok()
        );
    }
}

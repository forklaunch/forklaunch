use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use reqwest::{Method, blocking::Client, redirect::Policy};

use crate::{
    CliCommand,
    constants::get_platform_management_api_url,
    core::{
        command::command,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct WebsiteCommand;
impl WebsiteCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}
impl CliCommand for WebsiteCommand {
    fn command(&self) -> Command {
        command("website", "Read website hosting or enable the platform website address; does not deploy")
            .arg(Arg::new("environment").long("environment").required(true))
            .arg(Arg::new("region").long("region").required(true))
            .arg(Arg::new("enable").long("enable").action(ArgAction::SetTrue).help("Save separate website hosting for this environment; deployment is a separate step"))
            .arg(Arg::new("base_path").long("path").short('p'))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let token = require_auth()?;
        let (_, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;
        let environment = matches
            .get_one::<String>("environment")
            .context("Environment required")?;
        let region = matches
            .get_one::<String>("region")
            .context("Region required")?;
        let enable = matches.get_flag("enable");
        let mut url = format!(
            "{}/applications/{}/website",
            get_platform_management_api_url(),
            urlencoding::encode(&app)
        );
        if !enable {
            url.push_str(&format!(
                "?environment={}&region={}",
                urlencoding::encode(environment),
                urlencoding::encode(region)
            ));
        }
        // Do not follow a credential-bearing redirect or retry an ambiguous write.
        let client = Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(30))
            .build()?;
        let request = client
            .request(if enable { Method::PUT } else { Method::GET }, url)
            .bearer_auth(token);
        let request = if enable {
            request.json(&serde_json::json!({"environment":environment,"region":region}))
        } else {
            request
        };
        let response = request
            .send()
            .context("Website request failed; read current settings before retrying a write")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            let detail = if body.contains("Cannot GET /applications/") {
                "website route is unavailable on this server"
            } else if body.trim().trim_matches('"') == "Application not found" {
                "application is not visible to this website endpoint"
            } else {
                "read current settings before retrying"
            };
            bail!("Website request returned {status}: {detail}");
        }
        let body: serde_json::Value = response.json().context("Invalid website response")?;
        println!("{}", serde_json::to_string_pretty(&body)?);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn website_is_read_only_unless_explicitly_enabled() {
        let command = WebsiteCommand::new();
        let read = command
            .command()
            .version("0.0.0")
            .try_get_matches_from(["website", "--environment", "qa", "--region", "us-west-2"])
            .unwrap();
        assert!(!read.get_flag("enable"));
        let write = command
            .command()
            .version("0.0.0")
            .try_get_matches_from([
                "website",
                "--environment",
                "qa",
                "--region",
                "us-west-2",
                "--enable",
            ])
            .unwrap();
        assert!(write.get_flag("enable"));
    }
    #[test]
    fn environment_and_region_are_mandatory() {
        assert!(
            WebsiteCommand::new()
                .command()
                .version("0.0.0")
                .try_get_matches_from(["website", "--enable"])
                .is_err()
        );
        assert!(
            WebsiteCommand::new()
                .command()
                .version("0.0.0")
                .try_get_matches_from(["website", "--environment", "qa"])
                .is_err()
        );
    }
}

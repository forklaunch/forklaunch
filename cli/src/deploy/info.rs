use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::Deserialize;
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentSummary {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    release_version: Option<String>,
    #[serde(default)]
    deployed_by: Option<String>,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    completed_at: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeploymentListResponse {
    #[serde(default)]
    deployments: Vec<DeploymentSummary>,
}

fn print_field(out: &mut StandardStream, label: &str, value: &Option<String>) -> Result<()> {
    if let Some(v) = value {
        out.set_color(ColorSpec::new().set_bold(true))?;
        write!(out, "  {:<12}", label)?;
        out.reset()?;
        writeln!(out, "{}", v)?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct InfoCommand;

impl InfoCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for InfoCommand {
    fn command(&self) -> Command {
        command(
            "info",
            "Show deployment status (latest per environment/region, or one by id)",
        )
        .arg(
            Arg::new("deployment")
                .long("deployment")
                .short('d')
                .help("Deployment id to show"),
        )
        .arg(
            Arg::new("environment")
                .long("environment")
                .short('e')
                .help("Filter to an environment"),
        )
        .arg(
            Arg::new("region")
                .long("region")
                .short('r')
                .help("Filter to a region"),
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

        let mut url = format!(
            "{}/deployments/?applicationId={}&limit=25",
            get_platform_management_api_url(),
            app
        );
        if let Some(environment) = matches.get_one::<String>("environment") {
            url.push_str(&format!("&environment={}", environment));
        }
        if let Some(region) = matches.get_one::<String>("region") {
            url.push_str(&format!("&region={}", region));
        }

        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to list deployments: {}",
                response.text().unwrap_or_default()
            );
        }
        let list: DeploymentListResponse = response
            .json()
            .with_context(|| "Failed to parse deployment list response")?;

        let deployments: Vec<&DeploymentSummary> =
            match matches.get_one::<String>("deployment") {
                Some(id) => {
                    let Some(found) = list
                        .deployments
                        .iter()
                        .find(|d| d.id.as_deref() == Some(id))
                    else {
                        bail!("Deployment '{}' not found in the latest 25.", id);
                    };
                    vec![found]
                }
                None => list.deployments.iter().take(5).collect(),
            };

        if deployments.is_empty() {
            log_info!(stdout, "No deployments found for the given filters.");
            return Ok(());
        }

        for deployment in deployments {
            writeln!(stdout)?;
            print_field(&mut stdout, "Id:", &deployment.id)?;
            print_field(&mut stdout, "Status:", &deployment.status)?;
            print_field(&mut stdout, "Env:", &deployment.environment)?;
            print_field(&mut stdout, "Region:", &deployment.region)?;
            print_field(&mut stdout, "Release:", &deployment.release_version)?;
            print_field(&mut stdout, "By:", &deployment.deployed_by)?;
            print_field(&mut stdout, "Started:", &deployment.started_at)?;
            print_field(&mut stdout, "Completed:", &deployment.completed_at)?;
            print_field(&mut stdout, "Error:", &deployment.error_message)?;
        }

        Ok(())
    }
}

use std::fs::write;
use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::CliCommand;
use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, error_failed_to_write_file, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct PullCommand;

impl PullCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for PullCommand {
    fn command(&self) -> Command {
        command(
            "pull",
            "Pull environment configuration from the forklaunch platform",
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
                .required(false)
                .help("Filter to a specific service name"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .required(false)
                .help("Output file path (defaults to <environment>.env)"),
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

        let region = matches
            .get_one::<String>("region")
            .expect("region is required");
        let environment = matches
            .get_one::<String>("environment")
            .expect("environment is required");
        let service = matches.get_one::<String>("service");

        let output = format!("{}.env", environment);
        let output = matches.get_one::<String>("output").unwrap_or(&output);

        let mut url = format!(
            "{}/config/pull?applicationId={}&region={}&environment={}",
            get_platform_management_api_url(),
            app,
            region,
            environment
        );

        if let Some(svc) = service {
            url.push_str(&format!("&service={}", svc));
        }

        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        match response.status() {
            reqwest::StatusCode::OK => {
                let content = response.text()?;
                write(output, &content)
                    .with_context(|| error_failed_to_write_file(std::path::Path::new(output)))?;
                log_ok!(stdout, "[OK] Config pulled to {}", output);
            }
            _ => {
                let err_text = response.text()?;
                log_error!(stdout, "[ERROR] Failed to pull config: {}", err_text);
                anyhow::bail!("Failed to pull config: {}", err_text);
            }
        }

        Ok(())
    }
}

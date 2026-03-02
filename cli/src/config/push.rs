use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::CliCommand;
use crate::{
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{command::command, env::{parse_env_file_items, EnvFileItem}},
};

#[derive(Debug)]
pub(crate) struct PushCommand;

impl PushCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for PushCommand {
    fn command(&self) -> Command {
        command(
            "push",
            "Push environment configuration to the forklaunch platform",
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
            Arg::new("input")
                .short('i')
                .long("input")
                .required(false)
                .help("Input file path (defaults to <environment>.env)"),
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = crate::core::validate::require_auth()?;
        let (_app_root, manifest) = crate::core::validate::require_manifest(matches)?;
        let app = crate::core::validate::require_integration(&manifest)?;

        use crate::core::http_client;

        let region = matches
            .get_one::<String>("region")
            .expect("region is required");
        let environment = matches
            .get_one::<String>("environment")
            .expect("environment is required");

        let input = format!("{}.env", environment);
        let input = matches.get_one::<String>("input").unwrap_or(&input);

        let url = format!(
            "{}/config/push",
            get_platform_management_api_url()
        );

        let items = parse_env_file_items(std::path::Path::new(input))
            .with_context(|| format!("Failed to parse file {}. Please check file permissions.", input))?;

        let content = items
            .into_iter()
            .map(|item| match item {
                EnvFileItem::SectionHeader(line) => format!("{}\n", line),
                EnvFileItem::KeyValue(key, value) => {
                    if value.contains('\n') {
                        format!("{}=\"{}\"\n", key, value)
                    } else {
                        format!("{}={}\n", key, value)
                    }
                }
            })
            .collect::<String>();

        let body = serde_json::json!({
            "applicationId": app,
            "region": region,
            "environment": environment,
            "content": content
        });

        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        match response.status() {
            reqwest::StatusCode::OK => {
                log_ok!(stdout, "[OK] Config pushed successfully for {} ({})", environment, region);
            }
            _ => {
                let err_text = response.text()?;
                log_error!(stdout, "[ERROR] Failed to push config: {}", err_text);
                anyhow::bail!("Failed to push config: {}", err_text);
            }
        }

        Ok(())
    }
}

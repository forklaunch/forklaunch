use std::io::Write;

use anyhow::{Result, bail};
use clap::{Arg, ArgMatches, Command};
use reqwest::Method;
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{
        command::command,
        http_client::make_authenticated_request,
        validate::{require_auth, require_manifest},
    },
    github::application_github_url,
};

#[derive(Debug)]
pub(crate) struct DisconnectCommand;

impl DisconnectCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for DisconnectCommand {
    fn command(&self) -> Command {
        command(
            "disconnect",
            "Disconnect this application from its GitHub repository",
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        require_auth()?;
        let (_, manifest) = require_manifest(matches)?;

        let Some(app_id) = &manifest.platform_application_id else {
            bail!("This application is not integrated with the platform.");
        };

        let url = application_github_url(app_id, "disconnect");
        let response = make_authenticated_request(Method::DELETE, &url, None)?;
        if !response.status().is_success() {
            bail!(
                "Failed to disconnect repository (Status: {})",
                response.status()
            );
        }

        log_header!(stdout, Color::Green, "Repository disconnected.");
        Ok(())
    }
}

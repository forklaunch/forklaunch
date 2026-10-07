use std::io::Write;

use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, extract_list, get_value, require_managed_mode, resolve_managed_auth},
        types::{AppTemplate, dash},
    },
};

#[derive(Debug)]
pub(super) struct ListCommand;

impl ListCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ListCommand {
    fn command(&self) -> Command {
        command(
            "list",
            "List the app templates your organization has published",
        )
        .long_about(
            "List the app templates your organization has published.\n\n\
                 By default only published templates are shown, because only a published\n\
                 template can have instances launched from it. Pass --include-unpublished to\n\
                 also see drafts and retired templates.",
        )
        .arg(
            Arg::new("include_unpublished")
                .long("include-unpublished")
                .help("Also show draft and retired templates, not just published ones")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let json_output = matches.get_flag("json");
        let path = if matches.get_flag("include_unpublished") {
            "/templates?includeUnpublished=true"
        } else {
            "/templates"
        };

        let value = get_value(&auth_mode, path, Missing::Endpoint)?;
        let templates: Vec<AppTemplate> = extract_list(value, &["templates"])?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&templates)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "App templates")?;
        stdout.reset()?;
        writeln!(stdout)?;

        if templates.is_empty() {
            writeln!(
                stdout,
                "  No templates published yet. Create one with `forklaunch managed template create`."
            )?;
            writeln!(stdout)?;
            return Ok(());
        }

        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(stdout, "  {:<24} {:<28} {:<12}", "SLUG", "NAME", "STATUS")?;
        stdout.reset()?;
        for template in &templates {
            writeln!(
                stdout,
                "  {:<24} {:<28} {:<12}",
                dash(&template.slug),
                dash(&template.name),
                dash(&template.status),
            )?;
        }
        writeln!(stdout)?;

        Ok(())
    }
}

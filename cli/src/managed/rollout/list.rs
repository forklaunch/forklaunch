use std::io::Write;

use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{
            Missing, extract_list, get_value, print_dryrun, require_managed_mode,
            resolve_managed_auth,
        },
        types::{FleetRollout, dash},
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
        command("list", "List fleet rollouts, newest first")
            .arg(
                Arg::new("template")
                    .long("template")
                    .help("Only rollouts of this template"),
            )
            .arg(
                Arg::new("dryrun")
                    .long("dryrun")
                    .help("Print the request that would be sent without sending it")
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
        if matches.get_flag("dryrun") {
            return print_dryrun("GET", "/rollouts", None);
        }
        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let value = get_value(&auth_mode, "/rollouts", Missing::Endpoint)?;
        let template_filter = matches.get_one::<String>("template");
        let rollouts: Vec<FleetRollout> = extract_list::<FleetRollout>(value, &["rollouts"])?
            .into_iter()
            .filter(|rollout| match (template_filter, &rollout.template_slug) {
                (Some(wanted), Some(actual)) => actual == wanted,
                (Some(_), None) => false,
                (None, _) => true,
            })
            .collect();

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&rollouts)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Fleet rollouts")?;
        stdout.reset()?;
        writeln!(stdout)?;
        if rollouts.is_empty() {
            writeln!(
                stdout,
                "  No rollouts yet. Start one with `forklaunch managed rollout start`."
            )?;
            writeln!(stdout)?;
            return Ok(());
        }
        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(
            stdout,
            "  {:<38} {:<18} {:<10} {:<12} {:<8} {}",
            "ID", "TEMPLATE", "TARGET", "STATE", "WAVE", "ITEMS"
        )?;
        stdout.reset()?;
        for rollout in &rollouts {
            let mut counts = std::collections::BTreeMap::new();
            for item in &rollout.items {
                *counts
                    .entry(item.state.as_deref().unwrap_or("-"))
                    .or_insert(0u32) += 1;
            }
            let items: Vec<String> = counts.iter().map(|(k, v)| format!("{} {}", v, k)).collect();
            writeln!(
                stdout,
                "  {:<38} {:<18} {:<10} {:<12} {:<8} {}",
                dash(&rollout.id),
                dash(&rollout.template_slug),
                dash(&rollout.target_version_semver),
                dash(&rollout.state),
                format!(
                    "{}/{}",
                    rollout
                        .current_wave
                        .map(|w| (w + 1).min(rollout.wave_percents.len() as u64))
                        .unwrap_or(0),
                    rollout.wave_percents.len()
                ),
                items.join(", ")
            )?;
        }
        writeln!(stdout)?;
        Ok(())
    }
}

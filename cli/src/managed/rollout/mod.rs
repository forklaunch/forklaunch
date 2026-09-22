use std::io::Write;

use anyhow::Result;
use clap::{ArgMatches, Command};
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};

use crate::{CliCommand, core::command::command, managed::types::FleetRollout};

mod advance;
mod get;
mod list;
mod start;

use advance::AdvanceCommand;
use get::GetCommand;
use list::ListCommand;
use start::StartCommand;

/// `forklaunch managed rollout` — move a template's fleet to a published version in waves.
#[derive(Debug)]
pub(super) struct RolloutCommand {
    start: StartCommand,
    list: ListCommand,
    get: GetCommand,
    advance: AdvanceCommand,
}

impl RolloutCommand {
    pub(super) fn new() -> Self {
        Self {
            start: StartCommand::new(),
            list: ListCommand::new(),
            get: GetCommand::new(),
            advance: AdvanceCommand::new(),
        }
    }
}

impl CliCommand for RolloutCommand {
    fn command(&self) -> Command {
        command(
            "rollout",
            "Roll a published template version out to the fleet, a canary wave first",
        )
        .long_about(
            "Roll a published template version out to the fleet, a canary wave first.\n\n\
             A rollout pins the target version on every instance of a template that accepts\n\
             updates and redeploys them in waves: the first wave is the canary (10% by\n\
             default), the last is the whole fleet. Each item's deploy is followed to\n\
             completion; when a wave's failures exceed the threshold the rollout HALTS by\n\
             itself, and `advance` resumes it after you have fixed the cause. There is no\n\
             separate promote step — a healthy wave advances on its own.\n\n\
             Instances whose update policy is `deferred` are skipped (item state `deferred`)\n\
             and stay on their version; `instance update --update-policy auto` opts them\n\
             back in. An instance already on the target version is left alone.\n\n\
             While a rollout is updating an instance, that instance refuses reset, resize\n\
             and apply-variables until its item leaves `updating`.",
        )
        .subcommand(self.start.command())
        .subcommand(self.list.command())
        .subcommand(self.get.command())
        .subcommand(self.advance.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("start", sub_matches)) => self.start.handler(sub_matches),
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            Some(("get", sub_matches)) => self.get.handler(sub_matches),
            Some(("advance", sub_matches)) => self.advance.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

/// One rollout, rendered the same way by `start`, `get` and `advance`.
pub(super) fn print_rollout(stdout: &mut StandardStream, rollout: &FleetRollout) -> Result<()> {
    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(
        stdout,
        "Rollout {} → {}",
        rollout.id.as_deref().unwrap_or("-"),
        rollout.target_version_semver.as_deref().unwrap_or("-")
    )?;
    stdout.reset()?;
    writeln!(stdout)?;
    let waves: Vec<String> = rollout
        .wave_percents
        .iter()
        .map(|w| format!("{}%", w))
        .collect();
    writeln!(
        stdout,
        "  {:<22} {}",
        "Template",
        rollout.template_slug.as_deref().unwrap_or("-")
    )?;
    writeln!(
        stdout,
        "  {:<22} {}",
        "State",
        rollout.state.as_deref().unwrap_or("-")
    )?;
    writeln!(
        stdout,
        "  {:<22} {} of {} ({})",
        "Wave",
        rollout
            .current_wave
            .map(|w| (w + 1).min(rollout.wave_percents.len() as u64).to_string())
            .unwrap_or_else(|| "-".to_string()),
        rollout.wave_percents.len(),
        waves.join(" → ")
    )?;
    writeln!(
        stdout,
        "  {:<22} {}%",
        "Halts above",
        rollout.failure_threshold_percent.unwrap_or(10)
    )?;
    if let Some(started) = rollout.started_at.as_deref() {
        writeln!(stdout, "  {:<22} {}", "Started", started)?;
    }
    if let Some(finished) = rollout.finished_at.as_deref() {
        writeln!(stdout, "  {:<22} {}", "Finished", finished)?;
    }
    writeln!(stdout)?;

    if rollout.items.is_empty() {
        writeln!(stdout, "  No instances in this rollout.")?;
        writeln!(stdout)?;
        return Ok(());
    }
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<38} {:<6} {:<12} {}",
        "INSTANCE", "WAVE", "STATE", "RUNNING"
    )?;
    stdout.reset()?;
    for item in &rollout.items {
        writeln!(
            stdout,
            "  {:<38} {:<6} {:<12} {}",
            item.instance_id.as_deref().unwrap_or("-"),
            item.wave
                .map(|w| (w + 1).to_string())
                .unwrap_or_else(|| "-".to_string()),
            item.state.as_deref().unwrap_or("-"),
            item.current_version_semver.as_deref().unwrap_or("-"),
        )?;
        if let Some(error) = item.error.as_deref() {
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
            writeln!(stdout, "      {}", error)?;
            stdout.reset()?;
        }
    }
    writeln!(stdout)?;
    Ok(())
}

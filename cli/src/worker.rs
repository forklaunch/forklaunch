use action::ActionCommand;
use events::EventsCommand;
use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod action;
mod events;

// ── Top-level command ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub(crate) struct WorkerCommand {
    pause: ActionCommand,
    resume: ActionCommand,
    restart: ActionCommand,
    events: EventsCommand,
}

impl WorkerCommand {
    pub(crate) fn new() -> Self {
        Self {
            pause: ActionCommand::new("pause", "Pause a running worker", "pause", "paused"),
            resume: ActionCommand::new("resume", "Resume a paused worker", "resume", "resumed"),
            restart: ActionCommand::new(
                "restart",
                "Restart a worker (force a new deployment)",
                "restart",
                "restarted",
            ),
            events: EventsCommand::new(),
        }
    }
}

impl CliCommand for WorkerCommand {
    fn command(&self) -> Command {
        command("worker", "Pause, resume, or restart a deployed worker")
            .subcommand_required(true)
            .subcommand(self.pause.command())
            .subcommand(self.resume.command())
            .subcommand(self.restart.command())
            .subcommand(self.events.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("pause", sub_matches)) => self.pause.handler(sub_matches),
            Some(("resume", sub_matches)) => self.resume.handler(sub_matches),
            Some(("restart", sub_matches)) => self.restart.handler(sub_matches),
            Some(("events", sub_matches)) => self.events.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker_cmd() -> Command {
        WorkerCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        worker_cmd().debug_assert();
    }

    #[test]
    fn requires_a_subcommand() {
        assert!(worker_cmd().try_get_matches_from(["worker"]).is_err());
    }

    #[test]
    fn pause_requires_worker_id() {
        assert!(worker_cmd().try_get_matches_from(["worker", "pause"]).is_err());
        assert!(
            worker_cmd()
                .try_get_matches_from(["worker", "pause", "worker-1"])
                .is_ok()
        );
    }

    #[test]
    fn resume_requires_worker_id() {
        assert!(
            worker_cmd()
                .try_get_matches_from(["worker", "resume", "worker-1"])
                .is_ok()
        );
    }

    #[test]
    fn restart_requires_worker_id() {
        assert!(
            worker_cmd()
                .try_get_matches_from(["worker", "restart", "worker-1"])
                .is_ok()
        );
    }
}

use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod logs;
mod metrics;
mod status;
mod traces;

use logs::LogsCommand;
use metrics::MetricsCommand;
use status::StatusCommand;
use traces::TracesCommand;

#[derive(Debug)]
pub(crate) struct ObserveCommand {
    status: StatusCommand,
    logs: LogsCommand,
    metrics: MetricsCommand,
    traces: TracesCommand,
}

impl ObserveCommand {
    pub(crate) fn new() -> Self {
        Self {
            status: StatusCommand::new(),
            logs: LogsCommand::new(),
            metrics: MetricsCommand::new(),
            traces: TracesCommand::new(),
        }
    }
}

impl CliCommand for ObserveCommand {
    fn command(&self) -> Command {
        command(
            "observe",
            "Inspect logs, metrics, traces, and live health for a ForkLaunch application",
        )
        .subcommand(self.status.command())
        .subcommand(self.logs.command())
        .subcommand(self.metrics.command())
        .subcommand(self.traces.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            Some(("logs", sub_matches)) => self.logs.handler(sub_matches),
            Some(("metrics", sub_matches)) => self.metrics.handler(sub_matches),
            Some(("traces", sub_matches)) => self.traces.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

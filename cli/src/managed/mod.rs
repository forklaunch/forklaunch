use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod client;
pub(crate) mod detect;
mod instance;
mod summary;
mod template;
mod types;

use instance::InstanceCommand;
use summary::SummaryCommand;
use template::TemplateCommand;

/// `forklaunch managed` — publish app templates and run managed instances of them.
///
/// Two nouns matter here. A **template** is an app your organization publishes once:
/// a git repository plus a series of published versions. An **instance** is one
/// running copy of a template, provisioned for one end customer, with its own
/// deployment and its own one-time claim link.
#[derive(Debug)]
pub(crate) struct ManagedCommand {
    summary: SummaryCommand,
    template: TemplateCommand,
    instance: InstanceCommand,
}

impl ManagedCommand {
    pub(crate) fn new() -> Self {
        Self {
            summary: SummaryCommand::new(),
            template: TemplateCommand::new(),
            instance: InstanceCommand::new(),
        }
    }
}

impl CliCommand for ManagedCommand {
    fn command(&self) -> Command {
        command(
            "managed",
            "Publish app templates and launch managed instances of them for customers",
        )
        .long_about(
            "Publish app templates and launch managed instances of them for customers.\n\n\
             A TEMPLATE is an app your organization publishes once — a git repository plus a\n\
             series of published versions. An INSTANCE is one running copy of a template,\n\
             provisioned for a single end customer, with its own deployment and its own\n\
             one-time claim link that hands ownership to that customer.\n\n\
             The usual order is:\n\
             \x20 1. template create            register the template (it starts as a DRAFT)\n\
             \x20 2. template publish           add a version, pinning a semver to a git ref\n\
             \x20 3. template publish-template  flip the template itself to PUBLISHED\n\
             \x20 4. instance create            launch a copy for one customer\n\
             \x20 5. instance claim-link        reveal the one-time link, hand it to them\n\n\
             Steps 2 and 3 are both required and are NOT the same thing — see\n\
             `forklaunch managed template --help`.\n\n\
             All of these commands talk to the ForkLaunch control plane (platform-management),\n\
             never to the managed-apps service directly. Run `forklaunch login` first — the\n\
             one exception is `instance claim`, which is the customer-facing command and\n\
             needs no ForkLaunch account at all.",
        )
        .subcommand(self.summary.command())
        .subcommand(self.template.command())
        .subcommand(self.instance.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("summary", sub_matches)) => self.summary.handler(sub_matches),
            Some(("template", sub_matches)) => self.template.handler(sub_matches),
            Some(("instance", sub_matches)) => self.instance.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

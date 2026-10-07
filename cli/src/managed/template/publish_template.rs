use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::{
    CliCommand,
    core::command::command,
    managed::template::update::{TemplateUpdate, update_template},
};

/// `forklaunch managed template publish-template`
///
/// This is `template update --status published` under a name that says what it is for.
/// It exists because the step it performs is mandatory and was, until the control plane
/// gained `PATCH /managed-mode/templates/:slug`, impossible: templates are created as
/// `draft`, `instance create` requires `published`, and nothing could move one short of
/// writing the database column by hand.
#[derive(Debug)]
pub(super) struct PublishTemplateCommand;

impl PublishTemplateCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for PublishTemplateCommand {
    fn command(&self) -> Command {
        command(
            "publish-template",
            "Mark the TEMPLATE ITSELF as published, so instances can be launched from it",
        )
        .long_about(
            "Mark the template itself as published, so instances can be launched from it.\n\n\
             DO NOT CONFUSE THIS WITH `template publish`:\n\
             \x20 `template publish`           adds a VERSION to a template (a semver + git ref)\n\
             \x20 `template publish-template`  publishes the TEMPLATE, which is what makes it\n\
             \x20                              launchable at all\n\n\
             A template is created as a draft. `instance create` only accepts a published\n\
             template, so until this command is run — however many versions the template\n\
             has — no instance can be launched from it.\n\n\
             This is exactly `template update --status published`, under a name that says\n\
             what it is for. Use `template update` if you want to set other fields at the\n\
             same time, or to move a template to `retired`.",
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug of the template to publish"),
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
        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;

        update_template(
            slug,
            TemplateUpdate {
                status: Some("published"),
                dryrun: matches.get_flag("dryrun"),
                json: matches.get_flag("json"),
                ..Default::default()
            },
        )
    }
}

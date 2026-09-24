use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod app_claim_hook;
mod app_claim_link;
mod apply_variables;
mod claim;
mod claim_link;
mod create;
mod deployments;
mod destroy;
mod get;
mod list;
mod reset;
mod resume;
mod rotate_keys;
mod sms;
mod update;
mod vars;

use app_claim_hook::AppClaimHookCommand;
use app_claim_link::AppClaimLinkCommand;
use apply_variables::ApplyVariablesCommand;
use claim::ClaimCommand;
use claim_link::ClaimLinkCommand;
use create::CreateCommand;
use deployments::DeploymentsCommand;
use destroy::DestroyCommand;
use get::GetCommand;
use list::ListCommand;
use reset::ResetCommand;
use resume::ResumeCommand;
use rotate_keys::RotateKeysCommand;
use sms::SmsCommand;
use update::UpdateCommand;
use vars::VarsCommand;

#[derive(Debug)]
pub(super) struct InstanceCommand {
    list: ListCommand,
    create: CreateCommand,
    claim_link: ClaimLinkCommand,
    app_claim_link: AppClaimLinkCommand,
    app_claim_hook: AppClaimHookCommand,
    sms: SmsCommand,
    claim: ClaimCommand,
    destroy: DestroyCommand,
    vars: VarsCommand,
    get: GetCommand,
    update: UpdateCommand,
    apply_variables: ApplyVariablesCommand,
    deployments: DeploymentsCommand,
    reset: ResetCommand,
    resume: ResumeCommand,
    rotate_keys: RotateKeysCommand,
}

impl InstanceCommand {
    pub(super) fn new() -> Self {
        Self {
            list: ListCommand::new(),
            create: CreateCommand::new(),
            claim_link: ClaimLinkCommand::new(),
            app_claim_link: AppClaimLinkCommand::new(),
            app_claim_hook: AppClaimHookCommand::new(),
            sms: SmsCommand::new(),
            claim: ClaimCommand::new(),
            destroy: DestroyCommand::new(),
            vars: VarsCommand::new(),
            get: GetCommand::new(),
            update: UpdateCommand::new(),
            apply_variables: ApplyVariablesCommand::new(),
            deployments: DeploymentsCommand::new(),
            reset: ResetCommand::new(),
            resume: ResumeCommand::new(),
            rotate_keys: RotateKeysCommand::new(),
        }
    }
}

impl CliCommand for InstanceCommand {
    fn command(&self) -> Command {
        command(
            "instance",
            "Launch and manage managed instances of your app templates",
        )
        .long_about(
            "Launch and manage managed instances of your app templates.\n\n\
             Each instance is one running copy of a template, provisioned for a single end\n\
             customer with its own deployment. A newly created instance also gets a ONE-TIME\n\
             claim link, which is how the customer takes ownership.\n\n\
             TWO COMMANDS HAVE SIMILAR NAMES AND OPPOSITE AUDIENCES:\n\n\
             \x20 `claim-link`  REVEALS the one-time link. You run this — the operator. It\n\
             \x20               requires login, and the link is destroyed on reveal, so it can\n\
             \x20               be run only once per instance.\n\
             \x20 `claim`       CONSUMES the one-time link. YOUR CUSTOMER runs this, on their\n\
             \x20               own machine. It requires NO ForkLaunch account at all.\n\n\
             The normal sequence is: you `create`, you `claim-link`, you hand the output to\n\
             the customer, and the customer `claim`s it.\n\n\
             `vars` is the one thing that can come BEFORE `create`. If the template declares\n\
             a REQUIRED custom variable, the instance will not provision until it has a\n\
             value — run `vars list` to see which are still missing.\n\n\
             LIFECYCLE, after launch. `get` is the row a client polls: every command below\n\
             answers 202 with the state it moved TO and the outcome lands on the row later.\n\
             \x20 `update`           resize (redeploys) or set the fleet-update policy\n\
             \x20 `apply-variables`  redeploy the current version so new values reach the tasks\n\
             \x20 `deployments`      the instance's deploy feed, to follow any of the above\n\
             \x20 `resume`           retry a failed launch, or release one parked for approval\n\
             \x20 `reset`            wipe the data and return the instance to the pool (admin)\n\
             \x20 `rotate-keys`      new generation of the instance's secrets; the app re-encrypts (admin)",
        )
        .subcommand(self.list.command())
        .subcommand(self.create.command())
        .subcommand(self.claim_link.command())
        .subcommand(self.app_claim_link.command())
        .subcommand(self.app_claim_hook.command())
        .subcommand(self.sms.command())
        .subcommand(self.claim.command())
        .subcommand(self.destroy.command())
        .subcommand(self.vars.command())
        .subcommand(self.get.command())
        .subcommand(self.update.command())
        .subcommand(self.apply_variables.command())
        .subcommand(self.deployments.command())
        .subcommand(self.resume.command())
        .subcommand(self.reset.command())
        .subcommand(self.rotate_keys.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("claim-link", sub_matches)) => self.claim_link.handler(sub_matches),
            Some(("app-claim-link", sub_matches)) => self.app_claim_link.handler(sub_matches),
            Some(("app-claim-hook", sub_matches)) => self.app_claim_hook.handler(sub_matches),
            Some(("sms", sub_matches)) => self.sms.handler(sub_matches),
            Some(("claim", sub_matches)) => self.claim.handler(sub_matches),
            Some(("destroy", sub_matches)) => self.destroy.handler(sub_matches),
            Some(("vars", sub_matches)) => self.vars.handler(sub_matches),
            Some(("get", sub_matches)) => self.get.handler(sub_matches),
            Some(("update", sub_matches)) => self.update.handler(sub_matches),
            Some(("apply-variables", sub_matches)) => self.apply_variables.handler(sub_matches),
            Some(("deployments", sub_matches)) => self.deployments.handler(sub_matches),
            Some(("resume", sub_matches)) => self.resume.handler(sub_matches),
            Some(("reset", sub_matches)) => self.reset.handler(sub_matches),
            Some(("rotate-keys", sub_matches)) => self.rotate_keys.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

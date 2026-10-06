use anyhow::Result;
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod app_claim_link;
mod apply_variables;
mod claim;
mod claim_link;
mod claim_next;
mod create;
mod deployments;
mod destroy;
mod feature_override;
mod get;
mod list;
mod model_gateway;
mod model_usage;
mod reset;
mod resume;
mod rotate_keys;
mod update;
mod vars;

use app_claim_link::AppClaimLinkCommand;
use apply_variables::ApplyVariablesCommand;
use claim::ClaimCommand;
use claim_link::ClaimLinkCommand;
use claim_next::ClaimNextCommand;
use create::CreateCommand;
use deployments::DeploymentsCommand;
use destroy::DestroyCommand;
use feature_override::{Feature, FeatureOverrideCommand, GatewaySettingsCommand};
use get::GetCommand;
use list::ListCommand;
use model_gateway::ModelGatewayCommand;
use model_usage::ModelUsageCommand;
use reset::ResetCommand;
use resume::ResumeCommand;
use rotate_keys::RotateKeysCommand;
use update::UpdateCommand;
use vars::VarsCommand;

#[derive(Debug)]
pub(super) struct InstanceCommand {
    list: ListCommand,
    create: CreateCommand,
    claim_link: ClaimLinkCommand,
    claim_next: ClaimNextCommand,
    app_claim_link: AppClaimLinkCommand,
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
    model_usage: ModelUsageCommand,
    model_gateway: ModelGatewayCommand,
    sms_gateway: FeatureOverrideCommand,
    email_gateway: FeatureOverrideCommand,
    payments_gateway: FeatureOverrideCommand,
    voice: FeatureOverrideCommand,
    whatsapp: FeatureOverrideCommand,
    gateway_settings: GatewaySettingsCommand,
}

impl InstanceCommand {
    pub(super) fn new() -> Self {
        Self {
            list: ListCommand::new(),
            create: CreateCommand::new(),
            claim_link: ClaimLinkCommand::new(),
            claim_next: ClaimNextCommand::new(),
            app_claim_link: AppClaimLinkCommand::new(),
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
            model_usage: ModelUsageCommand::new(),
            model_gateway: ModelGatewayCommand::new(),
            sms_gateway: FeatureOverrideCommand::new(Feature::Sms),
            email_gateway: FeatureOverrideCommand::new(Feature::Email),
            payments_gateway: FeatureOverrideCommand::new(Feature::Payments),
            voice: FeatureOverrideCommand::new(Feature::Voice),
            whatsapp: FeatureOverrideCommand::new(Feature::WhatsApp),
            gateway_settings: GatewaySettingsCommand::new(),
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
             UNATTENDED (a signup backend): `claim-next --template <slug>` takes the oldest\n\
             unreserved instance from the template's pool and returns a fresh claim link in\n\
             one call; two concurrent calls never get the same instance. An empty pool exits\n\
             with POOL_EMPTY and the pool's counts, so you know to `create` more.\n\n\
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
             \x20 `rotate-keys`      new generation of the instance's secrets; the app re-encrypts (admin)\n\n\
             MODEL GATEWAY (platform-hosted AI models, no provider key in the instance):\n\
             \x20 `model-usage`      a month of calls, tokens and cost per model\n\
             \x20 `model-gateway`    override the product's models / budget / rate for one instance\n\n\
             OTHER PLATFORM-HELD FEATURES (the product's settings are `template update`\n\
             --sms-* / --whatsapp-* / --voice-* / --email-* / --payments-*; these override\n\
             them for ONE instance, and --clear removes the override):\n\
             \x20 `sms-gateway`       monthly segment cap, texts per minute, a dedicated number\n\
             \x20 `whatsapp`          this instance's own WhatsApp number, its rate, or off\n\
             \x20 `voice`             concurrent calls, monthly minutes\n\
             \x20 `email-gateway`     recipients per day, messages per minute\n\
             \x20 `payments-gateway`  application fee, Stripe calls per minute\n\
             \x20 `gateway-settings`  what is in force for each, and which is overridden",
        )
        .subcommand(self.list.command())
        .subcommand(self.create.command())
        .subcommand(self.claim_link.command())
        .subcommand(self.claim_next.command())
        .subcommand(self.app_claim_link.command())
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
        .subcommand(self.model_usage.command())
        .subcommand(self.model_gateway.command())
        .subcommand(self.sms_gateway.command())
        .subcommand(self.email_gateway.command())
        .subcommand(self.payments_gateway.command())
        .subcommand(self.voice.command())
        .subcommand(self.whatsapp.command())
        .subcommand(self.gateway_settings.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            Some(("create", sub_matches)) => self.create.handler(sub_matches),
            Some(("claim-link", sub_matches)) => self.claim_link.handler(sub_matches),
            Some(("claim-next", sub_matches)) => self.claim_next.handler(sub_matches),
            Some(("app-claim-link", sub_matches)) => self.app_claim_link.handler(sub_matches),
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
            Some(("model-usage", sub_matches)) => self.model_usage.handler(sub_matches),
            Some(("model-gateway", sub_matches)) => self.model_gateway.handler(sub_matches),
            Some(("sms-gateway", sub_matches)) => self.sms_gateway.handler(sub_matches),
            Some(("email-gateway", sub_matches)) => self.email_gateway.handler(sub_matches),
            Some(("payments-gateway", sub_matches)) => self.payments_gateway.handler(sub_matches),
            Some(("voice", sub_matches)) => self.voice.handler(sub_matches),
            Some(("whatsapp", sub_matches)) => self.whatsapp.handler(sub_matches),
            Some(("gateway-settings", sub_matches)) => self.gateway_settings.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

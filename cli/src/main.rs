use alerts::AlertsCommand;
use analyze::AnalyzeCommand;
use score::ScoreCommand;
use anyhow::Result;
use change::ChangeCommand;
use clap::{ArgMatches, Command, command};
use cloud_account::CloudAccountCommand;
use compliance::ComplianceCommand;
use data::DataCommand;
use config::ConfigCommand;
use context::ContextCommand;
use delete::DeleteCommand;
use depcheck::DepcheckCommand;
use deploy::DeployCommand;
use dlq::DlqCommand;
use drift::DriftCommand;
use eject::EjectCommand;
use environment::EnvironmentCommand;
use github::GithubCommand;
use infra::InfraCommand;
use init::InitCommand;
use app::AppCommand;
use integrate::IntegrateCommand;
use login::LoginCommand;
use logout::LogoutCommand;
use managed::ManagedCommand;
use notifiers::NotifiersCommand;
use observe::ObserveCommand;
use openapi::OpenApiCommand;
use release::ReleaseCommand;
use sync::SyncCommand;
use version::VersionCommand;
use whoami::WhoAmICommand;
use worker::WorkerCommand;

use crate::sdk::SdkCommand;

mod alerts;
mod analyze;
mod score;
mod constants;
#[macro_use]
mod core;
mod app;
mod change;
mod cloud_account;
mod compliance;
mod config;
mod data;
mod context;
mod delete;
mod depcheck;
mod deploy;
mod dlq;
mod drift;
mod eject;
mod environment;
mod github;
mod infra;
mod init;
mod integrate;
mod login;
mod logout;
mod managed;
mod notifiers;
mod observe;
mod openapi;
mod prompt;
mod release;
mod sdk;
mod sync;
mod version;
mod whoami;
mod worker;

pub(crate) trait CliCommand {
    fn command(&self) -> Command;
    fn handler(&self, matches: &ArgMatches) -> Result<()>;
}

fn main() -> Result<()> {
    // inject token into init, config
    let alerts = AlertsCommand::new();
    let init = InitCommand::new();
    let analyze = AnalyzeCommand::new();
    let score = ScoreCommand::new();
    let change = ChangeCommand::new();
    let cloud_account = CloudAccountCommand::new();
    let compliance = ComplianceCommand::new();
    let data = DataCommand::new();
    let config = ConfigCommand::new();
    let context = ContextCommand::new();
    let delete = DeleteCommand::new();
    let depcheck = DepcheckCommand::new();
    let deploy = DeployCommand::new();
    let dlq = DlqCommand::new();
    let drift = DriftCommand::new();
    let eject = EjectCommand::new();
    let environment = EnvironmentCommand::new();
    let infra = InfraCommand::new();
    let github = GithubCommand::new();
    let app = AppCommand::new();
    let integrate = IntegrateCommand::new();
    let login = LoginCommand::new();
    let logout = LogoutCommand::new();
    let managed = ManagedCommand::new();
    let notifiers = NotifiersCommand::new();
    let observe = ObserveCommand::new();
    let openapi = OpenApiCommand::new();
    let release = ReleaseCommand::new();
    let sdk = SdkCommand::new();
    let whoami = WhoAmICommand::new();
    let version = VersionCommand::new();
    let sync = SyncCommand::new();
    let worker = WorkerCommand::new();

    let matches = command!()
        .propagate_version(true)
        .arg_required_else_help(true)
        .subcommand_required(true)
        .subcommand(alerts.command())
        .subcommand(app.command())
        .subcommand(init.command())
        .subcommand(analyze.command())
        .subcommand(score.command())
        .subcommand(delete.command())
        .subcommand(change.command())
        .subcommand(cloud_account.command())
        .subcommand(compliance.command())
        .subcommand(data.command())
        .subcommand(eject.command())
        .subcommand(depcheck.command())
        .subcommand(config.command())
        .subcommand(context.command())
        .subcommand(deploy.command())
        .subcommand(dlq.command())
        .subcommand(drift.command())
        .subcommand(environment.command())
        .subcommand(infra.command())
        .subcommand(github.command())
        .subcommand(integrate.command())
        .subcommand(openapi.command())
        .subcommand(release.command())
        .subcommand(login.command())
        .subcommand(logout.command())
        .subcommand(managed.command())
        .subcommand(notifiers.command())
        .subcommand(observe.command())
        .subcommand(sdk.command())
        .subcommand(whoami.command())
        .subcommand(version.command())
        .subcommand(sync.command())
        .subcommand(worker.command())
        .get_matches();

    if let Some((cmd, sub_matches)) = matches.subcommand() {
        crate::core::version_check::precheck_version(sub_matches, cmd)?;
    }

    let result = match matches.subcommand() {
        Some(("alerts", sub_matches)) => alerts.handler(sub_matches),
        Some(("app", sub_matches)) => app.handler(sub_matches),
        Some(("init", sub_matches)) => init.handler(sub_matches),
        Some(("analyze", sub_matches)) => analyze.handler(sub_matches),
        Some(("score", sub_matches)) => score.handler(sub_matches),
        Some(("change", sub_matches)) => change.handler(sub_matches),
        Some(("cloud-account", sub_matches)) => cloud_account.handler(sub_matches),
        Some(("compliance", sub_matches)) => compliance.handler(sub_matches),
        Some(("data", sub_matches)) => data.handler(sub_matches),
        Some(("config", sub_matches)) => config.handler(sub_matches),
        Some(("context", sub_matches)) => context.handler(sub_matches),
        Some(("delete", sub_matches)) => delete.handler(sub_matches),
        Some(("depcheck", sub_matches)) => depcheck.handler(sub_matches),
        Some(("deploy", sub_matches)) => deploy.handler(sub_matches),
        Some(("dlq", sub_matches)) => dlq.handler(sub_matches),
        Some(("drift", sub_matches)) => drift.handler(sub_matches),
        Some(("eject", sub_matches)) => eject.handler(sub_matches),
        Some(("environment", sub_matches)) => environment.handler(sub_matches),
        Some(("infra", sub_matches)) => infra.handler(sub_matches),
        Some(("github", sub_matches)) => github.handler(sub_matches),
        Some(("integrate", sub_matches)) => integrate.handler(sub_matches),
        Some(("openapi", sub_matches)) => openapi.handler(sub_matches),
        Some(("release", sub_matches)) => release.handler(sub_matches),
        Some(("login", sub_matches)) => login.handler(sub_matches),
        Some(("logout", sub_matches)) => logout.handler(sub_matches),
        Some(("managed", sub_matches)) => managed.handler(sub_matches),
        Some(("notifiers", sub_matches)) => notifiers.handler(sub_matches),
        Some(("observe", sub_matches)) => observe.handler(sub_matches),
        Some(("sdk", sub_matches)) => sdk.handler(sub_matches),
        Some(("whoami", sub_matches)) => whoami.handler(sub_matches),
        Some(("version", sub_matches)) => version.handler(sub_matches),
        Some(("sync", sub_matches)) => sync.handler(sub_matches),
        Some(("worker", sub_matches)) => worker.handler(sub_matches),
        _ => unreachable!(),
    };

    match result {
        Ok(_) => result,
        // TODO: make sure that the error text returns in red color
        Err(error) => Err(error),
    }
}

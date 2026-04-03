use anyhow::Result;
use change::ChangeCommand;
use clap::{ArgMatches, Command, command};
use compliance::ComplianceCommand;
use config::ConfigCommand;
use delete::DeleteCommand;
use depcheck::DepcheckCommand;
use deploy::DeployCommand;
use eject::EjectCommand;
use environment::EnvironmentCommand;
use init::InitCommand;
use integrate::IntegrateCommand;
use login::LoginCommand;
use logout::LogoutCommand;
use openapi::OpenApiCommand;
use release::ReleaseCommand;
use version::VersionCommand;
use whoami::WhoAmICommand;
use sync::SyncCommand;

use crate::sdk::SdkCommand;

mod constants;
#[macro_use]
mod core;
mod change;
mod compliance;
mod config;
mod delete;
mod depcheck;
mod deploy;
mod eject;
mod environment;
mod init;
mod integrate;
mod login;
mod logout;
mod openapi;
mod prompt;
mod release;
mod sdk;
mod version;
mod whoami;
mod sync;

pub(crate) trait CliCommand {
    fn command(&self) -> Command;
    fn handler(&self, matches: &ArgMatches) -> Result<()>;
}

fn main() -> Result<()> {
    // inject token into init, config
    let init = InitCommand::new();
    let change = ChangeCommand::new();
    let compliance = ComplianceCommand::new();
    let config = ConfigCommand::new();
    let delete = DeleteCommand::new();
    let depcheck = DepcheckCommand::new();
    let deploy = DeployCommand::new();
    let eject = EjectCommand::new();
    let environment = EnvironmentCommand::new();
    let integrate = IntegrateCommand::new();
    let login = LoginCommand::new();
    let logout = LogoutCommand::new();
    let openapi = OpenApiCommand::new();
    let release = ReleaseCommand::new();
    let sdk = SdkCommand::new();
    let whoami = WhoAmICommand::new();
    let version = VersionCommand::new();
    let sync = SyncCommand::new();

    let matches = command!()
        .propagate_version(true)
        .arg_required_else_help(true)
        .subcommand_required(true)
        .subcommand(init.command())
        .subcommand(delete.command())
        .subcommand(change.command())
        .subcommand(compliance.command())
        .subcommand(eject.command())
        .subcommand(depcheck.command())
        .subcommand(config.command())
        .subcommand(deploy.command())
        .subcommand(environment.command())
        .subcommand(integrate.command())
        .subcommand(openapi.command())
        .subcommand(release.command())
        .subcommand(login.command())
        .subcommand(logout.command())
        .subcommand(sdk.command())
        .subcommand(whoami.command())
        .subcommand(version.command())
        .subcommand(sync.command())
        .get_matches();

    if let Some((cmd, sub_matches)) = matches.subcommand() {
        crate::core::version_check::precheck_version(sub_matches, cmd)?;
    }

    let result = match matches.subcommand() {
        Some(("init", sub_matches)) => init.handler(sub_matches),
        Some(("change", sub_matches)) => change.handler(sub_matches),
        Some(("compliance", sub_matches)) => compliance.handler(sub_matches),
        Some(("config", sub_matches)) => config.handler(sub_matches),
        Some(("delete", sub_matches)) => delete.handler(sub_matches),
        Some(("depcheck", sub_matches)) => depcheck.handler(sub_matches),
        Some(("deploy", sub_matches)) => deploy.handler(sub_matches),
        Some(("eject", sub_matches)) => eject.handler(sub_matches),
        Some(("environment", sub_matches)) => environment.handler(sub_matches),
        Some(("integrate", sub_matches)) => integrate.handler(sub_matches),
        Some(("openapi", sub_matches)) => openapi.handler(sub_matches),
        Some(("release", sub_matches)) => release.handler(sub_matches),
        Some(("login", sub_matches)) => login.handler(sub_matches),
        Some(("logout", sub_matches)) => logout.handler(sub_matches),
        Some(("sdk", sub_matches)) => sdk.handler(sub_matches),
        Some(("whoami", sub_matches)) => whoami.handler(sub_matches),
        Some(("version", sub_matches)) => version.handler(sub_matches),
        Some(("sync", sub_matches)) => sync.handler(sub_matches),
        _ => unreachable!(),
    };

    match result {
        Ok(_) => result,
        // TODO: make sure that the error text returns in red color
        Err(error) => Err(error),
    }
}

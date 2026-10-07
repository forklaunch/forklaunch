use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use dialoguer::{Input, theme::ColorfulTheme};

use crate::{
    CliCommand,
    core::{
        command::command,
        validate::{require_auth, require_integration, require_manifest},
    },
};

use super::{
    lifecycle::call_lifecycle_action,
    resource_resolver::{fetch_resource_detail, resolve},
};

#[derive(Debug)]
pub(super) struct DeleteCommand;

impl DeleteCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for DeleteCommand {
    fn command(&self) -> Command {
        command(
            "delete",
            "Permanently delete a provisioned database, cache, or queue resource and its AWS infrastructure",
        )
        .arg(
            Arg::new("resource")
                .help("Resource identifier: <project-name>:<resource-type> (e.g. billing-service:database)")
                .required(true),
        )
        .arg(Arg::new("base_path").short('p').long("path").help("The application path"))
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment to target (for example: dev, staging, production)"),
        )
        .arg(
            Arg::new("resource_id")
                .long("resource-id")
                .help("Skip name resolution and address a resource by its platform id directly"),
        )
        .arg(
            Arg::new("yes")
                .long("yes")
                .short('y')
                .help("Skip the confirmation prompt (for CI/scripted use) — use with care, this is irreversible")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let environment = matches
            .get_one::<String>("environment")
            .expect("--environment is required")
            .to_string();
        let resource_arg = matches
            .get_one::<String>("resource")
            .expect("<resource> is required");
        let resource_id_override = matches.get_one::<String>("resource_id").map(String::as_str);
        let skip_confirm = matches.get_flag("yes");

        let resolved = resolve(
            &manifest,
            &application_id,
            &environment,
            resource_arg,
            resource_id_override,
        )?;

        let detail = fetch_resource_detail(&resolved.id)?;

        if !skip_confirm {
            println!(
                "This will PERMANENTLY DELETE {} ({}) and its AWS infrastructure.",
                detail.name, detail.r#type
            );
            println!("This action cannot be undone — no snapshot is taken automatically before deletion.");
            println!();

            let typed: String = Input::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Type the resource name ({}) to confirm", detail.name))
                .allow_empty(true)
                .interact_text()
                .with_context(|| "Failed to read confirmation")?;

            if typed != detail.name {
                bail!("confirmation did not match '{}' — aborted, nothing was deleted", detail.name);
            }
        }

        let result = call_lifecycle_action(&resolved.id, "delete")?;
        println!("{}", result.message);

        Ok(())
    }
}

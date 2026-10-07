use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, delete_text, print_dryrun, require_managed_mode, resolve_managed_auth},
        template::vars::resolve_scope,
        types::VARIABLE_SCOPES,
    },
};

#[derive(Debug)]
pub(super) struct UnsetCommand;

impl UnsetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for UnsetCommand {
    fn command(&self) -> Command {
        command("unset", "Remove a variable declaration from a template")
            .long_about(
                "Remove a variable declaration from a template.\n\n\
                 New instances stop receiving the variable. Instances already deployed keep\n\
                 whatever they were given until their next provision.\n\n\
                 --scope and --service identify WHICH declaration to remove, not just which\n\
                 key: the same key can be declared once application-wide and again for a\n\
                 single service, and those are different declarations. Both default to the\n\
                 application-scoped one.\n\n\
                 To clear one instance's value while leaving the declaration in place, use\n\
                 `forklaunch managed instance vars unset` instead.",
            )
            .arg(
                Arg::new("slug")
                    .long("slug")
                    .required(true)
                    .help("Slug of the template to remove the declaration from"),
            )
            .arg(
                Arg::new("key")
                    .long("key")
                    .required(true)
                    .help("Name of the variable to remove"),
            )
            .arg(
                Arg::new("scope")
                    .long("scope")
                    .value_parser(VARIABLE_SCOPES.to_vec())
                    .default_value("application")
                    .help("Which scope's declaration to remove"),
            )
            .arg(
                Arg::new("service").long("service").help(
                    "Which service's declaration to remove — required with `--scope service`",
                ),
            )
            .arg(
                Arg::new("dryrun")
                    .long("dryrun")
                    .help("Print the request that would be sent without sending it")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;
        let key = matches
            .get_one::<String>("key")
            .context("--key is required")?;
        let scope = matches
            .get_one::<String>("scope")
            .map(String::as_str)
            .unwrap_or("application");
        let service = resolve_scope(
            scope,
            matches.get_one::<String>("service").map(String::as_str),
        )?;

        // Both the key and the scope go into the request — the key in the path, the
        // scope in the query — because a key alone does not identify a declaration.
        let mut path = format!(
            "/templates/{}/variables/{}?scope={}",
            urlencoding::encode(slug),
            urlencoding::encode(key),
            urlencoding::encode(scope)
        );
        if let Some(service) = service {
            path.push_str(&format!("&serviceName={}", urlencoding::encode(service)));
        }

        if matches.get_flag("dryrun") {
            return print_dryrun("DELETE", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        delete_text(
            &path,
            Missing::Resource(format!(
                "variable '{}' ({} scope) on template '{}'",
                key, scope, slug
            )),
        )?;

        log_ok!(
            stdout,
            "Removed the {}-scoped declaration of '{}' from template '{}'",
            scope,
            key,
            slug
        );
        log_info!(
            stdout,
            "New instances no longer receive it. Instances already deployed keep it until their next provision."
        );

        Ok(())
    }
}

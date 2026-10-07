use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::list::fetch_relay_config;
use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{
            Missing, print_dryrun, put_json_optional, require_managed_mode, resolve_managed_auth,
        },
        types::RelayRoute,
    },
};

#[derive(Debug)]
pub(super) struct ClearCommand;

impl ClearCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ClearCommand {
    fn command(&self) -> Command {
        command(
            "clear",
            "Remove one relay route by name, or every route with --all",
        )
        .long_about(
            "Remove a relay route.\n\n\
             With no routes left the product falls back to the legacy behaviour: the\n\
             platform runs the Epic exchange itself with credentials stored on the\n\
             template. If the product's app finishes its own sign-in (PKCE), that fallback\n\
             cannot work — declare a route again before customers sign in.",
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug of the template"),
        )
        .arg(
            Arg::new("name")
                .long("name")
                .conflicts_with("all")
                .required_unless_present("all")
                .help("Name of the route to remove"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .help("Remove every route")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;
        let all = matches.get_flag("all");
        let name = matches.get_one::<String>("name");
        let request_path = format!("/templates/{}/relay-routes", urlencoding::encode(slug));

        let routes: Vec<RelayRoute> = if all || matches.get_flag("dryrun") {
            Vec::new()
        } else {
            let name = name.context("--name is required unless --all")?;
            let current = fetch_relay_config(slug)?.routes;
            if !current.iter().any(|route| route.name == *name) {
                anyhow::bail!(
                    "template '{}' declares no route named '{}' (see `forklaunch managed template relay list --slug {}`)",
                    slug,
                    name,
                    slug
                );
            }
            current
                .into_iter()
                .filter(|route| route.name != *name)
                .map(|route| RelayRoute {
                    callback_url: None,
                    ..route
                })
                .collect()
        };
        let body = json!({ "routes": routes });

        if matches.get_flag("dryrun") {
            return print_dryrun("PUT", &request_path, Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        put_json_optional(
            &request_path,
            body,
            Missing::Resource(format!("template '{}'", slug)),
        )?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        if all {
            log_ok!(stdout, "Removed every relay route from template '{}'", slug);
        } else {
            log_ok!(
                stdout,
                "Removed route '{}' from template '{}'",
                name.map(String::as_str).unwrap_or("-"),
                slug
            );
        }
        if routes.is_empty() {
            log_info!(
                stdout,
                "No routes remain: the bare callback now runs the legacy Epic exchange on the platform. If the app finishes its own sign-in, declare a route again with `forklaunch managed template relay set`."
            );
        }
        Ok(())
    }
}

use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, post_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::AppTemplate,
    },
};

#[derive(Debug)]
pub(super) struct CreateCommand;

impl CreateCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command("create", "Register a new app template for your organization")
            .long_about(
                "Register a new app template for your organization.\n\n\
                 A NEW TEMPLATE IS A DRAFT, AND NOTHING CAN LAUNCH FROM A DRAFT. Two further\n\
                 steps are required before `instance create` will work:\n\
                 \x20 1. `template publish`          — add a version (a semver pinned to a git ref)\n\
                 \x20 2. `template publish-template` — move the template itself to `published`\n\n\
                 This command takes exactly what the control plane's create API accepts:\n\
                 slug, name, source repo, and an optional description. The Stripe product is\n\
                 set afterwards with `template update --stripe-product <id>`. The base domain\n\
                 is not settable through the API at all — instances use the platform-wide\n\
                 default.",
            )
            .arg(
                Arg::new("slug")
                    .long("slug")
                    .required(true)
                    .help("Short url-safe identifier for the template (for example: clinic-portal)"),
            )
            .arg(
                Arg::new("name")
                    .long("name")
                    .required(true)
                    .help("Human-readable template name"),
            )
            .arg(
                Arg::new("repo")
                    .long("repo")
                    .required(true)
                    .help("Git repository URL the template is built from"),
            )
            .arg(
                Arg::new("description")
                    .long("description")
                    .help("Optional description shown alongside the template"),
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
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;
        let name = matches
            .get_one::<String>("name")
            .context("--name is required")?;
        let repo = matches
            .get_one::<String>("repo")
            .context("--repo is required")?;

        let mut body = Map::new();
        body.insert("slug".to_string(), json!(slug));
        body.insert("name".to_string(), json!(name));
        // The platform's schema calls the repository URL `sourceRepo`. The flag is
        // `--repo` because that is what it reads as on a command line; do not rename the
        // wire field to match.
        body.insert("sourceRepo".to_string(), json!(repo));
        if let Some(description) = matches.get_one::<String>("description") {
            body.insert("description".to_string(), json!(description));
        }

        let body = Value::Object(body);

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", "/templates", Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let template: AppTemplate = post_json(&auth_mode, "/templates", body, Missing::Endpoint)?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&template)?);
            return Ok(());
        }

        log_ok!(
            stdout,
            "Created template '{}' ({}) — status: {}",
            template.slug.as_deref().unwrap_or(slug.as_str()),
            template.name.as_deref().unwrap_or(name.as_str()),
            template.status.as_deref().unwrap_or("draft")
        );
        // Both remaining steps, in order. Naming only the version step here is what let
        // the flow dead-end before: a template can have a published version and still be
        // a draft, and `instance create` refuses a draft.
        log_info!(
            stdout,
            "Nothing can launch from a draft. Two steps remain:\n    \
             1. forklaunch managed template publish --slug {} --semver <v> --git-ref <ref>\n    \
             2. forklaunch managed template publish-template --slug {}",
            slug,
            slug
        );

        Ok(())
    }
}

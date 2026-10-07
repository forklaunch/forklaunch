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
        types::TemplateVersion,
    },
};

#[derive(Debug)]
pub(super) struct PublishCommand;

impl PublishCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for PublishCommand {
    fn command(&self) -> Command {
        command(
            "publish",
            "Publish a new VERSION of a template (see publish-template for the template itself)",
        )
        .long_about(
            "Publish a new VERSION of an app template.\n\n\
             THIS IS NOT THE COMMAND THAT PUBLISHES A TEMPLATE. It adds a version — a\n\
             semantic version pinned to a git ref — to a template that already exists.\n\
             To move the TEMPLATE itself from `draft` to `published`, which is what\n\
             `instance create` requires, use `template publish-template`. Both steps are\n\
             needed, and doing only this one leaves the template still unlaunchable.\n\n\
             A newly added version starts pending: the platform builds the image from\n\
             --git-ref before instances can launch from it. There is no way to supply a\n\
             prebuilt image — the control plane's version API takes a semver and a git ref\n\
             and nothing else.",
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug of the template to publish a version of"),
        )
        .arg(
            Arg::new("semver")
                .long("semver")
                .required(true)
                .help("Semantic version for this release (for example: 1.4.0)"),
        )
        .arg(
            Arg::new("git_ref")
                .long("git-ref")
                .required(true)
                .help("Git ref (tag, branch, or commit sha) this version is built from"),
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
        let semver = matches
            .get_one::<String>("semver")
            .context("--semver is required")?;
        let git_ref = matches
            .get_one::<String>("git_ref")
            .context("--git-ref is required")?;

        let mut body = Map::new();
        body.insert("semver".to_string(), json!(semver));
        body.insert("gitRef".to_string(), json!(git_ref));
        let body = Value::Object(body);

        // Slugs are organization-authored identifiers, not free-form user input, but they
        // still land in a URL path — encode so a slug containing a separator cannot
        // restructure the request.
        let path = format!("/templates/{}/versions", urlencoding::encode(slug));

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", &path, Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let version: TemplateVersion = post_json(
            &auth_mode,
            &path,
            body,
            Missing::Resource(format!("template '{}'", slug)),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&version)?);
            return Ok(());
        }

        log_ok!(
            stdout,
            "Published version {} of '{}' from {}",
            version.semver.as_deref().unwrap_or(semver.as_str()),
            slug,
            version.git_ref.as_deref().unwrap_or(git_ref.as_str())
        );
        if let Some(status) = version.status.as_deref() {
            log_info!(stdout, "Version status: {}", status);
            if status == "pending" || status == "building" {
                log_info!(
                    stdout,
                    "The platform still has to build this version before instances can launch from it."
                );
            }
        }
        // A version is not the same as a published template, and the difference is
        // invisible until `instance create` refuses. Say it here every time.
        log_info!(
            stdout,
            "This published a VERSION, not the template. If '{}' is still a draft, run:\n    \
             forklaunch managed template publish-template --slug {}",
            slug,
            slug
        );

        Ok(())
    }
}

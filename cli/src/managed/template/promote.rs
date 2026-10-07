use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use serde_json::json;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, post_json, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::ManagedInstance,
    },
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Promotion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    template_slug: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version_semver: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version_status: Option<String>,
    #[serde(default)]
    adopted: Vec<ManagedInstance>,
    #[serde(default)]
    skipped: Vec<Skipped>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Skipped {
    environment: String,
    region: String,
    reason: String,
}

#[derive(Debug)]
pub(super) struct PromoteCommand;

impl PromoteCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for PromoteCommand {
    fn command(&self) -> Command {
        command(
            "promote",
            "Make a running application a managed template; optionally adopt its deploys as instances",
        )
        .long_about(
            "Make a running application a managed template; optionally adopt its deploys.\n\n\
             The application's repository becomes the template's source and its current\n\
             release becomes version 1. Adoption is the default; the template and the\n\
             version are published at once and every deploy the application already has,\n\
             one per environment x region with a recorded host, becomes an ACTIVE managed\n\
             instance of it: same application, same hosts, same secrets, same data. Nothing\n\
             is redeployed. From then on rollouts, resize, apply-variables and key rotation\n\
             drive it like any instance, and new customers get fresh copies.\n\n\
             With --no-adopt the template stays a draft and version 1 is built through the\n\
             template pipeline; the application is left as it is.\n\n\
             Needs the admin role. 409 names the reason: PROMOTE_NO_REPOSITORY (connect a\n\
             git repository first), PROMOTE_NO_RELEASE (release it first), PROMOTE_SLUG_TAKEN,\n\
             PROMOTE_ALREADY_MANAGED.",
        )
        .arg(
            Arg::new("application")
                .long("application")
                .required(true)
                .help("Id of the application to promote"),
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug for the new template (lower-case, digits, dashes)"),
        )
        .arg(Arg::new("name").long("name").help("Template name (default: the application's)"))
        .arg(
            Arg::new("description")
                .long("description")
                .help("Template description (default: the application's)"),
        )
        .arg(
            Arg::new("no_adopt")
                .long("no-adopt")
                .help("Do not adopt the running deploys; build version 1 through the pipeline")
                .action(ArgAction::SetTrue),
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
        let application = matches
            .get_one::<String>("application")
            .context("--application is required")?;
        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;
        let adopt = !matches.get_flag("no_adopt");

        let mut body = serde_json::Map::new();
        body.insert("applicationId".into(), json!(application));
        body.insert("slug".into(), json!(slug));
        body.insert("adopt".into(), json!(adopt));
        if let Some(name) = matches.get_one::<String>("name") {
            body.insert("name".into(), json!(name));
        }
        if let Some(description) = matches.get_one::<String>("description") {
            body.insert("description".into(), json!(description));
        }
        let body = serde_json::Value::Object(body);

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", "/templates/from-application", Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let out: Promotion = post_json(
            &auth_mode,
            "/templates/from-application",
            body,
            Missing::Resource(format!("application '{}'", application)),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&out)?);
            return Ok(());
        }

        log_ok!(
            stdout,
            "'{}' is now a managed template (version {} {})",
            out.template_slug.as_deref().unwrap_or(slug),
            out.version_semver.as_deref().unwrap_or("?"),
            out.version_status.as_deref().unwrap_or("?")
        );
        if adopt {
            log_info!(
                stdout,
                "{} deploy(s) adopted as instance(s):",
                out.adopted.len()
            );
            for instance in &out.adopted {
                writeln!(
                    stdout,
                    "    {:<38} {:<12} {:<12} {}",
                    instance.id.as_deref().unwrap_or("-"),
                    instance.environment.as_deref().unwrap_or("production"),
                    instance.region.as_deref().unwrap_or("-"),
                    instance.host.as_deref().unwrap_or("-")
                )?;
            }
            for skipped in &out.skipped {
                log_info!(
                    stdout,
                    "skipped {}/{}: {}",
                    skipped.environment,
                    skipped.region,
                    skipped.reason
                );
            }
        } else {
            log_info!(
                stdout,
                "Version 1 is building through the template pipeline; publish the template once it is built (`template publish-template --slug {}`).",
                slug
            );
        }
        writeln!(stdout)?;
        Ok(())
    }
}

use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, get_value, print_dryrun, require_managed_mode, resolve_managed_auth},
        types::{ManagedInstance, dash},
    },
};

#[derive(Debug)]
pub(super) struct GetCommand;

impl GetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for GetCommand {
    fn command(&self) -> Command {
        command("get", "Show one managed instance with its lifecycle detail")
            .long_about(
                "Show one managed instance with its lifecycle detail.\n\n\
                 This is the row a client polls to follow the state machine: every lifecycle\n\
                 command (create, reset, update, apply-variables) answers 202 with the state it\n\
                 moved TO, and the outcome lands here later. Poll every 20s or so until the\n\
                 state leaves provisioning / resetting / destroying, or `pendingUpdate` clears.\n\n\
                 `lastError` carries the platform's reason whenever the state is\n\
                 provisioning_failed or an update did not land. The row can say `active` while\n\
                 the service is down — check https://<host>/health separately when it matters.",
            )
            .arg(
                Arg::new("id")
                    .long("id")
                    .required(true)
                    .help("Id of the instance"),
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
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let path = format!("/instances/{}", urlencoding::encode(id));

        if matches.get_flag("dryrun") {
            return print_dryrun("GET", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let value = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("instance '{}'", id)),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&value)?);
            return Ok(());
        }

        let instance: ManagedInstance =
            serde_json::from_value(value).context("unexpected instance shape")?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "{}", dash(&instance.host))?;
        stdout.reset()?;
        writeln!(stdout)?;

        let row = |stdout: &mut StandardStream, label: &str, value: &str| -> Result<()> {
            writeln!(stdout, "  {:<22} {}", label, value)?;
            Ok(())
        };
        row(&mut stdout, "Id", dash(&instance.id))?;
        row(&mut stdout, "Template", dash(&instance.template_slug))?;
        row(&mut stdout, "Region", dash(&instance.region))?;
        row(&mut stdout, "State", dash(&instance.state))?;
        row(
            &mut stdout,
            "Version",
            instance
                .current_version_semver
                .as_deref()
                .unwrap_or("- (none recorded)"),
        )?;
        row(
            &mut stdout,
            "Tier",
            instance
                .instance_size
                .as_deref()
                .unwrap_or("pico (default)"),
        )?;
        row(
            &mut stdout,
            "Fleet updates",
            match instance.update_policy.as_deref() {
                Some("deferred") => "deferred",
                _ => "auto",
            },
        )?;
        if let Some(until) = instance.update_deferred_until.as_deref() {
            row(&mut stdout, "  deferred until", until)?;
        }
        row(
            &mut stdout,
            "Claimed",
            instance.claimed_at.as_deref().unwrap_or("- (not claimed)"),
        )?;
        row(
            &mut stdout,
            "App-side claim",
            dash(&instance.app_claimed_at),
        )?;
        row(
            &mut stdout,
            "Resets",
            &format!(
                "{}{}",
                instance.reset_count.unwrap_or(0),
                instance
                    .last_reset_at
                    .as_deref()
                    .map(|at| format!(" (last {})", at))
                    .unwrap_or_default()
            ),
        )?;
        row(
            &mut stdout,
            "Launch approval",
            dash(&instance.launch_approval_state),
        )?;
        row(
            &mut stdout,
            "Pending update",
            dash(&instance.pending_update),
        )?;
        row(
            &mut stdout,
            "Sign-in relay",
            match instance.relay_eligible {
                Some(true) => "eligible",
                Some(false) => "refused",
                None => "-",
            },
        )?;
        row(&mut stdout, "Application", dash(&instance.application_id))?;
        row(
            &mut stdout,
            "Latest deployment",
            dash(&instance.latest_deployment_id),
        )?;
        if let Some(url) = instance.frontend_url.as_deref() {
            row(&mut stdout, "Product URL", url)?;
        }
        if let Some(endpoints) = &instance.endpoints {
            for (name, url) in endpoints {
                row(&mut stdout, &format!("  {}", name), url)?;
            }
        }
        writeln!(stdout)?;

        if let Some(error) = instance.last_error.as_deref() {
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true))?;
            writeln!(stdout, "  Last error")?;
            stdout.reset()?;
            writeln!(stdout, "    {}", error)?;
            writeln!(stdout)?;
        }

        Ok(())
    }
}

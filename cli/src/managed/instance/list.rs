use std::io::Write;

use anyhow::Result;
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{
            extract_list, get_value_if_supported, require_managed_mode, resolve_managed_auth,
        },
        types::{INSTANCE_STATES, ManagedInstance, dash},
    },
};

#[derive(Debug)]
pub(super) struct ListCommand;

impl ListCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ListCommand {
    fn command(&self) -> Command {
        command(
            "list",
            "List the managed instances your organization is running",
        )
        .long_about(
            "List the managed instances your organization is running.\n\n\
                 Each row is one running copy of a template, provisioned for one end customer.\n\
                 Destroyed instances are not shown.\n\n\
                 --template and --state narrow the list. They are also applied locally, so they\n\
                 still work against a control plane whose instance list does not support query\n\
                 filters yet.",
        )
        .arg(
            Arg::new("template")
                .long("template")
                .help("Only show instances of this template slug"),
        )
        .arg(
            Arg::new("state")
                .long("state")
                .value_parser(INSTANCE_STATES.to_vec())
                .help("Only show instances in this lifecycle state"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let auth_mode = resolve_managed_auth()?;
        let summary = require_managed_mode(&auth_mode)?;

        let template_filter = matches.get_one::<String>("template");
        let state_filter = matches.get_one::<String>("state");
        let json_output = matches.get_flag("json");

        let mut query: Vec<String> = Vec::new();
        if let Some(template) = template_filter {
            query.push(format!("templateSlug={}", urlencoding::encode(template)));
        }
        if let Some(state) = state_filter {
            query.push(format!("state={}", urlencoding::encode(state)));
        }
        let path = if query.is_empty() {
            "/instances".to_string()
        } else {
            format!("/instances?{}", query.join("&"))
        };

        // Prefer the dedicated list endpoint, which can page and filter server-side. Not
        // every control plane mounts it yet, and the summary endpoint — already fetched
        // above for the availability check — carries the same instance projection, so
        // fall back to that rather than failing.
        let (instances, from_summary) = match get_value_if_supported(&auth_mode, &path)? {
            Some(value) => (
                extract_list::<ManagedInstance>(value, &["instances"])?,
                false,
            ),
            None => (summary.instances.clone(), true),
        };

        // Filter locally regardless of source. When the server honored the query this is
        // a no-op; when it ignored the query, or the data came from the summary, this is
        // what makes --template and --state mean anything.
        let instances: Vec<ManagedInstance> = instances
            .into_iter()
            .filter(
                |instance| match (template_filter, &instance.template_slug) {
                    (Some(wanted), Some(actual)) => actual == wanted,
                    (Some(_), None) => false,
                    (None, _) => true,
                },
            )
            .filter(|instance| match (state_filter, &instance.state) {
                (Some(wanted), Some(actual)) => actual == wanted,
                (Some(_), None) => false,
                (None, _) => true,
            })
            .collect();

        if json_output {
            println!("{}", serde_json::to_string_pretty(&instances)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Managed instances")?;
        stdout.reset()?;
        writeln!(stdout)?;

        if instances.is_empty() {
            if template_filter.is_some() || state_filter.is_some() {
                writeln!(stdout, "  No instances match those filters.")?;
            } else {
                writeln!(
                    stdout,
                    "  No instances running. Launch one with `forklaunch managed instance create`."
                )?;
            }
            writeln!(stdout)?;
            return Ok(());
        }

        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(
            stdout,
            "  {:<38} {:<20} {:<22} {:<14} {:<12}",
            "ID", "TEMPLATE", "HOST", "REGION", "STATE"
        )?;
        stdout.reset()?;
        for instance in &instances {
            writeln!(
                stdout,
                "  {:<38} {:<20} {:<22} {:<14} {:<12}",
                dash(&instance.id),
                dash(&instance.template_slug),
                dash(&instance.host),
                dash(&instance.region),
                dash(&instance.state),
            )?;
        }
        writeln!(stdout)?;

        let failed: Vec<&ManagedInstance> = instances
            .iter()
            .filter(|instance| instance.last_error.is_some())
            .collect();
        if !failed.is_empty() {
            stdout.set_color(ColorSpec::new().set_bold(true))?;
            writeln!(stdout, "  Errors")?;
            stdout.reset()?;
            for instance in failed {
                writeln!(
                    stdout,
                    "    {} — {}",
                    dash(&instance.id),
                    dash(&instance.last_error)
                )?;
            }
            writeln!(stdout)?;
        }

        if from_summary {
            log_info!(
                stdout,
                "This control plane has no dedicated instance list endpoint; the rows above came from the managed mode summary and filters were applied locally."
            );
        }

        Ok(())
    }
}

use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, extract_list, get_value, require_managed_mode, resolve_managed_auth},
        types::{InstanceVariable, SetState, dash, required_cell},
    },
};

/// What to print in the VALUE column.
///
/// Never the value. For `custom` this is the only thing this command exists to tell
/// you — whether the instance has one — and for the other two kinds the honest answer
/// is where the value comes from, since there is nothing instance-specific to report.
fn value_column(variable: &InstanceVariable) -> &'static str {
    match variable.kind.as_deref() {
        Some("static") => "(from template)",
        Some("generated") => "(derived per instance)",
        Some("custom") => match variable.set_state() {
            SetState::Set => "SET",
            SetState::Missing => "MISSING",
            SetState::Unknown => "?",
        },
        _ => "-",
    }
}

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
            "Show every variable an instance receives, and which custom ones still need a value",
        )
        .long_about(
            "Show every variable an instance receives, and which custom ones still need a value.\n\n\
             This lists the whole declaration from the template, not just what has been filled\n\
             in, so a variable nobody has touched still appears — as MISSING rather than as\n\
             nothing at all.\n\n\
             THE VALUE COLUMN NEVER CONTAINS A VALUE. For a `custom` variable it says SET or\n\
             MISSING; for `static` and `generated` it says where the value comes from. There\n\
             is no flag that changes this and no other command that reads a value back — once\n\
             a value is set, the only things that can read it are the provisioner and the\n\
             deployed app.\n\n\
             A row that is both REQUIRED and MISSING will stop this instance from being\n\
             provisioned. That is deliberate: failing at launch is better than deploying an\n\
             app that boots and then misbehaves because an environment variable was absent.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance whose variables should be listed"),
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

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let path = format!("/instances/{}/variables", urlencoding::encode(id));
        let value = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("instance '{}'", id)),
        )?;
        let variables: Vec<InstanceVariable> = extract_list(value, &["variables"])?;

        if matches.get_flag("json") {
            // `InstanceVariable::value` is `skip_serializing`, so a control plane that
            // sent values back cannot leak them through `--json` either.
            println!("{}", serde_json::to_string_pretty(&variables)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Variables for instance {}", id)?;
        stdout.reset()?;
        writeln!(stdout)?;

        if variables.is_empty() {
            writeln!(
                stdout,
                "  This instance's template declares no variables. Declare one with `forklaunch managed template vars set`."
            )?;
            writeln!(stdout)?;
            return Ok(());
        }

        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(
            stdout,
            "  {:<24} {:<10} {:<12} {:<14} {:<9} VALUE",
            "KEY", "KIND", "SCOPE", "SERVICE", "REQUIRED"
        )?;
        stdout.reset()?;
        for variable in &variables {
            writeln!(
                stdout,
                "  {:<24} {:<10} {:<12} {:<14} {:<9} {}",
                dash(&variable.key),
                dash(&variable.kind),
                dash(&variable.scope),
                dash(&variable.service_name),
                required_cell(&variable.kind, &variable.required),
                value_column(variable),
            )?;
        }
        writeln!(stdout)?;

        let blocking: Vec<&str> = variables
            .iter()
            .filter(|variable| variable.blocks_provisioning())
            .map(|variable| variable.key.as_deref().unwrap_or("(unnamed)"))
            .collect();

        if blocking.is_empty() {
            log_info!(
                stdout,
                "Values are never shown. SET means this instance has one; MISSING means it does not."
            );
        } else {
            log_warn!(
                stdout,
                "This instance CANNOT be provisioned yet — required and missing: {}",
                blocking.join(", ")
            );
            log_info!(
                stdout,
                "Supply each with: forklaunch managed instance vars set --id {} --key <key> --value <value>",
                id
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(required: bool, has_value: Option<bool>) -> InstanceVariable {
        InstanceVariable {
            key: Some("STRIPE_KEY".to_string()),
            kind: Some("custom".to_string()),
            required: Some(required),
            has_value,
            ..Default::default()
        }
    }

    #[test]
    fn a_custom_variable_reports_set_or_missing_but_never_a_value() {
        assert_eq!(value_column(&custom(true, Some(true))), "SET");
        assert_eq!(value_column(&custom(true, Some(false))), "MISSING");
    }

    #[test]
    fn an_unstated_set_state_is_reported_as_unknown_not_guessed() {
        // Guessing MISSING would send someone hunting for a value that is already
        // there; guessing SET would hide a variable that is about to fail the provision.
        assert_eq!(value_column(&custom(true, None)), "?");
        assert_eq!(custom(true, None).set_state(), SetState::Unknown);
    }

    #[test]
    fn set_state_falls_back_to_the_presence_of_a_value_the_server_should_not_have_sent() {
        let variable = InstanceVariable {
            kind: Some("custom".to_string()),
            value: Some("sk_live_supersecret".to_string()),
            ..Default::default()
        };
        assert_eq!(variable.set_state(), SetState::Set);
        assert_eq!(value_column(&variable), "SET");
    }

    #[test]
    fn a_value_the_control_plane_sent_anyway_is_never_rendered() {
        // The guarantee this command makes. Both the table cell and the `--json` form
        // have to be free of it, since `--json` serializes these same structs.
        let variables = vec![InstanceVariable {
            key: Some("STRIPE_KEY".to_string()),
            kind: Some("custom".to_string()),
            value: Some("sk_live_supersecret".to_string()),
            ..Default::default()
        }];
        assert!(!value_column(&variables[0]).contains("supersecret"));
        let rendered = serde_json::to_string(&variables).unwrap();
        assert!(!rendered.contains("supersecret"), "{}", rendered);
    }

    #[test]
    fn an_empty_value_counts_as_missing() {
        // Matches the provisioner, which treats '' the same as absent.
        let variable = InstanceVariable {
            kind: Some("custom".to_string()),
            value: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(variable.set_state(), SetState::Missing);
    }

    #[test]
    fn only_required_and_missing_custom_variables_block_provisioning() {
        assert!(custom(true, Some(false)).blocks_provisioning());
        assert!(!custom(false, Some(false)).blocks_provisioning());
        assert!(!custom(true, Some(true)).blocks_provisioning());
        // A generated variable is always derivable, so it can never block, even if a
        // control plane marked it required.
        let generated = InstanceVariable {
            kind: Some("generated".to_string()),
            required: Some(true),
            ..Default::default()
        };
        assert!(!generated.blocks_provisioning());
    }

    #[test]
    fn static_and_generated_rows_explain_themselves_rather_than_reporting_set_ness() {
        let static_variable = InstanceVariable {
            kind: Some("static".to_string()),
            ..Default::default()
        };
        assert_eq!(value_column(&static_variable), "(from template)");

        let generated = InstanceVariable {
            kind: Some("generated".to_string()),
            ..Default::default()
        };
        assert_eq!(value_column(&generated), "(derived per instance)");
    }

    #[test]
    fn set_ness_is_read_from_whichever_field_name_the_control_plane_used() {
        // The endpoint is still being written; `hasValue`, `isSet` and `set` are all
        // plausible names for the same fact. Reading only one would render every
        // variable MISSING if the server picked another.
        for field in ["hasValue", "isSet", "set"] {
            let json = format!(r#"{{"key":"K","kind":"custom","{}":true}}"#, field);
            let variable: InstanceVariable = serde_json::from_str(&json).unwrap();
            assert_eq!(variable.set_state(), SetState::Set, "{}", field);
        }
    }
}

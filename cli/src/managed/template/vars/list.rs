use std::io::Write;

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, extract_list, get_value, require_managed_mode, resolve_managed_auth},
        types::{TemplateVariable, dash, required_cell},
    },
};

/// What to show a reader in the SOURCE column: where this variable's value comes from.
///
/// A `static` row reports only that the template holds the value, never what it is. The
/// control plane does not send it — `TemplateVariableSchema` omits `value` on both the
/// managed-apps handler and the `/managed-mode` proxy, on the reasoning that a static
/// value can be a credential every instance shares and a list endpoint is the wrong
/// place to hand one back. That matches the question a list answers, which is "is this
/// configured", not "what is it".
fn source(variable: &TemplateVariable) -> String {
    match variable.kind.as_deref() {
        Some("static") => "set on template".to_string(),
        Some("generated") => dash(&variable.generator_type).to_string(),
        Some("custom") => "set per instance".to_string(),
        _ => "-".to_string(),
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
        command("list", "List the variables a template declares")
            .long_about(
                "List the variables a template declares, and where each one's value comes from.\n\n\
                 The SOURCE column reads differently per kind: for `generated` it is the recipe\n\
                 each instance derives from, for `custom` it says the value lives on the\n\
                 instance, and for `static` it says only that the template holds one.\n\n\
                 STATIC VALUES ARE NOT READABLE BACK, here or anywhere else in the CLI. The\n\
                 control plane does not return them — a static value can be a credential every\n\
                 instance shares, so listing is not the place to hand one out. This command\n\
                 tells you a variable IS configured, not what it is set to. To change one,\n\
                 declare it again with the new value:\n\
                 \x20 forklaunch managed template vars set --slug <slug> --key <key> --kind static --value <new>\n\n\
                 To see whether a particular instance has values for the `custom` ones, use\n\
                 `forklaunch managed instance vars list --id <instance-id>`.",
            )
            .arg(
                Arg::new("slug")
                    .long("slug")
                    .required(true)
                    .help("Slug of the template whose variables should be listed"),
            )
            .arg(
                Arg::new("json")
                    .long("json")
                    .help("Output raw JSON instead of formatted terminal output")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        let path = format!("/templates/{}/variables", urlencoding::encode(slug));
        let value = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("template '{}'", slug)),
        )?;
        let variables: Vec<TemplateVariable> = extract_list(value, &["variables"])?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&variables)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Variables declared by template '{}'", slug)?;
        stdout.reset()?;
        writeln!(stdout)?;

        if variables.is_empty() {
            writeln!(
                stdout,
                "  This template declares no variables. Add one with `forklaunch managed template vars set`."
            )?;
            writeln!(stdout)?;
            return Ok(());
        }

        stdout.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(
            stdout,
            "  {:<24} {:<10} {:<12} {:<14} {:<9} SOURCE",
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
                source(variable),
            )?;
        }
        writeln!(stdout)?;

        // Said only when there is a static row to explain. A blanket note on a template
        // with no static variables would be answering a question nobody asked.
        if variables
            .iter()
            .any(|variable| variable.kind.as_deref() == Some("static"))
        {
            log_info!(
                stdout,
                "Static values are not readable back — this shows that a value is set, not what it is. To change one, set it again with a new --value."
            );
        }

        if variables.iter().any(|variable| {
            variable.kind.as_deref() == Some("custom") && variable.required == Some(true)
        }) {
            log_info!(
                stdout,
                "Variables marked required must have a value on an instance before it can be provisioned. Check one with: forklaunch managed instance vars list --id <instance-id>"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_static_row_states_that_a_value_is_set_without_stating_the_value() {
        let variable = TemplateVariable {
            key: Some("SHARED_API_KEY".to_string()),
            kind: Some("static".to_string()),
            ..Default::default()
        };
        assert_eq!(source(&variable), "set on template");
    }

    #[test]
    fn the_parsed_shape_has_nowhere_to_put_a_value_the_server_might_send() {
        // The guarantee, enforced by the type rather than by a redaction step: the
        // control plane omits `value` from TemplateVariableSchema, and this struct has
        // no field for it, so a server that started sending one could not leak it
        // through the table or through `--json`.
        let variables: Vec<TemplateVariable> = serde_json::from_str(
            r#"[{"key":"SHARED_API_KEY","kind":"static","scope":"application","value":"sk_live_supersecret"}]"#,
        )
        .unwrap();
        assert_eq!(source(&variables[0]), "set on template");
        let rendered = serde_json::to_string(&variables).unwrap();
        assert!(!rendered.contains("supersecret"), "{}", rendered);
    }

    #[test]
    fn the_source_column_reads_differently_per_kind() {
        let generated = TemplateVariable {
            kind: Some("generated".to_string()),
            generator_type: Some("32-bytes-base64".to_string()),
            ..Default::default()
        };
        assert_eq!(source(&generated), "32-bytes-base64");

        let custom = TemplateVariable {
            kind: Some("custom".to_string()),
            ..Default::default()
        };
        assert_eq!(source(&custom), "set per instance");
    }

    #[test]
    fn the_control_planes_bare_array_response_parses() {
        // Both layers declare `200: array(TemplateVariableSchema)` — a bare array, not a
        // wrapped one. `required` is non-optional server-side; the rest may be absent.
        let variables: Vec<TemplateVariable> = extract_list(
            serde_json::json!([
                {"key":"LOG_LEVEL","scope":"application","kind":"static","required":false},
                {"key":"SESSION_SECRET","scope":"application","kind":"generated","generatorType":"32-bytes-base64","required":false}
            ]),
            &["variables"],
        )
        .unwrap();
        assert_eq!(variables.len(), 2);
        assert_eq!(source(&variables[0]), "set on template");
        assert_eq!(source(&variables[1]), "32-bytes-base64");
    }
}

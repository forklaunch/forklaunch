use anyhow::{Result, bail};
use clap::{ArgMatches, Command};

use crate::{CliCommand, core::command::command};

mod list;
mod set;
mod unset;

use list::ListCommand;
use set::SetCommand;
use unset::UnsetCommand;

/// Validates the `--scope` / `--service` pair, which is the one coupling between two
/// flags that the control plane cannot report as clearly as the CLI can: server-side it
/// surfaces as a schema violation on `serviceName`, which does not mention `--scope` at
/// all.
///
/// Returns the service name to send, if any.
pub(super) fn resolve_scope<'a>(scope: &str, service: Option<&'a str>) -> Result<Option<&'a str>> {
    match (scope, service) {
        ("service", None) => bail!(
            "--service is required when --scope service — a service-scoped variable reaches \
             exactly one service in the deployed app, so it has to name which one. Drop \
             --scope to make it application-scoped instead, which reaches every service."
        ),
        ("application", Some(name)) => bail!(
            "--service '{}' is meaningless with --scope application — an application-scoped \
             variable already reaches every service. Pass --scope service to target just '{}'.",
            name,
            name
        ),
        (_, service) => Ok(service),
    }
}

#[derive(Debug)]
pub(super) struct VarsCommand {
    list: ListCommand,
    set: SetCommand,
    unset: UnsetCommand,
}

impl VarsCommand {
    pub(super) fn new() -> Self {
        Self {
            list: ListCommand::new(),
            set: SetCommand::new(),
            unset: UnsetCommand::new(),
        }
    }
}

impl CliCommand for VarsCommand {
    fn command(&self) -> Command {
        command(
            "vars",
            "Declare the variables every instance of a template receives",
        )
        .long_about(
            "Declare the variables every instance of a template receives.\n\n\
             A template declares WHAT each of its instances needs as a deployment\n\
             environment variable. How the value is arrived at differs by KIND, and picking\n\
             the right one is the whole point of this command:\n\n\
             \x20 static     the same literal for every instance (LOG_LEVEL=info). The\n\
             \x20            template holds the value, so every customer gets the same one.\n\
             \x20 generated  a recipe, not a value. Each instance derives its OWN value,\n\
             \x20            seeded on its instance id so a provisioning retry re-derives the\n\
             \x20            same one. The template stores no secret at all.\n\
             \x20 custom     you set the value per instance (one customer's own Stripe key).\n\
             \x20            The template only declares that the variable exists; the value\n\
             \x20            lives on each instance — see `managed instance vars`.\n\n\
             If you are about to use `static` for a secret, you almost certainly want\n\
             `generated` instead: a static secret is one secret shared across every\n\
             customer's instance, and it sits in the template at rest.\n\n\
             SCOPE decides how far a variable reaches. `application` (the default) reaches\n\
             every service in the deployed app; `service` reaches exactly one named service\n\
             and requires --service.\n\n\
             A `custom` variable may be marked --required, which means an instance CANNOT be\n\
             provisioned until it has a value. That is a deliberate launch-time failure: the\n\
             alternative is deploying an app that boots and then misbehaves in a way nobody\n\
             traces back to a missing environment variable.",
        )
        .subcommand(self.list.command())
        .subcommand(self.set.command())
        .subcommand(self.unset.command())
        .subcommand_required(true)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("list", sub_matches)) => self.list.handler(sub_matches),
            Some(("set", sub_matches)) => self.set.handler(sub_matches),
            Some(("unset", sub_matches)) => self.unset.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_scope_without_a_service_name_is_refused() {
        let error = resolve_scope("service", None).unwrap_err().to_string();
        assert!(error.contains("--service is required"), "{}", error);
    }

    #[test]
    fn application_scope_with_a_service_name_is_refused() {
        // Accepting this would be worse than rejecting it: the control plane would store
        // an application-scoped variable and silently drop the service name, so the
        // variable would reach every service when the operator asked for one.
        let error = resolve_scope("application", Some("billing"))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("meaningless with --scope application"),
            "{}",
            error
        );
        assert!(error.contains("billing"), "{}", error);
    }

    #[test]
    fn matching_scope_and_service_pass_through() {
        assert_eq!(
            resolve_scope("service", Some("billing")).unwrap(),
            Some("billing")
        );
        assert_eq!(resolve_scope("application", None).unwrap(), None);
    }
}

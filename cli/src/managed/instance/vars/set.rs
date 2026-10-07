use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::json;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::client::{
        Missing, print_dryrun, put_json_optional, require_managed_mode, resolve_managed_auth,
    },
};

#[derive(Debug)]
pub(super) struct SetCommand;

impl SetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for SetCommand {
    fn command(&self) -> Command {
        command("set", "Set one instance's value for a custom variable")
            .long_about(
                "Set one instance's value for a custom variable.\n\n\
                 The key must already be declared on the instance's template with\n\
                 `--kind custom`. This command supplies a value for a variable that exists; it\n\
                 does not create one. The other two kinds take no value here: `static` values\n\
                 live on the template, and `generated` values are derived by the instance.\n\n\
                 There is no --scope or --service. The template's declaration already fixed how\n\
                 far this variable reaches; an instance supplies the value, not the scoping.\n\n\
                 The value is not printed back, here or by `vars list`. Re-running with a\n\
                 different value replaces the old one.\n\n\
                 Setting a value that was REQUIRED and MISSING unblocks provisioning; the\n\
                 instance does not redeploy on its own, so it takes effect at its next\n\
                 provision.",
            )
            .arg(
                Arg::new("id")
                    .long("id")
                    .required(true)
                    .help("Id of the instance to set the value on"),
            )
            .arg(
                Arg::new("key")
                    .long("key")
                    .required(true)
                    .help("Name of the custom variable the template declared"),
            )
            .arg(
                Arg::new("value")
                    .long("value")
                    .required(true)
                    .help("Value this instance should receive"),
            )
            .arg(
                Arg::new("dryrun")
                    .long("dryrun")
                    .help("Print the request that would be sent, with the value redacted")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let key = matches
            .get_one::<String>("key")
            .context("--key is required")?;
        let value = matches
            .get_one::<String>("value")
            .context("--value is required")?;

        if key.trim().is_empty() {
            bail!("--key cannot be empty");
        }
        // Same paste error as on the template side: `--key FOO=bar` would set a value on
        // a variable named `FOO=bar`, which no template ever declared, so it would fail
        // server-side with a not-found that does not explain itself.
        if key.contains('=') {
            bail!(
                "--key '{}' looks like a KEY=VALUE pair — pass the name and the value separately: \
                 --key {} --value {}",
                key,
                key.split_once('=').map(|pair| pair.0).unwrap_or(key),
                key.split_once('=').map(|pair| pair.1).unwrap_or("<value>")
            );
        }

        let path = format!("/instances/{}/variables", urlencoding::encode(id));

        if matches.get_flag("dryrun") {
            // --dryrun prints the request, and a real credential is exactly what is
            // being passed here — so it prints the shape, not the secret. Same treatment
            // `instance claim` gives the one-time token.
            let redacted = json!({
                "key": key,
                "value": "<redacted — the real value would be sent here>",
            });
            return print_dryrun("PUT", &path, Some(&redacted));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;

        put_json_optional(
            &path,
            json!({ "key": key, "value": value }),
            // A 404 here is most likely the key: the instance is one the caller just
            // listed, whereas the key is typed by hand and must match a `custom`
            // declaration on the template.
            Missing::Custom(format!(
                "no custom variable '{}' is declared for instance '{}' — either the instance \
                 does not exist, or its template does not declare '{}' with `--kind custom`. \
                 Check with: forklaunch managed instance vars list --id {}",
                key, id, key, id
            )),
        )?;

        log_ok!(stdout, "Set '{}' for instance {}", key, id);
        log_info!(
            stdout,
            "The value is not shown back. Confirm it registered with: forklaunch managed instance vars list --id {}",
            id
        );

        Ok(())
    }
}

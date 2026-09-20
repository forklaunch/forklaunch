use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Value, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::list::{fetch_relay_config, print_relay_config};
use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{
            Missing, print_dryrun, put_json_optional, require_managed_mode, resolve_managed_auth,
        },
        types::{RELAY_ROUTE_MODES, RelayRoute},
    },
};

/// Validates a route the way the control plane does, so a bad name or path is an
/// instant local error rather than a 400 after a round trip.
pub(super) fn validate_route(name: &str, component: &str, path: &str) -> Result<()> {
    let name_ok = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_lowercase() || b.is_ascii_digit() || (i > 0 && b == b'-'));
    if !name_ok {
        bail!(
            "--name '{}' must be 1-64 lowercase letters, digits or dashes (it becomes part of \
             the callback URL: .../callback/<name>)",
            name
        );
    }
    let component_ok = !component.is_empty()
        && component.len() <= 64
        && component
            .bytes()
            .enumerate()
            .all(|(i, b)| b.is_ascii_alphanumeric() || (i > 0 && b == b'-'));
    if !component_ok {
        bail!(
            "--component '{}' must be a service name from the manifest (letters, digits, dashes) \
             — it becomes the hostname label <prefix>-<component>",
            component
        );
    }
    if !path.starts_with('/') || path.contains(['?', '#']) || path.contains(char::is_whitespace) {
        bail!(
            "--path '{}' must be root-relative (start with /) and carry no query or fragment — \
             the provider's query is appended at relay time",
            path
        );
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct SetCommand;

impl SetCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for SetCommand {
    fn command(&self) -> Command {
        command(
            "set",
            "Declare (or replace) one relay route: which component and path handle a provider's callback",
        )
        .long_about(
            "Declare one relay route on a template, replacing a route of the same name.\n\n\
             Other routes are kept. Example — Epic's callback finishes on the `vault`\n\
             service, which holds the PKCE verifier:\n\n\
             \x20 forklaunch managed template relay set --slug health-vault \\\n\
             \x20     --name default --component vault --path /epic/callback --mode redirect\n\n\
             `default` answers at the bare .../callback (the URI already registered with the\n\
             provider); any other name answers at .../callback/<name>.",
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
                .default_value("default")
                .help("Route name; `default` is served at the bare /callback"),
        )
        .arg(
            Arg::new("component")
                .long("component")
                .required(true)
                .help("The instance service that handles the callback (a manifest service name)"),
        )
        .arg(
            Arg::new("path")
                .long("path")
                .required(true)
                .help("Root-relative path on that component, e.g. /epic/callback"),
        )
        .arg(
            Arg::new("mode")
                .long("mode")
                .value_parser(RELAY_ROUTE_MODES.to_vec())
                .default_value("redirect")
                .help("redirect (browser is 302'd there) | forward (HMAC POST over the mesh)"),
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
        let slug = matches
            .get_one::<String>("slug")
            .context("--slug is required")?;
        let name = matches.get_one::<String>("name").context("--name")?;
        let component = matches
            .get_one::<String>("component")
            .context("--component is required")?;
        let path = matches
            .get_one::<String>("path")
            .context("--path is required")?;
        let mode = matches.get_one::<String>("mode").context("--mode")?;
        validate_route(name, component, path)?;

        // The control plane replaces the whole list, so read the current routes and
        // splice this one in: `set` must not silently drop a product's other routes.
        let current = if matches.get_flag("dryrun") {
            Vec::new()
        } else {
            fetch_relay_config(slug)?.routes
        };
        let mut routes: Vec<RelayRoute> = current
            .into_iter()
            .filter(|route| route.name != *name)
            .map(|route| RelayRoute {
                callback_url: None,
                ..route
            })
            .collect();
        routes.push(RelayRoute {
            name: name.clone(),
            component: component.clone(),
            path: path.clone(),
            mode: mode.clone(),
            callback_url: None,
        });
        let body = json!({ "routes": routes });
        let request_path = format!("/templates/{}/relay-routes", urlencoding::encode(slug));

        if matches.get_flag("dryrun") {
            return print_dryrun("PUT", &request_path, Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        let response: Option<Value> = put_json_optional(
            &request_path,
            body,
            Missing::Resource(format!("template '{}'", slug)),
        )?;

        if matches.get_flag("json") {
            println!(
                "{}",
                serde_json::to_string_pretty(&response.unwrap_or(Value::Null))?
            );
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        log_ok!(
            stdout,
            "Route '{}' on template '{}': {} {} → component '{}' at {}",
            name,
            slug,
            mode,
            if name == "default" {
                "/callback"
            } else {
                "/callback/<name>"
            },
            component,
            path
        );
        let config = match response {
            Some(value) => {
                serde_json::from_value(value).context("Failed to parse the relay config")?
            }
            None => fetch_relay_config(slug)?,
        };
        print_relay_config(slug, &config)?;
        log_info!(
            stdout,
            "The next `template publish` checks that '{}' exposes {} {} — a route the build does not serve is refused at publish.",
            component,
            if mode == "redirect" { "GET" } else { "POST" },
            path
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_validation_mirrors_the_control_plane() {
        assert!(validate_route("default", "vault", "/epic/callback").is_ok());
        assert!(validate_route("stripe-webhook", "billing", "/webhooks/stripe").is_ok());
        assert!(
            validate_route("Epic", "vault", "/x").is_err(),
            "uppercase name"
        );
        assert!(validate_route("-x", "vault", "/x").is_err(), "leading dash");
        assert!(
            validate_route("x", "vault.evil.com", "/x").is_err(),
            "dotted component"
        );
        assert!(
            validate_route("x", "vault", "epic/callback").is_err(),
            "relative path"
        );
        assert!(
            validate_route("x", "vault", "/x?y=1").is_err(),
            "query in path"
        );
        assert!(
            validate_route("x", "vault", "/x#f").is_err(),
            "fragment in path"
        );
    }

    #[test]
    fn the_cli_mode_list_matches_what_the_control_plane_validates() {
        assert_eq!(RELAY_ROUTE_MODES, &["redirect", "forward"]);
    }
}

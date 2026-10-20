use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client::{self, make_authenticated_request},
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct DomainCommand {
    status: StatusCommand,
    set: SetCommand,
    remove: RemoveCommand,
    subdomain: SubdomainCommand,
}

impl DomainCommand {
    pub(crate) fn new() -> Self {
        Self {
            status: StatusCommand::new(),
            set: SetCommand::new(),
            remove: RemoveCommand::new(),
            subdomain: SubdomainCommand::new(),
        }
    }
}

impl CliCommand for DomainCommand {
    fn command(&self) -> Command {
        command("domain", "Manage this application's custom domain")
            .subcommand_required(true)
            .subcommand(self.status.command())
            .subcommand(self.set.command())
            .subcommand(self.remove.command())
            .subcommand(self.subdomain.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("status", sub_matches)) => self.status.handler(sub_matches),
            Some(("set", sub_matches)) => self.set.handler(sub_matches),
            Some(("remove", sub_matches)) => self.remove.handler(sub_matches),
            Some(("subdomain", sub_matches)) => self.subdomain.handler(sub_matches),
            _ => unreachable!(),
        }
    }
}

fn base_path_arg() -> Arg {
    Arg::new("base_path")
        .short('p')
        .long("path")
        .help("Path to application root (optional)")
}

/// Every write here resolves the application the same way `status` does.
fn application_id(matches: &ArgMatches) -> Result<String> {
    let _token = require_auth()?;
    let (_app_root, manifest) = require_manifest(matches)?;
    require_integration(&manifest)
}

#[derive(Debug)]
struct SetCommand;
impl SetCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for SetCommand {
    fn command(&self) -> Command {
        command("set", "Attach a custom domain to this application")
            .long_about(
                "Attach a custom domain to this application.\n\n\
                 Delegated mode (the default) asks you to point the domain's nameservers \
                 at the platform, which then manages its records. Use --mode manual for an \
                 apex domain whose MX records you need to keep. Either way the domain is \
                 not live until its validation records resolve — check with `app domain status`.",
            )
            .arg(
                Arg::new("domain")
                    .required(true)
                    .help("The domain to attach (e.g. app.example.com)"),
            )
            .arg(
                Arg::new("mode")
                    .long("mode")
                    .value_parser(["delegated", "manual"])
                    .help("delegated (default) or manual, for an apex domain"),
            )
            .arg(base_path_arg())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let app_id = application_id(matches)?;
        let domain = matches
            .get_one::<String>("domain")
            .context("domain is required")?;
        let mut body = serde_json::json!({ "domain": domain });
        if let Some(mode) = matches.get_one::<String>("mode") {
            body["mode"] = serde_json::Value::String(mode.clone());
        }

        let url = format!(
            "{}/applications/{}/custom-domain",
            get_platform_management_api_url(),
            urlencoding::encode(&app_id)
        );
        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to attach custom domain ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Attached")?;
        stdout.reset()?;
        writeln!(stdout, "  {}", domain)?;
        writeln!(
            stdout,
            "  It is not live until validation passes: forklaunch app domain status"
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct RemoveCommand;
impl RemoveCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for RemoveCommand {
    fn command(&self) -> Command {
        command("remove", "Detach this application's custom domain").arg(base_path_arg())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let app_id = application_id(matches)?;
        let url = format!(
            "{}/applications/{}/custom-domain",
            get_platform_management_api_url(),
            urlencoding::encode(&app_id)
        );
        let response = make_authenticated_request(Method::DELETE, &url, None)
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to remove custom domain ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Removed")?;
        stdout.reset()?;
        writeln!(stdout, "  the custom domain")?;
        Ok(())
    }
}

#[derive(Debug)]
struct SubdomainCommand {
    add: SubdomainAddCommand,
    remove: SubdomainRemoveCommand,
    list: SubdomainListCommand,
}
impl SubdomainCommand {
    fn new() -> Self {
        Self {
            add: SubdomainAddCommand,
            remove: SubdomainRemoveCommand,
            list: SubdomainListCommand,
        }
    }
}
impl CliCommand for SubdomainCommand {
    fn command(&self) -> Command {
        command(
            "subdomain",
            "Route a subdomain of the custom domain to one service",
        )
        .subcommand(self.add.command())
        .subcommand(self.remove.command())
        .subcommand(self.list.command())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("add", m)) => self.add.handler(m),
            Some(("remove", m)) => self.remove.handler(m),
            Some(("list", m)) => self.list.handler(m),
            _ => self.list.handler(matches),
        }
    }
}

#[derive(Debug)]
struct SubdomainAddCommand;
impl CliCommand for SubdomainAddCommand {
    fn command(&self) -> Command {
        command("add", "Point a subdomain at a service")
            .arg(
                Arg::new("prefix")
                    .required(true)
                    .help("The part in front of the custom domain (e.g. shop, eu.api)"),
            )
            .arg(
                Arg::new("service")
                    .long("service")
                    .short('s')
                    .required(true)
                    .help("Service id whose load balancer answers for it"),
            )
            .arg(
                Arg::new("environment")
                    .long("environment")
                    .short('e')
                    .help("Restrict to one environment's deployment (default: any)"),
            )
            .arg(base_path_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let app_id = application_id(matches)?;
        let prefix = matches
            .get_one::<String>("prefix")
            .context("prefix is required")?;
        let service = matches
            .get_one::<String>("service")
            .context("service is required")?;
        let mut body = serde_json::json!({ "prefix": prefix, "serviceId": service });
        if let Some(env) = matches.get_one::<String>("environment") {
            body["environmentName"] = serde_json::Value::String(env.clone());
        }
        let url = format!(
            "{}/applications/{}/custom-domain/subdomains",
            get_platform_management_api_url(),
            urlencoding::encode(&app_id)
        );
        let response =
            http_client::post(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to add subdomain ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Added")?;
        stdout.reset()?;
        writeln!(stdout, "  subdomain {}", prefix)?;
        Ok(())
    }
}

#[derive(Debug)]
struct SubdomainRemoveCommand;
impl CliCommand for SubdomainRemoveCommand {
    fn command(&self) -> Command {
        command("remove", "Remove a subdomain route")
            .arg(
                Arg::new("subdomain_id")
                    .required(true)
                    .help("Subdomain id (from `app domain subdomain list`)"),
            )
            .arg(base_path_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let app_id = application_id(matches)?;
        let id = matches
            .get_one::<String>("subdomain_id")
            .context("subdomain id is required")?;
        let url = format!(
            "{}/applications/{}/custom-domain/subdomains/{}",
            get_platform_management_api_url(),
            urlencoding::encode(&app_id),
            urlencoding::encode(id)
        );
        let response = make_authenticated_request(Method::DELETE, &url, None)
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to remove subdomain ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        write!(stdout, "  Removed")?;
        stdout.reset()?;
        writeln!(stdout, "  subdomain {}", id)?;
        Ok(())
    }
}

#[derive(Debug)]
struct SubdomainListCommand;
impl CliCommand for SubdomainListCommand {
    fn command(&self) -> Command {
        command("list", "List subdomain routes").arg(base_path_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let app_id = application_id(matches)?;
        let url = format!(
            "{}/applications/{}/custom-domain/subdomains",
            get_platform_management_api_url(),
            urlencoding::encode(&app_id)
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to list subdomains: {}",
                response.text().unwrap_or_default()
            );
        }
        let body: serde_json::Value = response
            .json()
            .with_context(|| "Failed to parse subdomains response")?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout, "{}", serde_json::to_string_pretty(&body)?)?;
        Ok(())
    }
}

#[derive(Debug)]
struct StatusCommand;

impl StatusCommand {
    fn new() -> Self {
        Self
    }
}

impl CliCommand for StatusCommand {
    fn command(&self) -> Command {
        command(
            "status",
            "Show custom-domain validation status for this application",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;

        let url = format!(
            "{}/applications/{}/custom-domain",
            get_platform_management_api_url(),
            urlencoding::encode(&application_id)
        );
        let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        if response.status().as_u16() == 204 {
            let mut stdout = StandardStream::stdout(ColorChoice::Always);
            writeln!(stdout, "No custom domain configured for this application.")?;
            return Ok(());
        }
        if !response.status().is_success() {
            bail!(
                "Failed to get custom domain status: {}",
                response.text().unwrap_or_default()
            );
        }

        let domain: CustomDomainStatus = response
            .json()
            .with_context(|| "Failed to parse custom domain response")?;

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        writeln!(stdout)?;
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "{}", domain.domain)?;
        stdout.reset()?;
        writeln!(stdout)?;
        writeln!(stdout, "  status:   {}", domain.status)?;
        if let Some(err) = &domain.error_message {
            writeln!(stdout, "  error:    {}", err)?;
        }
        if let Some(records) = &domain.validation_records {
            if !records.is_empty() {
                writeln!(stdout)?;
                writeln!(stdout, "  DNS validation records:")?;
                for r in records {
                    writeln!(
                        stdout,
                        "    {} {} -> {}",
                        r.record_type.as_deref().unwrap_or("CNAME"),
                        r.name.as_deref().unwrap_or("-"),
                        r.value.as_deref().unwrap_or("-")
                    )?;
                }
            }
        }
        writeln!(stdout)?;

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationRecord {
    #[serde(default, rename = "type")]
    record_type: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CustomDomainStatus {
    #[serde(default)]
    id: Option<String>,
    domain: String,
    status: String,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    validation_records: Option<Vec<ValidationRecord>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domain_cmd() -> Command {
        DomainCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        domain_cmd().debug_assert();
    }

    #[test]
    fn requires_a_subcommand() {
        assert!(domain_cmd().try_get_matches_from(["domain"]).is_err());
        assert!(
            domain_cmd()
                .try_get_matches_from(["domain", "status"])
                .is_ok()
        );
    }

    #[test]
    fn custom_domain_status_deserializes() {
        let json = r#"{
            "id": "cd-1",
            "domain": "example.com",
            "status": "validated",
            "validationRecords": [{"type": "CNAME", "name": "_acme.example.com", "value": "abc.acm-validations.aws"}]
        }"#;
        let domain: CustomDomainStatus = serde_json::from_str(json).unwrap();
        assert_eq!(domain.domain, "example.com");
        assert_eq!(domain.status, "validated");
        assert_eq!(domain.validation_records.unwrap().len(), 1);
    }
}

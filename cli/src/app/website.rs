use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use reqwest::{Method, blocking::Client, redirect::Policy};

use crate::{
    CliCommand,
    constants::get_platform_management_api_url,
    core::{
        command::command,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct WebsiteCommand;
impl WebsiteCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}
impl CliCommand for WebsiteCommand {
    fn command(&self) -> Command {
        command("website", "Read the public website URL or choose website hosting; does not deploy")
            .arg(Arg::new("environment").long("environment").required(true))
            .arg(Arg::new("region").long("region").required(true))
            .arg(Arg::new("enable").long("enable").action(ArgAction::SetTrue).conflicts_with("disable").help("Save separate website hosting for this environment; deployment is a separate step"))
            .arg(Arg::new("disable").long("disable").action(ArgAction::SetTrue).conflicts_with("enable").help("Choose backend-only deployment before the first website publication"))
            .arg(Arg::new("domain").long("domain").value_name("HOSTNAME").help("Stage a custom frontend domain and show its DNS validation records"))
            .arg(Arg::new("check-domain").long("check-domain").action(ArgAction::SetTrue).help("Check frontend domain ownership and TLS certificate validation"))
            .arg(Arg::new("remove-domain").long("remove-domain").action(ArgAction::SetTrue).help("Stage returning the frontend to its platform address; deploy to apply"))
            .group(ArgGroup::new("change").args(["enable", "disable", "domain", "check-domain", "remove-domain"]).multiple(false))
            .arg(Arg::new("base_path").long("path").short('p'))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let token = require_auth()?;
        let (_, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;
        let environment = matches
            .get_one::<String>("environment")
            .context("Environment required")?;
        let region = matches
            .get_one::<String>("region")
            .context("Region required")?;
        let url = format!(
            "{}/applications/{}/website",
            get_platform_management_api_url(),
            urlencoding::encode(&app)
        );
        let read_url = format!(
            "{url}?environment={}&region={}",
            urlencoding::encode(environment),
            urlencoding::encode(region)
        );
        // Do not follow a credential-bearing redirect or retry an ambiguous write.
        let client = Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(30))
            .build()?;
        let current = read_response(
            client
                .get(&read_url)
                .bearer_auth(&token)
                .send()
                .context("Could not read website settings")?,
        )?;
        let body = if let Some((method, suffix, payload)) =
            mutation_request(matches, &current, environment, region)?
        {
            read_response(
                client
                    .request(method, format!("{url}{suffix}"))
                    .bearer_auth(&token)
                    .json(&payload)
                    .send()
                    .context(
                        "Website request failed; read current settings before retrying a write",
                    )?,
            )?
        } else {
            current
        };
        println!("{}", serde_json::to_string_pretty(&body)?);
        Ok(())
    }
}
fn mutation_request(
    matches: &ArgMatches,
    current: &serde_json::Value,
    environment: &str,
    region: &str,
) -> Result<Option<(Method, &'static str, serde_json::Value)>> {
    if matches.get_flag("enable") || matches.get_flag("disable") {
        return Ok(Some((
            Method::PUT,
            "",
            selection_payload(current, environment, region, matches.get_flag("enable"))?,
        )));
    }
    if let Some(domain) = matches.get_one::<String>("domain") {
        let domain = domain.trim();
        if domain.len() > 253
            || !domain.contains('.')
            || domain.split('.').any(|label| {
                label.is_empty()
                    || label.len() > 63
                    || label.starts_with('-')
                    || label.ends_with('-')
                    || !label
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            bail!(
                "Use a hostname such as study.example.com, without a URL scheme, port, query, or path."
            );
        }
        let mut payload = revision_payload(current, environment, region)?;
        payload["domain"] = domain.into();
        payload["enabled"] = true.into();
        return Ok(Some((Method::PUT, "", payload)));
    }
    for (flag, suffix) in [
        ("check-domain", "/check"),
        ("remove-domain", "/domain/remove"),
    ] {
        if matches.get_flag(flag) {
            return Ok(Some((
                Method::POST,
                suffix,
                revision_payload(current, environment, region)?,
            )));
        }
    }
    Ok(None)
}

// A read-only request precedes every change. Older servers must never interpret
// an unknown enabled:false field as the legacy enable operation.
fn selection_payload(
    current: &serde_json::Value,
    environment: &str,
    region: &str,
    enabled: bool,
) -> Result<serde_json::Value> {
    if !enabled && current["canDisable"] != true {
        bail!(
            "This website cannot be disabled through this server. Published websites need a separate retirement flow; older servers need an update. No settings were changed."
        );
    }
    let mut payload = revision_payload(current, environment, region)?;
    payload["enabled"] = enabled.into();
    Ok(payload)
}
fn revision_payload(
    current: &serde_json::Value,
    environment: &str,
    region: &str,
) -> Result<serde_json::Value> {
    let revision = |name: &str| -> Result<serde_json::Value> {
        match current.get(name) {
            Some(value)
                if value.is_null()
                    || value
                        .as_str()
                        .is_some_and(|s| !s.is_empty() && s.len() <= 256) =>
            {
                Ok(value.clone())
            }
            _ => bail!("Server did not return valid website revisions. No settings were changed."),
        }
    };
    Ok(serde_json::json!({
        "environment": environment, "region": region,
        "expectedCurrentRevision": revision("currentRevision")?,
        "expectedPendingRevision": revision("pendingRevision")?,
    }))
}
fn read_response(response: reqwest::blocking::Response) -> Result<serde_json::Value> {
    if !response.status().is_success() {
        let status = response.status();
        let detail = match status.as_u16() {
            403 => "permission denied; check your application access and operation permissions",
            404 => "website route or application is unavailable on this server",
            409 => {
                "website settings changed or the website is already published; read current settings before retrying"
            }
            _ => "read current settings before retrying",
        };
        bail!("Website request returned {status}: {detail}");
    }
    response.json().context("Invalid website response")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn domain_actions_use_exact_revision_and_dedicated_routes() {
        let current = serde_json::json!({"currentRevision":"live", "pendingRevision":null});
        for (flags, method, suffix) in [
            (vec!["--domain", "study.example.com"], Method::PUT, ""),
            (vec!["--check-domain"], Method::POST, "/check"),
            (vec!["--remove-domain"], Method::POST, "/domain/remove"),
        ] {
            let mut args = vec![
                "website",
                "--environment",
                "production",
                "--region",
                "us-east-2",
            ];
            args.extend(flags);
            let matches = WebsiteCommand::new()
                .command()
                .version("0.0.0")
                .try_get_matches_from(args)
                .unwrap();
            let (actual_method, actual_suffix, body) =
                mutation_request(&matches, &current, "production", "us-east-2")
                    .unwrap()
                    .unwrap();
            assert_eq!(actual_method, method);
            assert_eq!(actual_suffix, suffix);
            assert_eq!(body["expectedCurrentRevision"], "live");
            assert!(body["expectedPendingRevision"].is_null());
            assert_eq!(body["environment"], "production");
            if suffix.is_empty() {
                assert_eq!(body["domain"], "study.example.com");
            }
            assert!(
                mutation_request(&matches, &serde_json::json!({}), "production", "us-east-2")
                    .is_err()
            );
        }
    }
    #[test]
    fn invalid_hostnames_never_prepare_writes() {
        let current = serde_json::json!({"currentRevision":null,"pendingRevision":null});
        for domain in [
            "",
            "https://study.example.com",
            "study.example.com/path",
            "study.example.com:443",
            "study.example.com?test=1",
            "study..example.com",
            "-study.example.com",
            "study.example.com#part",
        ] {
            let matches = WebsiteCommand::new()
                .command()
                .version("0.0.0")
                .try_get_matches_from([
                    "website",
                    "--environment",
                    "production",
                    "--region",
                    "us-east-2",
                    &format!("--domain={domain}"),
                ])
                .unwrap();
            assert!(
                mutation_request(&matches, &current, "production", "us-east-2").is_err(),
                "{domain}"
            );
        }
    }
    #[test]
    fn domain_mutations_cannot_be_combined() {
        for extra in ["--enable", "--disable", "--check-domain", "--remove-domain"] {
            assert!(
                WebsiteCommand::new()
                    .command()
                    .version("0.0.0")
                    .try_get_matches_from([
                        "website",
                        "--environment",
                        "production",
                        "--region",
                        "us-east-2",
                        "--domain",
                        "study.example.com",
                        extra
                    ])
                    .is_err()
            );
        }
    }
    #[test]
    fn website_is_read_only_unless_explicitly_enabled() {
        let command = WebsiteCommand::new();
        let read = command
            .command()
            .version("0.0.0")
            .try_get_matches_from(["website", "--environment", "qa", "--region", "us-west-2"])
            .unwrap();
        assert!(!read.get_flag("enable"));
        let write = command
            .command()
            .version("0.0.0")
            .try_get_matches_from([
                "website",
                "--environment",
                "qa",
                "--region",
                "us-west-2",
                "--enable",
            ])
            .unwrap();
        assert!(write.get_flag("enable"));
    }
    #[test]
    fn environment_and_region_are_mandatory() {
        assert!(
            WebsiteCommand::new()
                .command()
                .version("0.0.0")
                .try_get_matches_from(["website", "--enable"])
                .is_err()
        );
        assert!(
            WebsiteCommand::new()
                .command()
                .version("0.0.0")
                .try_get_matches_from(["website", "--environment", "qa"])
                .is_err()
        );
    }
    #[test]
    fn selection_flags_are_exclusive() {
        assert!(
            WebsiteCommand::new()
                .command()
                .version("0.0.0")
                .try_get_matches_from([
                    "website",
                    "--environment",
                    "qa",
                    "--region",
                    "us-west-2",
                    "--enable",
                    "--disable"
                ])
                .is_err()
        );
    }
    #[test]
    fn old_servers_cannot_turn_disable_into_enable() {
        let legacy =
            serde_json::json!({"enabled":false,"currentRevision":null,"pendingRevision":null});
        assert!(selection_payload(&legacy, "qa", "us-west-2", false).is_err());
        assert!(selection_payload(&legacy, "qa", "us-west-2", true).is_ok());
    }
    #[test]
    fn opt_out_carries_exact_read_revisions() {
        let read = serde_json::json!({"canDisable":true,"currentRevision":null,"pendingRevision":"pending-one"});
        let payload = selection_payload(&read, "qa", "us-west-2", false).unwrap();
        assert_eq!(
            payload,
            serde_json::json!({"environment":"qa","region":"us-west-2","enabled":false,"expectedCurrentRevision":null,"expectedPendingRevision":"pending-one"})
        );
        let live =
            serde_json::json!({"canDisable":false,"currentRevision":"live","pendingRevision":null});
        assert!(selection_payload(&live, "qa", "us-west-2", false).is_err());
    }
    #[test]
    fn missing_or_malformed_revisions_prevent_writes() {
        for read in [
            serde_json::json!({"canDisable":true}),
            serde_json::json!({"canDisable":true,"currentRevision":null,"pendingRevision":17}),
        ] {
            assert!(selection_payload(&read, "qa", "us-west-2", false).is_err());
            assert!(selection_payload(&read, "qa", "us-west-2", true).is_err());
        }
    }
}

use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::ERROR_FAILED_TO_SEND_REQUEST,
    core::{command::command, http_client::post_with_auth},
    managed::client::{
        Missing, ensure_success, managed_url, print_dryrun, require_managed_mode,
        resolve_managed_auth,
    },
};

/// What `POST /managed-mode/templates/:slug/claim-next` answers on success.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ClaimNext {
    pub(super) instance_id: String,
    pub(super) host: String,
    pub(super) claim_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) reference: Option<String>,
    /// Present when `--email` was passed: whether the platform's mail went out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) emailed: Option<bool>,
}

/// The 409 body when the template's pool has nothing to hand out.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PoolEmpty {
    #[serde(default)]
    pub(super) code: Option<String>,
    #[serde(default)]
    pub(super) message: Option<String>,
    #[serde(default)]
    pub(super) pool_size: Option<u64>,
    #[serde(default)]
    pub(super) reserved: Option<u64>,
    #[serde(default)]
    pub(super) provisioning: Option<u64>,
}

#[derive(Debug)]
pub(super) struct ClaimNextCommand;

impl ClaimNextCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

/// The request body: only the fields that were passed.
pub(super) fn claim_next_body(email: Option<&String>, reference: Option<&String>) -> Value {
    let mut body = Map::new();
    if let Some(email) = email {
        body.insert("email".to_string(), json!(email));
    }
    if let Some(reference) = reference {
        body.insert("reference".to_string(), json!(reference));
    }
    Value::Object(body)
}

/// The sentence for an empty pool: what is there, and what to do about it.
pub(super) fn pool_empty_message(template: &str, empty: &PoolEmpty) -> String {
    format!(
        "POOL_EMPTY: no unreserved instance of '{}' is waiting to be claimed \
         (pool size {}: {} reserved, {} still launching). Launch more with \
         `forklaunch managed instance create --template {} --region <region>` and retry.",
        template,
        empty.pool_size.unwrap_or(0),
        empty.reserved.unwrap_or(0),
        empty.provisioning.unwrap_or(0),
        template
    )
}

impl CliCommand for ClaimNextCommand {
    fn command(&self) -> Command {
        command(
            "claim-next",
            "Take the next unreserved instance of a template from the pool, with a fresh claim link",
        )
        .long_about(
            "Take the next unreserved instance of a template from your organization's pool\n\
             and get a fresh one-time claim link for it — the unattended handover a signup\n\
             backend uses.\n\n\
             The platform picks the OLDEST instance of the template that is awaiting claim\n\
             and has no live link out, reserves it, rotates its claim token, and returns the\n\
             link. Two calls at the same moment never get the same instance. Nobody reveals\n\
             or approves anything.\n\n\
             \x20 --reference  your own id for the customer; stored on the instance (it\n\
             \x20              survives the claim; a reset clears it).\n\
             \x20 --email      also email the claim link to this address. The link is still\n\
             \x20              printed; if the mail fails it says so and you deliver it.\n\n\
             The reservation lapses with the link (72 h): an instance whose customer never\n\
             claims returns to the pool on its own.\n\n\
             When the pool is empty this exits non-zero with POOL_EMPTY and the pool's\n\
             counts (with --json, the 409 body is printed on stdout first). Launch more\n\
             with `instance create`. To launch without waiting for approval as well, an admin\n\
             sets `template update --auto-approve-launches true`.",
        )
        .arg(
            Arg::new("template")
                .long("template")
                .required(true)
                .help("Slug of the template whose pool to take an instance from"),
        )
        .arg(
            Arg::new("email")
                .long("email")
                .help("Also email the claim link to this address"),
        )
        .arg(
            Arg::new("reference")
                .long("reference")
                .help("Your own id for the customer, stored on the instance"),
        )
        .arg(
            Arg::new("dryrun")
                .long("dryrun")
                .help("Print the request that would be sent without sending it — reserves nothing")
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

        let template = matches
            .get_one::<String>("template")
            .context("--template is required")?;
        let body = claim_next_body(
            matches.get_one::<String>("email"),
            matches.get_one::<String>("reference"),
        );
        let path = format!("/templates/{}/claim-next", urlencoding::encode(template));

        if matches.get_flag("dryrun") {
            return print_dryrun("POST", &path, Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        let json_output = matches.get_flag("json");

        let url = managed_url(&path);
        let response =
            post_with_auth(&auth_mode, &url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

        // An empty pool is an answer, not a fault: it carries the counts the caller needs
        // to decide whether to launch more, so read them rather than printing "conflict".
        if response.status().as_u16() == 409 {
            let text = response.text().unwrap_or_default();
            let empty: PoolEmpty = serde_json::from_str(&text).unwrap_or_default();
            if json_output {
                println!("{}", text);
            }
            bail!("{}", pool_empty_message(template, &empty));
        }

        let response = ensure_success(
            response,
            Missing::Resource(format!("template '{}'", template)),
        )?;
        let claimed: ClaimNext = response
            .json()
            .with_context(|| format!("Failed to parse the response from {}", url))?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&claimed)?);
            return Ok(());
        }

        writeln!(stdout)?;
        log_ok!(
            stdout,
            "Reserved instance {} of '{}'",
            claimed.instance_id,
            template
        );
        log_info!(stdout, "Host: {}", claimed.host);
        if let Some(reference) = claimed.reference.as_deref() {
            log_info!(stdout, "Reference: {}", reference);
        }
        writeln!(stdout)?;
        log_header!(
            stdout,
            Color::Yellow,
            "ONE-TIME CLAIM LINK — it is not kept on the platform"
        );
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
        writeln!(stdout, "  {}", claimed.claim_url)?;
        stdout.reset()?;
        writeln!(stdout)?;
        if let Some(expires_at) = claimed.expires_at.as_deref() {
            log_info!(
                stdout,
                "Expires: {} (unclaimed by then, the instance returns to the pool)",
                expires_at
            );
        }
        match claimed.emailed {
            Some(true) => log_ok!(stdout, "Claim link emailed."),
            Some(false) => log_warn!(
                stdout,
                "The claim email could not be sent — deliver the link above yourself."
            ),
            None => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_carries_only_what_was_passed() {
        assert_eq!(claim_next_body(None, None), json!({}));
        let email = "owner@clinic.example".to_string();
        let reference = "cust_42".to_string();
        assert_eq!(
            claim_next_body(Some(&email), Some(&reference)),
            json!({ "email": "owner@clinic.example", "reference": "cust_42" })
        );
        assert_eq!(
            claim_next_body(None, Some(&reference)),
            json!({ "reference": "cust_42" })
        );
    }

    #[test]
    fn an_empty_pool_says_what_is_there_and_what_to_do() {
        let empty: PoolEmpty = serde_json::from_value(json!({
            "code": "POOL_EMPTY",
            "message": "launch more",
            "poolSize": 3,
            "reserved": 3,
            "provisioning": 1
        }))
        .unwrap();
        let message = pool_empty_message("acme-books", &empty);
        assert!(message.starts_with("POOL_EMPTY"), "{}", message);
        assert!(
            message.contains("pool size 3: 3 reserved, 1 still launching"),
            "{}",
            message
        );
        assert!(
            message.contains("instance create --template acme-books"),
            "{}",
            message
        );
    }

    #[test]
    fn the_success_body_parses_and_round_trips() {
        let claimed: ClaimNext = serde_json::from_value(json!({
            "instanceId": "inst-1",
            "host": "acme-a1b2c3-instance.example.app",
            "claimUrl": "https://forklaunch.com/claim/tok",
            "expiresAt": "2026-10-07T00:00:00.000Z",
            "reference": "cust_42",
            "emailed": false
        }))
        .unwrap();
        assert_eq!(claimed.instance_id, "inst-1");
        assert_eq!(claimed.emailed, Some(false));
        let back = serde_json::to_value(&claimed).unwrap();
        assert_eq!(back["claimUrl"], "https://forklaunch.com/claim/tok");
    }

    #[test]
    fn template_is_required_and_dryrun_reserves_nothing() {
        let command = ClaimNextCommand::new().command().version("0.0.0-test");
        assert!(
            command
                .clone()
                .try_get_matches_from(["claim-next"])
                .is_err()
        );
        let matches = command
            .try_get_matches_from([
                "claim-next",
                "--template",
                "acme-books",
                "--reference",
                "cust_42",
                "--dryrun",
            ])
            .unwrap();
        // Dry run returns before any auth or network.
        ClaimNextCommand::new().handler(&matches).unwrap();
    }
}

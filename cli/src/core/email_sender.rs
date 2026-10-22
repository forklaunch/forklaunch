//! Shared pieces of the custom email sender commands (Pro): `app domain email`
//! (per application environment, on the app's custom domain) and
//! `managed instance email` (per managed instance, on a domain the operator names).
//!
//! Both talk to the same platform model: a sender object with SES DKIM records,
//! verified asynchronously. Until it verifies, mail keeps going from the default
//! platform sender, so sends never fail while the user waits on DNS.

use std::io::Write;

use anyhow::{Result, anyhow, bail};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};

pub(crate) const UPGRADE_HINT: &str = "Custom email senders are a Pro feature: upgrade the organization to the Pro or \
     Enterprise plan in the ForkLaunch dashboard, then run this again.";

/// The platform answers errors either as plain text or as a JSON envelope
/// (`{ "message": ... }` / `{ "error": ... }`). Either way show the sentence.
pub(crate) fn error_text(body: &str) -> String {
    let trimmed = body.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(text) = value.as_str() {
            return text.to_string();
        }
        for key in ["message", "error", "detail"] {
            if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
                return text.to_string();
            }
        }
    }
    trimmed.to_string()
}

/// 400 (validation), 403 (plan gate) and 409 (domain claimed elsewhere) carry a
/// sentence meant for the user: print it plainly, with an upgrade hint on 403.
/// Everything else keeps the status for debugging.
pub(crate) fn request_error(action: &str, status: StatusCode, body: &str) -> anyhow::Error {
    let text = error_text(body);
    match status.as_u16() {
        403 => anyhow!("{}\n{}", text, UPGRADE_HINT),
        400 | 409 => anyhow!("{}", text),
        502 | 503 => anyhow!(
            "{}\nThe platform could not reach the mail provider or the domain ownership check; try again shortly.",
            text
        ),
        _ => anyhow!("Failed to {} ({}): {}", action, status, text),
    }
}

/// Local part rules shared with the platform: lowercase `[a-z0-9._+-]`, at
/// most 64 characters, not starting or ending with a dot.
pub(crate) fn validate_local_part(value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("--from must not be empty");
    }
    if value.len() > 64 {
        bail!("--from must be at most 64 characters");
    }
    if let Some(bad) = value
        .chars()
        .find(|c| !matches!(c, 'a'..='z' | '0'..='9' | '.' | '_' | '+' | '-'))
    {
        bail!(
            "--from \"{}\" contains '{}': use lowercase letters, digits and . _ + - only (the part before the @, e.g. billing)",
            value,
            bad
        );
    }
    if value.starts_with('.') || value.ends_with('.') {
        bail!("--from \"{}\" must not start or end with a dot", value);
    }
    Ok(())
}

/// Display name: one line, at most 64 characters, no `<`, `>` or `"`.
pub(crate) fn validate_display_name(value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("--name must not be empty");
    }
    if value.chars().count() > 64 {
        bail!("--name must be at most 64 characters");
    }
    if value.contains('\n') || value.contains('\r') {
        bail!("--name must be a single line");
    }
    if value.contains(['<', '>', '"']) {
        bail!("--name must not contain <, > or \"");
    }
    Ok(())
}

/// Dot-separated DNS labels (1-63 of `[a-z0-9-]`, no leading or trailing hyphen).
/// `flag` names the option in the error (`--subdomain`, `--domain`).
pub(crate) fn validate_dns_name(flag: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        bail!("{} must not be empty", flag);
    }
    for label in value.split('.') {
        let valid = !label.is_empty()
            && label.len() <= 63
            && label
                .chars()
                .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-'))
            && !label.starts_with('-')
            && !label.ends_with('-');
        if !valid {
            bail!(
                "{} \"{}\" is not a valid DNS name: each dot-separated label must be 1-63 lowercase letters, digits or hyphens, not starting or ending with a hyphen",
                flag,
                value
            );
        }
    }
    Ok(())
}

/// The platform accepts one to three labels in front of the custom domain.
pub(crate) fn validate_subdomain(value: &str) -> Result<()> {
    validate_dns_name("--subdomain", value)?;
    if value.split('.').count() > 3 {
        bail!(
            "--subdomain \"{}\" has more than three labels (e.g. mail or eu.mail)",
            value
        );
    }
    Ok(())
}

/// A domain to send from must have at least two labels (`acme.com`, not `acme`).
pub(crate) fn validate_domain(value: &str) -> Result<()> {
    validate_dns_name("--domain", value)?;
    if !value.contains('.') {
        bail!(
            "--domain \"{}\" must be a full domain such as acme.com",
            value
        );
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DnsRecord {
    #[serde(default, rename = "type")]
    pub(crate) record_type: Option<String>,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) value: Option<String>,
    /// `false` for the advisory DMARC TXT. Older responses omit it.
    #[serde(default)]
    pub(crate) required: Option<bool>,
}

impl DnsRecord {
    /// The `_dmarc` TXT is a recommendation, not something verification waits on.
    pub(crate) fn is_advisory(&self) -> bool {
        if let Some(required) = self.required {
            return !required;
        }
        self.name
            .as_deref()
            .is_some_and(|n| n == "_dmarc" || n.starts_with("_dmarc."))
    }
}

/// The sender object. `environment` is present on application senders only.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmailSender {
    #[serde(default)]
    pub(crate) environment: Option<String>,
    #[serde(default)]
    pub(crate) domain: Option<String>,
    pub(crate) from_address: String,
    #[serde(default)]
    pub(crate) display_name: Option<String>,
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) dns_records: Vec<DnsRecord>,
    #[serde(default)]
    pub(crate) dns_records_written: bool,
    #[serde(default)]
    pub(crate) error_message: Option<String>,
    /// Instance senders only: whether mail goes out from it right now.
    #[serde(default)]
    pub(crate) active: Option<bool>,
}

impl EmailSender {
    pub(crate) fn formatted_from(&self) -> String {
        match self.display_name.as_deref().filter(|n| !n.is_empty()) {
            Some(name) => format!("{} <{}>", name, self.from_address),
            None => self.from_address.clone(),
        }
    }
}

/// `GET` on the application route without `environment` returns every
/// environment's sender; with it (and on the instance route) one object.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum EmailSenderResponse {
    Many(Vec<EmailSender>),
    One(EmailSender),
}

impl EmailSenderResponse {
    pub(crate) fn into_vec(self) -> Vec<EmailSender> {
        match self {
            Self::Many(senders) => senders,
            Self::One(sender) => vec![sender],
        }
    }
}

fn write_dns_records(stdout: &mut StandardStream, records: &[&DnsRecord]) -> Result<()> {
    let rows: Vec<(&str, &str, &str, bool)> = records
        .iter()
        .map(|r| {
            (
                r.record_type.as_deref().unwrap_or("CNAME"),
                r.name.as_deref().unwrap_or("-"),
                r.value.as_deref().unwrap_or("-"),
                r.is_advisory(),
            )
        })
        .collect();
    let type_width = rows.iter().map(|r| r.0.len()).max().unwrap_or(4).max(4);
    let name_width = rows.iter().map(|r| r.1.len()).max().unwrap_or(4).max(4);
    writeln!(
        stdout,
        "    {:<tw$}  {:<nw$}  VALUE",
        "TYPE",
        "NAME",
        tw = type_width,
        nw = name_width
    )?;
    for (record_type, name, value, advisory) in rows {
        writeln!(
            stdout,
            "    {:<tw$}  {:<nw$}  {}{}",
            record_type,
            name,
            value,
            if advisory { "  (advisory)" } else { "" },
            tw = type_width,
            nw = name_width
        )?;
    }
    Ok(())
}

/// Renders one sender. `label` goes after the address (an environment or an
/// instance id); `default_sender` is the address mail uses until verification.
pub(crate) fn write_sender(
    stdout: &mut StandardStream,
    sender: &EmailSender,
    label: &str,
    default_sender: &str,
) -> Result<()> {
    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    write!(stdout, "{}", sender.formatted_from())?;
    stdout.reset()?;
    writeln!(stdout, "  ({})", label)?;
    writeln!(stdout)?;
    if let Some(domain) = &sender.domain {
        writeln!(stdout, "  domain:   {}", domain)?;
    }
    let status_color = match sender.status.as_str() {
        "verified" => Color::Green,
        "failed" => Color::Red,
        _ => Color::Yellow,
    };
    write!(stdout, "  status:   ")?;
    stdout.set_color(ColorSpec::new().set_fg(Some(status_color)))?;
    writeln!(stdout, "{}", sender.status)?;
    stdout.reset()?;
    if let Some(active) = sender.active {
        writeln!(
            stdout,
            "  in use:   {}",
            if active {
                "yes"
            } else {
                "no, the default sender is used until this verifies"
            }
        )?;
    }
    if let Some(err) = &sender.error_message {
        writeln!(stdout, "  error:    {}", err)?;
    }

    let (advisory, required): (Vec<_>, Vec<_>) =
        sender.dns_records.iter().partition(|r| r.is_advisory());
    if sender.dns_records_written {
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  DKIM records were written for you (the domain is delegated to ForkLaunch)."
        )?;
        if !advisory.is_empty() {
            writeln!(
                stdout,
                "  Recommended, publish if you manage DMARC yourself:"
            )?;
            write_dns_records(stdout, &advisory)?;
        }
    } else if !sender.dns_records.is_empty() {
        writeln!(stdout)?;
        writeln!(stdout, "  Publish these records at your DNS provider:")?;
        let mut all = required;
        all.extend(advisory);
        write_dns_records(stdout, &all)?;
    } else if sender.status != "verified" {
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  No DNS records yet: they appear once the platform has registered the domain\n  (for an app environment, on its next deploy with email)."
        )?;
    }

    if sender.status == "failed" {
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  Verification failed (the mail provider stops checking after 72 hours, or the plan\n  no longer includes custom domains). Fix the cause, then run `set` again."
        )?;
    }
    if sender.status != "verified" {
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  Until the domain verifies, mail keeps going from the default sender\n  ({}); sends never fail because of this.",
            default_sender
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_deserializes_object() {
        let json = r#"{
            "environment": "production",
            "domain": "acme.com",
            "fromAddress": "billing@acme.com",
            "displayName": "Acme Billing",
            "status": "pending_verification",
            "dnsRecords": [
                {"type": "CNAME", "name": "abc._domainkey.acme.com", "value": "abc.dkim.amazonses.com"},
                {"type": "TXT", "name": "_dmarc.acme.com", "value": "v=DMARC1; p=none;"}
            ],
            "dnsRecordsWritten": false
        }"#;
        let senders = serde_json::from_str::<EmailSenderResponse>(json)
            .unwrap()
            .into_vec();
        assert_eq!(senders.len(), 1);
        let sender = &senders[0];
        assert_eq!(sender.environment.as_deref(), Some("production"));
        assert_eq!(sender.formatted_from(), "Acme Billing <billing@acme.com>");
        assert_eq!(sender.status, "pending_verification");
        assert!(!sender.dns_records_written);
        assert_eq!(sender.dns_records.len(), 2);
        assert!(!sender.dns_records[0].is_advisory());
        assert!(sender.dns_records[1].is_advisory());
        assert_eq!(sender.dns_records[0].record_type.as_deref(), Some("CNAME"));
    }

    #[test]
    fn sender_deserializes_array_and_missing_optionals() {
        let json = r#"[
            {"environment": "production", "domain": "acme.com", "fromAddress": "no-reply@acme.com",
             "status": "verified", "dnsRecords": [], "dnsRecordsWritten": true},
            {"environment": "staging", "fromAddress": "billing@mail.acme.com", "status": "failed",
             "errorMessage": "DKIM records not found"}
        ]"#;
        let senders = serde_json::from_str::<EmailSenderResponse>(json)
            .unwrap()
            .into_vec();
        assert_eq!(senders.len(), 2);
        assert!(senders[0].active.is_none());
        assert_eq!(senders[0].formatted_from(), "no-reply@acme.com");
        assert!(senders[0].dns_records_written);
        assert!(senders[0].display_name.is_none());
        assert!(senders[1].domain.is_none());
        assert!(senders[1].dns_records.is_empty());
        assert!(!senders[1].dns_records_written);
        assert_eq!(
            senders[1].error_message.as_deref(),
            Some("DKIM records not found")
        );
    }

    #[test]
    fn advisory_uses_required_when_present() {
        let json = r#"[
            {"type": "TXT", "name": "_dmarc.acme.com", "value": "v=DMARC1; p=none;", "required": false},
            {"type": "CNAME", "name": "a._domainkey.acme.com", "value": "a.dkim.amazonses.com", "required": true},
            {"type": "TXT", "name": "_dmarc.acme.com", "value": "v=DMARC1; p=none;", "required": true},
            {"type": "CNAME", "name": "b._domainkey.acme.com", "value": "b.dkim.amazonses.com", "required": false},
            {"type": "TXT", "name": "_dmarc.acme.com", "value": "v=DMARC1; p=none;"},
            {"type": "CNAME", "name": "c._domainkey.acme.com", "value": "c.dkim.amazonses.com"}
        ]"#;
        let records: Vec<DnsRecord> = serde_json::from_str(json).unwrap();
        let advisory: Vec<bool> = records.iter().map(DnsRecord::is_advisory).collect();
        // `required` wins when present; the `_dmarc` name decides when it is absent.
        assert_eq!(advisory, [true, false, false, true, true, false]);
    }

    #[test]
    fn instance_sender_has_no_environment() {
        let json = r#"{"domain": "clinic.com", "fromAddress": "care@clinic.com",
            "status": "pending_verification", "dnsRecords": [], "dnsRecordsWritten": false,
            "active": false}"#;
        let sender: EmailSender = serde_json::from_str(json).unwrap();
        assert!(sender.environment.is_none());
        assert_eq!(sender.active, Some(false));
        assert_eq!(sender.formatted_from(), "care@clinic.com");
    }

    #[test]
    fn local_part_validation() {
        for ok in ["billing", "no-reply", "a.b", "team+ops", "x_1"] {
            assert!(validate_local_part(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", "Billing", ".billing", "billing.", "bill ing", "bill@ing", "bïlling",
        ] {
            assert!(validate_local_part(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn display_name_validation() {
        assert!(validate_display_name("Acme Billing").is_ok());
        assert!(validate_display_name(&"a".repeat(64)).is_ok());
        assert!(validate_display_name(&"a".repeat(65)).is_err());
        assert!(validate_display_name("").is_err());
        assert!(validate_display_name("Acme\nBilling").is_err());
        assert!(validate_display_name("Acme <Billing>").is_err());
        assert!(validate_display_name("\"Acme\"").is_err());
    }

    #[test]
    fn dns_name_validation() {
        for ok in ["mail", "eu.mail", "mail-1", "a1"] {
            assert!(validate_dns_name("--subdomain", ok).is_ok(), "{ok}");
        }
        let long = "a".repeat(64);
        for bad in [
            "",
            "Mail",
            "-mail",
            "mail-",
            "mail..eu",
            ".mail",
            "mail.",
            "ma_il",
            long.as_str(),
        ] {
            assert!(validate_dns_name("--subdomain", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn local_part_length_and_subdomain_label_count() {
        assert!(validate_local_part(&"a".repeat(64)).is_ok());
        assert!(validate_local_part(&"a".repeat(65)).is_err());
        assert!(validate_subdomain("mail").is_ok());
        assert!(validate_subdomain("a.b.c").is_ok());
        assert!(validate_subdomain("a.b.c.d").is_err());
        assert!(validate_subdomain("Mail").is_err());
    }

    #[test]
    fn domain_validation() {
        assert!(validate_domain("acme.com").is_ok());
        assert!(validate_domain("mail.acme.co.uk").is_ok());
        assert!(validate_domain("acme").is_err());
        assert!(validate_domain("Acme.com").is_err());
    }

    #[test]
    fn errors_print_the_platform_message() {
        let plan = "Custom email senders require a Pro or Enterprise plan";
        let err = request_error("set", StatusCode::FORBIDDEN, plan).to_string();
        assert!(err.starts_with(plan));
        assert!(err.contains("upgrade"));
        let err = request_error(
            "set",
            StatusCode::BAD_REQUEST,
            r#"{"message": "Application has no custom domain"}"#,
        )
        .to_string();
        assert_eq!(err, "Application has no custom domain");
        let err = request_error(
            "set",
            StatusCode::CONFLICT,
            "clinic.com is claimed by another organization",
        )
        .to_string();
        assert_eq!(err, "clinic.com is claimed by another organization");
        let err =
            request_error("set", StatusCode::SERVICE_UNAVAILABLE, "SES unavailable").to_string();
        assert!(err.starts_with("SES unavailable") && err.contains("try again"));
        let err = request_error("set", StatusCode::INTERNAL_SERVER_ERROR, "boom").to_string();
        assert!(err.contains("500") && err.contains("boom"));
    }
}

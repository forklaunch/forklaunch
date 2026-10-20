use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{
            Missing, get_value, print_dryrun, put_json_optional, require_managed_mode,
            resolve_managed_auth,
        },
        feature_settings::{application_fee, describe},
    },
};

/// The managed features an instance can override, beside the model gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Feature {
    Sms,
    Email,
    Payments,
    Voice,
    WhatsApp,
}

impl Feature {
    pub(super) const ALL: [Feature; 5] = [
        Feature::Sms,
        Feature::Email,
        Feature::Payments,
        Feature::Voice,
        Feature::WhatsApp,
    ];

    /// The subcommand, which is also the control plane's path segment.
    pub(super) fn subcommand(self) -> &'static str {
        match self {
            Feature::Sms => "sms-gateway",
            Feature::Email => "email-gateway",
            Feature::Payments => "payments-gateway",
            Feature::Voice => "voice",
            Feature::WhatsApp => "whatsapp",
        }
    }

    /// The key the control plane answers under.
    fn key(self) -> &'static str {
        match self {
            Feature::Sms => "sms",
            Feature::Email => "email",
            Feature::Payments => "payments",
            Feature::Voice => "voice",
            Feature::WhatsApp => "whatsapp",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Feature::Sms => "Override the product's SMS cap, rate or number for one instance",
            Feature::Email => "Override the product's email quota or rate for one instance",
            Feature::Payments => {
                "Override the product's application fee or Stripe rate for one instance"
            }
            Feature::Voice => "Override the product's voice call limits for one instance",
            Feature::WhatsApp => "Link one instance's own WhatsApp number, or change its rate",
        }
    }

    fn long_about(self) -> &'static str {
        match self {
            Feature::Sms => {
                "Override the product's SMS settings for one instance.\n\n\
                 The product (template) sets the origination pool, the monthly segment cap,\n\
                 the texts-per-minute limit and whether promotional texts are allowed\n\
                 (`managed template update --sms-*`). This sets exceptions for ONE instance: a\n\
                 larger cap for a big customer, or a dedicated number (--number-id with its\n\
                 E.164 --number, so replies map back to this instance). Fields you leave out\n\
                 keep coming from the template; --clear removes the override."
            }
            Feature::Email => {
                "Override the product's email limits for one instance.\n\n\
                 The product sets recipients per day and messages per minute\n\
                 (`managed template update --email-*`); unset, the platform defaults apply.\n\
                 Fields you leave out keep coming from the template; --clear removes the\n\
                 override."
            }
            Feature::Payments => {
                "Override the product's payments settings for one instance.\n\n\
                 The product sets the platform's application fee on charges its instances\n\
                 create and the Stripe calls per minute (`managed template update\n\
                 --payments-*`); unset, the platform's env fallback applies. A fee set here\n\
                 replaces the product's fee as a whole (percent and amount together).\n\
                 --clear removes the override."
            }
            Feature::Voice => {
                "Override the product's voice call limits for one instance.\n\n\
                 The product sets its flow catalog and the limits (`managed template update\n\
                 --voice-*`). An instance can only have different limits — concurrent calls\n\
                 and monthly minutes — never other flows. --clear removes the override."
            }
            Feature::WhatsApp => {
                "Link one instance's own WhatsApp number, or change its rate.\n\n\
                 The product's channel (`managed template update --whatsapp-*`) applies to\n\
                 every instance. An instance with its own number (--number-id, the End User\n\
                 Messaging Social phone number id) sends from it instead; --rpm left unset\n\
                 falls through to the product's rate. --disabled keeps the link but turns\n\
                 WhatsApp off for this instance. --clear unlinks it, and the product's\n\
                 channel applies again. Linking the WhatsApp Business Account itself is\n\
                 done in the AWS console (Meta embedded signup)."
            }
        }
    }

    fn args(self) -> Vec<Arg> {
        let rate = |id: &'static str, long: &'static str, help: &'static str| {
            Arg::new(id)
                .long(long)
                .value_parser(clap::value_parser!(u64).range(1..))
                .help(help)
        };
        match self {
            Feature::Sms => vec![
                rate(
                    "monthly_segments",
                    "monthly-segments",
                    "Billed segments per month for this instance",
                ),
                rate(
                    "per_minute",
                    "per-minute",
                    "Texts per minute for this instance",
                ),
                Arg::new("number_id")
                    .long("number-id")
                    .requires("number")
                    .help("A dedicated origination number (id or ARN) this instance sends from"),
                Arg::new("number")
                    .long("number")
                    .requires("number_id")
                    .help("That number in E.164 form, e.g. +15551230000"),
            ],
            Feature::Email => vec![
                rate(
                    "daily_quota",
                    "daily-quota",
                    "Recipients per UTC day for this instance",
                ),
                rate(
                    "per_minute",
                    "per-minute",
                    "Messages per minute for this instance",
                ),
            ],
            Feature::Payments => vec![
                Arg::new("fee_percent")
                    .long("fee-percent")
                    .value_parser(clap::value_parser!(f64))
                    .help("Application fee as a percent of the charge (0 to under 100)"),
                Arg::new("fee_amount")
                    .long("fee-amount")
                    .value_parser(clap::value_parser!(u64))
                    .help("Application fee as a flat amount in minor units (cents)"),
                rate("rpm", "rpm", "Stripe calls per minute for this instance"),
            ],
            Feature::Voice => vec![
                rate(
                    "max_concurrent",
                    "max-concurrent",
                    "Calls in progress at once for this instance",
                ),
                rate(
                    "monthly_minutes",
                    "monthly-minutes",
                    "Billed minutes per month for this instance",
                ),
            ],
            Feature::WhatsApp => vec![
                Arg::new("number_id")
                    .long("number-id")
                    .help("This instance's own phone number id (phone-number-id-…)"),
                rate("rpm", "rpm", "Sends per minute for this instance"),
                Arg::new("disabled")
                    .long("disabled")
                    .action(ArgAction::SetTrue)
                    .help("Keep the link but turn WhatsApp off for this instance"),
            ],
        }
    }

    fn arg_ids(self) -> &'static [&'static str] {
        match self {
            Feature::Sms => &["monthly_segments", "per_minute", "number_id", "number"],
            Feature::Email => &["daily_quota", "per_minute"],
            Feature::Payments => &["fee_percent", "fee_amount", "rpm"],
            Feature::Voice => &["max_concurrent", "monthly_minutes"],
            Feature::WhatsApp => &["number_id", "rpm", "disabled"],
        }
    }

    fn body(self, matches: &ArgMatches) -> Result<Value> {
        let u = |id: &str| matches.get_one::<u64>(id).copied();
        let s = |id: &str| matches.get_one::<String>(id);
        let clear = matches.get_flag("clear");
        match self {
            Feature::Sms => sms_body(
                u("monthly_segments"),
                u("per_minute"),
                s("number_id"),
                s("number"),
                clear,
            ),
            Feature::Email => email_body(u("daily_quota"), u("per_minute"), clear),
            Feature::Payments => payments_body(
                matches.get_one::<f64>("fee_percent").copied(),
                u("fee_amount"),
                u("rpm"),
                clear,
            ),
            Feature::Voice => voice_body(u("max_concurrent"), u("monthly_minutes"), clear),
            Feature::WhatsApp => whatsapp_body(
                s("number_id"),
                u("rpm"),
                matches.get_flag("disabled"),
                clear,
            ),
        }
    }
}

fn finish(body: Map<String, Value>, clear: bool, flags: &str) -> Result<Value> {
    if clear {
        return Ok(json!({ "clear": true }));
    }
    if body.is_empty() {
        bail!("nothing to set — pass {}, or --clear", flags);
    }
    Ok(Value::Object(body))
}

pub(super) fn sms_body(
    monthly_segments: Option<u64>,
    per_minute: Option<u64>,
    number_id: Option<&String>,
    number: Option<&String>,
    clear: bool,
) -> Result<Value> {
    let mut body = Map::new();
    if let Some(v) = monthly_segments {
        body.insert("monthlySegmentCap".into(), json!(v));
    }
    if let Some(v) = per_minute {
        body.insert("messagesPerMinute".into(), json!(v));
    }
    match (number_id, number) {
        (Some(id), Some(number)) => {
            body.insert("originationIdentity".into(), json!(id));
            body.insert("originationNumber".into(), json!(number));
        }
        (None, None) => {}
        _ => bail!("a dedicated number needs both --number-id and --number"),
    }
    finish(
        body,
        clear,
        "--monthly-segments, --per-minute, --number-id/--number",
    )
}

pub(super) fn email_body(
    daily_quota: Option<u64>,
    per_minute: Option<u64>,
    clear: bool,
) -> Result<Value> {
    let mut body = Map::new();
    if let Some(v) = daily_quota {
        body.insert("dailyQuota".into(), json!(v));
    }
    if let Some(v) = per_minute {
        body.insert("perMinute".into(), json!(v));
    }
    finish(body, clear, "--daily-quota, --per-minute")
}

pub(super) fn payments_body(
    fee_percent: Option<f64>,
    fee_amount: Option<u64>,
    rpm: Option<u64>,
    clear: bool,
) -> Result<Value> {
    let mut body = Map::new();
    if let Some(fee) = application_fee(fee_percent, fee_amount)? {
        body.insert("applicationFee".into(), fee);
    }
    if let Some(v) = rpm {
        body.insert("requestsPerMinute".into(), json!(v));
    }
    finish(body, clear, "--fee-percent, --fee-amount, --rpm")
}

pub(super) fn voice_body(
    max_concurrent: Option<u64>,
    monthly_minutes: Option<u64>,
    clear: bool,
) -> Result<Value> {
    let mut body = Map::new();
    if let Some(v) = max_concurrent {
        body.insert("maxConcurrentCalls".into(), json!(v));
    }
    if let Some(v) = monthly_minutes {
        body.insert("monthlyMinutes".into(), json!(v));
    }
    finish(body, clear, "--max-concurrent, --monthly-minutes")
}

/// The instance's channel is a whole row: a rate or `--disabled` without a number
/// would unlink it, so the number is required unless `--clear`.
pub(super) fn whatsapp_body(
    number_id: Option<&String>,
    rpm: Option<u64>,
    disabled: bool,
    clear: bool,
) -> Result<Value> {
    if clear {
        return Ok(json!({ "clear": true }));
    }
    let Some(number_id) = number_id else {
        if rpm.is_some() || disabled {
            bail!(
                "--rpm / --disabled set this instance's own channel, so they need --number-id \
                 too (the instance's phone number id); --clear unlinks it"
            );
        }
        bail!("nothing to set — pass --number-id (with --rpm / --disabled), or --clear");
    };
    let mut body = Map::new();
    body.insert("originationPhoneNumberId".into(), json!(number_id));
    if let Some(v) = rpm {
        body.insert("requestsPerMinute".into(), json!(v));
    }
    if disabled {
        body.insert("enabled".into(), json!(false));
    }
    Ok(Value::Object(body))
}

#[derive(Debug)]
pub(super) struct FeatureOverrideCommand {
    feature: Feature,
}

impl FeatureOverrideCommand {
    pub(super) fn new(feature: Feature) -> Self {
        Self { feature }
    }
}

fn output_args(cmd: Command) -> Command {
    cmd.arg(
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

impl CliCommand for FeatureOverrideCommand {
    fn command(&self) -> Command {
        let feature = self.feature;
        let cmd = command(feature.subcommand(), feature.about())
            .long_about(feature.long_about())
            .arg(
                Arg::new("id")
                    .long("id")
                    .required(true)
                    .help("Id of the instance"),
            )
            .args(feature.args())
            .arg(
                Arg::new("clear")
                    .long("clear")
                    .help(if feature == Feature::WhatsApp {
                        "Unlink this instance's own number; the product's channel applies again"
                    } else {
                        "Remove the override; the template's settings apply again"
                    })
                    .action(ArgAction::SetTrue)
                    .conflicts_with_all(feature.arg_ids()),
            );
        output_args(cmd)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let body = self.feature.body(matches)?;
        let path = format!(
            "/instances/{}/{}",
            urlencoding::encode(id),
            self.feature.subcommand()
        );

        if matches.get_flag("dryrun") {
            return print_dryrun("PUT", &path, Some(&body));
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        let result =
            put_json_optional(&path, body, Missing::Resource(format!("instance '{}'", id)))?
                .unwrap_or(Value::Null);

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        log_ok!(
            stdout,
            "Instance {} {} now: {}",
            id,
            self.feature.key(),
            describe(self.feature.key(), &result)
        );
        Ok(())
    }
}

/// `instance gateway-settings --id` — every feature's settings for one instance.
#[derive(Debug)]
pub(super) struct GatewaySettingsCommand;

impl GatewaySettingsCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for GatewaySettingsCommand {
    fn command(&self) -> Command {
        let cmd = command(
            "gateway-settings",
            "Show an instance's SMS, WhatsApp, voice, email and payments settings in force",
        )
        .long_about(
            "Show an instance's SMS, WhatsApp, voice, email and payments settings: what is\n\
             in force, and which of it is the instance's own override (the rest comes from\n\
             the template, then the platform defaults). The model gateway has its own\n\
             `model-usage`.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance"),
        );
        output_args(cmd)
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let path = format!("/instances/{}/gateway-settings", urlencoding::encode(id));
        if matches.get_flag("dryrun") {
            return print_dryrun("GET", &path, None);
        }
        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        let settings = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("instance '{}'", id)),
        )?;
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&settings)?);
            return Ok(());
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        for feature in Feature::ALL {
            let key = feature.key();
            let value = settings.get(key).cloned().unwrap_or(Value::Null);
            writeln!(stdout, "  {:<9} {}", key, describe(key, &value))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(feature: Feature, args: &[&str]) -> Result<Value> {
        // `command()` propagates the binary's version, which a lone subcommand lacks.
        let cmd = FeatureOverrideCommand::new(feature)
            .command()
            .version("test");
        let mut argv = vec![feature.subcommand(), "--id", "inst-1"];
        argv.extend_from_slice(args);
        let matches = cmd.try_get_matches_from(argv)?;
        feature.body(&matches)
    }

    #[test]
    fn each_feature_sends_only_the_fields_given() {
        assert_eq!(
            parse(Feature::Sms, &["--monthly-segments", "5000"]).unwrap(),
            json!({ "monthlySegmentCap": 5000 })
        );
        assert_eq!(
            parse(
                Feature::Email,
                &["--daily-quota", "1000", "--per-minute", "60"]
            )
            .unwrap(),
            json!({ "dailyQuota": 1000, "perMinute": 60 })
        );
        assert_eq!(
            parse(Feature::Payments, &["--fee-percent", "1.5", "--rpm", "50"]).unwrap(),
            json!({ "applicationFee": { "percent": 1.5 }, "requestsPerMinute": 50 })
        );
        assert_eq!(
            parse(Feature::Voice, &["--monthly-minutes", "2000"]).unwrap(),
            json!({ "monthlyMinutes": 2000 })
        );
        assert_eq!(
            parse(
                Feature::WhatsApp,
                &["--number-id", "phone-number-id-abc", "--disabled"]
            )
            .unwrap(),
            json!({ "originationPhoneNumberId": "phone-number-id-abc", "enabled": false })
        );
    }

    #[test]
    fn clear_and_nothing_are_distinct() {
        for feature in Feature::ALL {
            assert_eq!(
                parse(feature, &["--clear"]).unwrap(),
                json!({ "clear": true }),
                "{:?}",
                feature
            );
            assert!(parse(feature, &[]).is_err(), "{:?}", feature);
        }
    }

    #[test]
    fn clear_conflicts_with_setting_a_value() {
        assert!(parse(Feature::Email, &["--clear", "--per-minute", "5"]).is_err());
        assert!(parse(Feature::WhatsApp, &["--clear", "--rpm", "5"]).is_err());
    }

    #[test]
    fn a_dedicated_sms_number_needs_its_id_and_its_e164_form() {
        assert!(parse(Feature::Sms, &["--number-id", "phone-1"]).is_err());
        assert_eq!(
            parse(
                Feature::Sms,
                &["--number-id", "phone-1", "--number", "+15551230000"]
            )
            .unwrap(),
            json!({ "originationIdentity": "phone-1", "originationNumber": "+15551230000" })
        );
    }

    #[test]
    fn a_whatsapp_rate_without_a_number_is_refused_rather_than_unlinking() {
        let error = parse(Feature::WhatsApp, &["--rpm", "10"]).unwrap_err();
        assert!(error.to_string().contains("--number-id"), "{}", error);
    }

    #[test]
    fn rates_must_be_positive_and_fees_in_range() {
        assert!(parse(Feature::Voice, &["--max-concurrent", "0"]).is_err());
        assert!(parse(Feature::Payments, &["--fee-percent", "100"]).is_err());
    }

    #[test]
    fn the_subcommand_is_the_control_plane_path() {
        let paths: Vec<&str> = Feature::ALL.iter().map(|f| f.subcommand()).collect();
        assert_eq!(
            paths,
            [
                "sms-gateway",
                "email-gateway",
                "payments-gateway",
                "voice",
                "whatsapp"
            ]
        );
    }
}

//! `managed template update` flags for the product's SMS, WhatsApp, voice, email and
//! payments settings, and `managed template gateway-settings`.
//!
//! Like `--gateway-*` (the model gateway), each feature's setting is REPLACED AS A
//! WHOLE by the control plane: pass every field you want the product to keep. A
//! `--clear-*` / `--disable-*` / `--unlink-*` flag returns it to the platform
//! defaults (voice: off; WhatsApp: unlinked).

use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value, json};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::{
        client::{Missing, get_value, print_dryrun, require_managed_mode, resolve_managed_auth},
        feature_settings::{application_fee, voice_flow},
    },
};

fn positive(id: &'static str, long: &'static str, help: &'static str) -> Arg {
    Arg::new(id)
        .long(long)
        .value_parser(clap::value_parser!(u64).range(1..))
        .help(help)
}

fn flag(id: &'static str, long: &'static str, help: &'static str) -> Arg {
    Arg::new(id)
        .long(long)
        .action(ArgAction::SetTrue)
        .help(help)
}

/// The flags `template update` gains, one group per feature.
pub(super) fn add_args(cmd: Command) -> Command {
    cmd.args([
        // SMS
        Arg::new("sms_pool_id")
            .long("sms-pool-id")
            .help("SMS: the product's origination pool (id or ARN); default: the platform's"),
        positive(
            "sms_monthly_segments",
            "sms-monthly-segments",
            "SMS: billed segments per instance per month",
        ),
        positive(
            "sms_per_minute",
            "sms-per-minute",
            "SMS: texts per instance per minute",
        ),
        flag(
            "sms_allow_promotional",
            "sms-allow-promotional",
            "SMS: allow promotional texts (default: transactional only)",
        ),
        flag(
            "sms_disabled",
            "sms-disabled",
            "SMS: turn SMS off for this product",
        ),
        flag(
            "clear_sms",
            "clear-sms",
            "SMS: return the product to the platform defaults",
        )
        .conflicts_with_all(SMS_IDS),
        // WhatsApp
        Arg::new("whatsapp_number_id")
            .long("whatsapp-number-id")
            .help("WhatsApp: link the product's phone number id (phone-number-id-…)"),
        positive(
            "whatsapp_rpm",
            "whatsapp-rpm",
            "WhatsApp: sends per instance per minute",
        ),
        flag(
            "whatsapp_disabled",
            "whatsapp-disabled",
            "WhatsApp: keep the link but turn WhatsApp off",
        ),
        flag(
            "unlink_whatsapp",
            "unlink-whatsapp",
            "WhatsApp: unlink the product's phone number",
        )
        .conflicts_with_all(WHATSAPP_IDS),
        // Voice
        Arg::new("voice_flow")
            .long("voice-flow")
            .action(ArgAction::Append)
            .help("Voice: a flow instances may start, name=contact-flow-id (repeat for each)"),
        positive(
            "voice_max_concurrent",
            "voice-max-concurrent",
            "Voice: calls in progress at once per instance",
        ),
        positive(
            "voice_monthly_minutes",
            "voice-monthly-minutes",
            "Voice: billed minutes per instance per month",
        ),
        flag(
            "disable_voice",
            "disable-voice",
            "Voice: remove the flow catalog (voice off)",
        )
        .conflicts_with_all(VOICE_IDS),
        // Email
        positive(
            "email_daily_quota",
            "email-daily-quota",
            "Email: recipients per instance per UTC day",
        ),
        positive(
            "email_per_minute",
            "email-per-minute",
            "Email: messages per instance per minute",
        ),
        flag(
            "clear_email",
            "clear-email",
            "Email: return the product to the platform defaults",
        )
        .conflicts_with_all(EMAIL_IDS),
        // Payments
        Arg::new("payments_fee_percent")
            .long("payments-fee-percent")
            .value_parser(clap::value_parser!(f64))
            .help("Payments: application fee as a percent of each charge (0 to under 100)"),
        Arg::new("payments_fee_amount")
            .long("payments-fee-amount")
            .value_parser(clap::value_parser!(u64))
            .help("Payments: application fee as a flat amount in minor units (cents)"),
        positive(
            "payments_rpm",
            "payments-rpm",
            "Payments: Stripe calls per instance per minute",
        ),
        flag(
            "clear_payments",
            "clear-payments",
            "Payments: return the product to the platform's env fallback",
        )
        .conflicts_with_all(PAYMENTS_IDS),
    ])
}

const SMS_IDS: [&str; 5] = [
    "sms_pool_id",
    "sms_monthly_segments",
    "sms_per_minute",
    "sms_allow_promotional",
    "sms_disabled",
];
const WHATSAPP_IDS: [&str; 3] = ["whatsapp_number_id", "whatsapp_rpm", "whatsapp_disabled"];
const VOICE_IDS: [&str; 3] = [
    "voice_flow",
    "voice_max_concurrent",
    "voice_monthly_minutes",
];
const EMAIL_IDS: [&str; 2] = ["email_daily_quota", "email_per_minute"];
const PAYMENTS_IDS: [&str; 3] = [
    "payments_fee_percent",
    "payments_fee_amount",
    "payments_rpm",
];

/// The PATCH fields these flags produce, in a stable order.
pub(super) fn bodies(matches: &ArgMatches) -> Result<Vec<(&'static str, Value)>> {
    let u = |id: &str| matches.get_one::<u64>(id).copied();
    let s = |id: &str| matches.get_one::<String>(id);
    let f = |id: &str| matches.get_flag(id);
    let flows: Vec<String> = matches
        .get_many::<String>("voice_flow")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();

    let mut out = Vec::new();
    let mut push = |key: &'static str, value: Option<Value>| {
        if let Some(value) = value {
            out.push((key, value));
        }
    };
    push(
        "smsGateway",
        sms_setting(
            s("sms_pool_id"),
            u("sms_monthly_segments"),
            u("sms_per_minute"),
            f("sms_allow_promotional"),
            f("sms_disabled"),
            f("clear_sms"),
        ),
    );
    push(
        "whatsapp",
        whatsapp_setting(
            s("whatsapp_number_id"),
            u("whatsapp_rpm"),
            f("whatsapp_disabled"),
            f("unlink_whatsapp"),
        )?,
    );
    push(
        "voice",
        voice_setting(
            &flows,
            u("voice_max_concurrent"),
            u("voice_monthly_minutes"),
            f("disable_voice"),
        )?,
    );
    push(
        "emailGateway",
        email_setting(
            u("email_daily_quota"),
            u("email_per_minute"),
            f("clear_email"),
        ),
    );
    push(
        "paymentsGateway",
        payments_setting(
            matches.get_one::<f64>("payments_fee_percent").copied(),
            u("payments_fee_amount"),
            u("payments_rpm"),
            f("clear_payments"),
        )?,
    );
    Ok(out)
}

/// None when no flag of the feature was given; `{}` to clear.
fn object(fields: Map<String, Value>, clear: bool) -> Option<Value> {
    if clear {
        return Some(json!({}));
    }
    (!fields.is_empty()).then_some(Value::Object(fields))
}

pub(super) fn sms_setting(
    pool_id: Option<&String>,
    monthly_segments: Option<u64>,
    per_minute: Option<u64>,
    allow_promotional: bool,
    disabled: bool,
    clear: bool,
) -> Option<Value> {
    let mut m = Map::new();
    if let Some(v) = pool_id {
        m.insert("poolId".into(), json!(v));
    }
    if let Some(v) = monthly_segments {
        m.insert("monthlySegmentCap".into(), json!(v));
    }
    if let Some(v) = per_minute {
        m.insert("messagesPerMinute".into(), json!(v));
    }
    if allow_promotional {
        m.insert("allowPromotional".into(), json!(true));
    }
    if disabled {
        m.insert("enabled".into(), json!(false));
    }
    object(m, clear)
}

/// The product's channel is one row: a rate or `--whatsapp-disabled` without the
/// number would unlink it, so it is refused here as the control plane refuses it.
pub(super) fn whatsapp_setting(
    number_id: Option<&String>,
    rpm: Option<u64>,
    disabled: bool,
    unlink: bool,
) -> Result<Option<Value>> {
    if unlink {
        return Ok(Some(json!({})));
    }
    let Some(number_id) = number_id else {
        if rpm.is_some() || disabled {
            bail!(
                "--whatsapp-rpm / --whatsapp-disabled replace the product's whole channel, so \
                 they need --whatsapp-number-id too (to unlink, use --unlink-whatsapp)"
            );
        }
        return Ok(None);
    };
    let mut m = Map::new();
    m.insert("originationPhoneNumberId".into(), json!(number_id));
    if let Some(v) = rpm {
        m.insert("requestsPerMinute".into(), json!(v));
    }
    if disabled {
        m.insert("enabled".into(), json!(false));
    }
    Ok(Some(Value::Object(m)))
}

/// Limits without flows would turn voice off, so they need `--voice-flow` too.
pub(super) fn voice_setting(
    flows: &[String],
    max_concurrent: Option<u64>,
    monthly_minutes: Option<u64>,
    disable: bool,
) -> Result<Option<Value>> {
    if disable {
        return Ok(Some(json!({ "flows": {} })));
    }
    if flows.is_empty() {
        if max_concurrent.is_some() || monthly_minutes.is_some() {
            bail!(
                "--voice-max-concurrent / --voice-monthly-minutes replace the product's whole \
                 voice setting, so they need --voice-flow too (the flows instances may start); \
                 to turn voice off, use --disable-voice"
            );
        }
        return Ok(None);
    }
    let mut catalog = Map::new();
    for pair in flows {
        let (name, id) = voice_flow(pair)?;
        if catalog.insert(name.clone(), json!(id)).is_some() {
            bail!("--voice-flow lists '{}' twice", name);
        }
    }
    let mut m = Map::new();
    m.insert("flows".into(), Value::Object(catalog));
    if let Some(v) = max_concurrent {
        m.insert("maxConcurrentCalls".into(), json!(v));
    }
    if let Some(v) = monthly_minutes {
        m.insert("monthlyMinutes".into(), json!(v));
    }
    Ok(Some(Value::Object(m)))
}

pub(super) fn email_setting(
    daily_quota: Option<u64>,
    per_minute: Option<u64>,
    clear: bool,
) -> Option<Value> {
    let mut m = Map::new();
    if let Some(v) = daily_quota {
        m.insert("dailyQuota".into(), json!(v));
    }
    if let Some(v) = per_minute {
        m.insert("perMinute".into(), json!(v));
    }
    object(m, clear)
}

pub(super) fn payments_setting(
    fee_percent: Option<f64>,
    fee_amount: Option<u64>,
    rpm: Option<u64>,
    clear: bool,
) -> Result<Option<Value>> {
    let mut m = Map::new();
    if let Some(fee) = application_fee(fee_percent, fee_amount)? {
        m.insert("applicationFee".into(), fee);
    }
    if let Some(v) = rpm {
        m.insert("requestsPerMinute".into(), json!(v));
    }
    Ok(object(m, clear))
}

/// `template gateway-settings --slug` — the product's settings and the defaults.
#[derive(Debug)]
pub(super) struct GatewaySettingsCommand;

impl GatewaySettingsCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for GatewaySettingsCommand {
    fn command(&self) -> Command {
        command(
            "gateway-settings",
            "Show a template's SMS, WhatsApp, voice, email and payments settings",
        )
        .long_about(
            "Show a template's SMS, WhatsApp, voice, email and payments settings and the\n\
             platform defaults that apply where it sets nothing. Change them with\n\
             `template update --sms-* / --whatsapp-* / --voice-* / --email-* / --payments-*`;\n\
             one instance can differ (`instance sms-gateway | whatsapp | voice |\n\
             email-gateway | payments-gateway`).",
        )
        .arg(
            Arg::new("slug")
                .long("slug")
                .required(true)
                .help("Slug of the template"),
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
        let path = format!("/templates/{}/gateway-settings", urlencoding::encode(slug));
        if matches.get_flag("dryrun") {
            return print_dryrun("GET", &path, None);
        }
        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        let settings = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("template '{}'", slug)),
        )?;
        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&settings)?);
            return Ok(());
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
        writeln!(stdout, "Template '{}'", slug)?;
        stdout.reset()?;
        let defaults = settings.get("defaults").cloned().unwrap_or(Value::Null);
        for (label, key, default_key) in [
            ("sms", "smsGateway", "sms"),
            ("whatsapp", "whatsapp", "whatsapp"),
            ("voice", "voice", "voice"),
            ("email", "emailGateway", "email"),
            ("payments", "paymentsGateway", "payments"),
        ] {
            let set = settings.get(key).filter(|v| !v.is_null());
            let default = defaults.get(default_key).cloned().unwrap_or(Value::Null);
            match set {
                Some(value) => writeln!(stdout, "  {:<9} {} (defaults {})", label, value, default)?,
                None => writeln!(stdout, "  {:<9} not set — defaults {}", label, default)?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update_bodies(args: &[&str]) -> Result<Vec<(&'static str, Value)>> {
        let cmd = add_args(Command::new("update"));
        let mut argv = vec!["update"];
        argv.extend_from_slice(args);
        bodies(&cmd.try_get_matches_from(argv)?)
    }

    #[test]
    fn no_feature_flag_sends_no_feature_field() {
        assert!(update_bodies(&[]).unwrap().is_empty());
    }

    #[test]
    fn each_feature_builds_its_whole_setting() {
        let bodies = update_bodies(&[
            "--sms-pool-id",
            "pool-abc",
            "--sms-monthly-segments",
            "1000",
            "--sms-allow-promotional",
            "--whatsapp-number-id",
            "phone-number-id-1",
            "--whatsapp-rpm",
            "20",
            "--voice-flow",
            "reminder=flow-1",
            "--voice-flow",
            "intake=flow-2",
            "--voice-monthly-minutes",
            "900",
            "--email-daily-quota",
            "500",
            "--payments-fee-percent",
            "2.5",
            "--payments-fee-amount",
            "30",
            "--payments-rpm",
            "120",
        ])
        .unwrap();
        assert_eq!(
            bodies,
            vec![
                (
                    "smsGateway",
                    json!({ "poolId": "pool-abc", "monthlySegmentCap": 1000, "allowPromotional": true })
                ),
                (
                    "whatsapp",
                    json!({ "originationPhoneNumberId": "phone-number-id-1", "requestsPerMinute": 20 })
                ),
                (
                    "voice",
                    json!({ "flows": { "reminder": "flow-1", "intake": "flow-2" }, "monthlyMinutes": 900 })
                ),
                ("emailGateway", json!({ "dailyQuota": 500 })),
                (
                    "paymentsGateway",
                    json!({ "applicationFee": { "percent": 2.5, "amount": 30 }, "requestsPerMinute": 120 })
                ),
            ]
        );
    }

    #[test]
    fn clearing_sends_an_empty_setting_or_turns_voice_off() {
        let bodies = update_bodies(&[
            "--clear-sms",
            "--unlink-whatsapp",
            "--disable-voice",
            "--clear-email",
            "--clear-payments",
        ])
        .unwrap();
        assert_eq!(
            bodies,
            vec![
                ("smsGateway", json!({})),
                ("whatsapp", json!({})),
                ("voice", json!({ "flows": {} })),
                ("emailGateway", json!({})),
                ("paymentsGateway", json!({})),
            ]
        );
    }

    #[test]
    fn a_clear_flag_conflicts_with_setting_the_same_feature() {
        assert!(update_bodies(&["--clear-email", "--email-per-minute", "5"]).is_err());
        assert!(update_bodies(&["--disable-voice", "--voice-flow", "a=b"]).is_err());
    }

    #[test]
    fn partial_settings_that_would_silently_turn_a_feature_off_are_refused() {
        let error = update_bodies(&["--voice-monthly-minutes", "10"]).unwrap_err();
        assert!(error.to_string().contains("--voice-flow"), "{}", error);
        let error = update_bodies(&["--whatsapp-rpm", "10"]).unwrap_err();
        assert!(
            error.to_string().contains("--whatsapp-number-id"),
            "{}",
            error
        );
        assert!(update_bodies(&["--voice-flow", "a=1", "--voice-flow", "a=2"]).is_err());
        assert!(update_bodies(&["--payments-fee-percent", "150"]).is_err());
    }
}

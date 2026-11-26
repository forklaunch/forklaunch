//! Admin settings for the managed features other than the model gateway: SMS,
//! WhatsApp, voice, email and payments.
//!
//! They follow the model gateway exactly. The PRODUCT's settings ride on
//! `managed template update` (`--sms-*`, `--whatsapp-*`, `--voice-*`, `--email-*`,
//! `--payments-*`), each feature replaced as a whole. ONE INSTANCE can differ:
//! `managed instance sms-gateway | whatsapp | voice | email-gateway |
//! payments-gateway --id <id> ...`, where fields left out come from the template and
//! `--clear` removes the override. `gateway-settings` (template or instance) shows
//! what is set and what is in force.
//!
//! This file holds what both sides share: the application fee, and how a feature's
//! effective settings are described in one line.

use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

/// The `applicationFee` object, or None when neither flag was given.
///
/// Stripe takes a percent on subscriptions and a flat amount (minor units, e.g.
/// cents) on one-off payments; a product can set either or both. Zero is a real
/// value — "no fee", overriding the env fallback.
pub(crate) fn application_fee(percent: Option<f64>, amount: Option<u64>) -> Result<Option<Value>> {
    let mut fee = Map::new();
    if let Some(percent) = percent {
        if !percent.is_finite() || !(0.0..100.0).contains(&percent) {
            bail!(
                "the fee percent must be from 0 to under 100 (got {})",
                percent
            );
        }
        fee.insert("percent".to_string(), json!(percent));
    }
    if let Some(amount) = amount {
        fee.insert("amount".to_string(), json!(amount));
    }
    Ok(if fee.is_empty() {
        None
    } else {
        Some(Value::Object(fee))
    })
}

/// A `name=contact-flow-id` pair from `--voice-flow`.
pub(crate) fn voice_flow(pair: &str) -> Result<(String, String)> {
    let Some((name, id)) = pair.split_once('=') else {
        bail!(
            "--voice-flow '{}' must be name=contact-flow-id, e.g. appointment_reminder=0f1e2d3c-…",
            pair
        );
    };
    let (name, id) = (name.trim(), id.trim());
    let valid_name = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !valid_name {
        bail!(
            "voice flow name '{}' must be lowercase letters, digits, _ or - (starting with a letter)",
            name
        );
    }
    if id.is_empty() {
        bail!("--voice-flow '{}' has no contact flow id after '='", pair);
    }
    Ok((name.to_string(), id.to_string()))
}

fn num(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn fee_text(fee: Option<&Value>) -> String {
    let Some(fee) = fee else {
        return "no application fee".to_string();
    };
    let percent = fee.get("percent").and_then(Value::as_f64);
    let amount = fee.get("amount").and_then(Value::as_u64);
    match (percent, amount) {
        (Some(p), Some(a)) => format!("fee {}% / {} minor units", p, a),
        (Some(p), None) => format!("fee {}%", p),
        (None, Some(a)) => format!("fee {} minor units", a),
        (None, None) => "no application fee".to_string(),
    }
}

/// One line for a feature's effective settings, as `PUT /instances/:id/<feature>`
/// and `GET /instances/:id/gateway-settings` answer them.
pub(crate) fn describe(feature: &str, settings: &Value) -> String {
    let overridden = settings.get("override").is_some_and(|o| !o.is_null());
    let suffix = if overridden {
        " (instance override)"
    } else {
        ""
    };
    let line = match feature {
        "sms" => {
            if settings.get("enabled").and_then(Value::as_bool) == Some(false) {
                "off for this product".to_string()
            } else {
                format!(
                    "from {}, {} segments/month, {} texts/min, promotional {}",
                    str_of(settings, "originationIdentity").unwrap_or("(no pool configured)"),
                    num(settings, "monthlySegmentCap"),
                    num(settings, "messagesPerMinute"),
                    if settings.get("allowPromotional").and_then(Value::as_bool) == Some(true) {
                        "allowed"
                    } else {
                        "not allowed"
                    }
                )
            }
        }
        "email" => format!(
            "{} recipients/day, {} messages/min",
            num(settings, "dailyQuota"),
            num(settings, "perMinute")
        ),
        "payments" => format!(
            "{} (from {}), {} Stripe calls/min",
            fee_text(settings.get("applicationFee")),
            str_of(settings, "applicationFeeSource").unwrap_or("none"),
            num(settings, "requestsPerMinute")
        ),
        "voice" => {
            let flows: Vec<&str> = settings
                .get("flows")
                .and_then(Value::as_array)
                .map(|f| f.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            if settings.get("enabled").and_then(Value::as_bool) == Some(false) {
                "off (the product has no flows)".to_string()
            } else {
                format!(
                    "flows {}, {} concurrent calls, {} minutes/month",
                    flows.join(", "),
                    num(settings, "maxConcurrentCalls"),
                    num(settings, "monthlyMinutes")
                )
            }
        }
        "whatsapp" => {
            if settings.get("linked").and_then(Value::as_bool) != Some(true) {
                "no number linked".to_string()
            } else {
                format!(
                    "{} ({} number){}, {} sends/min",
                    str_of(settings, "originationPhoneNumberId").unwrap_or("-"),
                    str_of(settings, "source").unwrap_or("-"),
                    if settings.get("enabled").and_then(Value::as_bool) == Some(true) {
                        ""
                    } else {
                        " — turned off"
                    },
                    num(settings, "requestsPerMinute")
                )
            }
        }
        _ => settings.to_string(),
    };
    format!("{}{}", line, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fee_is_a_percent_under_100_and_or_an_amount() {
        assert_eq!(application_fee(None, None).unwrap(), None);
        assert_eq!(
            application_fee(Some(2.5), Some(30)).unwrap(),
            Some(json!({ "percent": 2.5, "amount": 30 }))
        );
        assert_eq!(
            application_fee(Some(0.0), None).unwrap(),
            Some(json!({ "percent": 0.0 }))
        );
        assert!(application_fee(Some(100.0), None).is_err());
        assert!(application_fee(Some(-1.0), None).is_err());
    }

    #[test]
    fn a_voice_flow_is_a_catalog_name_and_an_id() {
        assert_eq!(
            voice_flow(" reminder = flow-1 ").unwrap(),
            ("reminder".to_string(), "flow-1".to_string())
        );
        assert!(voice_flow("reminder").is_err());
        assert!(voice_flow("Reminder=flow-1").is_err());
        assert!(voice_flow("reminder=").is_err());
    }

    #[test]
    fn describes_what_is_in_force_and_whether_it_is_overridden() {
        assert_eq!(
            describe(
                "email",
                &json!({ "dailyQuota": 500, "perMinute": 10, "override": { "dailyQuota": 500 } })
            ),
            "500 recipients/day, 10 messages/min (instance override)"
        );
        assert_eq!(
            describe(
                "payments",
                &json!({
                    "applicationFee": { "percent": 3 },
                    "applicationFeeSource": "env",
                    "requestsPerMinute": 300
                })
            ),
            "fee 3% (from env), 300 Stripe calls/min"
        );
        assert_eq!(
            describe(
                "whatsapp",
                &json!({ "linked": false, "requestsPerMinute": 30 })
            ),
            "no number linked"
        );
        assert_eq!(
            describe("voice", &json!({ "enabled": false, "flows": [] })),
            "off (the product has no flows)"
        );
    }
}

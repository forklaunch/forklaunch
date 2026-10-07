use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use super::AlertRule;
use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        http_client::post,
        validate::{require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct CreateCommand;

impl CreateCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command("create", "Create an alert rule")
            .arg(
                Arg::new("metric")
                    .long("metric")
                    .required(true)
                    .help("Metric name to threshold on"),
            )
            .arg(
                Arg::new("operator")
                    .long("operator")
                    .required(true)
                    .help("GT, LT, GTE, or LTE"),
            )
            .arg(
                Arg::new("value")
                    .long("value")
                    .required(true)
                    .help("Threshold value"),
            )
            .arg(
                Arg::new("window")
                    .long("window")
                    .required(true)
                    .help("Evaluation window: 5m, 15m, 1h, 6h, or 24h"),
            )
            .arg(
                Arg::new("severity")
                    .long("severity")
                    .required(true)
                    .help("ERROR, ALERT, or INCIDENT"),
            )
            .arg(
                Arg::new("notifier")
                    .long("notifier")
                    .help("Notifier config id to fire when this rule triggers"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let env = matches
            .get_one::<String>("env")
            .context("--env is required")?;
        let service = matches.get_one::<String>("service");
        let metric = matches
            .get_one::<String>("metric")
            .context("--metric is required")?;
        let operator = matches
            .get_one::<String>("operator")
            .context("--operator is required")?;
        let value: f64 = matches
            .get_one::<String>("value")
            .context("--value is required")?
            .parse()
            .context("--value must be a number")?;
        // f64::parse happily accepts "nan"/"inf"/"-infinity"; serde_json can't
        // represent either (Value::from(f64) silently becomes Null for both),
        // so a threshold rule would be created with a missing/invalid value
        // server-side instead of failing here with a clear message.
        if !value.is_finite() {
            bail!("--value must be a finite number, got {}", value);
        }
        let window = matches
            .get_one::<String>("window")
            .context("--window is required")?;
        let severity = matches
            .get_one::<String>("severity")
            .context("--severity is required")?;
        let notifier = matches.get_one::<String>("notifier");
        let json_output = matches.get_flag("json");

        let mut body = serde_json::json!({
            "appId": application_id,
            "env": env,
            "metricName": metric,
            "operator": operator,
            "value": value,
            "windowSize": window,
            "severity": severity,
        });
        if let Some(s) = service {
            body["serviceId"] = serde_json::Value::String(s.clone());
        }
        if let Some(n) = notifier {
            body["notifierConfigId"] = serde_json::Value::String(n.clone());
        }

        let url = format!("{}/alert-rules", get_observability_api_url());
        let response = post(&url, body).with_context(|| "Failed to reach observability API")?;

        let status = response.status();
        if !status.is_success() {
            bail!(
                "Failed to create alert rule ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let rule: AlertRule = response
            .json()
            .with_context(|| "Failed to parse alert rule response")?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&rule)?);
        } else {
            let mut stdout = StandardStream::stdout(ColorChoice::Always);
            stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
            write!(stdout, "  Created")?;
            stdout.reset()?;
            writeln!(stdout, "  alert rule {}", rule.id)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_cmd() -> Command {
        CreateCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        create_cmd().debug_assert();
    }

    #[test]
    fn requires_all_flags() {
        assert!(create_cmd().try_get_matches_from(["create"]).is_err());
        assert!(
            create_cmd()
                .try_get_matches_from([
                    "create",
                    "--metric",
                    "error_rate",
                    "--operator",
                    "GT",
                    "--value",
                    "0.05",
                    "--window",
                    "5m",
                    "--severity",
                    "ALERT",
                ])
                .is_ok()
        );
    }

    #[test]
    fn nan_and_infinity_parse_but_are_not_finite() {
        // Regression: str::parse::<f64> happily accepts these, and
        // serde_json::json! silently turns either into `null` rather than
        // erroring — the CLI has to reject them itself before that happens.
        for input in ["nan", "NaN", "inf", "-infinity"] {
            let v: f64 = input.parse().unwrap();
            assert!(!v.is_finite(), "{input} parsed to a finite value");
        }
        let ok: f64 = "0.05".parse().unwrap();
        assert!(ok.is_finite());
    }
}

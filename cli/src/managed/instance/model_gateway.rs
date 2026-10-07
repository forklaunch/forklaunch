use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Map, Value, json};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::client::{
        Missing, print_dryrun, put_json_optional, require_managed_mode, resolve_managed_auth,
    },
};

#[derive(Debug)]
pub(super) struct ModelGatewayCommand;

impl ModelGatewayCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ModelGatewayCommand {
    fn command(&self) -> Command {
        command(
            "model-gateway",
            "Override the product's model gateway settings for one instance",
        )
        .long_about(
            "Override the product's model gateway settings for one instance.\n\n\
             The product (template) decides which models every instance may call through\n\
             the gateway, the monthly token budget and the requests-per-minute limit\n\
             (`managed template update --gateway-*`). This sets exceptions for ONE\n\
             instance — a bigger budget for a large customer, say. Fields you leave out\n\
             keep coming from the template; --clear removes the override.\n\n\
             --models with an empty list (--models '') turns the gateway off for just this\n\
             instance. A HIPAA product is only ever offered BAA-covered models, whatever\n\
             is listed here.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance"),
        )
        .arg(
            Arg::new("models")
                .long("models")
                .help("Comma-separated catalog aliases this instance may call ('' turns it off)"),
        )
        .arg(
            Arg::new("monthly_tokens")
                .long("monthly-tokens")
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Monthly token budget for this instance"),
        )
        .arg(
            Arg::new("rpm")
                .long("rpm")
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Requests per minute for this instance"),
        )
        .arg(
            Arg::new("clear")
                .long("clear")
                .help("Remove the override; the template's settings apply again")
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["models", "monthly_tokens", "rpm"]),
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
        let id = matches
            .get_one::<String>("id")
            .context("--id is required")?;
        let body = override_body(
            matches.get_one::<String>("models"),
            matches.get_one::<u64>("monthly_tokens").copied(),
            matches.get_one::<u64>("rpm").copied(),
            matches.get_flag("clear"),
        )?;
        let path = format!("/instances/{}/model-gateway", urlencoding::encode(id));

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
        let models: Vec<&str> = result
            .get("models")
            .and_then(Value::as_array)
            .map(|m| m.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        log_ok!(
            stdout,
            "Instance {} now: models {}, {} tokens/month, {} requests/min",
            id,
            if models.is_empty() {
                "none (gateway off)".to_string()
            } else {
                models.join(", ")
            },
            result
                .get("monthlyTokenBudget")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            result
                .get("requestsPerMinute")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        Ok(())
    }
}

pub(super) fn override_body(
    models: Option<&String>,
    monthly_tokens: Option<u64>,
    rpm: Option<u64>,
    clear: bool,
) -> Result<Value> {
    if clear {
        return Ok(json!({ "clear": true }));
    }
    let mut body = Map::new();
    if let Some(models) = models {
        let aliases: Vec<&str> = models
            .split(',')
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .collect();
        body.insert("models".to_string(), json!(aliases));
    }
    if let Some(tokens) = monthly_tokens {
        body.insert("monthlyTokenBudget".to_string(), json!(tokens));
    }
    if let Some(rpm) = rpm {
        body.insert("requestsPerMinute".to_string(), json!(rpm));
    }
    if body.is_empty() {
        bail!("nothing to set — pass --models, --monthly-tokens, --rpm, or --clear");
    }
    Ok(Value::Object(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_only_the_fields_given() {
        assert_eq!(
            override_body(None, Some(5_000_000), None, false).unwrap(),
            json!({ "monthlyTokenBudget": 5_000_000 })
        );
        let models = "terra".to_string();
        assert_eq!(
            override_body(Some(&models), None, Some(10), false).unwrap(),
            json!({ "models": ["terra"], "requestsPerMinute": 10 })
        );
    }

    #[test]
    fn an_empty_model_list_turns_the_gateway_off_for_the_instance() {
        let none = String::new();
        assert_eq!(
            override_body(Some(&none), None, None, false).unwrap(),
            json!({ "models": [] })
        );
    }

    #[test]
    fn clear_and_nothing_are_distinct() {
        assert_eq!(
            override_body(None, None, None, true).unwrap(),
            json!({ "clear": true })
        );
        assert!(override_body(None, None, None, false).is_err());
    }
}

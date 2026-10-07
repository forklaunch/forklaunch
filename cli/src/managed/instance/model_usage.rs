use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::Value;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::command::command,
    managed::client::{
        Missing, get_value, print_dryrun, require_managed_mode, resolve_managed_auth,
    },
};

#[derive(Debug)]
pub(super) struct ModelUsageCommand;

impl ModelUsageCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for ModelUsageCommand {
    fn command(&self) -> Command {
        command(
            "model-usage",
            "Show a month of an instance's model gateway usage: calls, tokens and cost per model",
        )
        .long_about(
            "Show a month of an instance's model gateway usage.\n\n\
             The model gateway is how a managed instance calls platform-hosted AI models\n\
             without holding a provider key. Every call that reached a model is counted\n\
             here: calls, errors, input and output tokens, and cost, per model and in\n\
             total, next to the budget and models in force for the instance. Prompts and\n\
             completions are never stored, so they never appear here.\n\n\
             --month is a UTC calendar month (YYYY-MM); the default is the current one.",
        )
        .arg(
            Arg::new("id")
                .long("id")
                .required(true)
                .help("Id of the instance"),
        )
        .arg(
            Arg::new("month")
                .long("month")
                .help("UTC calendar month, YYYY-MM (default: this month)"),
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
        let month = matches.get_one::<String>("month");
        if let Some(month) = month {
            if !is_month(month) {
                bail!("--month '{}' must be YYYY-MM, e.g. 2026-09", month);
            }
        }
        let path = usage_path(id, month.map(String::as_str));

        if matches.get_flag("dryrun") {
            return print_dryrun("GET", &path, None);
        }

        let auth_mode = resolve_managed_auth()?;
        require_managed_mode(&auth_mode)?;
        let report = get_value(
            &auth_mode,
            &path,
            Missing::Resource(format!("instance '{}'", id)),
        )?;

        if matches.get_flag("json") {
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(());
        }
        print_report(&report)
    }
}

pub(super) fn usage_path(id: &str, month: Option<&str>) -> String {
    let base = format!("/instances/{}/model-usage", urlencoding::encode(id));
    match month {
        Some(month) => format!("{}?month={}", base, urlencoding::encode(month)),
        None => base,
    }
}

fn is_month(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7
        && bytes[4] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || b.is_ascii_digit())
        && matches!(value[5..].parse::<u8>(), Ok(1..=12))
}

fn num(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn print_report(report: &Value) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    let totals = report.get("totals").cloned().unwrap_or(Value::Null);
    let budget = num(report, "monthlyTokenBudget");
    let used = num(&totals, "totalTokens");

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(
        stdout,
        "Model usage, {}",
        report.get("month").and_then(Value::as_str).unwrap_or("-")
    )?;
    stdout.reset()?;
    writeln!(stdout)?;
    let models: Vec<&str> = report
        .get("models")
        .and_then(Value::as_array)
        .map(|m| m.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    writeln!(
        stdout,
        "  {:<18} {}",
        "Models",
        if models.is_empty() {
            "- (gateway off)".to_string()
        } else {
            models.join(", ")
        }
    )?;
    if let Some(withheld) = report.get("withheldForHipaa").and_then(Value::as_array) {
        if !withheld.is_empty() {
            let names: Vec<&str> = withheld.iter().filter_map(Value::as_str).collect();
            writeln!(
                stdout,
                "  {:<18} {} (not BAA-covered; this product is HIPAA)",
                "Withheld",
                names.join(", ")
            )?;
        }
    }
    writeln!(
        stdout,
        "  {:<18} {:.0} of {:.0} tokens ({:.1}%)",
        "Budget",
        used,
        budget,
        if budget > 0.0 {
            used / budget * 100.0
        } else {
            0.0
        }
    )?;
    writeln!(
        stdout,
        "  {:<18} {}/min",
        "Rate limit",
        num(report, "requestsPerMinute")
    )?;
    writeln!(stdout)?;

    writeln!(
        stdout,
        "  {:<12} {:>7} {:>7} {:>12} {:>12} {:>10}",
        "MODEL", "CALLS", "ERRORS", "IN TOKENS", "OUT TOKENS", "COST USD"
    )?;
    let rows = report
        .get("byModel")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for row in rows.iter().chain(std::iter::once(&totals)) {
        let is_total = std::ptr::eq(row, &totals);
        writeln!(
            stdout,
            "  {:<12} {:>7} {:>7} {:>12} {:>12} {:>10.4}",
            if is_total {
                "total"
            } else {
                row.get("model").and_then(Value::as_str).unwrap_or("-")
            },
            num(row, "calls"),
            num(row, "errors"),
            num(row, "promptTokens"),
            num(row, "completionTokens"),
            num(row, "costUsd")
        )?;
    }
    writeln!(stdout)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_month_is_passed_as_a_query_and_the_id_is_encoded() {
        assert_eq!(usage_path("abc", None), "/instances/abc/model-usage");
        assert_eq!(
            usage_path("a/b", Some("2026-09")),
            "/instances/a%2Fb/model-usage?month=2026-09"
        );
    }

    #[test]
    fn only_real_months_are_accepted() {
        assert!(is_month("2026-09"));
        assert!(is_month("2026-12"));
        assert!(!is_month("2026-13"));
        assert!(!is_month("2026-9"));
        assert!(!is_month("26-09-01"));
    }
}

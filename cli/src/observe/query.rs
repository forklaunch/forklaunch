use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        http_client::post,
        validate::{require_integration, require_manifest},
    },
};

/// Raw PromQL query access — a power-user escape hatch. Reaches only
/// OTel-derived application metrics (request rate, latency, error rate); it
/// does NOT reach CloudWatch-sourced infra metrics (RDS/ElastiCache CPU%,
/// memory, connections) — use `fl infra status --metrics` for those instead.
#[derive(Debug)]
pub(super) struct QueryCommand;

impl QueryCommand {
    pub(super) fn new() -> Self {
        Self
    }
}

impl CliCommand for QueryCommand {
    fn command(&self) -> Command {
        command(
            "query",
            "Run a raw PromQL query against application metrics (does not reach CloudWatch/infra metrics — see `fl infra status --metrics`)",
        )
        .arg(Arg::new("promql").required(true).help("The PromQL query"))
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .help("Filter to an environment"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .help("Filter to a region"),
        )
        .arg(
            Arg::new("start")
                .long("start")
                .help("Range start (RFC3339 or unix timestamp)"),
        )
        .arg(
            Arg::new("end")
                .long("end")
                .help("Range end (RFC3339 or unix timestamp)"),
        )
        .arg(Arg::new("step").long("step").help("Query resolution step"))
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("The application path"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let promql = matches
            .get_one::<String>("promql")
            .context("PromQL query is required")?;

        let mut body = serde_json::json!({
            "query": promql,
            "applicationId": application_id,
        });
        if let Some(v) = matches.get_one::<String>("environment") {
            body["environment"] = serde_json::Value::String(v.clone());
        }
        if let Some(v) = matches.get_one::<String>("region") {
            body["region"] = serde_json::Value::String(v.clone());
        }
        if let Some(v) = matches.get_one::<String>("start") {
            body["start"] = serde_json::Value::String(v.clone());
        }
        if let Some(v) = matches.get_one::<String>("end") {
            body["end"] = serde_json::Value::String(v.clone());
        }
        if let Some(v) = matches.get_one::<String>("step") {
            body["step"] = serde_json::Value::String(v.clone());
        }

        let url = format!("{}/monitoring/promql", get_observability_api_url());
        let response = post(&url, body)
            .with_context(|| "Failed to reach observability API")?;

        let status = response.status();
        if !status.is_success() {
            bail!(
                "PromQL query failed ({}): {}",
                status,
                response.text().unwrap_or_default()
            );
        }

        let result: serde_json::Value = response
            .json()
            .with_context(|| "Failed to parse PromQL response")?;
        println!("{}", serde_json::to_string_pretty(&result)?);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_cmd() -> Command {
        QueryCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        query_cmd().debug_assert();
    }

    #[test]
    fn requires_promql_argument() {
        assert!(query_cmd().try_get_matches_from(["query"]).is_err());
        assert!(
            query_cmd()
                .try_get_matches_from(["query", "rate(http_requests_total[5m])"])
                .is_ok()
        );
    }
}

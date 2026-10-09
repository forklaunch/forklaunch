//! `forklaunch app log-limit` — read and change a component's log limit.
//!
//! A log limit caps how many lines per second each container of a service or
//! worker may ship. Who gets to set it depends on where the component runs,
//! and the platform reports that as `logThrottleControl`:
//!
//! - `platform`: shared platform hosts. The limit is fixed and a change is
//!   refused with an explanation.
//! - `pool-host`: an org compute pool. Every container on a host shares the
//!   pool's per-host budget (`forklaunch org pool log-budget`).
//! - `component`: Fargate or dedicated EC2. This command sets it.
//!
//! Without a change flag this is a read; with one it is a PATCH that is saved
//! for the next deploy unless `--deploy` asks for one now.

use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgGroup, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug)]
pub(crate) struct LogLimitCommand;

impl LogLimitCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

/// A component as `GET /applications/{id}/services` lists it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComponentSummary {
    id: String,
    name: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default, rename = "type")]
    r#type: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ComponentsResponse {
    services: Vec<ComponentSummary>,
}

/// The log-limit fields of `GET /services|workers/{id}/infrastructure`. The
/// response carries much more; only these are read.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct InfrastructureLogLimit {
    #[serde(default)]
    hosting_type: Option<String>,
    #[serde(default)]
    log_throttle_control: Option<String>,
    #[serde(default)]
    log_throttle_enabled: Option<bool>,
    #[serde(default)]
    log_lines_per_second: Option<u64>,
}

/// Response of `PATCH .../log-limit`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogLimitUpdateResponse {
    message: String,
    log_throttle_control: String,
    log_throttle_enabled: bool,
    #[serde(default)]
    log_lines_per_second: Option<u64>,
    deployed: bool,
}

/// One row of the table, and one element of `--json` output.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComponentLogLimit {
    component: String,
    id: String,
    #[serde(rename = "type")]
    kind: String,
    deployed: bool,
    #[serde(flatten)]
    infrastructure: InfrastructureLogLimit,
}

/// `service` or `worker`: which infrastructure route a component lives behind.
fn component_kind(summary: &ComponentSummary) -> &'static str {
    match summary.r#type.as_deref() {
        Some(t) if t.eq_ignore_ascii_case("worker") => "worker",
        _ => "service",
    }
}

fn kind_path(kind: &str) -> &'static str {
    if kind == "worker" {
        "workers"
    } else {
        "services"
    }
}

/// Who decides this component's limit, in words.
fn who_sets(control: Option<&str>) -> &'static str {
    match control {
        Some("platform") => "fixed (platform shared hosts)",
        Some("pool-host") => "org pool budget per host",
        Some("component") => "this component",
        _ => "—",
    }
}

/// The limit itself, in words.
fn limit_text(enabled: Option<bool>, lines_per_second: Option<u64>) -> String {
    match (enabled, lines_per_second) {
        (Some(false), _) => "off".to_string(),
        (_, Some(n)) => format!("{} lines/s per container", n),
        _ => "—".to_string(),
    }
}

/// Body of `PATCH .../log-limit`. Unset flags are omitted so the platform
/// keeps whatever it already holds for them.
fn build_log_limit_body(
    enabled: Option<bool>,
    rate: Option<u64>,
    deploy: bool,
) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(enabled) = enabled {
        body.insert("logThrottleEnabled".into(), serde_json::json!(enabled));
    }
    if let Some(rate) = rate {
        body.insert("logLinesPerSecond".into(), serde_json::json!(rate));
    }
    if deploy {
        body.insert("deploy".into(), serde_json::json!(true));
    }
    serde_json::Value::Object(body)
}

/// `--on` / `--off` as the tri-state the body wants.
fn requested_enabled(on: bool, off: bool) -> Option<bool> {
    match (on, off) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

fn query(environment: &str, region: &str) -> String {
    format!(
        "environment={}&region={}",
        urlencoding::encode(environment),
        urlencoding::encode(region)
    )
}

fn list_components(
    api_url: &str,
    application_id: &str,
    environment: &str,
    region: &str,
) -> Result<Vec<ComponentSummary>> {
    let url = format!(
        "{}/applications/{}/services?{}",
        api_url,
        urlencoding::encode(application_id),
        query(environment, region)
    );
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        bail!(
            "Failed to list services ({}): {}",
            response.status(),
            response.text().unwrap_or_default()
        );
    }
    let result: ComponentsResponse = response
        .json()
        .with_context(|| "Failed to parse services response")?;
    Ok(result.services)
}

fn fetch_log_limit(
    api_url: &str,
    summary: &ComponentSummary,
    environment: &str,
    region: &str,
) -> Result<ComponentLogLimit> {
    let kind = component_kind(summary);
    let url = format!(
        "{}/{}/{}/infrastructure?{}",
        api_url,
        kind_path(kind),
        urlencoding::encode(&summary.id),
        query(environment, region)
    );
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let status = response.status();
    // 404 means there is no infrastructure for it in this environment/region yet.
    if status.as_u16() == 404 {
        return Ok(ComponentLogLimit {
            component: summary.name.clone(),
            id: summary.id.clone(),
            kind: kind.to_string(),
            deployed: false,
            infrastructure: InfrastructureLogLimit::default(),
        });
    }
    if !status.is_success() {
        bail!(
            "Failed to read infrastructure for {} ({}): {}",
            summary.name,
            status,
            response.text().unwrap_or_default()
        );
    }
    let infrastructure: InfrastructureLogLimit = response
        .json()
        .with_context(|| format!("Failed to parse infrastructure for {}", summary.name))?;
    Ok(ComponentLogLimit {
        component: summary.name.clone(),
        id: summary.id.clone(),
        kind: kind.to_string(),
        deployed: true,
        infrastructure,
    })
}

fn find_component<'a>(
    components: &'a [ComponentSummary],
    name: &str,
) -> Result<&'a ComponentSummary> {
    components.iter().find(|c| c.name == name).with_context(|| {
        let names: Vec<&str> = components.iter().map(|c| c.name.as_str()).collect();
        if names.is_empty() {
            format!(
                "No component named '{}': this application has no services or workers here",
                name
            )
        } else {
            format!(
                "No component named '{}'. Known services and workers: {}",
                name,
                names.join(", ")
            )
        }
    })
}

fn print_table(rows: &[ComponentLogLimit]) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    if rows.is_empty() {
        writeln!(stdout, "No services or workers found.")?;
        return Ok(());
    }

    writeln!(stdout)?;
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(
        stdout,
        "  {:<28}  {:<8}  {:<16}  {:<30}  LIMIT",
        "COMPONENT", "TYPE", "HOSTING", "SET BY"
    )?;
    stdout.reset()?;

    for row in rows {
        let (hosting, set_by, limit) = if row.deployed {
            (
                row.infrastructure
                    .hosting_type
                    .clone()
                    .unwrap_or_else(|| "—".to_string()),
                who_sets(row.infrastructure.log_throttle_control.as_deref()).to_string(),
                limit_text(
                    row.infrastructure.log_throttle_enabled,
                    row.infrastructure.log_lines_per_second,
                ),
            )
        } else {
            ("not deployed".to_string(), "—".to_string(), "—".to_string())
        };
        writeln!(
            stdout,
            "  {:<28}  {:<8}  {:<16}  {:<30}  {}",
            row.component, row.kind, hosting, set_by, limit
        )?;
    }
    writeln!(stdout)?;
    Ok(())
}

impl CliCommand for LogLimitCommand {
    fn command(&self) -> Command {
        command(
            "log-limit",
            "Show or change how many log lines per second each container of a service or worker may ship",
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment to read or change"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("Region to read or change"),
        )
        .arg(
            Arg::new("component")
                .short('s')
                .long("component")
                .help(
                    "Service or worker manifest name (e.g. billing-service, billing-worker). \
                     Without it, every service and worker is listed",
                ),
        )
        .arg(
            Arg::new("on")
                .long("on")
                .action(ArgAction::SetTrue)
                .conflicts_with("off")
                .help("Turn the log limit on"),
        )
        .arg(
            Arg::new("off")
                .long("off")
                .action(ArgAction::SetTrue)
                .help("Turn the log limit off"),
        )
        .arg(
            Arg::new("rate")
                .long("rate")
                .value_name("N")
                .value_parser(clap::value_parser!(u64).range(1..))
                .help("Lines per second per container (whole number, at least 1)"),
        )
        .group(
            ArgGroup::new("change")
                .args(["on", "off", "rate"])
                .multiple(true)
                .requires("component"),
        )
        .arg(
            Arg::new("deploy")
                .long("deploy")
                .action(ArgAction::SetTrue)
                .requires("change")
                .help("Redeploy the component now (default: save for the next deploy)"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Output raw JSON instead of formatted terminal output"),
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let api_url = get_platform_management_api_url();

        let environment = matches
            .get_one::<String>("environment")
            .context("--environment is required")?;
        let region = matches
            .get_one::<String>("region")
            .context("--region is required")?;
        let component = matches.get_one::<String>("component");
        let enabled = requested_enabled(matches.get_flag("on"), matches.get_flag("off"));
        let rate = matches.get_one::<u64>("rate").copied();
        let deploy = matches.get_flag("deploy");
        let json_output = matches.get_flag("json");

        let components = list_components(&api_url, &application_id, environment, region)?;

        let Some(component) = component else {
            let rows = components
                .iter()
                .map(|c| fetch_log_limit(&api_url, c, environment, region))
                .collect::<Result<Vec<_>>>()?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&rows)?);
                return Ok(());
            }
            return print_table(&rows);
        };

        let summary = find_component(&components, component)?;

        if enabled.is_none() && rate.is_none() {
            let row = fetch_log_limit(&api_url, summary, environment, region)?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&row)?);
                return Ok(());
            }
            return print_table(std::slice::from_ref(&row));
        }

        let kind = component_kind(summary);
        let url = format!(
            "{}/{}/{}/infrastructure/{}/{}/log-limit",
            api_url,
            kind_path(kind),
            urlencoding::encode(&summary.id),
            urlencoding::encode(environment),
            urlencoding::encode(region)
        );
        let body = build_log_limit_body(enabled, rate, deploy);
        let response =
            http_client::patch(&url, body).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().unwrap_or_default();
            // 400 and 404 carry a reason written for a human (e.g. shared
            // hosts refusing a per-component limit): relay it as-is.
            if matches!(status.as_u16(), 400 | 404) && !detail.trim().is_empty() {
                bail!("{}", detail.trim());
            }
            bail!("Failed to change the log limit ({}): {}", status, detail);
        }

        let result: LogLimitUpdateResponse = response
            .json()
            .with_context(|| "Failed to parse log-limit response")?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
            return Ok(());
        }

        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        log_ok!(stdout, "{}", result.message);
        stdout.set_color(ColorSpec::new().set_bold(true))?;
        write!(stdout, "  {}", summary.name)?;
        stdout.reset()?;
        writeln!(
            stdout,
            "  {} (set by: {})",
            limit_text(
                Some(result.log_throttle_enabled),
                result.log_lines_per_second
            ),
            who_sets(Some(result.log_throttle_control.as_str()))
        )?;
        if result.deployed {
            log_write!(stdout, Color::Green, "  Deploying now.");
            writeln!(stdout)?;
        } else {
            log_info!(
                stdout,
                "Saved. It takes effect on the next deploy (or re-run with --deploy)."
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> Command {
        LogLimitCommand::new().command().version("0.0.0-test")
    }

    fn base() -> Vec<&'static str> {
        vec!["log-limit", "-e", "dev", "-r", "us-east-1"]
    }

    fn with(extra: &[&'static str]) -> Vec<&'static str> {
        let mut args = base();
        args.extend_from_slice(extra);
        args
    }

    #[test]
    fn command_definition_is_valid() {
        cmd().debug_assert();
    }

    #[test]
    fn environment_and_region_are_required() {
        assert!(cmd().try_get_matches_from(["log-limit"]).is_err());
        assert!(
            cmd()
                .try_get_matches_from(["log-limit", "-e", "dev"])
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(["log-limit", "-r", "us-east-1"])
                .is_err()
        );
        assert!(cmd().try_get_matches_from(base()).is_ok());
    }

    #[test]
    fn component_alone_is_a_read() {
        let m = cmd()
            .try_get_matches_from(with(&["-s", "billing-service"]))
            .unwrap();
        assert_eq!(m.get_one::<String>("component").unwrap(), "billing-service");
        assert!(!m.get_flag("on") && !m.get_flag("off"));
        assert!(m.get_one::<u64>("rate").is_none());
    }

    #[test]
    fn on_and_off_are_mutually_exclusive() {
        assert!(
            cmd()
                .try_get_matches_from(with(&["-s", "svc", "--on", "--off"]))
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(with(&["-s", "svc", "--on"]))
                .is_ok()
        );
        assert!(
            cmd()
                .try_get_matches_from(with(&["-s", "svc", "--off"]))
                .is_ok()
        );
    }

    #[test]
    fn change_flags_require_a_component() {
        for flags in [
            &["--on"][..],
            &["--off"][..],
            &["--rate", "50"][..],
            &["--on", "--rate", "50"][..],
        ] {
            assert!(
                cmd().try_get_matches_from(with(flags)).is_err(),
                "{flags:?}"
            );
        }
    }

    #[test]
    fn deploy_requires_a_change() {
        assert!(cmd().try_get_matches_from(with(&["--deploy"])).is_err());
        assert!(
            cmd()
                .try_get_matches_from(with(&["-s", "svc", "--deploy"]))
                .is_err()
        );
        assert!(
            cmd()
                .try_get_matches_from(with(&["-s", "svc", "--rate", "10", "--deploy"]))
                .is_ok()
        );
    }

    #[test]
    fn rate_must_be_a_whole_number_of_at_least_one() {
        for bad in ["0", "-1", "1.5", "abc", ""] {
            assert!(
                cmd()
                    .try_get_matches_from(with(&["-s", "svc", "--rate", bad]))
                    .is_err(),
                "{bad}"
            );
        }
        let m = cmd()
            .try_get_matches_from(with(&["-s", "svc", "--rate", "1"]))
            .unwrap();
        assert_eq!(*m.get_one::<u64>("rate").unwrap(), 1);
        let m = cmd()
            .try_get_matches_from(with(&["-s", "svc", "--rate", "5000"]))
            .unwrap();
        assert_eq!(*m.get_one::<u64>("rate").unwrap(), 5000);
    }

    #[test]
    fn requested_enabled_maps_flags() {
        assert_eq!(requested_enabled(true, false), Some(true));
        assert_eq!(requested_enabled(false, true), Some(false));
        assert_eq!(requested_enabled(false, false), None);
    }

    #[test]
    fn body_omits_unset_fields() {
        assert_eq!(
            build_log_limit_body(Some(true), None, false),
            serde_json::json!({ "logThrottleEnabled": true })
        );
        assert_eq!(
            build_log_limit_body(Some(false), None, false),
            serde_json::json!({ "logThrottleEnabled": false })
        );
        assert_eq!(
            build_log_limit_body(None, Some(200), false),
            serde_json::json!({ "logLinesPerSecond": 200 })
        );
        assert_eq!(
            build_log_limit_body(Some(true), Some(200), false),
            serde_json::json!({ "logThrottleEnabled": true, "logLinesPerSecond": 200 })
        );
        assert_eq!(
            build_log_limit_body(Some(false), None, true),
            serde_json::json!({ "logThrottleEnabled": false, "deploy": true })
        );
        assert_eq!(
            build_log_limit_body(None, Some(10), true),
            serde_json::json!({ "logLinesPerSecond": 10, "deploy": true })
        );
    }

    #[test]
    fn who_sets_describes_each_control() {
        assert_eq!(who_sets(Some("platform")), "fixed (platform shared hosts)");
        assert_eq!(who_sets(Some("pool-host")), "org pool budget per host");
        assert_eq!(who_sets(Some("component")), "this component");
        assert_eq!(who_sets(None), "—");
    }

    #[test]
    fn limit_text_covers_off_rate_and_unknown() {
        assert_eq!(limit_text(Some(false), Some(100)), "off");
        assert_eq!(
            limit_text(Some(true), Some(100)),
            "100 lines/s per container"
        );
        assert_eq!(limit_text(None, Some(100)), "100 lines/s per container");
        assert_eq!(limit_text(Some(true), None), "—");
        assert_eq!(limit_text(None, None), "—");
    }

    #[test]
    fn component_kind_picks_the_route() {
        let worker = ComponentSummary {
            id: "w1".into(),
            name: "jobs-worker".into(),
            status: None,
            r#type: Some("worker".into()),
            version: None,
        };
        let service = ComponentSummary {
            r#type: Some("service".into()),
            ..worker.clone()
        };
        let untyped = ComponentSummary {
            r#type: None,
            ..worker.clone()
        };
        assert_eq!(kind_path(component_kind(&worker)), "workers");
        assert_eq!(kind_path(component_kind(&service)), "services");
        assert_eq!(kind_path(component_kind(&untyped)), "services");
    }

    #[test]
    fn find_component_matches_by_name_and_lists_known_names() {
        let components: Vec<ComponentSummary> = serde_json::from_str::<ComponentsResponse>(
            r#"{"services":[
                {"id":"s1","name":"billing-service","status":"RUNNING","type":"service","version":"1.0.0"},
                {"id":"w1","name":"billing-worker","status":"RUNNING","type":"worker"}
            ]}"#,
        )
        .unwrap()
        .services;
        assert_eq!(
            find_component(&components, "billing-worker").unwrap().id,
            "w1"
        );
        let err = find_component(&components, "nope").unwrap_err().to_string();
        assert!(
            err.contains("billing-service") && err.contains("billing-worker"),
            "{err}"
        );
    }

    #[test]
    fn infrastructure_deserializes_with_and_without_log_fields() {
        let full: InfrastructureLogLimit = serde_json::from_str(
            r#"{"id":"x","hostingType":"ecs-fargate","logThrottleControl":"component",
                "logThrottleEnabled":true,"logLinesPerSecond":250,"cpu":256}"#,
        )
        .unwrap();
        assert_eq!(full.hosting_type.as_deref(), Some("ecs-fargate"));
        assert_eq!(full.log_throttle_control.as_deref(), Some("component"));
        assert_eq!(full.log_throttle_enabled, Some(true));
        assert_eq!(full.log_lines_per_second, Some(250));

        let pooled: InfrastructureLogLimit =
            serde_json::from_str(r#"{"hostingType":"ec2","logThrottleControl":"pool-host"}"#)
                .unwrap();
        assert_eq!(pooled.log_throttle_control.as_deref(), Some("pool-host"));
        assert!(pooled.log_lines_per_second.is_none());

        let bare: InfrastructureLogLimit = serde_json::from_str("{}").unwrap();
        assert!(bare.hosting_type.is_none());
        assert!(bare.log_throttle_enabled.is_none());
    }

    #[test]
    fn update_response_deserializes_with_and_without_rate() {
        let with_rate: LogLimitUpdateResponse = serde_json::from_str(
            r#"{"message":"Log limit saved","logThrottleControl":"component",
                "logThrottleEnabled":true,"logLinesPerSecond":100,"deployed":false}"#,
        )
        .unwrap();
        assert_eq!(with_rate.log_lines_per_second, Some(100));
        assert!(!with_rate.deployed);

        let off: LogLimitUpdateResponse = serde_json::from_str(
            r#"{"message":"Log limit off; deploying","logThrottleControl":"component",
                "logThrottleEnabled":false,"deployed":true}"#,
        )
        .unwrap();
        assert!(off.log_lines_per_second.is_none());
        assert!(off.deployed);
        assert!(!off.log_throttle_enabled);
    }

    #[test]
    fn json_row_flattens_infrastructure() {
        let row = ComponentLogLimit {
            component: "billing-service".into(),
            id: "s1".into(),
            kind: "service".into(),
            deployed: true,
            infrastructure: InfrastructureLogLimit {
                hosting_type: Some("ecs-fargate".into()),
                log_throttle_control: Some("component".into()),
                log_throttle_enabled: Some(true),
                log_lines_per_second: Some(100),
            },
        };
        let value = serde_json::to_value(&row).unwrap();
        assert_eq!(value["type"], "service");
        assert_eq!(value["logLinesPerSecond"], 100);
        assert_eq!(value["logThrottleControl"], "component");
    }
}

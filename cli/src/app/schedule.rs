//! `forklaunch app schedule`: have the platform call a service at set times.
//!
//! A timer inside a process (`setInterval`, an in-process cron) stops while a
//! scale-to-zero service is asleep. A scheduled call replaces it: at each time
//! the platform calls a path on the service, waking it first if it sleeps. An
//! always-on service is called the same way, so the same route works on both.
//!
//! Say a `notifications` service sweeps for due reminders every 15 minutes. It
//! exposes the sweep as `POST /internal/sweep` and stores a schedule for it:
//!
//! ```text
//! forklaunch app schedule -e production -r us-west-2 --service notifications \
//!     --set sweep --schedule "rate(15 minutes)" --call /internal/sweep
//! ```
//!
//! Schedules are kept per service, environment and region. With no `--set` or
//! `--remove` this command lists them. A change is recorded and takes effect on
//! the next deployment.
//!
//! The platform stores a service's schedules as one list and a write replaces
//! the whole list, so a change here reads the list, changes the one entry and
//! writes it back. The same request carries the service's instance size and
//! count, which the platform requires; they are sent back exactly as read, and
//! no deployment is asked for.
//!
//! Each call is signed with the service's `FORKLAUNCH_SCHEDULE_SECRET`. The
//! route is reachable from the internet, so the service must check the
//! signature: see "Scheduled calls" in the `deploy-mode` skill.

use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
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

/// The methods a scheduled call may use, the default first.
pub(crate) const SCHEDULE_METHODS: &[&str] = &["POST", "GET", "PUT", "PATCH", "DELETE"];
/// The most scheduled calls one service may have.
const MAX_SCHEDULED_CALLS: usize = 20;
const MAX_PATH_LENGTH: usize = 512;

/// One scheduled call, as the platform stores it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct ScheduledCall {
    name: String,
    schedule: String,
    /// The platform reads a missing method as POST.
    #[serde(default = "default_method")]
    method: String,
    path: String,
}

fn default_method() -> String {
    "POST".to_string()
}

/// 1 to 32 lower-case letters, digits or dashes, not starting or ending with
/// a dash.
fn validate_name(name: &str) -> Result<()> {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
    if name.is_empty()
        || name.len() > 32
        || !name.chars().all(allowed)
        || name.starts_with('-')
        || name.ends_with('-')
    {
        bail!(
            "'{}' is not a schedule name: use 1 to 32 lower-case letters, digits or dashes, \
             starting and ending with a letter or digit (e.g. sweep)",
            name
        );
    }
    Ok(())
}

/// `rate(N minutes|hours|days)` or a six-field `cron(...)`, in UTC.
fn validate_schedule(schedule: &str) -> Result<()> {
    let hint = "use \"rate(15 minutes)\" or a six-field \"cron(0 14 * * ? *)\" (UTC)";
    if let Some(inner) = schedule
        .strip_prefix("rate(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let Some((count, unit)) = inner.split_once(' ') else {
            bail!("--schedule '{}' is not valid: {}", schedule, hint);
        };
        let digits =
            !count.is_empty() && count.len() <= 5 && count.bytes().all(|b| b.is_ascii_digit());
        let known_unit = ["minute", "minutes", "hour", "hours", "day", "days"].contains(&unit);
        if !digits || !known_unit {
            bail!("--schedule '{}' is not valid: {}", schedule, hint);
        }
        let n: u32 = count.parse()?;
        if n < 1 {
            bail!(
                "--schedule '{}' is not valid: the rate must be at least 1",
                schedule
            );
        }
        // "1 minute", "2 minutes": the platform refuses the other way round.
        if (n == 1) == unit.ends_with('s') {
            let singular = unit.trim_end_matches('s');
            bail!(
                "--schedule '{}' is not valid: write \"rate({} {}{})\"",
                schedule,
                n,
                singular,
                if n == 1 { "" } else { "s" }
            );
        }
        return Ok(());
    }
    if let Some(inner) = schedule
        .strip_prefix("cron(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let fields: Vec<&str> = inner.split(' ').collect();
        let well_formed = |field: &&str| {
            !field.is_empty()
                && !field
                    .chars()
                    .any(|c| c.is_whitespace() || c == '(' || c == ')')
        };
        if fields.len() == 6 && fields.iter().all(well_formed) {
            return Ok(());
        }
        bail!(
            "--schedule '{}' is not valid: a cron schedule has six fields separated by single \
             spaces: minutes hours day-of-month month day-of-week year, in UTC \
             (e.g. \"cron(0 14 * * ? *)\" is every day at 14:00 UTC)",
            schedule
        );
    }
    bail!("--schedule '{}' is not valid: {}", schedule, hint)
}

/// A URL path starting with "/", at most 512 characters.
fn validate_path(path: &str) -> Result<()> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || "-._~!$&'()*+,;=:@/%?".contains(c);
    if !path.starts_with('/') || path.len() > MAX_PATH_LENGTH || !path.chars().all(allowed) {
        bail!(
            "--call '{}' is not valid: it must start with \"/\" and be a URL path of at most {} \
             characters (e.g. /internal/sweep)",
            path,
            MAX_PATH_LENGTH
        );
    }
    Ok(())
}

/// Checks one call with the platform's rules and returns it as it will be
/// stored: schedule trimmed, method in upper case.
fn checked_call(name: &str, schedule: &str, method: &str, path: &str) -> Result<ScheduledCall> {
    validate_name(name)?;
    let schedule = schedule.trim();
    validate_schedule(schedule)?;
    let method = method.to_ascii_uppercase();
    if !SCHEDULE_METHODS.contains(&method.as_str()) {
        bail!(
            "--method '{}' is not valid: one of {}",
            method,
            SCHEDULE_METHODS.join(", ")
        );
    }
    validate_path(path)?;
    Ok(ScheduledCall {
        name: name.to_string(),
        schedule: schedule.to_string(),
        method,
        path: path.to_string(),
    })
}

/// The list with `call` added, or put in place of the one with its name.
/// Returns whether one was replaced.
fn with_call(current: &[ScheduledCall], call: ScheduledCall) -> Result<(Vec<ScheduledCall>, bool)> {
    let mut next = current.to_vec();
    if let Some(existing) = next.iter_mut().find(|c| c.name == call.name) {
        *existing = call;
        return Ok((next, true));
    }
    if next.len() >= MAX_SCHEDULED_CALLS {
        bail!(
            "A service can have at most {} scheduled calls, and this one has {}. Remove one \
             first (--remove <name>)",
            MAX_SCHEDULED_CALLS,
            next.len()
        );
    }
    next.push(call);
    Ok((next, false))
}

/// The list without the call named `name`.
fn without_call(current: &[ScheduledCall], name: &str) -> Result<Vec<ScheduledCall>> {
    if !current.iter().any(|c| c.name == name) {
        let names: Vec<&str> = current.iter().map(|c| c.name.as_str()).collect();
        if names.is_empty() {
            bail!("No scheduled call named '{}': this service has none", name);
        }
        bail!(
            "No scheduled call named '{}'. This service has: {}",
            name,
            names.join(", ")
        );
    }
    Ok(current.iter().filter(|c| c.name != name).cloned().collect())
}

/// A component as `GET /applications/{id}/services` lists it.
#[derive(Debug, Clone, Deserialize)]
struct ComponentSummary {
    id: String,
    name: String,
    #[serde(default, rename = "type")]
    r#type: Option<String>,
}

impl ComponentSummary {
    fn is_worker(&self) -> bool {
        self.r#type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("worker"))
    }
}

#[derive(Debug, Deserialize)]
struct ComponentsResponse {
    services: Vec<ComponentSummary>,
}

/// What this command reads from `GET /services/{id}/infrastructure`. The
/// response carries much more; the size and count are read only to be sent
/// back unchanged, because the update requires them.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServiceSettings {
    #[serde(default)]
    instance_type: Option<String>,
    #[serde(default)]
    desired_count: Option<u64>,
    /// Absent when nothing is saved for the service here yet, and on a
    /// platform from before scheduled calls.
    #[serde(default)]
    schedules: Option<Vec<ScheduledCall>>,
}

/// One service in the listing, and one element of `--json` output.
#[derive(Debug, Serialize)]
struct ServiceSchedules {
    service: String,
    id: String,
    /// `None`: the platform holds no list for this service here.
    schedules: Option<Vec<ScheduledCall>>,
}

/// Body of `PATCH /services/{id}/infrastructure` that changes the schedules
/// and nothing else. `instanceType` and `desiredCount` are required by the
/// route and overwrite what is stored, so they go back as read. Every other
/// setting is left out, which keeps it. `deploy` is true unless said
/// otherwise, so it is said.
fn update_body(
    environment: &str,
    region: &str,
    instance_type: &str,
    desired_count: u64,
    schedules: &[ScheduledCall],
) -> serde_json::Value {
    serde_json::json!({
        "environment": environment,
        "region": region,
        "instanceType": instance_type,
        "desiredCount": desired_count,
        "schedules": schedules,
        "deploy": false,
    })
}

fn query(environment: &str, region: &str) -> String {
    format!(
        "environment={}&region={}",
        urlencoding::encode(environment),
        urlencoding::encode(region)
    )
}

fn list_services(
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

/// `None` when the platform has nothing for the service in this environment
/// and region.
fn fetch_settings(
    api_url: &str,
    service: &ComponentSummary,
    environment: &str,
    region: &str,
) -> Result<Option<ServiceSettings>> {
    let url = format!(
        "{}/services/{}/infrastructure?{}",
        api_url,
        urlencoding::encode(&service.id),
        query(environment, region)
    );
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    let status = response.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        bail!(
            "Failed to read the settings of {} ({}): {}",
            service.name,
            status,
            response.text().unwrap_or_default()
        );
    }
    let settings: ServiceSettings = response
        .json()
        .with_context(|| format!("Failed to parse the settings of {}", service.name))?;
    Ok(Some(settings))
}

fn find_service<'a>(
    components: &'a [ComponentSummary],
    name: &str,
) -> Result<&'a ComponentSummary> {
    let Some(found) = components.iter().find(|c| c.name == name) else {
        let names: Vec<&str> = components
            .iter()
            .filter(|c| !c.is_worker())
            .map(|c| c.name.as_str())
            .collect();
        if names.is_empty() {
            bail!(
                "No service named '{}': this application has no services here",
                name
            );
        }
        bail!(
            "No service named '{}'. Known services: {}",
            name,
            names.join(", ")
        );
    };
    if found.is_worker() {
        bail!(
            "'{}' is a worker. Scheduled calls are for services: a worker has no route to call",
            name
        );
    }
    Ok(found)
}

/// The calls as aligned rows: name, schedule, method, path.
fn call_rows(calls: &[ScheduledCall]) -> Vec<String> {
    let name_width = calls.iter().map(|c| c.name.len()).max().unwrap_or(0);
    let schedule_width = calls.iter().map(|c| c.schedule.len()).max().unwrap_or(0);
    let method_width = calls.iter().map(|c| c.method.len()).max().unwrap_or(0);
    calls
        .iter()
        .map(|c| {
            format!(
                "{:<name_width$}  {:<schedule_width$}  {:<method_width$}  {}",
                c.name, c.schedule, c.method, c.path
            )
        })
        .collect()
}

const NOT_STORED: &str =
    "not available: nothing saved here yet, or the platform does not offer scheduled calls";

/// The listing below its heading, one string per line.
fn listing_lines(rows: &[ServiceSchedules]) -> Vec<String> {
    let mut lines = Vec::new();
    if rows.is_empty() {
        lines.push("  No services found.".to_string());
        return lines;
    }
    for row in rows {
        lines.push(format!("    {}", row.service));
        match &row.schedules {
            None => lines.push(format!("      {}", NOT_STORED)),
            Some(calls) if calls.is_empty() => lines.push("      none".to_string()),
            Some(calls) => {
                lines.extend(call_rows(calls).into_iter().map(|r| format!("      {}", r)));
            }
        }
    }
    lines
}

fn print_heading(stdout: &mut StandardStream, color: Color, text: &str) -> Result<()> {
    stdout.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
    write!(stdout, "  {}", text)?;
    stdout.reset()?;
    writeln!(stdout)?;
    Ok(())
}

#[derive(Debug)]
pub(crate) struct ScheduleCommand;

impl ScheduleCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ScheduleCommand {
    fn command(&self) -> Command {
        command(
            "schedule",
            "Show or set the calls the platform makes to a service at set times",
        )
        .long_about(
            "Show or set the calls the platform makes to a service at set times.\n\n\
             A timer inside a process stops while a scale-to-zero service is asleep. A \
             scheduled call replaces it: at each time the platform calls a path on the \
             service, waking it first if it sleeps. An always-on service is called the same \
             way.\n\n\
             Example: a notifications service sweeps for due reminders every 15 minutes. It \
             exposes the sweep as POST /internal/sweep, and this stores the schedule:\n\n\
             \x20 forklaunch app schedule -e production -r us-west-2 --service notifications \\\n\
             \x20     --set sweep --schedule \"rate(15 minutes)\" --call /internal/sweep\n\n\
             With no --set or --remove, the schedules are listed. A change takes effect on the \
             next deployment. A service can have at most 20, and the finest schedule is one \
             minute.\n\n\
             Each call is signed with the service's FORKLAUNCH_SCHEDULE_SECRET, which the \
             platform sets. The route is reachable from the internet, so the service must \
             check the signature: see \"Scheduled calls\" in the deploy-mode skill \
             (forklaunch context).",
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .required(true)
                .help("Environment to show or change"),
        )
        .arg(
            Arg::new("region")
                .short('r')
                .long("region")
                .required(true)
                .help("Region to show or change"),
        )
        .arg(Arg::new("service").short('s').long("service").help(
            "Service manifest name (e.g. notifications). Without it, every service is listed",
        ))
        .arg(
            Arg::new("set")
                .long("set")
                .value_name("NAME")
                .requires_all(["service", "schedule", "call"])
                .conflicts_with("remove")
                .help(
                    "Add a scheduled call with this name, or replace the one that has it. \
                     1 to 32 lower-case letters, digits or dashes (e.g. --set sweep)",
                ),
        )
        .arg(Arg::new("schedule").long("schedule").requires("set").help(
            "When to call, in UTC: rate(N minutes|hours|days) or a six-field cron \
                     (minutes hours day-of-month month day-of-week year). \
                     E.g. \"rate(15 minutes)\", or \"cron(0 14 * * ? *)\" for every day at 14:00",
        ))
        .arg(
            Arg::new("call")
                .long("call")
                .value_name("PATH")
                .requires("set")
                .help("Path on the service to call, starting with \"/\" (e.g. /internal/sweep)"),
        )
        .arg(
            Arg::new("method")
                .long("method")
                .requires("set")
                .ignore_case(true)
                .value_parser(SCHEDULE_METHODS.to_vec())
                .default_value("POST")
                .help("HTTP method of the call"),
        )
        .arg(
            Arg::new("remove")
                .long("remove")
                .value_name("NAME")
                .requires("service")
                .help("Remove the scheduled call with this name"),
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
        let environment = matches
            .get_one::<String>("environment")
            .context("--environment is required")?;
        let region = matches
            .get_one::<String>("region")
            .context("--region is required")?;
        let service_name = matches.get_one::<String>("service");
        let remove = matches.get_one::<String>("remove");
        let json_output = matches.get_flag("json");

        // Checked before anything is sent, so a typo is reported at once.
        let set = match matches.get_one::<String>("set") {
            Some(name) => Some(checked_call(
                name,
                matches
                    .get_one::<String>("schedule")
                    .context("--schedule is required with --set")?,
                matches
                    .get_one::<String>("method")
                    .map(String::as_str)
                    .unwrap_or("POST"),
                matches
                    .get_one::<String>("call")
                    .context("--call is required with --set")?,
            )?),
            None => None,
        };

        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;
        let api_url = get_platform_management_api_url();

        let components = list_services(&api_url, &application_id, environment, region)?;

        if set.is_none() && remove.is_none() {
            let services: Vec<&ComponentSummary> = match service_name {
                Some(name) => vec![find_service(&components, name)?],
                None => components.iter().filter(|c| !c.is_worker()).collect(),
            };
            let rows = services
                .into_iter()
                .map(|service| {
                    Ok(ServiceSchedules {
                        service: service.name.clone(),
                        id: service.id.clone(),
                        schedules: fetch_settings(&api_url, service, environment, region)?
                            .and_then(|settings| settings.schedules),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            if json_output {
                println!("{}", serde_json::to_string_pretty(&rows)?);
                return Ok(());
            }
            let mut stdout = StandardStream::stdout(ColorChoice::Always);
            print_heading(
                &mut stdout,
                Color::Cyan,
                &format!("Scheduled calls: {} ({})", environment, region),
            )?;
            for line in listing_lines(&rows) {
                writeln!(stdout, "{}", line)?;
            }
            return Ok(());
        }

        let service = find_service(
            &components,
            service_name.context("--service is required to change a schedule")?,
        )?;
        let settings = fetch_settings(&api_url, service, environment, region)?;
        // Without a stored list there is nothing safe to write back to: the
        // update would create the service's settings with a size this command
        // made up, or reach a platform that drops the schedules unread.
        let (Some(current), Some(instance_type), Some(desired_count)) = (
            settings.as_ref().and_then(|s| s.schedules.clone()),
            settings.as_ref().and_then(|s| s.instance_type.clone()),
            settings.as_ref().and_then(|s| s.desired_count),
        ) else {
            bail!(
                "{} has nothing saved in {} ({}) yet, or the platform does not offer scheduled \
                 calls. Deploy the service there first (forklaunch deploy create -e {} --region \
                 {}), then set its schedules",
                service.name,
                environment,
                region,
                environment,
                region
            );
        };

        let (next, headline) = if let Some(call) = set {
            let name = call.name.clone();
            let (next, replaced) = with_call(&current, call)?;
            (
                next,
                format!(
                    "Scheduled call '{}' {} on {}",
                    name,
                    if replaced { "replaced" } else { "added" },
                    service.name
                ),
            )
        } else {
            let name = remove.context("--set or --remove is required")?;
            (
                without_call(&current, name)?,
                format!("Scheduled call '{}' removed from {}", name, service.name),
            )
        };

        let response = http_client::patch(
            &format!(
                "{}/services/{}/infrastructure",
                api_url,
                urlencoding::encode(&service.id)
            ),
            update_body(environment, region, &instance_type, desired_count, &next),
        )
        .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
        if !response.status().is_success() {
            bail!(
                "Failed to set the schedules of {} ({}): {}",
                service.name,
                response.status(),
                response.text().unwrap_or_default()
            );
        }

        let rows = [ServiceSchedules {
            service: service.name.clone(),
            id: service.id.clone(),
            schedules: Some(next),
        }];
        if json_output {
            println!("{}", serde_json::to_string_pretty(&rows)?);
            return Ok(());
        }
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        print_heading(&mut stdout, Color::Green, &headline)?;
        for line in listing_lines(&rows) {
            writeln!(stdout, "{}", line)?;
        }
        writeln!(stdout)?;
        writeln!(
            stdout,
            "  Deploy to apply: forklaunch deploy create -e {} --region {}",
            environment, region
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule_cmd() -> Command {
        ScheduleCommand::new().command().version("0.0.0-test")
    }

    fn call(name: &str) -> ScheduledCall {
        checked_call(name, "rate(15 minutes)", "POST", "/internal/sweep").unwrap()
    }

    const BASE: [&str; 5] = ["schedule", "-e", "production", "-r", "us-west-2"];

    fn parse(extra: &[&str]) -> Result<ArgMatches, clap::Error> {
        schedule_cmd().try_get_matches_from(BASE.iter().chain(extra.iter()))
    }

    #[test]
    fn command_definition_is_valid() {
        schedule_cmd().debug_assert();
    }

    #[test]
    fn listing_needs_only_environment_and_region() {
        assert!(schedule_cmd().try_get_matches_from(["schedule"]).is_err());
        let matches = parse(&[]).unwrap();
        assert!(matches.get_one::<String>("service").is_none());
        assert!(parse(&["-s", "notifications"]).is_ok());
    }

    #[test]
    fn set_needs_a_service_a_schedule_and_a_path() {
        let full = [
            "--service",
            "notifications",
            "--set",
            "sweep",
            "--schedule",
            "rate(15 minutes)",
            "--call",
            "/internal/sweep",
        ];
        let matches = parse(&full).unwrap();
        assert_eq!(matches.get_one::<String>("method").unwrap(), "POST");

        // Each of the three pairs dropped in turn.
        for skip in [0, 4, 6] {
            let partial: Vec<&str> = full
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != skip && *i != skip + 1)
                .map(|(_, arg)| *arg)
                .collect();
            assert!(parse(&partial).is_err(), "{partial:?}");
        }
    }

    #[test]
    fn method_takes_the_five_methods_in_any_case() {
        let with_method = |method: &'static str| {
            parse(&[
                "-s",
                "notifications",
                "--set",
                "sweep",
                "--schedule",
                "rate(1 hour)",
                "--call",
                "/sweep",
                "--method",
                method,
            ])
        };
        for method in SCHEDULE_METHODS {
            assert!(with_method(method).is_ok());
        }
        assert!(with_method("get").is_ok());
        assert!(with_method("HEAD").is_err());
    }

    #[test]
    fn remove_needs_a_service_and_cannot_be_combined_with_set() {
        assert!(parse(&["--remove", "sweep"]).is_err());
        assert!(parse(&["-s", "notifications", "--remove", "sweep"]).is_ok());
        assert!(
            parse(&[
                "-s",
                "notifications",
                "--remove",
                "sweep",
                "--set",
                "sweep",
                "--schedule",
                "rate(1 hour)",
                "--call",
                "/sweep",
            ])
            .is_err()
        );
        // A schedule with nothing to attach to.
        assert!(parse(&["-s", "notifications", "--schedule", "rate(1 hour)"]).is_err());
    }

    #[test]
    fn names_follow_the_platform_rule() {
        for good in ["sweep", "a", "nightly-digest-2", &"a".repeat(32)] {
            assert!(validate_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "Sweep",
            "-sweep",
            "sweep-",
            "sweep_all",
            "sweep all",
            &"a".repeat(33),
        ] {
            assert!(validate_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rates_need_a_whole_number_and_the_matching_singular_or_plural() {
        for good in [
            "rate(1 minute)",
            "rate(15 minutes)",
            "rate(1 hour)",
            "rate(6 hours)",
            "rate(1 day)",
            "rate(99999 days)",
        ] {
            assert!(validate_schedule(good).is_ok(), "{good}");
        }
        for bad in [
            "rate(0 minutes)",
            "rate(15 seconds)",
            "rate(15  minutes)",
            "rate(1.5 hours)",
            "rate(100000 days)",
            "rate(15minutes)",
            "rate(minutes)",
            "every 15 minutes",
            "15 minutes",
        ] {
            assert!(validate_schedule(bad).is_err(), "{bad}");
        }
        let err = validate_schedule("rate(1 minutes)")
            .unwrap_err()
            .to_string();
        assert!(err.contains("write \"rate(1 minute)\""), "{err}");
        let err = validate_schedule("rate(15 minute)")
            .unwrap_err()
            .to_string();
        assert!(err.contains("write \"rate(15 minutes)\""), "{err}");
    }

    #[test]
    fn cron_needs_six_fields() {
        for good in ["cron(0 14 * * ? *)", "cron(0/15 8-18 ? * MON-FRI *)"] {
            assert!(validate_schedule(good).is_ok(), "{good}");
        }
        for bad in [
            "cron(0 14 * * *)",
            "cron(0 14 * * ? * *)",
            "cron(0  14 * * ? *)",
            "cron(0 14 * * ? (*))",
            "cron()",
            "0 14 * * ? *",
        ] {
            let err = validate_schedule(bad).unwrap_err().to_string();
            assert!(err.contains("six"), "{bad}: {err}");
        }
    }

    #[test]
    fn paths_start_with_a_slash_and_hold_only_url_characters() {
        for good in ["/", "/internal/sweep", "/jobs/run?kind=digest&dry=0"] {
            assert!(validate_path(good).is_ok(), "{good}");
        }
        let too_long = format!("/{}", "a".repeat(512));
        for bad in [
            "",
            "internal/sweep",
            "/internal sweep",
            "https://example.com/sweep",
            "/sweep#now",
            &too_long,
        ] {
            assert!(validate_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_checked_call_is_stored_trimmed_with_its_method_in_upper_case() {
        let call = checked_call("sweep", "  rate(15 minutes) ", "post", "/internal/sweep").unwrap();
        assert_eq!(call.schedule, "rate(15 minutes)");
        assert_eq!(call.method, "POST");
        assert!(checked_call("sweep", "rate(15 minutes)", "HEAD", "/sweep").is_err());
    }

    #[test]
    fn setting_adds_a_new_name_and_replaces_an_existing_one_in_place() {
        let current = vec![call("sweep"), call("digest")];

        let (added, replaced) = with_call(&current, call("cleanup")).unwrap();
        assert!(!replaced);
        let names: Vec<&str> = added.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["sweep", "digest", "cleanup"]);

        let hourly = checked_call("sweep", "rate(1 hour)", "GET", "/sweep").unwrap();
        let (changed, replaced) = with_call(&current, hourly.clone()).unwrap();
        assert!(replaced);
        assert_eq!(changed, vec![hourly, call("digest")]);
    }

    #[test]
    fn the_twenty_first_call_is_refused_and_a_replacement_at_twenty_is_not() {
        let full: Vec<ScheduledCall> = (0..MAX_SCHEDULED_CALLS)
            .map(|i| call(&format!("job-{i}")))
            .collect();
        let err = with_call(&full, call("one-more")).unwrap_err().to_string();
        assert!(err.contains("at most 20"), "{err}");
        assert!(with_call(&full, call("job-3")).is_ok());
    }

    #[test]
    fn removing_keeps_the_rest_and_names_what_exists_when_the_name_is_wrong() {
        let current = vec![call("sweep"), call("digest")];
        assert_eq!(
            without_call(&current, "sweep").unwrap(),
            vec![call("digest")]
        );

        let err = without_call(&current, "swep").unwrap_err().to_string();
        assert!(err.contains("sweep, digest"), "{err}");
        let err = without_call(&[], "sweep").unwrap_err().to_string();
        assert!(err.contains("has none"), "{err}");
    }

    #[test]
    fn the_update_sends_back_size_and_count_and_asks_for_no_deployment() {
        let body = update_body("production", "us-west-2", "t3.micro", 2, &[call("sweep")]);
        assert_eq!(
            body,
            serde_json::json!({
                "environment": "production",
                "region": "us-west-2",
                "instanceType": "t3.micro",
                "desiredCount": 2,
                "schedules": [{
                    "name": "sweep",
                    "schedule": "rate(15 minutes)",
                    "method": "POST",
                    "path": "/internal/sweep"
                }],
                "deploy": false
            })
        );
        // Removing the last one sends an empty list, which removes them all.
        let body = update_body("production", "us-west-2", "t3.micro", 1, &[]);
        assert_eq!(body["schedules"], serde_json::json!([]));
        // Nothing that would change the service's mode or size.
        for untouched in ["compute", "cpu", "memory", "hostingType", "minInstances"] {
            assert!(body.get(untouched).is_none(), "{untouched}");
        }
    }

    #[test]
    fn settings_parse_with_schedules_without_a_method_and_without_schedules() {
        let stored: ServiceSettings = serde_json::from_str(
            r#"{"environment": "production", "region": "us-west-2",
                "instanceType": "t3.micro", "desiredCount": 1, "compute": "auto",
                "schedules": [
                    {"name": "sweep", "schedule": "rate(15 minutes)", "path": "/internal/sweep"}
                ]}"#,
        )
        .unwrap();
        assert_eq!(stored.schedules.unwrap(), vec![call("sweep")]);

        // Nothing saved yet, or an older platform: no list at all.
        let bare: ServiceSettings =
            serde_json::from_str(r#"{"instanceType": "t3.micro", "desiredCount": 1}"#).unwrap();
        assert!(bare.schedules.is_none());
        assert_eq!(bare.instance_type.as_deref(), Some("t3.micro"));
    }

    #[test]
    fn workers_are_refused_and_an_unknown_name_lists_the_services() {
        let components: Vec<ComponentSummary> = serde_json::from_str(
            r#"[{"id": "s1", "name": "notifications", "status": "active", "type": "service"},
                {"id": "s2", "name": "api", "status": "active"},
                {"id": "w1", "name": "mailer-worker", "status": "active", "type": "worker"}]"#,
        )
        .unwrap();
        assert_eq!(find_service(&components, "notifications").unwrap().id, "s1");

        let err = find_service(&components, "mailer-worker")
            .unwrap_err()
            .to_string();
        assert!(err.contains("is a worker"), "{err}");

        let err = find_service(&components, "notification")
            .unwrap_err()
            .to_string();
        assert!(err.contains("notifications, api"), "{err}");
        assert!(!err.contains("mailer-worker"), "{err}");
    }

    #[test]
    fn the_listing_lines_up_each_services_calls_and_fits_100_columns() {
        let rows = [
            ServiceSchedules {
                service: "notifications".to_string(),
                id: "s1".to_string(),
                schedules: Some(vec![
                    call("sweep"),
                    checked_call("nightly-digest", "cron(0 14 * * ? *)", "GET", "/digest").unwrap(),
                ]),
            },
            ServiceSchedules {
                service: "api".to_string(),
                id: "s2".to_string(),
                schedules: Some(vec![]),
            },
            ServiceSchedules {
                service: "admin".to_string(),
                id: "s3".to_string(),
                schedules: None,
            },
        ];
        let lines = listing_lines(&rows);
        assert_eq!(
            lines,
            [
                "    notifications",
                "      sweep           rate(15 minutes)    POST  /internal/sweep",
                "      nightly-digest  cron(0 14 * * ? *)  GET   /digest",
                "    api",
                "      none",
                "    admin",
                &format!("      {}", NOT_STORED),
            ]
        );
        assert!(lines.iter().all(|line| line.len() <= 100), "{lines:?}");
        assert_eq!(listing_lines(&[]), ["  No services found."]);
    }
}

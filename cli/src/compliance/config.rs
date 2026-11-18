//! `forklaunch compliance config` — read and change an environment's
//! compliance settings: which standards it is held to, which individual
//! protections are on, and the outbound allowlist.
//!
//! A change takes effect on the environment's next deploy. `set` reads the
//! saved settings, applies only what the flags name, and writes the whole
//! thing back, because the platform replaces the settings on save.

use std::{collections::BTreeMap, fs, io::Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::Deserialize;
use serde_json::{Map, Value, json};
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

const EGRESS_ALLOWLIST: &str = "egress.whitelist";
const EGRESS_RESTRICTED: &str = "egress.restricted";

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Feature {
    slug: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Standard {
    slug: String,
    #[serde(default)]
    required_features: Vec<Feature>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct EnvironmentConfig {
    #[serde(default)]
    region: String,
    #[serde(default)]
    feature_overrides: BTreeMap<String, bool>,
    #[serde(default)]
    feature_configs: BTreeMap<String, String>,
    #[serde(default)]
    enabled_features: Vec<Feature>,
    #[serde(default)]
    enabled_standards: Vec<Standard>,
}

#[derive(Debug, Deserialize)]
struct StandardList {
    standards: Vec<Standard>,
}

/// One line of an allowlist file, as the platform stores it.
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct AllowlistEntry {
    kind: &'static str,
    value: String,
    port: Option<u16>,
}

impl AllowlistEntry {
    fn to_json(&self) -> Value {
        let mut entry = Map::new();
        entry.insert("type".into(), json!(self.kind));
        entry.insert("value".into(), json!(self.value));
        if let Some(port) = self.port {
            entry.insert("port".into(), json!(port));
        }
        Value::Object(entry)
    }
}

fn looks_like_cidr(value: &str) -> bool {
    let Some((address, prefix)) = value.rsplit_once('/') else {
        return false;
    };
    if prefix.is_empty() || !prefix.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let v4 = address.split('.').count() == 4
        && address
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    let v6 = address.contains(':') && address.chars().all(|c| c.is_ascii_hexdigit() || c == ':');
    v4 || v6
}

/// Reads the same text the dashboard's allowlist box accepts: one entry per
/// line or comma, `#` comments, an optional `dns:`/`cidr:` prefix, and an
/// optional trailing `:port` on a name or an IPv4 range. 443 is always open,
/// so it is not recorded as a port.
pub(crate) fn parse_allowlist(text: &str) -> Vec<AllowlistEntry> {
    let mut entries: Vec<AllowlistEntry> = Vec::new();
    // Comment lines go first, whole: a comma inside a comment is not a
    // separator.
    let tokens = text
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(|line| line.split(','));
    for raw in tokens {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let lower = token.to_ascii_lowercase();
        let (forced, rest) = ["dns:", "domain:", "cidr:", "ip:"]
            .iter()
            .find(|prefix| lower.starts_with(**prefix))
            .map(|prefix| (Some(*prefix), token[prefix.len()..].trim()))
            .unwrap_or((None, token));

        let mut value = rest.to_string();
        let mut port = None;
        let is_url = lower.contains("://");
        if !is_url
            && rest.matches(':').count() == 1
            && let Some((host, maybe_port)) = rest.rsplit_once(':')
            && let Ok(parsed) = maybe_port.parse::<u16>()
        {
            value = host.to_string();
            if parsed != 0 && parsed != 443 {
                port = Some(parsed);
            }
        }
        if is_url {
            let without_scheme = value.split("://").nth(1).unwrap_or("").to_string();
            value = without_scheme
                .split(['/', ':', '?'])
                .next()
                .unwrap_or("")
                .to_string();
        }
        let value = value.trim().to_ascii_lowercase();
        if value.is_empty() {
            continue;
        }
        let kind = if matches!(forced, Some("cidr:") | Some("ip:")) || looks_like_cidr(&value) {
            "cidr"
        } else {
            "dns"
        };
        let entry = AllowlistEntry { kind, value, port };
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    entries
}

/// The mode and entry count of a saved allowlist payload. A bare list has no
/// mode: it was saved before modes existed and is enforced.
fn describe_allowlist(payload: Option<&String>) -> (Option<String>, usize) {
    let Some(parsed) = payload.and_then(|raw| serde_json::from_str::<Value>(raw).ok()) else {
        return (None, 0);
    };
    match parsed {
        Value::Array(list) => (None, list.len()),
        Value::Object(map) => (
            map.get("mode").and_then(Value::as_str).map(str::to_string),
            map.get("whitelist")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
        ),
        _ => (None, 0),
    }
}

fn saved_entries(payload: Option<&String>) -> Vec<Value> {
    match payload.and_then(|raw| serde_json::from_str::<Value>(raw).ok()) {
        Some(Value::Array(list)) => list,
        Some(Value::Object(map)) => map
            .get("whitelist")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn fail(what: &str, response: reqwest::blocking::Response) -> anyhow::Error {
    let status = response.status().as_u16();
    let body = response.text().unwrap_or_default();
    match status {
        401 => anyhow::anyhow!("{what}: not signed in. Run `forklaunch login`."),
        403 => anyhow::anyhow!("{what}: your role does not allow this ({body})"),
        _ => anyhow::anyhow!("{what} ({status}): {body}"),
    }
}

fn config_url(application_id: &str, environment: &str) -> String {
    format!(
        "{}/compliance/applications/{}/environments/{}",
        get_platform_management_api_url(),
        urlencoding::encode(application_id),
        urlencoding::encode(environment)
    )
}

fn fetch_config(
    application_id: &str,
    environment: &str,
    region: Option<&String>,
) -> Result<EnvironmentConfig> {
    let mut url = config_url(application_id, environment);
    if let Some(region) = region {
        url.push_str(&format!("?region={}", urlencoding::encode(region)));
    }
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail("Could not read the compliance settings", response));
    }
    response
        .json()
        .context("The platform returned compliance settings this CLI could not read")
}

fn fetch_standards() -> Result<Vec<Standard>> {
    let url = format!("{}/compliance/standards", get_platform_management_api_url());
    let response = http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail("Could not list compliance standards", response));
    }
    Ok(response
        .json::<StandardList>()
        .context("The platform returned a standards list this CLI could not read")?
        .standards)
}

fn print_config(config: &EnvironmentConfig) -> Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    stdout.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(stdout, "Region: {}", config.region)?;
    stdout.reset()?;

    let standards: Vec<&str> = config
        .enabled_standards
        .iter()
        .map(|s| s.slug.as_str())
        .collect();
    writeln!(
        stdout,
        "Standards: {}",
        if standards.is_empty() {
            "none".to_string()
        } else {
            standards.join(", ")
        }
    )?;

    let features: Vec<&str> = config
        .enabled_features
        .iter()
        .map(|f| f.slug.as_str())
        .collect();
    writeln!(
        stdout,
        "Protections on: {}",
        if features.is_empty() {
            "none".to_string()
        } else {
            features.join(", ")
        }
    )?;

    let allowlist_on = config
        .feature_overrides
        .get(EGRESS_ALLOWLIST)
        .copied()
        .unwrap_or(false)
        || features.contains(&EGRESS_ALLOWLIST);
    let (mode, count) = describe_allowlist(config.feature_configs.get(EGRESS_ALLOWLIST));
    if allowlist_on {
        let mode = match mode.as_deref() {
            Some("watch") => "watching (nothing is blocked; lookups off the list are logged)",
            _ => "enforcing (names off the list do not resolve)",
        };
        writeln!(stdout, "Outbound allowlist: on, {mode}, {count} entries")?;
    } else {
        writeln!(stdout, "Outbound allowlist: off")?;
    }
    Ok(())
}

fn common_args(cmd: Command) -> Command {
    cmd.arg(
        Arg::new("environment")
            .long("environment")
            .short('e')
            .required(true)
            .help("Environment name"),
    )
    .arg(
        Arg::new("region")
            .long("region")
            .help("Region of the environment (needed when it has more than one)"),
    )
    .arg(
        Arg::new("base_path")
            .short('p')
            .long("path")
            .help("Path to application root (optional)"),
    )
}

fn multi(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .action(ArgAction::Append)
        .value_name("SLUG")
        .help(help)
}

#[derive(Debug)]
pub(crate) struct ConfigCommand;

impl ConfigCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for ConfigCommand {
    fn command(&self) -> Command {
        command(
            "config",
            "Read and change an environment's compliance settings (standards, protections, outbound allowlist)",
        )
        .subcommand_required(true)
        .subcommand(common_args(command(
            "show",
            "Show an environment's standards, protections and outbound allowlist",
        ))
        .arg(
            Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Print the saved allowlist entries as JSON after the summary"),
        ))
        .subcommand(
            common_args(command(
                "set",
                "Change an environment's compliance settings. Takes effect on its next deploy",
            ))
            .arg(multi("standard", "Hold the environment to this standard (soc2, hipaa, ...)"))
            .arg(multi("remove-standard", "Stop holding the environment to this standard"))
            .arg(multi("enable", "Switch a protection on by its slug"))
            .arg(multi("disable", "Switch a protection off by its slug"))
            .arg(
                Arg::new("allowlist-file")
                    .long("allowlist-file")
                    .value_name("FILE")
                    .help("Switch the outbound allowlist on with the entries in this file (one per line; dns:host[:port] or cidr:range[:port])"),
            )
            .arg(
                Arg::new("allowlist-mode")
                    .long("allowlist-mode")
                    .value_parser(["watch", "enforce"])
                    .help("watch: block nothing and log lookups off the list. enforce: names off the list do not resolve. A newly switched-on allowlist starts in watch"),
            )
            .arg(
                Arg::new("allow-restricted-egress")
                    .long("allow-restricted-egress")
                    .action(ArgAction::SetTrue)
                    .help("Permit a standard that restricts outbound traffic while no outbound allowlist is on"),
            )
            .arg(
                Arg::new("dry-run")
                    .long("dry-run")
                    .action(ArgAction::SetTrue)
                    .help("Print what would be saved without saving it"),
            ),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("show", m)) => show(m),
            Some(("set", m)) => set(m),
            _ => unreachable!(),
        }
    }
}

fn target(matches: &ArgMatches) -> Result<(String, String)> {
    let _token = require_auth()?;
    let (_app_root, manifest) = require_manifest(matches)?;
    let application_id = require_integration(&manifest)?;
    let environment = matches
        .get_one::<String>("environment")
        .context("environment is required")?
        .clone();
    Ok((application_id, environment))
}

fn show(matches: &ArgMatches) -> Result<()> {
    let (application_id, environment) = target(matches)?;
    let config = fetch_config(
        &application_id,
        &environment,
        matches.get_one::<String>("region"),
    )?;
    print_config(&config)?;
    if matches.get_flag("json") {
        println!(
            "{}",
            serde_json::to_string_pretty(&saved_entries(
                config.feature_configs.get(EGRESS_ALLOWLIST)
            ))?
        );
    }
    Ok(())
}

fn values(matches: &ArgMatches, name: &str) -> Vec<String> {
    matches
        .get_many::<String>(name)
        .map(|v| v.cloned().collect())
        .unwrap_or_default()
}

/// What to save, worked out from the saved settings and the flags. Kept apart
/// from the network calls so the rules can be tested.
#[derive(Debug, PartialEq)]
pub(crate) struct Plan {
    standards: Vec<String>,
    feature_overrides: BTreeMap<String, bool>,
    feature_configs: BTreeMap<String, String>,
}

pub(crate) struct Changes {
    pub add_standards: Vec<String>,
    pub remove_standards: Vec<String>,
    pub enable: Vec<String>,
    pub disable: Vec<String>,
    pub allowlist: Option<Vec<AllowlistEntry>>,
    pub mode: Option<String>,
}

fn plan(
    current_standards: &[String],
    current_overrides: &BTreeMap<String, bool>,
    current_configs: &BTreeMap<String, String>,
    changes: &Changes,
) -> Result<Plan> {
    let mut standards: Vec<String> = current_standards.to_vec();
    for slug in &changes.add_standards {
        if !standards.contains(slug) {
            standards.push(slug.clone());
        }
    }
    standards.retain(|slug| !changes.remove_standards.contains(slug));

    let mut overrides = current_overrides.clone();
    for slug in &changes.enable {
        overrides.insert(slug.clone(), true);
    }
    for slug in &changes.disable {
        overrides.insert(slug.clone(), false);
    }

    let mut configs = current_configs.clone();
    if changes.allowlist.is_some() || changes.mode.is_some() {
        let saved = configs.get(EGRESS_ALLOWLIST);
        let (saved_mode, _) = describe_allowlist(saved);
        let was_on = current_overrides
            .get(EGRESS_ALLOWLIST)
            .copied()
            .unwrap_or(false);
        let entries: Vec<Value> = match &changes.allowlist {
            Some(list) => list.iter().map(AllowlistEntry::to_json).collect(),
            None => saved_entries(saved),
        };
        // An allowlist that is already on keeps the behavior it has unless a
        // mode is named; one being switched on starts by watching.
        let mode = match (&changes.mode, was_on) {
            (Some(mode), _) => Some(mode.clone()),
            (None, true) => saved_mode,
            (None, false) => Some("watch".to_string()),
        };
        if mode.as_deref() == Some("enforce") && entries.is_empty() {
            bail!(
                "Refusing to enforce an empty allowlist: every outside name would stop resolving. Give --allowlist-file."
            );
        }
        let payload = match mode {
            Some(mode) => json!({ "mode": mode, "whitelist": entries }),
            None => Value::Array(entries),
        };
        configs.insert(EGRESS_ALLOWLIST.to_string(), payload.to_string());
        if !changes.disable.iter().any(|slug| slug == EGRESS_ALLOWLIST) {
            overrides.insert(EGRESS_ALLOWLIST.to_string(), true);
        }
    }

    Ok(Plan {
        standards,
        feature_overrides: overrides,
        feature_configs: configs,
    })
}

fn set(matches: &ArgMatches) -> Result<()> {
    let (application_id, environment) = target(matches)?;
    let region = matches.get_one::<String>("region");

    let allowlist = match matches.get_one::<String>("allowlist-file") {
        Some(path) => {
            let text = fs::read_to_string(path)
                .with_context(|| format!("Could not read the allowlist file {path}"))?;
            let entries = parse_allowlist(&text);
            if entries.is_empty() {
                bail!("{path} has no allowlist entries");
            }
            Some(entries)
        }
        None => None,
    };
    let changes = Changes {
        add_standards: values(matches, "standard"),
        remove_standards: values(matches, "remove-standard"),
        enable: values(matches, "enable"),
        disable: values(matches, "disable"),
        allowlist,
        mode: matches.get_one::<String>("allowlist-mode").cloned(),
    };
    if changes.add_standards.is_empty()
        && changes.remove_standards.is_empty()
        && changes.enable.is_empty()
        && changes.disable.is_empty()
        && changes.allowlist.is_none()
        && changes.mode.is_none()
    {
        bail!("Nothing to change. See `forklaunch compliance config set --help`.");
    }

    let current = fetch_config(&application_id, &environment, region)?;
    let current_standards: Vec<String> = current
        .enabled_standards
        .iter()
        .map(|s| s.slug.clone())
        .collect();
    let next = plan(
        &current_standards,
        &current.feature_overrides,
        &current.feature_configs,
        &changes,
    )?;

    let known = fetch_standards()?;
    for slug in &changes.add_standards {
        if !known.iter().any(|s| &s.slug == slug) {
            let names: Vec<&str> = known.iter().map(|s| s.slug.as_str()).collect();
            bail!("Unknown standard '{slug}'. Known: {}", names.join(", "));
        }
    }
    // A standard that restricts outbound traffic, saved with no allowlist on,
    // cuts the environment off from everything outside AWS on its next deploy.
    let allowlist_on = next
        .feature_overrides
        .get(EGRESS_ALLOWLIST)
        .copied()
        .unwrap_or(false);
    let restricting: Vec<&str> = known
        .iter()
        .filter(|s| next.standards.contains(&s.slug))
        .filter(|s| !current_standards.contains(&s.slug))
        .filter(|s| {
            s.required_features
                .iter()
                .any(|f| f.slug == EGRESS_RESTRICTED)
        })
        .map(|s| s.slug.as_str())
        .collect();
    if !restricting.is_empty() && !allowlist_on && !matches.get_flag("allow-restricted-egress") {
        bail!(
            "{} restricts outbound traffic, and this environment has no outbound allowlist on. Its next deploy would cut it off from everything outside AWS. Switch the allowlist on in the same command (--allowlist-file FILE), or pass --allow-restricted-egress if that is what you want.",
            restricting.join(", ")
        );
    }

    let mut body = Map::new();
    body.insert("standards".into(), json!(next.standards));
    body.insert("featureOverrides".into(), json!(next.feature_overrides));
    body.insert("featureConfigs".into(), json!(next.feature_configs));
    body.insert("region".into(), json!(current.region));

    let (mode, count) = describe_allowlist(next.feature_configs.get(EGRESS_ALLOWLIST));
    let mut stdout = StandardStream::stdout(ColorChoice::Auto);
    writeln!(
        stdout,
        "Standards: {}",
        if next.standards.is_empty() {
            "none".to_string()
        } else {
            next.standards.join(", ")
        }
    )?;
    if allowlist_on {
        writeln!(
            stdout,
            "Outbound allowlist: on, {}, {count} entries",
            if mode.as_deref() == Some("watch") {
                "watching"
            } else {
                "enforcing"
            }
        )?;
    }

    if matches.get_flag("dry-run") {
        writeln!(stdout, "Dry run: nothing saved.")?;
        return Ok(());
    }

    let response = http_client::put(
        &config_url(&application_id, &environment),
        Value::Object(body),
    )
    .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
    if !response.status().is_success() {
        return Err(fail("Could not save the compliance settings", response));
    }

    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
    writeln!(
        stdout,
        "Saved for {environment} ({}). It takes effect on the next deploy.",
        current.region
    )?;
    stdout.reset()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_changes() -> Changes {
        Changes {
            add_standards: vec![],
            remove_standards: vec![],
            enable: vec![],
            disable: vec![],
            allowlist: None,
            mode: None,
        }
    }

    #[test]
    fn command_definition_is_valid() {
        ConfigCommand::new()
            .command()
            .version("0.0.0-test")
            .debug_assert();
    }

    #[test]
    fn allowlist_text_matches_the_dashboard_box() {
        let entries = parse_allowlist(
            "# comment, with a comma\ndns:Example.com\ndns:metrics.example.com:9090\napi.example.com:443\n10.0.0.0/16\ncidr:10.1.0.0/16:5432, https://docs.example.com/path\ndns:example.com\n",
        );
        let shown: Vec<String> = entries
            .iter()
            .map(|e| format!("{}:{}:{}", e.kind, e.value, e.port.unwrap_or(0)))
            .collect();
        assert_eq!(
            shown,
            vec![
                "dns:example.com:0",
                "dns:metrics.example.com:9090",
                "dns:api.example.com:0",
                "cidr:10.0.0.0/16:0",
                "cidr:10.1.0.0/16:5432",
                "dns:docs.example.com:0",
            ]
        );
    }

    #[test]
    fn wildcards_and_ipv6_ranges_survive() {
        let entries = parse_allowlist("dns:*.example.com\n2001:db8::/32");
        assert_eq!(entries[0].value, "*.example.com");
        assert_eq!(entries[1].kind, "cidr");
        assert_eq!(entries[1].value, "2001:db8::/32");
        assert_eq!(entries[1].port, None);
    }

    #[test]
    fn a_newly_switched_on_allowlist_watches() {
        let mut changes = no_changes();
        changes.allowlist = Some(parse_allowlist("dns:example.com"));
        let next = plan(&[], &BTreeMap::new(), &BTreeMap::new(), &changes).unwrap();
        assert_eq!(next.feature_overrides.get(EGRESS_ALLOWLIST), Some(&true));
        let (mode, count) = describe_allowlist(next.feature_configs.get(EGRESS_ALLOWLIST));
        assert_eq!(mode.as_deref(), Some("watch"));
        assert_eq!(count, 1);
    }

    #[test]
    fn an_allowlist_saved_before_modes_stays_a_bare_list() {
        let overrides = BTreeMap::from([(EGRESS_ALLOWLIST.to_string(), true)]);
        let configs = BTreeMap::from([(
            EGRESS_ALLOWLIST.to_string(),
            r#"[{"type":"dns","value":"old.example.com"}]"#.to_string(),
        )]);
        let mut changes = no_changes();
        changes.allowlist = Some(parse_allowlist("dns:new.example.com"));
        let next = plan(&[], &overrides, &configs, &changes).unwrap();
        let saved: Value =
            serde_json::from_str(next.feature_configs.get(EGRESS_ALLOWLIST).unwrap()).unwrap();
        assert!(saved.is_array());
        assert_eq!(saved[0]["value"], "new.example.com");
    }

    #[test]
    fn changing_only_the_mode_keeps_the_entries() {
        let overrides = BTreeMap::from([(EGRESS_ALLOWLIST.to_string(), true)]);
        let configs = BTreeMap::from([(
            EGRESS_ALLOWLIST.to_string(),
            r#"{"mode":"watch","whitelist":[{"type":"dns","value":"a.example.com","port":9090}]}"#
                .to_string(),
        )]);
        let mut changes = no_changes();
        changes.mode = Some("enforce".to_string());
        let next = plan(&[], &overrides, &configs, &changes).unwrap();
        let saved: Value =
            serde_json::from_str(next.feature_configs.get(EGRESS_ALLOWLIST).unwrap()).unwrap();
        assert_eq!(saved["mode"], "enforce");
        assert_eq!(saved["whitelist"][0]["port"], 9090);
    }

    #[test]
    fn enforcing_an_empty_allowlist_is_refused() {
        let mut changes = no_changes();
        changes.mode = Some("enforce".to_string());
        assert!(plan(&[], &BTreeMap::new(), &BTreeMap::new(), &changes).is_err());
    }

    #[test]
    fn standards_and_protections_are_added_to_what_is_saved() {
        let overrides = BTreeMap::from([("security.waf".to_string(), true)]);
        let configs = BTreeMap::from([("audit.log-retention".to_string(), "365".to_string())]);
        let mut changes = no_changes();
        changes.add_standards = vec!["soc2".to_string()];
        changes.remove_standards = vec!["gdpr".to_string()];
        changes.disable = vec!["security.waf".to_string()];
        let next = plan(
            &["hipaa".to_string(), "gdpr".to_string()],
            &overrides,
            &configs,
            &changes,
        )
        .unwrap();
        assert_eq!(next.standards, vec!["hipaa", "soc2"]);
        assert_eq!(next.feature_overrides.get("security.waf"), Some(&false));
        assert_eq!(
            next.feature_configs
                .get("audit.log-retention")
                .map(String::as_str),
            Some("365")
        );
    }
}

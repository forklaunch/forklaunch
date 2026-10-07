use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        http_client::{delete, get, patch, post},
    },
};

#[derive(Debug)]
pub(super) struct RedisCommand {
    keys: KeysCommand,
    get: GetCommand,
    set: SetCommand,
    delete: DeleteCommand,
    ttl: TtlCommand,
    command: RunCommand,
}

impl RedisCommand {
    pub(super) fn new() -> Self {
        Self {
            keys: KeysCommand::new(),
            get: GetCommand::new(),
            set: SetCommand::new(),
            delete: DeleteCommand::new(),
            ttl: TtlCommand::new(),
            command: RunCommand::new(),
        }
    }
}

impl CliCommand for RedisCommand {
    fn command(&self) -> Command {
        command("redis", "Explore a provisioned Redis/ElastiCache resource")
            .subcommand_required(true)
            .subcommand(self.keys.command())
            .subcommand(self.get.command())
            .subcommand(self.set.command())
            .subcommand(self.delete.command())
            .subcommand(self.ttl.command())
            .subcommand(self.command.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("keys", m)) => self.keys.handler(m),
            Some(("get", m)) => self.get.handler(m),
            Some(("set", m)) => self.set.handler(m),
            Some(("delete", m)) => self.delete.handler(m),
            Some(("ttl", m)) => self.ttl.handler(m),
            Some(("command", m)) => self.command.handler(m),
            _ => unreachable!(),
        }
    }
}

fn resource_arg() -> Arg {
    Arg::new("resource")
        .long("resource")
        .required(true)
        .help("The provisioned Redis resource id")
}

fn explorer_url(resource_id: &str, path: &str) -> String {
    format!(
        "{}/resources/{}/explorer{}",
        get_observability_api_url(),
        urlencoding::encode(resource_id),
        path
    )
}

fn print_pretty(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn check(response: reqwest::blocking::Response) -> Result<serde_json::Value> {
    if !response.status().is_success() {
        bail!(
            "Request failed ({}): {}",
            response.status(),
            response.text().unwrap_or_default()
        );
    }
    response.json().with_context(|| "Failed to parse response")
}

#[derive(Debug)]
struct KeysCommand;
impl KeysCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for KeysCommand {
    fn command(&self) -> Command {
        command("keys", "List keys, optionally by glob pattern")
            .arg(resource_arg())
            .arg(Arg::new("pattern").long("pattern").help("Glob pattern (e.g. session:*)"))
            .arg(Arg::new("cursor").long("cursor").help("Pagination cursor"))
            .arg(Arg::new("count").long("count").help("Approximate scan count"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let mut url = explorer_url(resource, "/keys");
        let mut params = Vec::new();
        if let Some(v) = matches.get_one::<String>("pattern") {
            params.push(format!("pattern={}", urlencoding::encode(v)));
        }
        if let Some(v) = matches.get_one::<String>("cursor") {
            params.push(format!("cursor={}", urlencoding::encode(v)));
        }
        if let Some(v) = matches.get_one::<String>("count") {
            params.push(format!("count={}", urlencoding::encode(v)));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        print_pretty(&check(
            get(&url).with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct GetCommand;
impl GetCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for GetCommand {
    fn command(&self) -> Command {
        command("get", "Get a key's value, type, and TTL")
            .arg(resource_arg())
            .arg(Arg::new("key").required(true).help("The Redis key"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let key = matches.get_one::<String>("key").context("key is required")?;
        let url = explorer_url(resource, &format!("/keys/{}", urlencoding::encode(key)));
        print_pretty(&check(
            get(&url).with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct SetCommand;
impl SetCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for SetCommand {
    fn command(&self) -> Command {
        command("set", "Set a key's value")
            .arg(resource_arg())
            .arg(Arg::new("key").required(true).help("The Redis key"))
            .arg(Arg::new("value").required(true).help("The value to set"))
            .arg(
                Arg::new("type")
                    .long("type")
                    .default_value("string")
                    .help("Redis type (string, list, set, hash, ...)"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let key = matches.get_one::<String>("key").context("key is required")?;
        let value = matches.get_one::<String>("value").context("value is required")?;
        let r#type = matches.get_one::<String>("type").map(String::as_str).unwrap_or("string");
        let body = serde_json::json!({ "key": key, "type": r#type, "value": value });
        let url = explorer_url(resource, "/keys");
        print_pretty(&check(
            post(&url, body)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct DeleteCommand;
impl DeleteCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for DeleteCommand {
    fn command(&self) -> Command {
        command("delete", "Delete a key")
            .arg(resource_arg())
            .arg(Arg::new("key").required(true).help("The Redis key"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let key = matches.get_one::<String>("key").context("key is required")?;
        let url = explorer_url(resource, &format!("/keys/{}", urlencoding::encode(key)));
        print_pretty(&check(
            delete(&url)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

/// `ttl` is optional(number) server-side, which means "omit the key", not
/// "send null" — `serde_json::json!({"ttl": seconds})` would serialize
/// `None` as an explicit `null` and fail schema validation on the clear-ttl
/// path, so the key is only inserted when a value is actually given.
fn build_ttl_body(seconds: Option<i64>) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(s) = seconds {
        body.insert("ttl".to_string(), serde_json::Value::from(s));
    }
    serde_json::Value::Object(body)
}

#[derive(Debug)]
struct TtlCommand;
impl TtlCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for TtlCommand {
    fn command(&self) -> Command {
        command("ttl", "Set (or clear) a key's expiry")
            .arg(resource_arg())
            .arg(Arg::new("key").required(true).help("The Redis key"))
            .arg(
                Arg::new("seconds")
                    .long("seconds")
                    .help("New TTL in seconds; omit to clear the expiry"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let key = matches.get_one::<String>("key").context("key is required")?;
        let seconds: Option<i64> = matches
            .get_one::<String>("seconds")
            .map(|s| s.parse())
            .transpose()
            .context("--seconds must be an integer")?;
        let body = build_ttl_body(seconds);
        let url = explorer_url(resource, &format!("/keys/{}/ttl", urlencoding::encode(key)));
        print_pretty(&check(
            patch(&url, body)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct RunCommand;
impl RunCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for RunCommand {
    fn command(&self) -> Command {
        command("command", "Run an arbitrary Redis command")
            .arg(resource_arg())
            .arg(Arg::new("redis_command").required(true).help("The Redis command, e.g. INFO"))
            .arg(
                Arg::new("arg")
                    .action(ArgAction::Append)
                    .help("Command arguments, repeatable"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let redis_command = matches
            .get_one::<String>("redis_command")
            .context("command is required")?;
        let args: Vec<&String> = matches
            .get_many::<String>("arg")
            .map(|v| v.collect())
            .unwrap_or_default();
        let body = serde_json::json!({ "command": redis_command, "args": args });
        let url = explorer_url(resource, "/redis-command");
        print_pretty(&check(
            post(&url, body)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redis_cmd() -> Command {
        RedisCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        redis_cmd().debug_assert();
    }

    #[test]
    fn keys_requires_resource() {
        assert!(redis_cmd().try_get_matches_from(["redis", "keys"]).is_err());
        assert!(
            redis_cmd()
                .try_get_matches_from(["redis", "keys", "--resource", "res-1"])
                .is_ok()
        );
    }

    #[test]
    fn set_requires_key_and_value() {
        assert!(
            redis_cmd()
                .try_get_matches_from(["redis", "set", "--resource", "res-1", "mykey"])
                .is_err()
        );
        assert!(
            redis_cmd()
                .try_get_matches_from(["redis", "set", "--resource", "res-1", "mykey", "myvalue"])
                .is_ok()
        );
    }

    #[test]
    fn command_accepts_multiple_args() {
        let matches = redis_cmd()
            .try_get_matches_from(["redis", "command", "--resource", "res-1", "SET", "a", "b"])
            .unwrap();
        let sub = matches.subcommand_matches("command").unwrap();
        let args: Vec<&String> = sub.get_many::<String>("arg").unwrap().collect();
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn ttl_body_omits_the_key_when_clearing() {
        // Regression: must NOT be `{"ttl": null}` — optional(number) server-side
        // rejects an explicit null, it expects the key omitted entirely.
        let body = build_ttl_body(None);
        assert_eq!(body, serde_json::json!({}));
        assert!(body.get("ttl").is_none());
    }

    #[test]
    fn ttl_body_includes_the_key_when_setting() {
        let body = build_ttl_body(Some(60));
        assert_eq!(body, serde_json::json!({ "ttl": 60 }));
    }
}

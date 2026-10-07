use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        http_client::{delete, get, patch, post},
    },
};

#[derive(Debug)]
pub(super) struct KafkaCommand {
    topics: TopicsCommand,
    create_topic: CreateTopicCommand,
    delete_topic: DeleteTopicCommand,
    update_topic_config: UpdateTopicConfigCommand,
    messages: MessagesCommand,
    topic_metadata: TopicMetadataCommand,
    produce: ProduceCommand,
    consumer_groups: ConsumerGroupsCommand,
    reset_offsets: ResetOffsetsCommand,
}

impl KafkaCommand {
    pub(super) fn new() -> Self {
        Self {
            topics: TopicsCommand::new(),
            create_topic: CreateTopicCommand::new(),
            delete_topic: DeleteTopicCommand::new(),
            update_topic_config: UpdateTopicConfigCommand::new(),
            messages: MessagesCommand::new(),
            topic_metadata: TopicMetadataCommand::new(),
            produce: ProduceCommand::new(),
            consumer_groups: ConsumerGroupsCommand::new(),
            reset_offsets: ResetOffsetsCommand::new(),
        }
    }
}

impl CliCommand for KafkaCommand {
    fn command(&self) -> Command {
        command("kafka", "Explore a provisioned Kafka/Redpanda resource")
            .subcommand_required(true)
            .subcommand(self.topics.command())
            .subcommand(self.create_topic.command())
            .subcommand(self.delete_topic.command())
            .subcommand(self.update_topic_config.command())
            .subcommand(self.messages.command())
            .subcommand(self.topic_metadata.command())
            .subcommand(self.produce.command())
            .subcommand(self.consumer_groups.command())
            .subcommand(self.reset_offsets.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("topics", m)) => self.topics.handler(m),
            Some(("create-topic", m)) => self.create_topic.handler(m),
            Some(("delete-topic", m)) => self.delete_topic.handler(m),
            Some(("update-topic-config", m)) => self.update_topic_config.handler(m),
            Some(("messages", m)) => self.messages.handler(m),
            Some(("topic-metadata", m)) => self.topic_metadata.handler(m),
            Some(("produce", m)) => self.produce.handler(m),
            Some(("consumer-groups", m)) => self.consumer_groups.handler(m),
            Some(("reset-offsets", m)) => self.reset_offsets.handler(m),
            _ => unreachable!(),
        }
    }
}

fn resource_arg() -> Arg {
    Arg::new("resource")
        .long("resource")
        .required(true)
        .help("The provisioned Kafka resource id")
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
struct TopicsCommand;
impl TopicsCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for TopicsCommand {
    fn command(&self) -> Command {
        command("topics", "List topics").arg(resource_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let url = explorer_url(resource, "/topics");
        print_pretty(&check(
            get(&url).with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct CreateTopicCommand;
impl CreateTopicCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for CreateTopicCommand {
    fn command(&self) -> Command {
        command("create-topic", "Create a topic")
            .arg(resource_arg())
            .arg(Arg::new("name").required(true).help("Topic name"))
            .arg(Arg::new("partitions").long("partitions").help("Partition count"))
            .arg(
                Arg::new("replication_factor")
                    .long("replication-factor")
                    .help("Replication factor"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let name = matches.get_one::<String>("name").context("name is required")?;
        let mut body = serde_json::json!({ "topicName": name });
        if let Some(p) = matches.get_one::<String>("partitions") {
            body["partitions"] = serde_json::Value::from(
                p.parse::<u32>().context("--partitions must be an integer")?,
            );
        }
        if let Some(r) = matches.get_one::<String>("replication_factor") {
            body["replicationFactor"] = serde_json::Value::from(
                r.parse::<u32>().context("--replication-factor must be an integer")?,
            );
        }
        let url = explorer_url(resource, "/topics");
        print_pretty(&check(
            post(&url, body)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct DeleteTopicCommand;
impl DeleteTopicCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for DeleteTopicCommand {
    fn command(&self) -> Command {
        command("delete-topic", "Delete a topic")
            .arg(resource_arg())
            .arg(Arg::new("name").required(true).help("Topic name"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let name = matches.get_one::<String>("name").context("name is required")?;
        let url = explorer_url(resource, &format!("/topics/{}", urlencoding::encode(name)));
        print_pretty(&check(
            delete(&url)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

fn parse_config_entries<'a, I: Iterator<Item = &'a String>>(
    entries: Option<I>,
) -> Result<Vec<serde_json::Value>> {
    let configs: Vec<serde_json::Value> = entries
        .map(|entries| {
            entries
                .map(|e| {
                    let (n, v) = e
                        .split_once('=')
                        .with_context(|| format!("--config '{}' must be in <name>=<value> form", e))?;
                    if n.is_empty() {
                        bail!("--config '{}' has an empty name", e);
                    }
                    Ok(serde_json::json!({ "name": n, "value": v }))
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    if configs.is_empty() {
        bail!("At least one --config <name>=<value> is required");
    }
    Ok(configs)
}

fn validate_reset_target(target: &str) -> Result<()> {
    if target != "earliest"
        && target != "latest"
        && !matches!(target.parse::<i64>(), Ok(offset) if offset >= 0)
    {
        bail!(
            "--target must be 'earliest', 'latest', or a specific non-negative integer offset, got '{}'",
            target
        );
    }
    Ok(())
}

#[derive(Debug)]
struct UpdateTopicConfigCommand;
impl UpdateTopicConfigCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for UpdateTopicConfigCommand {
    fn command(&self) -> Command {
        command("update-topic-config", "Update a topic's configuration")
            .arg(resource_arg())
            .arg(Arg::new("name").required(true).help("Topic name"))
            .arg(
                Arg::new("config")
                    .long("config")
                    .action(clap::ArgAction::Append)
                    .help("<name>=<value>, repeatable"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let name = matches.get_one::<String>("name").context("name is required")?;
        let configs =
            parse_config_entries(matches.get_many::<String>("config").map(|v| v.into_iter()))?;
        let body = serde_json::json!({ "configs": configs });
        let url = explorer_url(resource, &format!("/topics/{}/config", urlencoding::encode(name)));
        print_pretty(&check(
            patch(&url, body)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct MessagesCommand;
impl MessagesCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for MessagesCommand {
    fn command(&self) -> Command {
        command("messages", "Read messages from a topic")
            .arg(resource_arg())
            .arg(Arg::new("name").required(true).help("Topic name"))
            .arg(Arg::new("partition").long("partition").help("Partition number"))
            .arg(Arg::new("offset").long("offset").help("Start offset"))
            .arg(Arg::new("count").long("count").help("Max messages to read"))
            .arg(Arg::new("timestamp").long("timestamp").help("Seek to this timestamp"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let name = matches.get_one::<String>("name").context("name is required")?;
        let mut url = explorer_url(resource, &format!("/topics/{}/messages", urlencoding::encode(name)));
        let mut params = Vec::new();
        for (flag, key) in [
            ("partition", "partition"),
            ("offset", "offset"),
            ("count", "count"),
            ("timestamp", "timestamp"),
        ] {
            if let Some(v) = matches.get_one::<String>(flag) {
                params.push(format!("{}={}", key, urlencoding::encode(v)));
            }
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
struct TopicMetadataCommand;
impl TopicMetadataCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for TopicMetadataCommand {
    fn command(&self) -> Command {
        command("topic-metadata", "Get a topic's partition/replica metadata")
            .arg(resource_arg())
            .arg(Arg::new("name").required(true).help("Topic name"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let name = matches.get_one::<String>("name").context("name is required")?;
        let url = explorer_url(resource, &format!("/topics/{}/metadata", urlencoding::encode(name)));
        print_pretty(&check(
            get(&url).with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct ProduceCommand;
impl ProduceCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for ProduceCommand {
    fn command(&self) -> Command {
        command("produce", "Produce a message to a topic")
            .arg(resource_arg())
            .arg(Arg::new("name").required(true).help("Topic name"))
            .arg(Arg::new("value").required(true).help("Message value"))
            .arg(Arg::new("key").long("key").help("Message key"))
            .arg(Arg::new("partition").long("partition").help("Target partition"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let name = matches.get_one::<String>("name").context("name is required")?;
        let value = matches.get_one::<String>("value").context("value is required")?;
        let mut body = serde_json::json!({ "topicName": name, "value": value });
        if let Some(k) = matches.get_one::<String>("key") {
            body["key"] = serde_json::Value::String(k.clone());
        }
        if let Some(p) = matches.get_one::<String>("partition") {
            body["partition"] = serde_json::Value::from(
                p.parse::<u32>().context("--partition must be an integer")?,
            );
        }
        let url = explorer_url(resource, "/produce");
        print_pretty(&check(
            post(&url, body)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct ConsumerGroupsCommand;
impl ConsumerGroupsCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for ConsumerGroupsCommand {
    fn command(&self) -> Command {
        command("consumer-groups", "List consumer groups").arg(resource_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let url = explorer_url(resource, "/consumer-groups");
        print_pretty(&check(
            get(&url).with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[derive(Debug)]
struct ResetOffsetsCommand;
impl ResetOffsetsCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for ResetOffsetsCommand {
    fn command(&self) -> Command {
        command("reset-offsets", "Reset a consumer group's offsets for a topic")
            .arg(resource_arg())
            .arg(Arg::new("group").required(true).help("Consumer group id"))
            .arg(
                Arg::new("topic")
                    .long("topic")
                    .required(true)
                    .help("Topic name"),
            )
            .arg(
                Arg::new("target")
                    .long("target")
                    .required(true)
                    .help("earliest, latest, or a specific offset"),
            )
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let group = matches.get_one::<String>("group").context("group is required")?;
        let topic = matches.get_one::<String>("topic").context("--topic is required")?;
        let target = matches.get_one::<String>("target").context("--target is required")?;
        validate_reset_target(target)?;
        let body = serde_json::json!({ "topicName": topic, "target": target });
        let url = explorer_url(resource, &format!("/consumer-groups/{}/offsets", urlencoding::encode(group)));
        print_pretty(&check(
            post(&url, body)
                .with_context(|| "Failed to reach observability API")?,
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kafka_cmd() -> Command {
        KafkaCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        kafka_cmd().debug_assert();
    }

    #[test]
    fn topics_requires_resource() {
        assert!(kafka_cmd().try_get_matches_from(["kafka", "topics"]).is_err());
        assert!(
            kafka_cmd()
                .try_get_matches_from(["kafka", "topics", "--resource", "res-1"])
                .is_ok()
        );
    }

    #[test]
    fn reset_offsets_requires_topic_and_target() {
        assert!(
            kafka_cmd()
                .try_get_matches_from(["kafka", "reset-offsets", "--resource", "res-1", "group-1"])
                .is_err()
        );
        assert!(
            kafka_cmd()
                .try_get_matches_from([
                    "kafka", "reset-offsets", "--resource", "res-1", "group-1", "--topic", "orders",
                    "--target", "earliest"
                ])
                .is_ok()
        );
    }

    #[test]
    fn parse_config_entries_rejects_malformed_or_empty_name() {
        assert!(parse_config_entries::<std::slice::Iter<String>>(None).is_err());
        let missing_eq = vec!["retention.ms".to_string()];
        assert!(parse_config_entries(Some(missing_eq.iter())).is_err());
        let empty_name = vec!["=1000".to_string()];
        assert!(parse_config_entries(Some(empty_name.iter())).is_err());
        let valid = vec!["retention.ms=1000".to_string()];
        assert!(parse_config_entries(Some(valid.iter())).is_ok());
    }

    #[test]
    fn validate_reset_target_accepts_named_or_numeric_only() {
        assert!(validate_reset_target("earliest").is_ok());
        assert!(validate_reset_target("latest").is_ok());
        assert!(validate_reset_target("42").is_ok());
        assert!(validate_reset_target("0").is_ok());
        assert!(validate_reset_target("-1").is_err());
        assert!(validate_reset_target("bogus").is_err());
        assert!(validate_reset_target("").is_err());
    }

    #[test]
    fn produce_requires_value() {
        assert!(
            kafka_cmd()
                .try_get_matches_from(["kafka", "produce", "--resource", "res-1", "orders"])
                .is_err()
        );
        assert!(
            kafka_cmd()
                .try_get_matches_from(["kafka", "produce", "--resource", "res-1", "orders", "hello"])
                .is_ok()
        );
    }
}

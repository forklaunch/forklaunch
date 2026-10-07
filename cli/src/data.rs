use anyhow::Result;
use clap::{ArgMatches, Command};
use db::DbCommand;
use kafka::KafkaCommand;
use redis::RedisCommand;

use crate::{CliCommand, core::command::command};

mod db;
mod kafka;
mod redis;

/// Ad-hoc access to a provisioned resource's actual data — direct Postgres
/// queries, Redis key inspection, Kafka topic/consumer-group management.
/// Deliberately its own top-level verb rather than folded into `fl observe`,
/// since this reaches into application data rather than telemetry about it.
#[derive(Debug)]
pub(crate) struct DataCommand {
    db: DbCommand,
    redis: RedisCommand,
    kafka: KafkaCommand,
}

impl DataCommand {
    pub(crate) fn new() -> Self {
        Self {
            db: DbCommand::new(),
            redis: RedisCommand::new(),
            kafka: KafkaCommand::new(),
        }
    }
}

impl CliCommand for DataCommand {
    fn command(&self) -> Command {
        command(
            "data",
            "Explore a provisioned resource's actual data (Postgres/Redis/Kafka)",
        )
        .subcommand_required(true)
        .subcommand(self.db.command())
        .subcommand(self.redis.command())
        .subcommand(self.kafka.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("db", m)) => self.db.handler(m),
            Some(("redis", m)) => self.redis.handler(m),
            Some(("kafka", m)) => self.kafka.handler(m),
            _ => unreachable!(),
        }
    }
}

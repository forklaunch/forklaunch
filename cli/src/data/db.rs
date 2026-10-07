use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};

use crate::{
    CliCommand,
    constants::get_observability_api_url,
    core::{
        command::command,
        http_client::{get, post},
    },
};

#[derive(Debug)]
pub(super) struct DbCommand {
    tables: TablesCommand,
    schema: SchemaCommand,
    rows: RowsCommand,
    query: QueryCommand,
}

impl DbCommand {
    pub(super) fn new() -> Self {
        Self {
            tables: TablesCommand::new(),
            schema: SchemaCommand::new(),
            rows: RowsCommand::new(),
            query: QueryCommand::new(),
        }
    }
}

impl CliCommand for DbCommand {
    fn command(&self) -> Command {
        command("db", "Explore a provisioned Postgres resource")
            .subcommand_required(true)
            .subcommand(self.tables.command())
            .subcommand(self.schema.command())
            .subcommand(self.rows.command())
            .subcommand(self.query.command())
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        match matches.subcommand() {
            Some(("tables", m)) => self.tables.handler(m),
            Some(("schema", m)) => self.schema.handler(m),
            Some(("rows", m)) => self.rows.handler(m),
            Some(("query", m)) => self.query.handler(m),
            _ => unreachable!(),
        }
    }
}

fn resource_arg() -> Arg {
    Arg::new("resource")
        .long("resource")
        .required(true)
        .help("The provisioned Postgres resource id")
}

fn explorer_url(resource_id: &str, path: &str) -> String {
    format!(
        "{}/resources/{}/explorer{}",
        get_observability_api_url(),
        urlencoding::encode(resource_id),
        path
    )
}

fn get_json(url: &str) -> Result<serde_json::Value> {
    let response = get(url).with_context(|| "Failed to reach observability API")?;
    if !response.status().is_success() {
        bail!(
            "Request failed ({}): {}",
            response.status(),
            response.text().unwrap_or_default()
        );
    }
    response.json().with_context(|| "Failed to parse response")
}

fn post_json(url: &str, body: serde_json::Value) -> Result<serde_json::Value> {
    let response = post(url, body)
        .with_context(|| "Failed to reach observability API")?;
    if !response.status().is_success() {
        bail!(
            "Request failed ({}): {}",
            response.status(),
            response.text().unwrap_or_default()
        );
    }
    response.json().with_context(|| "Failed to parse response")
}

fn print_pretty(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[derive(Debug)]
struct TablesCommand;
impl TablesCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for TablesCommand {
    fn command(&self) -> Command {
        command("tables", "List tables in the database").arg(resource_arg())
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        print_pretty(&get_json(&explorer_url(resource, "/tables"))?)
    }
}

#[derive(Debug)]
struct SchemaCommand;
impl SchemaCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for SchemaCommand {
    fn command(&self) -> Command {
        command("schema", "Get a table's columns, indexes, and constraints")
            .arg(resource_arg())
            .arg(Arg::new("table").required(true).help("Table name"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let table = matches.get_one::<String>("table").context("table is required")?;
        print_pretty(&get_json(&explorer_url(
            resource,
            &format!("/tables/{}/schema", urlencoding::encode(table)),
        ))?)
    }
}

#[derive(Debug)]
struct RowsCommand;
impl RowsCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for RowsCommand {
    fn command(&self) -> Command {
        command("rows", "Get paginated rows from a table")
            .arg(resource_arg())
            .arg(Arg::new("table").required(true).help("Table name"))
            .arg(Arg::new("page").long("page").help("Page number"))
            .arg(Arg::new("page_size").long("page-size").help("Rows per page"))
            .arg(Arg::new("sort_column").long("sort-column").help("Column to sort by"))
            .arg(Arg::new("sort_direction").long("sort-direction").help("asc or desc"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let table = matches.get_one::<String>("table").context("table is required")?;
        let mut url = explorer_url(resource, &format!("/tables/{}/rows", urlencoding::encode(table)));
        let mut params = Vec::new();
        if let Some(v) = matches.get_one::<String>("page") {
            params.push(format!("page={}", urlencoding::encode(v)));
        }
        if let Some(v) = matches.get_one::<String>("page_size") {
            params.push(format!("pageSize={}", urlencoding::encode(v)));
        }
        if let Some(v) = matches.get_one::<String>("sort_column") {
            params.push(format!("sortColumn={}", urlencoding::encode(v)));
        }
        if let Some(v) = matches.get_one::<String>("sort_direction") {
            params.push(format!("sortDirection={}", urlencoding::encode(v)));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        print_pretty(&get_json(&url)?)
    }
}

#[derive(Debug)]
struct QueryCommand;
impl QueryCommand {
    fn new() -> Self {
        Self
    }
}
impl CliCommand for QueryCommand {
    fn command(&self) -> Command {
        command("query", "Execute a raw SQL query")
            .arg(resource_arg())
            .arg(Arg::new("sql").required(true).help("The SQL to execute"))
    }
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let resource = matches.get_one::<String>("resource").context("--resource is required")?;
        let sql = matches.get_one::<String>("sql").context("sql is required")?;
        let body = serde_json::json!({ "sql": sql });
        print_pretty(&post_json(&explorer_url(resource, "/query"), body)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_cmd() -> Command {
        DbCommand::new().command().version("0.0.0-test")
    }

    #[test]
    fn command_definition_is_valid() {
        db_cmd().debug_assert();
    }

    #[test]
    fn requires_a_subcommand() {
        assert!(db_cmd().try_get_matches_from(["db"]).is_err());
    }

    #[test]
    fn tables_requires_resource() {
        assert!(db_cmd().try_get_matches_from(["db", "tables"]).is_err());
        assert!(
            db_cmd()
                .try_get_matches_from(["db", "tables", "--resource", "res-1"])
                .is_ok()
        );
    }

    #[test]
    fn schema_requires_resource_and_table() {
        assert!(
            db_cmd()
                .try_get_matches_from(["db", "schema", "--resource", "res-1"])
                .is_err()
        );
        assert!(
            db_cmd()
                .try_get_matches_from(["db", "schema", "--resource", "res-1", "orders"])
                .is_ok()
        );
    }

    #[test]
    fn query_requires_sql() {
        assert!(
            db_cmd()
                .try_get_matches_from(["db", "query", "--resource", "res-1"])
                .is_err()
        );
        assert!(
            db_cmd()
                .try_get_matches_from(["db", "query", "--resource", "res-1", "SELECT 1"])
                .is_ok()
        );
    }
}

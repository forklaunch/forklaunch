use std::{collections::HashMap, io::Write};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde_json::{Value, json};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_manifest},
    },
    github::application_github_url,
};

#[derive(Debug)]
pub(crate) struct ConnectCommand;

impl ConnectCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for ConnectCommand {
    fn command(&self) -> Command {
        command(
            "connect",
            "Connect this application to a GitHub repository and configure autodeploy",
        )
        .arg(
            Arg::new("repo")
                .long("repo")
                .short('r')
                .required(true)
                .help("Repository URL (e.g. https://github.com/org/repo)"),
        )
        .arg(
            Arg::new("default_branch")
                .long("default-branch")
                .default_value("main")
                .help("Default branch of the repository"),
        )
        .arg(
            Arg::new("auto_deploy")
                .long("auto-deploy")
                .action(ArgAction::SetTrue)
                .help("Automatically release and deploy on push"),
        )
        .arg(
            Arg::new("release_environment")
                .long("release-environment")
                .help("Environment autodeployed releases target (e.g. production)"),
        )
        .arg(
            Arg::new("region")
                .long("region")
                .help("Region autodeployed releases target (e.g. us-east-2)"),
        )
        .arg(
            Arg::new("branch_mapping")
                .long("branch-mapping")
                .action(ArgAction::Append)
                .value_name("BRANCH=ENVIRONMENT")
                .help("Map a branch to an environment (repeatable, e.g. --branch-mapping main=production)"),
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        require_auth()?;
        let (_, manifest) = require_manifest(matches)?;

        let Some(app_id) = &manifest.platform_application_id else {
            bail!(
                "This application is not integrated with the platform. Run `forklaunch integrate --app <application-id>` first."
            );
        };

        let repo = matches
            .get_one::<String>("repo")
            .expect("repo is required by clap");
        let default_branch = matches
            .get_one::<String>("default_branch")
            .expect("default_branch has a default");

        let mut branch_mapping: HashMap<String, String> = HashMap::new();
        if let Some(mappings) = matches.get_many::<String>("branch_mapping") {
            for mapping in mappings {
                let Some((branch, environment)) = mapping.split_once('=') else {
                    bail!(
                        "Invalid --branch-mapping '{}': expected BRANCH=ENVIRONMENT",
                        mapping
                    );
                };
                branch_mapping.insert(branch.to_string(), environment.to_string());
            }
        }

        let mut cicd_config = serde_json::Map::new();
        if matches.get_flag("auto_deploy") {
            cicd_config.insert("autoDeploy".into(), json!(true));
            cicd_config.insert("autoDeployReleases".into(), json!(true));
        }
        if let Some(env) = matches.get_one::<String>("release_environment") {
            cicd_config.insert("releaseEnvironment".into(), json!(env));
        }
        if let Some(region) = matches.get_one::<String>("region") {
            cicd_config.insert("region".into(), json!(region));
        }
        if !branch_mapping.is_empty() {
            cicd_config.insert("branchMapping".into(), json!(branch_mapping));
        }

        let mut body = json!({
            "repositoryUrl": repo,
            "defaultBranch": default_branch,
        });
        if !cicd_config.is_empty() {
            body["cicdConfig"] = Value::Object(cicd_config);
        }

        log_info!(stdout, "Connecting {} to {}...", app_id, repo);
        let url = application_github_url(app_id, "connect");
        let response = http_client::post(&url, body)?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().unwrap_or_default();
            bail!(
                "Failed to connect repository (Status: {}): {}",
                status,
                detail
            );
        }
        let result: Value = response
            .json()
            .with_context(|| "Failed to parse connect response")?;

        log_header!(stdout, Color::Green, "Repository connected!");
        if let Some(message) = result.get("message").and_then(Value::as_str) {
            log_info!(stdout, "{}", message);
        }
        if matches.get_flag("auto_deploy") {
            log_info!(
                stdout,
                "Autodeploy is on: pushes to {} will release and deploy automatically.",
                default_branch
            );
        }
        log_info!(
            stdout,
            "Inspect the connection any time with `forklaunch github status`."
        );

        Ok(())
    }
}

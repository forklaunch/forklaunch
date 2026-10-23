//! `forklaunch init agent`: an agent is a Service with the "agent" variant, so
//! release, deploy and preview treat it as an ordinary HTTP service. On top of
//! the service scaffold it adds the agent routes (`/api/v1/agent/tools`,
//! `/tasks`, `/tasks/stream`), a tools folder and `agent.config.ts`, which
//! wires `@forklaunch/core/agent`. See docs/design/init-agent.md.

use std::{fs::read_to_string, path::Path};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use convert_case::{Case, Casing};
use rustyline::{Editor, history::DefaultHistory};
use termcolor::{ColorChoice, StandardStream, WriteColor};
use toml::from_str;

use super::{
    rust_project,
    service::{generate_basic_service, service_manifest_data},
};
use crate::{
    CliCommand,
    constants::{Database, ERROR_FAILED_TO_PARSE_MANIFEST, ERROR_FAILED_TO_READ_MANIFEST},
    core::{
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        docker::DockerCompose,
        env_defaults::{find_existing_hmac_secret, generate_random_b64_secret},
        format::format_code,
        manifest::{
            ApplicationInitializationMetadata, InitializableManifestConfig,
            InitializableManifestConfigMetadata, ManifestData, ProjectType,
            application::ApplicationManifestData,
        },
        name::validate_name,
        rendered_template::{RenderedTemplate, write_rendered_templates},
        template::{PathIO, generate_with_template},
    },
    prompt::{ArrayCompleter, prompt_with_validation},
};

pub(crate) const AGENT_VARIANT: &str = "agent";

const DEFAULT_INSTRUCTIONS: &str = "You are a careful assistant for this app. Use the tools you have to answer accurately. If you cannot do something with your tools, say so. Never guess at data you have not read.";

#[derive(Debug)]
pub(crate) struct AgentCommand;

impl AgentCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

/// `--mcp name=url`: an external MCP server reached over streamable HTTP.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct McpServerArg {
    pub(crate) name: String,
    pub(crate) url: String,
}

pub(crate) fn parse_mcp_server(value: &str) -> Result<McpServerArg> {
    let Some((name, url)) = value.split_once('=') else {
        bail!("--mcp expects name=url, for example --mcp linear=https://mcp.linear.app/mcp");
    };
    let name = name.trim();
    if name.is_empty()
        || name.len() > 32
        || !name.as_bytes()[0].is_ascii_lowercase()
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        // Underscores as well as dashes, so the same --mcp name works with
        // --language rust, which takes ^[a-z][a-z0-9_]*$.
        bail!(
            "MCP server name '{name}' must start with a lowercase letter and use lowercase letters, digits, underscores or dashes."
        );
    }
    let url = url.trim();
    if !(url.starts_with("https://") || url.starts_with("http://"))
        || url.contains(char::is_whitespace)
    {
        bail!("MCP server URL '{url}' must be an http(s) URL.");
    }
    Ok(McpServerArg {
        name: name.to_string(),
        url: url.to_string(),
    })
}

/// Setting name for a value: `records` → `RECORDS`, `my-tool` → `MY_TOOL`.
pub(crate) fn setting_prefix(name: &str) -> String {
    name.replace('-', "_").to_ascii_uppercase()
}

fn ts_string(value: &str) -> String {
    serde_json::to_string(value).expect("strings always serialize")
}

/// `agent.config.ts`: instructions, limits and tool sources. Users and Codex
/// edit this file to add services, MCP servers and connectors later.
pub(crate) fn render_agent_config(
    agent_name: &str,
    instructions: &str,
    services: &[String],
    mcp_servers: &[McpServerArg],
) -> String {
    let mut imports = vec!["defineAgent", "localTools"];
    if !mcp_servers.is_empty() {
        imports.push("mcpServer");
    }
    if !services.is_empty() {
        imports.push("serviceTools");
    }
    let mut sources = vec![format!(
        "    // The agent's own tools, in ./tools. Connectors to external APIs live there too.\n    localTools({}, [currentTime])",
        ts_string(agent_name)
    )];
    for service in services {
        sources.push(format!(
            "    // Routes of the {service} service, called as the signed-in user ({}_URL).\n    serviceTools({})",
            setting_prefix(service),
            ts_string(service)
        ));
    }
    for server in mcp_servers {
        let prefix = setting_prefix(&server.name);
        sources.push(format!(
            "    // External MCP server. Its URL and token are settings, never source.\n    mcpServer({{\n      name: {},\n      url: process.env.{prefix}_MCP_URL,\n      headers: {{\n        authorization: process.env.{prefix}_MCP_TOKEN\n          ? `Bearer ${{process.env.{prefix}_MCP_TOKEN}}`\n          : undefined\n      }}\n    }})",
            ts_string(&server.name)
        ));
    }
    format!(
        "import {{ {} }} from '@forklaunch/core/agent';\nimport {{ currentTime }} from './tools/currentTime.tool';\n\n/**\n * The {agent_name} agent. Tools follow MCP: only tools annotated\n * `readOnlyHint: true` run without asking; every other tool waits for the\n * user's approval. Read the `agents` skill before adding tools.\n */\nexport const agent = defineAgent({{\n  name: {},\n  instructions: {},\n  // Optional: model (default AGENT_MODEL) and limits, e.g. {{ maxSteps: 8 }}.\n  sources: [\n{}\n  ]\n}});\n",
        imports.join(", "),
        ts_string(agent_name),
        ts_string(instructions),
        sources.join(",\n")
    )
}

/// Mounts the agent router next to the scaffold's own router in server.ts.
pub(crate) fn mount_agent_router(server_ts: &str, camel_case_name: &str) -> Result<String> {
    let router_import = format!(
        "import {{ {camel_case_name}Router }} from './api/routes/{camel_case_name}.routes';"
    );
    let router_use = format!("app.use({camel_case_name}Router);");
    if !server_ts.contains(&router_import) || !server_ts.contains(&router_use) {
        bail!("The generated server.ts does not mount its router where expected.");
    }
    Ok(server_ts
        .replacen(
            &router_import,
            &format!("{router_import}\nimport {{ agentRouter }} from './api/routes/agent.routes';"),
            1,
        )
        .replacen(
            &router_use,
            &format!("{router_use}\n  app.use(agentRouter);"),
            1,
        ))
}

/// Settings the agent reads: each exposed service's URL, each MCP server's
/// URL, and the approval key. Existing values are kept.
pub(crate) fn agent_settings(
    compose: &DockerCompose,
    base_path: &Path,
    services: &[String],
    mcp_servers: &[McpServerArg],
    hmac_secret: &str,
    local: bool,
) -> Vec<(String, String)> {
    let mut settings = Vec::new();
    for service in services {
        // Use the port the service itself runs on in each context.
        let port = if local {
            read_to_string(base_path.join(service).join(".env.local"))
                .ok()
                .and_then(|env| {
                    env.lines()
                        .find_map(|line| line.trim().strip_prefix("PORT=").map(str::to_string))
                })
        } else {
            compose
                .services
                .get(service)
                .and_then(|s| s.environment.as_ref())
                .and_then(|e| e.get("PORT"))
                .cloned()
        }
        .unwrap_or_else(|| "8000".to_string());
        let host = if local { "localhost" } else { service.as_str() };
        settings.push((
            format!("{}_URL", setting_prefix(service)),
            format!("http://{host}:{port}"),
        ));
    }
    for server in mcp_servers {
        settings.push((
            format!("{}_MCP_URL", setting_prefix(&server.name)),
            server.url.clone(),
        ));
    }
    settings.push(("HMAC_SECRET_KEY".to_string(), hmac_secret.to_string()));
    settings
}

fn add_missing_env_lines(contents: &str, settings: &[(String, String)]) -> String {
    let mut lines: Vec<String> = contents.trim_end().lines().map(str::to_string).collect();
    for (key, value) in settings {
        let prefix = format!("{key}=");
        match lines
            .iter_mut()
            .find(|line| line.trim_start().starts_with(&prefix))
        {
            Some(line) if line.trim_start()[prefix.len()..].trim().is_empty() => {
                *line = format!("{key}={value}");
            }
            Some(_) => {}
            None => lines.push(format!("{key}={value}")),
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

impl CliCommand for AgentCommand {
    fn command(&self) -> Command {
        command("agent", "Initialize a new agent")
            .alias("agt")
            .arg(Arg::new("name").help("The name of the agent"))
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The application path to initialize the agent in"),
            )
            .arg(
                Arg::new("language")
                    .long("language")
                    .help("Project language to generate")
                    .value_parser(rust_project::PROJECT_LANGUAGE_VARIANTS)
                    .default_value("typescript"),
            )
            .arg(
                Arg::new("description")
                    .short('D')
                    .long("description")
                    .help("The description of the agent"),
            )
            .arg(
                Arg::new("instructions").long("instructions").help(
                    "The agent's system instructions (default: a careful, tool-using assistant)",
                ),
            )
            .arg(
                Arg::new("services")
                    .long("service")
                    .help("Expose this app service's routes as tools (repeatable)")
                    .action(ArgAction::Append),
            )
            .arg(
                Arg::new("mcp_servers")
                    .long("mcp")
                    .value_name("NAME=URL")
                    .help("Use an external MCP server over streamable HTTP (repeatable)")
                    .action(ArgAction::Append),
            )
            .arg(
                Arg::new("database")
                    .short('d')
                    .long("database")
                    .help("The database to use (default: the app's database)")
                    .value_parser(Database::VARIANTS),
            )
            // Read by the shared Rust service generator; agents never set them.
            .arg(
                Arg::new("infrastructure")
                    .long("infrastructure")
                    .hide(true)
                    .action(ArgAction::Append),
            )
            .arg(
                Arg::new("mappers")
                    .long("mappers")
                    .hide(true)
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("dryrun")
                    .short('n')
                    .long("dryrun")
                    .help("Dry run the command")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        if rust_project::uses_rust_generation(matches) {
            return rust_project::generate_rust_agent(matches);
        }
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");
        let existing_manifest_data = from_str::<ApplicationManifestData>(
            &read_to_string(&manifest_path).with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        if !existing_manifest_data
            .projects
            .iter()
            .any(|p| p.name == "iam")
        {
            bail!(
                "Agents run as the signed-in user, so they need IAM. Add it first with `forklaunch init module iam --module iam-better-auth`."
            );
        }

        let base_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Application(&existing_manifest_data),
            &None,
            &mut line_editor,
            &mut stdout,
            matches,
            0,
        )?;

        let manifest_data = existing_manifest_data.initialize(
            InitializableManifestConfigMetadata::Application(ApplicationInitializationMetadata {
                app_name: existing_manifest_data.app_name.clone(),
                database: None,
            }),
        );

        let agent_name = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "name",
            matches,
            "agent name",
            None,
            |input: &str| {
                validate_name(input)
                    && !manifest_data.app_name.contains(input)
                    && !manifest_data.projects.iter().any(|p| p.name == input)
            },
            |_| {
                "Agent name must be new, cannot be a substring of the application name, and cannot be empty or include numbers or spaces. Please try again"
                    .to_string()
            },
        )?;

        let services: Vec<String> = matches
            .get_many::<String>("services")
            .map(|values| values.cloned().collect())
            .unwrap_or_default();
        for service in &services {
            let known = manifest_data.projects.iter().any(|p| {
                p.name == *service
                    && matches!(p.r#type, ProjectType::Service)
                    && p.variant.as_deref() != Some(AGENT_VARIANT)
            });
            if !known {
                bail!("--service {service}: there is no service named '{service}' in this app.");
            }
        }
        let mcp_servers = matches
            .get_many::<String>("mcp_servers")
            .map(|values| {
                values
                    .map(|v| parse_mcp_server(v))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        // Non-interactive by default so Codex can run it: fall back to the
        // app's existing database rather than prompting.
        let database: Database = match matches.get_one::<String>("database") {
            Some(database) => database.parse()?,
            None => manifest_data
                .projects
                .iter()
                .find_map(|p| p.resources.as_ref().and_then(|r| r.database.clone()))
                .unwrap_or_else(|| "postgresql".to_string())
                .parse()?,
        };

        let description = matches
            .get_one::<String>("description")
            .cloned()
            .unwrap_or_else(|| "AI agent".to_string());
        let instructions = matches
            .get_one::<String>("instructions")
            .cloned()
            .unwrap_or_else(|| DEFAULT_INSTRUCTIONS.to_string());

        let mut service_data = service_manifest_data(
            &manifest_data,
            &base_path,
            &agent_name,
            &description,
            &database,
            &[],
            false,
        );

        let dryrun = matches.get_flag("dryrun");
        generate_basic_service(
            &agent_name,
            &base_path,
            &app_root_path,
            &mut service_data,
            Some(AGENT_VARIANT.to_string()),
            &mut stdout,
            dryrun,
        )
        .with_context(|| "Failed to create the agent's service scaffold")?;

        let output_path = base_path.join(&agent_name);
        let mut rendered = generate_with_template(
            None,
            &PathIO {
                input_path: Path::new("project")
                    .join("agent")
                    .to_string_lossy()
                    .to_string(),
                output_path: output_path.to_string_lossy().to_string(),
                module_id: None,
            },
            &ManifestData::Service(&service_data),
            &vec![],
            &vec![],
            &vec![],
            dryrun,
        )?;
        rendered.push(RenderedTemplate {
            path: output_path.join("agent.config.ts"),
            content: render_agent_config(&agent_name, &instructions, &services, &mcp_servers),
            context: Some("Failed to write agent.config.ts".to_string()),
        });

        if !dryrun {
            let server_path = output_path.join("server.ts");
            rendered.push(RenderedTemplate {
                content: mount_agent_router(
                    &read_to_string(&server_path)?,
                    &agent_name.to_case(Case::Camel),
                )?,
                path: server_path,
                context: Some("Failed to mount the agent routes".to_string()),
            });

            let compose_path = app_root_path.join(
                service_data
                    .docker_compose_path
                    .clone()
                    .unwrap_or_else(|| "docker-compose.yaml".to_string()),
            );
            let mut compose: DockerCompose =
                serde_yml::from_str(&read_to_string(&compose_path)?)
                    .with_context(|| "Failed to read docker-compose.yaml")?;
            let hmac_secret = find_existing_hmac_secret(&base_path)
                .unwrap_or_else(|| generate_random_b64_secret(32));
            let docker_settings = agent_settings(
                &compose,
                &base_path,
                &services,
                &mcp_servers,
                &hmac_secret,
                false,
            );
            let local_settings = agent_settings(
                &compose,
                &base_path,
                &services,
                &mcp_servers,
                &hmac_secret,
                true,
            );
            let service_key = compose
                .services
                .keys()
                .find(|k| *k == &agent_name || k.ends_with(&format!("-{agent_name}")))
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("The agent is missing from docker-compose.yaml."))?;
            let environment = compose
                .services
                .get_mut(&service_key)
                .expect("key was just found")
                .environment
                .get_or_insert_with(Default::default);
            // Keep values the developer set; fill missing or empty ones (approvals
            // need a real HMAC_SECRET_KEY).
            for (key, value) in docker_settings {
                let entry = environment.entry(key).or_default();
                if entry.is_empty() {
                    *entry = value;
                }
            }
            rendered.push(RenderedTemplate {
                path: compose_path,
                content: serde_yml::to_string(&compose)?,
                context: Some(
                    "Failed to add the agent's settings to docker-compose.yaml".to_string(),
                ),
            });

            let env_local_path = output_path.join(".env.local");
            rendered.push(RenderedTemplate {
                content: add_missing_env_lines(
                    &read_to_string(&env_local_path).unwrap_or_default(),
                    &local_settings,
                ),
                path: env_local_path,
                context: Some("Failed to add the agent's settings to .env.local".to_string()),
            });
        }

        write_rendered_templates(&rendered, dryrun, &mut stdout)
            .with_context(|| "Failed to write the agent files")?;

        if !dryrun {
            stdout.set_color(termcolor::ColorSpec::new().set_fg(Some(termcolor::Color::Green)))?;
            writeln_ok(&mut stdout, &agent_name)?;
            stdout.reset()?;
            format_code(&base_path, &service_data.runtime.parse()?);
        }
        Ok(())
    }
}

fn writeln_ok(stdout: &mut StandardStream, agent_name: &str) -> Result<()> {
    use std::io::Write;
    writeln!(
        stdout,
        "{agent_name} agent initialized. Add tools in {agent_name}/tools and {agent_name}/agent.config.ts; it serves GET /api/v1/agent/tools, POST /api/v1/agent/tasks and /tasks/stream."
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mcp_servers_and_rejects_bad_ones() {
        assert_eq!(
            parse_mcp_server("linear=https://mcp.linear.app/mcp").unwrap(),
            McpServerArg {
                name: "linear".into(),
                url: "https://mcp.linear.app/mcp".into()
            }
        );
        assert_eq!(parse_mcp_server("my_linear=https://x").unwrap().name, "my_linear");
        assert!(parse_mcp_server("linear").is_err());
        assert!(parse_mcp_server("_linear=https://x").is_err());
        assert!(parse_mcp_server("Linear=https://x").is_err());
        assert!(parse_mcp_server("linear=ftp://x").is_err());
        assert!(parse_mcp_server("linear=https://x y").is_err());
    }

    #[test]
    fn renders_config_with_each_tool_source() {
        let config = render_agent_config(
            "assistant",
            "Say \"hi\".",
            &["records".into(), "health-records".into()],
            &[McpServerArg {
                name: "my-linear".into(),
                url: "https://mcp.linear.app/mcp".into(),
            }],
        );
        assert!(config.contains("import { defineAgent, localTools, mcpServer, serviceTools } from '@forklaunch/core/agent';"));
        assert!(config.contains("instructions: \"Say \\\"hi\\\".\""));
        assert!(config.contains("serviceTools(\"records\")"));
        assert!(config.contains("(HEALTH_RECORDS_URL)"));
        assert!(config.contains("process.env.MY_LINEAR_MCP_URL"));
        assert!(config.contains("process.env.MY_LINEAR_MCP_TOKEN"));
        let plain = render_agent_config("assistant", "x", &[], &[]);
        assert!(plain.contains("import { defineAgent, localTools } from"));
        assert!(!plain.contains("serviceTools"));
    }

    #[test]
    fn mounts_the_agent_router_after_the_scaffold_router() {
        let server = "import { assistantRouter } from './api/routes/assistant.routes';\nconst x = 1;\n  app.use(assistantRouter);\n";
        let mounted = mount_agent_router(server, "assistant").unwrap();
        assert!(mounted.contains("import { agentRouter } from './api/routes/agent.routes';"));
        assert!(mounted.contains("app.use(assistantRouter);\n  app.use(agentRouter);"));
        assert!(mount_agent_router("nothing", "assistant").is_err());
    }

    #[test]
    fn adds_only_missing_settings() {
        let env = add_missing_env_lines(
            "PORT=8003\nRECORDS_URL=http://custom:1\n",
            &[
                ("RECORDS_URL".into(), "http://localhost:8001".into()),
                ("HMAC_SECRET_KEY".into(), "k".into()),
            ],
        );
        assert_eq!(
            env,
            "PORT=8003\nRECORDS_URL=http://custom:1\nHMAC_SECRET_KEY=k\n"
        );
        let filled = add_missing_env_lines(
            "HMAC_SECRET_KEY=\nPORT=1",
            &[("HMAC_SECRET_KEY".into(), "k".into())],
        );
        assert_eq!(filled, "HMAC_SECRET_KEY=k\nPORT=1\n");
    }
}

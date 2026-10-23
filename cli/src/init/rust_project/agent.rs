//! `forklaunch init agent --language rust`: a Rust service tagged `variant = "agent"`
//! that serves `/api/v1/agent` from the framework agent runtime.
use convert_case::{Case, Casing};
use indexmap::IndexMap;

use crate::core::{rendered_template::RenderedTemplate, watermark::apply_watermark};

use super::*;

/// An external MCP server from `--mcp <name>=<url>`.
pub(super) struct McpServer {
    pub(super) name: String,
    pub(super) url: String,
}

fn valid_tool_prefix(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && name.len() <= 32
}

pub(super) fn setting(name: &str, suffix: &str) -> String {
    format!("{}_{suffix}", name.to_case(Case::UpperSnake))
}

pub(super) fn mcp_servers(matches: &ArgMatches) -> Result<Vec<McpServer>> {
    let mut servers: Vec<McpServer> = Vec::new();
    for value in matches.get_many::<String>("mcp_servers").into_iter().flatten() {
        let (name, url) = value
            .split_once('=')
            .context("--mcp expects <name>=<url>, for example --mcp linear=https://mcp.linear.app/mcp")?;
        anyhow::ensure!(
            valid_tool_prefix(name),
            "MCP server name {name:?} must match ^[a-z][a-z0-9_]*$ (it prefixes tool names)"
        );
        let parsed = url::Url::parse(url).with_context(|| format!("Invalid MCP server URL for {name}"))?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "https" | "http") && parsed.username().is_empty() && parsed.password().is_none(),
            "MCP server {name} needs an http(s) URL without credentials; put tokens in {}",
            setting(name, "MCP_TOKEN")
        );
        anyhow::ensure!(!servers.iter().any(|s| s.name == name), "Duplicate MCP server {name}");
        servers.push(McpServer { name: name.to_owned(), url: url.to_owned() });
    }
    Ok(servers)
}

/// `--service` names, each an existing service project, with its Docker base URL.
pub(super) fn services(matches: &ArgMatches, app_root: &Path) -> Result<Vec<(String, String)>> {
    let manifest = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
    let compose = read_or_create_rust_docker_compose(&app_root.join("docker-compose.yaml"))?;
    let mut resolved = Vec::new();
    for name in matches.get_many::<String>("services").into_iter().flatten() {
        let project = project_table(&manifest, name).with_context(|| format!("No project named {name}"))?;
        anyhow::ensure!(
            project.get("type").and_then(Value::as_str) == Some("Service"),
            "--service {name} must name a service project"
        );
        anyhow::ensure!(valid_tool_prefix(name), "Service {name} cannot prefix tool names; rename it to snake_case");
        let port = compose
            .services
            .get(name)
            .and_then(|service| service.environment.as_ref())
            .and_then(|env| env.get("PORT"))
            .with_context(|| format!("Service {name} has no PORT in docker-compose.yaml"))?;
        resolved.push((name.clone(), format!("http://{name}:{port}")));
    }
    Ok(resolved)
}

/// Agent settings in the Compose service. Required values use `${VAR:?}` so the
/// local runtime fills them in `.env`; model settings come from the platform.
pub(super) fn compose(
    app_root: &Path,
    name: &str,
    services: &[(String, String)],
    servers: &[McpServer],
) -> Result<()> {
    let path = app_root.join("docker-compose.yaml");
    let mut compose = read_or_create_rust_docker_compose(&path)?;
    let service = compose.services.get_mut(name).context("Expected agent Compose service")?;
    let env = service.environment.get_or_insert_with(IndexMap::new);
    let mut set = |key: String, value: String| {
        env.entry(key).or_insert(value);
    };
    let prefix = name.to_case(Case::UpperSnake);
    set("HMAC_SECRET_KEY".into(), format!("${{{prefix}_HMAC_SECRET_KEY:?Configure the agent continuation secret}}"));
    set("JWKS_PUBLIC_KEY_URL".into(), format!("${{{prefix}_JWKS_PUBLIC_KEY_URL:?Configure trusted JWKS}}"));
    for key in ["AGENT_MODEL", "PLATFORM_GATEWAY_URL", "INSTANCE_ID", "INSTANCE_HMAC_KEY", "FORKLAUNCH_MODEL_STREAMING"] {
        set(key.into(), format!("${{{key}:-}}"));
    }
    for (service, url) in services {
        set(setting(service, "URL"), url.clone());
    }
    for server in servers {
        set(setting(&server.name, "MCP_URL"), server.url.clone());
        let token = setting(&server.name, "MCP_TOKEN");
        set(token.clone(), format!("${{{token}:-}}"));
    }
    write(&path, yaml_to_string(&compose)?)?;
    Ok(())
}

/// Tags the generated service as an agent, the shared manifest contract.
pub(super) fn mark_variant(app_root: &Path, name: &str) -> Result<()> {
    let path = app_root.join(".forklaunch/manifest.toml");
    let mut manifest = read_manifest_value(&path)?;
    project_table_mut(&mut manifest, name)
        .context("Expected generated agent project")?
        .insert("variant".into(), Value::String("agent".into()));
    write_manifest_value(&path, &manifest)
}

/// Instructions when `--instructions` is not given.
const DEFAULT_INSTRUCTIONS: &str = "You are a careful assistant for this application. Answer from the tools you have, say when you do not know, and do not change data unless the user asks you to.";

#[derive(Content)]
struct AgentTemplate {
    name: String,
    crate_module: String,
    instructions: String,
    has_services: bool,
    services: Vec<ServiceSource>,
    has_servers: bool,
    servers: Vec<ServerSource>,
}

#[derive(Content)]
struct ServiceSource {
    service: String,
}

#[derive(Content)]
struct ServerSource {
    server: String,
    url_setting: String,
    token_setting: String,
}

/// Where a mounted agent declares its routes, relative to the service crate.
const BINDING_SOURCE: &str = "agent/mod.rs";

/// Adds the agent routes and their static bindings to a contracts host
/// `lib.rs`, formatted or not. Idempotent.
pub(super) fn host_with_agent(host: &str) -> Result<String> {
    if host.contains("agent::contract::routes()") {
        return Ok(host.to_owned());
    }
    let mut parts = host.splitn(3, "Vec::new();");
    let (Some(routes), Some(declarations), Some(rest)) = (parts.next(), parts.next(), parts.next())
    else {
        bail!("Unexpected contracts host layout");
    };
    Ok(format!(
        "{routes}Vec::new();routes.extend(forklaunch_core::agent::contract::routes());{declarations}Vec::new();declarations.extend(forklaunch_core::agent::contract::bindings(\"{BINDING_SOURCE}\")?);{rest}"
    ))
}

/// Mounts the agent on the routes `router()` builds, before `finish()`.
pub(super) fn routes_with_agent(source: &str) -> Result<String> {
    if source.contains("agent::mount(") {
        return Ok(source.to_owned());
    }
    let at = source.rfind(".finish()").context("Unexpected api/routes/mod.rs layout")?;
    let head = &source[..at];
    let statement_start = head.rfind('{').context("Unexpected api/routes/mod.rs layout")? + 1;
    let (before, body) = head.split_at(statement_start);
    // `router()` ends in `<expression>.finish()`; wrap that expression.
    let trimmed = body.trim_start();
    let leading = &body[..body.len() - trimmed.len()];
    let (bindings, expression) = trimmed.rsplit_once(';').map_or(("", trimmed), |(b, e)| (b, e));
    let bindings = if bindings.is_empty() { String::new() } else { format!("{bindings};") };
    Ok(format!(
        "{before}{leading}{bindings} agent::mount({})?{}",
        expression.trim(),
        &source[at..]
    )
    .replacen("use crate::registrations::AppState;", "use crate::{agent, registrations::AppState};", 1))
}

fn edit_features(cargo: &mut toml_edit::DocumentMut, table: &str, dependency: &str, features: &[&str]) -> Result<()> {
    let entry = cargo[table][dependency]
        .as_table_like_mut()
        .with_context(|| format!("Expected {dependency} in [{table}]"))?;
    let list = entry
        .entry("features")
        .or_insert(toml_edit::value(toml_edit::Array::new()))
        .as_array_mut()
        .with_context(|| format!("Expected a features list for {dependency}"))?;
    for feature in features {
        if !list.iter().any(|value| value.as_str() == Some(feature)) {
            list.push(*feature);
        }
    }
    list.fmt();
    Ok(())
}

fn patch(path: &Path, edit: impl FnOnce(&str) -> Result<String>) -> Result<()> {
    let source = read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let next = edit(&source)?;
    if next != source {
        write(path, native_layout::format_source(path, &next))
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }
    Ok(())
}

fn insert_after(source: &str, anchor: &str, text: &str, file: &str) -> Result<String> {
    if source.contains(text.trim()) {
        return Ok(source.to_owned());
    }
    let at = source
        .find(anchor)
        .with_context(|| format!("Unexpected {file} layout: missing `{anchor}`"))?
        + anchor.len();
    Ok(format!("{}{text}{}", &source[..at], &source[at..]))
}

/// Overlays the generated service with the agent: its definition and tools,
/// the authenticated `/api/v1/agent` routes, their contracts and tests.
pub(super) fn sources(
    app_root: &Path,
    name: &str,
    instructions: Option<&str>,
    services: &[(String, String)],
    servers: &[McpServer],
) -> Result<()> {
    let manifest = read_manifest_value(&app_root.join(".forklaunch/manifest.toml"))?;
    let crate_path = project_table(&manifest, name)
        .and_then(|project| project.get("metadata"))
        .and_then(|metadata| metadata.get("cratePath"))
        .and_then(Value::as_str)
        .map_or_else(|| format!("src/{name}"), str::to_owned);
    let dir = app_root.join(crate_path);
    let crate_module = name.to_case(Case::Snake);
    let data = AgentTemplate {
        name: name.to_owned(),
        crate_module: crate_module.clone(),
        instructions: format!("{:?}", instructions.map_or(DEFAULT_INSTRUCTIONS, str::trim)),
        has_services: !services.is_empty(),
        services: services
            .iter()
            .map(|(service, _)| ServiceSource { service: service.clone() })
            .collect(),
        has_servers: !servers.is_empty(),
        servers: servers
            .iter()
            .map(|server| ServerSource {
                server: server.name.clone(),
                url_setting: setting(&server.name, "MCP_URL"),
                token_setting: setting(&server.name, "MCP_TOKEN"),
            })
            .collect(),
    };
    let mut stdout = StandardStream::stdout(ColorChoice::Never);
    for (template, output) in [
        ("rust/agent/mod.rs", "agent/mod.rs"),
        ("rust/agent/tools/mod.rs", "agent/tools/mod.rs"),
        ("rust/agent/tools/current_time.rs", "agent/tools/current_time.rs"),
        ("rust/agent/__test__/agent.rs", "__test__/agent.rs"),
    ] {
        let path = dir.join(output);
        let content = apply_watermark(&RenderedTemplate {
            path: path.clone(),
            content: render_rust_template(template, &data),
            context: None,
        })?;
        write_text_file(&path, &content, false, &mut stdout)?;
    }
    patch(&dir.join("lib.rs"), |source| insert_before_line(source, "pub mod api;", "pub mod agent;\n", "lib.rs"))?;
    patch(&dir.join("registrations.rs"), |source| {
        let source = insert_after(source, "pub struct AppState {\n", "    pub agent: AgentRuntime,\n", "registrations.rs")?;
        // The service template builds its state on one line or across several.
        let source = if source.contains("Ok(AppState {\n") {
            insert_after(&source, "Ok(AppState {\n", "        agent: AgentRuntime::from_env()?,\n", "registrations.rs")?
        } else {
            insert_after(&source, "Ok(AppState { ", "agent: AgentRuntime::from_env()?, ", "registrations.rs")?
        };
        insert_before_line(&source, "use ", "use crate::agent::AgentRuntime;\n", "registrations.rs")
    })?;
    patch(&dir.join("bootstrapper.rs"), |source| {
        insert_after(
            source,
            "let state = registrations::register(config)?;\n",
            "    // Fail fast on tool-name collisions.\n    state.agent.check().await?;\n",
            "bootstrapper.rs",
        )
    })?;
    patch(&dir.join("api/routes/mod.rs"), routes_with_agent)?;
    let host = contracts_layout::new_host_directory(&dir);
    patch(&host.join("lib.rs"), host_with_agent)?;
    for (path, edits) in [
        (dir.join("Cargo.toml"), vec![
            ("dependencies", "forklaunch-core", vec!["agent"]),
            ("dependencies", "forklaunch-axum", vec!["agent"]),
        ]),
        (host.join("Cargo.toml"), vec![("dependencies", "forklaunch-core", vec!["agent-contract"])]),
    ] {
        let original = read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
        let mut cargo: toml_edit::DocumentMut = original.parse()?;
        for (table, dependency, features) in edits {
            edit_features(&mut cargo, table, dependency, &features)?;
        }
        if path == dir.join("Cargo.toml") {
            let tests = cargo
                .entry("test")
                .or_insert(toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
                .as_array_of_tables_mut()
                .context("Expected [[test]] tables")?;
            if !tests.iter().any(|test| test.get("name").and_then(|n| n.as_str()) == Some("agent")) {
                let mut test = toml_edit::Table::new();
                test["name"] = toml_edit::value("agent");
                test["path"] = toml_edit::value("__test__/agent.rs");
                tests.push(test);
            }
        }
        write(&path, cargo.to_string())?;
    }
    Ok(())
}

/// Inserts `text` before the first line starting with `prefix`. Idempotent.
fn insert_before_line(source: &str, prefix: &str, text: &str, file: &str) -> Result<String> {
    if source.lines().any(|line| line == text.trim_end()) {
        return Ok(source.to_owned());
    }
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if line.starts_with(prefix) {
            return Ok(format!("{}{text}{}", &source[..offset], &source[offset..]));
        }
        offset += line.len();
    }
    bail!("Unexpected {file} layout: no line starting with `{prefix}`")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_and_prefixes_follow_the_shared_contract() {
        assert_eq!(setting("records", "URL"), "RECORDS_URL");
        assert_eq!(setting("linear", "MCP_TOKEN"), "LINEAR_MCP_TOKEN");
        assert!(valid_tool_prefix("records"));
        assert!(!valid_tool_prefix("Records"));
        assert!(!valid_tool_prefix("my-server"));
    }

    #[test]
    fn agent_routes_join_the_contracts_host_and_router() {
        let host = super::super::contracts_layout::host_source("assistant", &["health".into()]);
        let joined = host_with_agent(&host).unwrap();
        assert!(joined.contains("routes.extend(forklaunch_core::agent::contract::routes());"));
        assert!(joined.contains("forklaunch_core::agent::contract::bindings(\"agent/mod.rs\")?"));
        assert_eq!(host_with_agent(&joined).unwrap(), joined);
        let generated = "use crate::registrations::AppState;\npub fn router() -> Result<Router<AppState>, String> {\n    health::mount(ContractRouter::new(contracts::registry()))?.finish()\n}\n";
        let mounted = routes_with_agent(generated).unwrap();
        assert!(mounted.contains("agent::mount(health::mount(ContractRouter::new(contracts::registry()))?)?.finish()"));
        assert!(mounted.contains("use crate::{agent, registrations::AppState};"));
        let regenerated = "use crate::registrations::AppState;\npub fn router() -> Result<Router<AppState>, String> { let router = ContractRouter::new(contracts::registry()); let router = health::mount(router)?;\n router.finish() }\n";
        assert!(routes_with_agent(regenerated).unwrap().contains("agent::mount(router)?.finish()"));
        assert_eq!(routes_with_agent(&mounted).unwrap(), mounted);
    }

    #[test]
    fn imports_are_inserted_once_before_the_first_use() {
        let source = "/*\n * header\n */\n\nuse crate::config::ServiceConfig;\n";
        let once = insert_before_line(source, "use ", "use crate::agent::AgentRuntime;\n", "x").unwrap();
        assert!(once.contains("*/\n\nuse crate::agent::AgentRuntime;\nuse crate::config"));
        assert_eq!(insert_before_line(&once, "use ", "use crate::agent::AgentRuntime;\n", "x").unwrap(), once);
    }
}

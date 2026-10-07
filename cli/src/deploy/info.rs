use std::io::Write;

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use serde::Deserialize;
use termcolor::{ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url},
    core::{
        command::command,
        http_client,
        validate::{require_auth, require_integration, require_manifest},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentSummary {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    environment: Option<String>,
    #[serde(default)]
    region: Option<String>,
    #[serde(default)]
    release_version: Option<String>,
    #[serde(default)]
    deployed_by: Option<String>,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    completed_at: Option<String>,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct DeploymentListResponse {
    #[serde(default)]
    deployments: Vec<DeploymentSummary>,
}

/// One component the platform refused to deploy, and the keys it wants.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct BlockedComponent {
    #[serde(rename = "componentType")]
    pub(crate) component_type: String,
    pub(crate) name: String,
    #[serde(rename = "missingKeys")]
    pub(crate) missing_keys: Vec<String>,
}

/// Recover the structured detail from a blocked-deploy message.
///
/// The platform builds this error from a `DeploymentBlockedDetail[]` — component
/// type, name, and the unset keys — and then flattens it to prose before storing
/// it on `deployment.errorMessage`. By the time anyone reads it back, the only
/// thing left is a sentence.
///
/// That is fine for the notification email a human gets and useless to an agent,
/// which needs the keys as data to run `config set` against. Parsing the prose
/// back is a bridge, not the destination: the durable fix is for the API to
/// return the details it already had. Written to fail closed — anything that
/// does not match the exact shape yields nothing rather than a guess.
///
/// Shape, with details joined by "; ":
///   `<type> '<name>'[ (qualifier)] missing keys: KEY_A, KEY_B`
pub(crate) fn parse_blocked_components(error_message: &str) -> Vec<BlockedComponent> {
    const PREFIX: &str = "Deployment blocked due to missing configuration:";
    let Some(rest) = error_message.split_once(PREFIX).map(|(_, rest)| rest) else {
        return Vec::new();
    };

    let mut components = Vec::new();
    for detail in rest.split("; ") {
        let detail = detail.trim();
        let Some((head, keys)) = detail.split_once("missing keys:") else {
            continue;
        };
        // `<type> '<name>'` — the name is quoted, which is what makes this
        // parseable at all; a component name may contain spaces or hyphens.
        let Some(open) = head.find('\'') else { continue };
        let Some(close) = head[open + 1..].find('\'') else {
            continue;
        };
        let name = head[open + 1..open + 1 + close].to_string();
        let component_type = head[..open].trim().to_string();
        if component_type.is_empty() || name.is_empty() {
            continue;
        }
        let missing_keys: Vec<String> = keys
            .split(',')
            .map(|key| key.trim().trim_end_matches('.').to_string())
            .filter(|key| !key.is_empty())
            .collect();
        if missing_keys.is_empty() {
            continue;
        }
        components.push(BlockedComponent {
            component_type,
            name,
            missing_keys,
        });
    }
    components
}

/// The exact commands that unblock a deploy, so an agent does not have to
/// assemble them from prose. Scoped with `-s`: a key one component needs does
/// not belong in every container's environment.
pub(crate) fn remediation_commands(
    blocked: &[BlockedComponent],
    environment: Option<&str>,
    region: Option<&str>,
) -> Vec<String> {
    let environment = environment.unwrap_or("<environment>");
    let region = region.unwrap_or("<region>");
    blocked
        .iter()
        .flat_map(|component| {
            component.missing_keys.iter().map(move |key| {
                format!(
                    "forklaunch config set {key}=<value> -e {environment} -r {region} -s {}",
                    component.name
                )
            })
        })
        .collect()
}

/// A prompt the user can copy straight into a coding agent.
///
/// A failed deploy leaves a person holding an error and a decision they may not
/// be equipped to make. The commands under "To unblock" say what to run but not
/// what to put in it, and the missing piece is usually knowledge — which value,
/// from where, and whether it is a secret. Handing them a paste-able prompt turns
/// the dead end into a next step, and carries the context (app, environment,
/// region, component, keys) that they would otherwise have to retype from a
/// terminal or a screenshot.
///
/// Deliberately instructs the agent NOT to invent values: the failure mode this
/// prevents is a blocked deploy becoming a crash loop, which is strictly harder
/// to diagnose than the block was.
pub(crate) fn remediation_prompt(
    blocked: &[BlockedComponent],
    environment: Option<&str>,
    region: Option<&str>,
) -> Option<String> {
    if blocked.is_empty() {
        return None;
    }
    let environment = environment.unwrap_or("<environment>");
    let region = region.unwrap_or("<region>");

    let needs = blocked
        .iter()
        .map(|component| {
            format!(
                "{} '{}' needs {}",
                component.component_type,
                component.name,
                component.missing_keys.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");

    Some(format!(
        "My ForkLaunch deploy to {environment} ({region}) was blocked because \
configuration is missing: {needs}. Read the /investigator skill, then run \
`forklaunch deploy info -e {environment} -r {region} --json` to confirm. Help me \
work out the correct value for each key — ask me for anything only I would know, \
and do not invent or guess a value — then set them with `forklaunch config set` \
scoped to the component that needs them, and redeploy."
    ))
}

/// Render the "here is what went wrong and what to do" block shared by
/// `deploy info` and the failure path of a streamed deploy. A person watching a
/// deploy fail should not have to run a second command to find out why.
pub(crate) fn print_remediation(
    out: &mut StandardStream,
    error_message: &str,
    environment: Option<&str>,
    region: Option<&str>,
) -> Result<()> {
    let blocked = parse_blocked_components(error_message);
    if blocked.is_empty() {
        return Ok(());
    }
    let commands = remediation_commands(&blocked, environment, region);

    writeln!(out)?;
    out.set_color(ColorSpec::new().set_bold(true))?;
    writeln!(out, "  To unblock:")?;
    out.reset()?;
    for command in &commands {
        writeln!(out, "    {}", command)?;
    }

    if let Some(prompt) = remediation_prompt(&blocked, environment, region) {
        writeln!(out)?;
        out.set_color(ColorSpec::new().set_bold(true))?;
        writeln!(out, "  Copy this to your coding agent:")?;
        out.reset()?;
        writeln!(out)?;
        // Printed unadorned so it survives a copy: no box drawing, no leading
        // pipes, nothing a selection would drag in.
        for line in wrap_prompt(&prompt, 76) {
            writeln!(out, "    {}", line)?;
        }
    }
    writeln!(out)?;
    Ok(())
}

/// Word-wrap for the copyable block. Kept naive on purpose — the prompt is
/// plain ASCII prose and command names, and a wrapped command still pastes.
fn wrap_prompt(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn print_field(out: &mut StandardStream, label: &str, value: &Option<String>) -> Result<()> {
    if let Some(v) = value {
        out.set_color(ColorSpec::new().set_bold(true))?;
        write!(out, "  {:<12}", label)?;
        out.reset()?;
        writeln!(out, "{}", v)?;
    }
    Ok(())
}

/// Pulumi's generated program exports one `url_<service>` output per service
/// (see the deploy pipeline's Pulumi codegen); the worker persists the raw
/// stack outputs onto `deployment.metadata.outputs` on completion. Surface
/// them here so a service's URL can be looked up after the fact, not just
/// during the one foreground `deploy create` run that happened to produce it.
fn print_service_urls(out: &mut StandardStream, metadata: &Option<serde_json::Value>) -> Result<()> {
    let Some(outputs) = metadata.as_ref().and_then(|m| m.get("outputs")) else {
        return Ok(());
    };
    let Some(outputs) = outputs.as_object() else {
        return Ok(());
    };

    let mut urls: Vec<(&str, &str)> = outputs
        .iter()
        .filter_map(|(key, value)| {
            let service_name = key.strip_prefix("url_")?;
            let url = value.as_str()?;
            Some((service_name, url))
        })
        .collect();
    urls.sort_by_key(|(name, _)| *name);

    if urls.is_empty() {
        return Ok(());
    }

    out.set_color(ColorSpec::new().set_bold(true))?;
    write!(out, "  {:<12}", "URLs:")?;
    out.reset()?;
    writeln!(out)?;
    for (service_name, url) in urls {
        writeln!(out, "    {:<20}  {}", service_name, url)?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) struct InfoCommand;

impl InfoCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for InfoCommand {
    fn command(&self) -> Command {
        command(
            "info",
            "Show deployment status (latest per environment/region, or one by id)",
        )
        .arg(
            Arg::new("deployment")
                .long("deployment")
                .short('d')
                .help("Deployment id to show"),
        )
        .arg(
            Arg::new("environment")
                .long("environment")
                .short('e')
                .help("Filter to an environment"),
        )
        .arg(
            Arg::new("region")
                .long("region")
                .short('r')
                .help("Filter to a region"),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .action(clap::ArgAction::SetTrue)
                .help(
                    "Output raw JSON, including structured `blocked` details and the exact \
                     `config set` commands that would unblock a deploy",
                ),
        )
        .arg(
            Arg::new("base_path")
                .long("path")
                .short('p')
                .help("Path to application root (optional)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let _token = require_auth()?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let app = require_integration(&manifest)?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        // When an explicit deployment id is given, fetch it directly by id so we
        // never miss deployments that fall outside a capped list page.
        let owned: Vec<DeploymentSummary> = match matches.get_one::<String>("deployment") {
            Some(id) => {
                let url = format!(
                    "{}/deployments/{}",
                    get_platform_management_api_url(),
                    id
                );
                let response =
                    http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
                if response.status().as_u16() == 404 {
                    bail!("Deployment '{}' not found.", id);
                }
                if !response.status().is_success() {
                    bail!(
                        "Failed to get deployment: {}",
                        response.text().unwrap_or_default()
                    );
                }
                let deployment: DeploymentSummary = response
                    .json()
                    .with_context(|| "Failed to parse deployment response")?;
                vec![deployment]
            }
            None => {
                let mut url = format!(
                    "{}/deployments/?applicationId={}&limit=25",
                    get_platform_management_api_url(),
                    app
                );
                if let Some(environment) = matches.get_one::<String>("environment") {
                    url.push_str(&format!("&environment={}", environment));
                }
                if let Some(region) = matches.get_one::<String>("region") {
                    url.push_str(&format!("&region={}", region));
                }

                let response =
                    http_client::get(&url).with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;
                if !response.status().is_success() {
                    bail!(
                        "Failed to list deployments: {}",
                        response.text().unwrap_or_default()
                    );
                }
                let list: DeploymentListResponse = response
                    .json()
                    .with_context(|| "Failed to parse deployment list response")?;
                list.deployments.into_iter().take(5).collect()
            }
        };

        if owned.is_empty() {
            log_info!(stdout, "No deployments found for the given filters.");
            return Ok(());
        }

        // Machine-readable output exists for exactly this case: a deploy that
        // failed on missing configuration. The reason is retrievable today, but
        // only as a sentence in a terminal — so an agent asked to fix it has to
        // pattern-match prose. Emitting the parsed components and the commands
        // that resolve them turns "deploy failed, see logs" into something
        // actionable in one call.
        if matches.get_flag("json") {
            let payload: Vec<serde_json::Value> = owned
                .iter()
                .map(|deployment| {
                    let blocked = deployment
                        .error_message
                        .as_deref()
                        .map(parse_blocked_components)
                        .unwrap_or_default();
                    let remediation = remediation_commands(
                        &blocked,
                        deployment.environment.as_deref(),
                        deployment.region.as_deref(),
                    );
                    serde_json::json!({
                        "id": deployment.id,
                        "status": deployment.status,
                        "environment": deployment.environment,
                        "region": deployment.region,
                        "releaseVersion": deployment.release_version,
                        "deployedBy": deployment.deployed_by,
                        "startedAt": deployment.started_at,
                        "completedAt": deployment.completed_at,
                        "errorMessage": deployment.error_message,
                        "blocked": blocked,
                        "remediation": remediation,
                    })
                })
                .collect();
            writeln!(
                stdout,
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({ "deployments": payload }))?
            )?;
            return Ok(());
        }

        for deployment in &owned {
            writeln!(stdout)?;
            print_field(&mut stdout, "Id:", &deployment.id)?;
            print_field(&mut stdout, "Status:", &deployment.status)?;
            print_field(&mut stdout, "Env:", &deployment.environment)?;
            print_field(&mut stdout, "Region:", &deployment.region)?;
            print_field(&mut stdout, "Release:", &deployment.release_version)?;
            print_field(&mut stdout, "By:", &deployment.deployed_by)?;
            print_field(&mut stdout, "Started:", &deployment.started_at)?;
            print_field(&mut stdout, "Completed:", &deployment.completed_at)?;
            print_field(&mut stdout, "Error:", &deployment.error_message)?;
            if let Some(error_message) = deployment.error_message.as_deref() {
                print_remediation(
                    &mut stdout,
                    error_message,
                    deployment.environment.as_deref(),
                    deployment.region.as_deref(),
                )?;
            }
            print_service_urls(&mut stdout, &deployment.metadata)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The exact message behind a real production "Deploy failed" notification.
    /// The email's only call to action is a dashboard button, so this string is
    /// all an agent has to work from.
    const REAL_BLOCKED: &str = "Deployment blocked due to missing configuration: \
         worker 'managed-apps-worker' missing keys: TEMPLATE_BUILD_ORG_ID";

    #[test]
    fn a_real_blocked_deploy_message_yields_the_component_and_its_keys() {
        let blocked = parse_blocked_components(REAL_BLOCKED);
        assert_eq!(
            blocked,
            vec![BlockedComponent {
                component_type: "worker".to_string(),
                name: "managed-apps-worker".to_string(),
                missing_keys: vec!["TEMPLATE_BUILD_ORG_ID".to_string()],
            }]
        );
    }

    #[test]
    fn remediation_is_a_runnable_command_scoped_to_the_component() {
        let blocked = parse_blocked_components(REAL_BLOCKED);
        let commands = remediation_commands(&blocked, Some("production"), Some("us-west-2"));
        assert_eq!(
            commands,
            vec![
                "forklaunch config set TEMPLATE_BUILD_ORG_ID=<value> -e production -r us-west-2 -s managed-apps-worker"
            ]
        );
    }

    #[test]
    fn several_components_and_keys_are_all_recovered() {
        let blocked = parse_blocked_components(
            "Deployment blocked due to missing configuration: \
             worker 'managed-apps-worker' missing keys: A_KEY, B_KEY; \
             service 'billing' missing keys: STRIPE_API_KEY",
        );
        assert_eq!(blocked.len(), 2);
        assert_eq!(blocked[0].missing_keys, vec!["A_KEY", "B_KEY"]);
        assert_eq!(blocked[1].component_type, "service");
        assert_eq!(blocked[1].name, "billing");
    }

    /// The platform appends a parenthetical when a key is set on some other
    /// component. The name is quoted, so the qualifier must not disturb it.
    #[test]
    fn a_set_elsewhere_qualifier_does_not_break_the_name() {
        let blocked = parse_blocked_components(
            "Deployment blocked due to missing configuration: \
             service 'api' (1 of these has a value on worker 'jobs' but not on this service) \
             missing keys: SHARED_KEY",
        );
        assert_eq!(blocked.len(), 1);
        assert_eq!(blocked[0].name, "api");
        assert_eq!(blocked[0].missing_keys, vec!["SHARED_KEY"]);
    }

    /// Fail closed. A deploy that failed for some other reason must produce no
    /// blocked components at all — inventing a key to set would be worse than
    /// saying nothing, because an agent would then go and set it.
    #[test]
    fn an_unrelated_failure_yields_nothing_to_guess_at() {
        assert!(parse_blocked_components("Pulumi stack update failed: timeout").is_empty());
        assert!(parse_blocked_components("").is_empty());
        assert!(
            parse_blocked_components("Deployment blocked due to missing configuration:").is_empty()
        );
    }

    #[test]
    fn the_agent_prompt_carries_the_context_a_user_would_have_to_retype() {
        let blocked = parse_blocked_components(REAL_BLOCKED);
        let prompt = remediation_prompt(&blocked, Some("production"), Some("us-west-2")).unwrap();
        // Everything the agent needs to act without going back to the user.
        assert!(prompt.contains("production"), "{prompt}");
        assert!(prompt.contains("us-west-2"), "{prompt}");
        assert!(prompt.contains("managed-apps-worker"), "{prompt}");
        assert!(prompt.contains("TEMPLATE_BUILD_ORG_ID"), "{prompt}");
        assert!(prompt.contains("deploy info"), "{prompt}");
        assert!(prompt.contains("config set"), "{prompt}");
    }

    /// The single most important line in the prompt. An agent that guesses a
    /// value converts a blocked deploy into a crash loop, which is harder to
    /// diagnose than the block it replaced.
    #[test]
    fn the_agent_prompt_forbids_inventing_a_value() {
        let blocked = parse_blocked_components(REAL_BLOCKED);
        let prompt = remediation_prompt(&blocked, Some("production"), Some("us-west-2")).unwrap();
        assert!(prompt.contains("do not invent"), "{prompt}");
    }

    #[test]
    fn no_blocked_components_means_no_prompt_at_all() {
        // A deploy that failed for some other reason gets no prompt — a generic
        // "ask your agent" would be noise, and would imply a fix that isn't one.
        assert!(remediation_prompt(&[], Some("production"), Some("us-west-2")).is_none());
    }

    #[test]
    fn the_prompt_wraps_without_losing_words() {
        let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
        let wrapped = wrap_prompt(text, 20);
        assert!(wrapped.iter().all(|line| line.len() <= 20), "{wrapped:?}");
        assert_eq!(wrapped.join(" "), text);
    }

    #[test]
    fn missing_environment_and_region_render_as_placeholders_not_omissions() {
        let blocked = parse_blocked_components(REAL_BLOCKED);
        let commands = remediation_commands(&blocked, None, None);
        assert!(commands[0].contains("-e <environment>"), "{:?}", commands);
        assert!(commands[0].contains("-r <region>"), "{:?}", commands);
    }

    #[test]
    fn deployment_summary_deserializes_metadata_outputs() {
        let json = r#"{
            "id": "d1",
            "status": "completed",
            "metadata": {
                "outputs": {
                    "url_iam": "http://alb-1.us-east-1.elb.amazonaws.com",
                    "url_billing": "http://alb-2.us-east-1.elb.amazonaws.com",
                    "vpcId": "vpc-123"
                }
            }
        }"#;
        let summary: DeploymentSummary = serde_json::from_str(json).unwrap();
        let outputs = summary
            .metadata
            .as_ref()
            .and_then(|m| m.get("outputs"))
            .and_then(|o| o.as_object())
            .unwrap();
        assert_eq!(
            outputs.get("url_iam").and_then(|v| v.as_str()),
            Some("http://alb-1.us-east-1.elb.amazonaws.com")
        );
    }

    #[test]
    fn print_service_urls_handles_missing_metadata() {
        let mut stdout = StandardStream::stdout(ColorChoice::Never);
        assert!(print_service_urls(&mut stdout, &None).is_ok());
    }

    #[test]
    fn print_service_urls_handles_metadata_with_no_outputs() {
        let mut stdout = StandardStream::stdout(ColorChoice::Never);
        let metadata = Some(json!({ "resources": [] }));
        assert!(print_service_urls(&mut stdout, &metadata).is_ok());
    }

    #[test]
    fn print_service_urls_filters_non_url_output_keys() {
        let mut stdout = StandardStream::stdout(ColorChoice::Never);
        let metadata = Some(json!({
            "outputs": {
                "url_iam": "http://alb.example.com",
                "vpcId": "vpc-123",
                "url_billing": "http://alb2.example.com"
            }
        }));
        assert!(print_service_urls(&mut stdout, &metadata).is_ok());
    }
}

#[cfg(test)]
mod render_preview {
    use super::*;
    use termcolor::{ColorChoice, StandardStream};

    /// Not an assertion — prints the block so its shape can be reviewed with
    /// `cargo test render_the_block -- --nocapture`. The value of this output is
    /// whether a person can select and paste it, which no assertion checks.
    #[test]
    fn render_the_block() {
        let mut out = StandardStream::stdout(ColorChoice::Never);
        print_remediation(
            &mut out,
            "Deployment blocked due to missing configuration: worker 'managed-apps-worker' missing keys: TEMPLATE_BUILD_ORG_ID",
            Some("production"),
            Some("us-west-2"),
        )
        .unwrap();
    }
}

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgMatches, Command};
use dialoguer::{Confirm, Select, theme::ColorfulTheme};
use serde::{Deserialize, Serialize};
use serde_json;
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_SEND_REQUEST, get_platform_management_api_url, get_platform_ui_url},
    core::{
        command::command,
        env::extract_env_value,
        env_scope::is_pulumi_injected,
        http_client,
        validate::{require_active_account, require_integration, require_manifest, resolve_auth},
    },
    deploy::utils::stream_deployment_status,
};

#[derive(Debug, Serialize)]
struct CreateDeploymentRequest {
    #[serde(rename = "applicationId")]
    application_id: String,
    #[serde(rename = "releaseVersion")]
    release_version: String,
    environment: String,
    region: String,
    #[serde(rename = "distributionConfig")]
    distribution_config: Option<String>,
    #[serde(rename = "forceRefresh")]
    #[serde(skip_serializing_if = "Option::is_none")]
    force_refresh: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateDeploymentResponse {
    id: String,
    #[allow(dead_code)]
    status: String,
}

#[derive(Debug, Deserialize)]
struct DeploymentBlockedError {
    message: String,
    details: Vec<DeploymentErrorDetail>,
}

#[derive(Debug, Deserialize)]
struct DeploymentErrorDetail {
    #[serde(rename = "type")]
    component_type: String,
    id: String,
    name: String,
    #[serde(rename = "missingKeys")]
    missing_keys: Vec<MissingKey>,
}

#[derive(Debug, Deserialize, Clone)]
struct MissingKey {
    #[serde(rename = "key")]
    name: String,
    component: Option<ComponentMetadata>,
    #[serde(rename = "defaultValue")]
    default_value: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ComponentMetadata {
    #[serde(rename = "type")]
    component_type: String,
    property: String,
    passthrough: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct DeploymentPreviewResponse {
    #[serde(rename = "applicationId")]
    application_id: String,
    #[serde(rename = "releaseVersion")]
    release_version: String,
    environment: String,
    region: String,
    variables: Vec<PreviewVariable>,
    summary: PreviewSummary,
    components: Option<Vec<PreviewComponent>>,
    #[serde(rename = "componentVariables")]
    component_variables: Option<Vec<PreviewComponentVariables>>,
}

#[derive(Debug, Deserialize)]
struct PreviewVariable {
    key: String,
    value: String,
    scope: String,
    #[serde(rename = "scopeId")]
    scope_id: Option<String>,
    source: String,
    #[allow(dead_code)]
    component: Option<ComponentMetadata>,
}

#[derive(Debug, Deserialize)]
struct PreviewSummary {
    total: u32,
    resolved: u32,
    existing: u32,
    empty: u32,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PreviewComponent {
    name: String,
    #[serde(rename = "type")]
    component_type: String,
    #[serde(rename = "instanceSize")]
    instance_size: Option<String>,
    replicas: Option<u32>,
    #[serde(rename = "runtimeDependencies")]
    runtime_dependencies: Option<Vec<String>>,
    #[serde(rename = "workerType")]
    worker_type: Option<String>,
    concurrency: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PreviewComponentVariable {
    key: String,
    value: String,
    source: String,
}

#[derive(Debug, Deserialize)]
struct PreviewComponentVariables {
    name: String,
    #[serde(rename = "type")]
    component_type: String,
    variables: Vec<PreviewComponentVariable>,
}

#[derive(Debug, Serialize)]
struct UpdateEnvironmentVariablesRequest {
    region: String,
    variables: Vec<EnvironmentVariableUpdate>,
}

#[derive(Debug, Serialize, Clone)]
struct EnvironmentVariableUpdate {
    key: String,
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    component: Option<ComponentMetadata>,
    #[serde(rename = "isUnset")]
    #[serde(skip_serializing_if = "Option::is_none")]
    is_unset: Option<bool>,
}

#[derive(Debug, Serialize)]
struct UpdateApplicationVariablesRequest {
    region: String,
    variables: Vec<EnvironmentVariableCreation>,
}

#[derive(Debug, Serialize)]
struct EnvironmentVariableCreation {
    key: String,
    value: String,
    source: String,
    required: bool,
    #[serde(rename = "hasValue")]
    has_value: bool,
    #[serde(rename = "isUnset")]
    #[serde(skip_serializing_if = "Option::is_none")]
    is_unset: Option<bool>,
}


/// Get a default value for a missing key from defaultValue or passthrough
fn get_default_value(key: &MissingKey) -> Option<&str> {
    key.default_value
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            key.component
                .as_ref()
                .and_then(|c| c.passthrough.as_deref())
                .filter(|v| !v.trim().is_empty())
        })
}

fn hint_for_key(key: &MissingKey) -> String {
    if let Some(ref comp) = key.component {
        match comp.property.as_str() {
            "port" => " (hint: port number 0–65535)".to_string(),
            "base64-bytes-32" => " (hint: base64-encoded 32-byte value)".to_string(),
            "base64-bytes-64" => " (hint: base64-encoded 64-byte value)".to_string(),
            other if !other.is_empty() => format!(" (hint: {})", other),
            _ => String::new(),
        }
    } else {
        String::new()
    }
}

fn write_missing_vars_template(
    blocked_error: &DeploymentBlockedError,
    environment: &str,
    region: &str,
    release_version: &str,
    application_id: &str,
    app_var_keys: &HashSet<String>,
    existing_config: &HashMap<String, String>,
) -> Result<PathBuf> {
    let temp_path = std::env::temp_dir()
        .join(format!("forklaunch-env-{}-{}.env", environment, region));

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");

    let mut content = format!(
        "# ═══════════════════════════════════════════════════════════════════\n\
         # FORKLAUNCH DEPLOY — Missing Environment Variables\n\
         # ═══════════════════════════════════════════════════════════════════\n\
         # Deployment: {} -> {} ({})\n\
         # Generated: {}\n\
         #\n\
         # HOW TO USE:\n\
         #   1. Fill in ALL values marked with ⚠ below\n\
         #   2. Pre-filled values are already set — edit only if needed\n\
         #   3. Leave a value empty or set to NONE to keep it unset\n\
         #   4. Save the file, then press Enter in the terminal to continue\n\
         #\n\
         # VALUE FORMAT:\n\
         #   • Simple:    MY_VAR=somevalue\n\
         #   • Quoted:    MY_VAR=\"value with spaces\"\n\
         #   • Lines starting with # are comments (ignored)\n\
         #   • Do not modify lines starting with #@ (machine-readable metadata)\n\
         # ═══════════════════════════════════════════════════════════════════\n\n",
        release_version, environment, region, now
    );

    // --- APPLICATION section (shared vars, shown first) ---
    if !app_var_keys.is_empty() {
        // Collect in first-seen order across all details
        let mut seen = HashSet::new();
        let mut ordered: Vec<&MissingKey> = Vec::new();
        for detail in &blocked_error.details {
            for key in &detail.missing_keys {
                if app_var_keys.contains(&key.name) && !seen.contains(&key.name) {
                    seen.insert(key.name.clone());
                    ordered.push(key);
                }
            }
        }

        let needs_config: Vec<&&MissingKey> = ordered.iter()
            .filter(|k| !existing_config.contains_key(&k.name) && get_default_value(k).is_none())
            .collect();
        let has_default: Vec<&&MissingKey> = ordered.iter()
            .filter(|k| !existing_config.contains_key(&k.name) && get_default_value(k).is_some())
            .collect();
        let already_set: Vec<&&MissingKey> = ordered.iter()
            .filter(|k| existing_config.contains_key(&k.name))
            .collect();

        let mut status_parts = Vec::new();
        if !needs_config.is_empty() {
            status_parts.push(format!("{} var(s) need configuration", needs_config.len()));
        }
        if !has_default.is_empty() {
            status_parts.push(format!("{} var(s) pre-filled", has_default.len()));
        }
        if !already_set.is_empty() {
            status_parts.push(format!("{} var(s) already set", already_set.len()));
        }
        let status_line = status_parts.join(", ");

        content.push_str(&format!(
            "#@type=application\n\
             #@id={}\n\
             #@name=Application\n\
             # ══════════════════════════════════════════════════════\n\
             # [APPLICATION] Shared across all components — {}\n\
             # ══════════════════════════════════════════════════════\n",
            application_id, status_line
        ));

        for key in &ordered {
            if let Some(existing_val) = existing_config.get(&key.name) {
                content.push_str(&format!("{}={}\n", key.name, existing_val));
            } else if let Some(default_val) = get_default_value(key) {
                content.push_str(&format!("{}={} # default — edit if needed\n", key.name, default_val));
            } else {
                let hint = hint_for_key(key);
                content.push_str(&format!("{}= # ⚠ NEEDS CONFIGURATION{}\n", key.name, hint));
            }
        }
        content.push('\n');
    }

    // --- Per-component sections (only vars not already covered by application scope) ---
    for detail in &blocked_error.details {
        if detail.component_type == "application" {
            continue;
        }

        let component_keys: Vec<&MissingKey> = detail.missing_keys.iter()
            .filter(|k| !app_var_keys.contains(&k.name))
            .collect();

        // Include vars that need a value or have a default to review
        let actionable: Vec<&&MissingKey> = component_keys.iter()
            .filter(|k| !existing_config.contains_key(&k.name))
            .collect();

        if actionable.is_empty() {
            continue;
        }

        let needs_config_count = actionable.iter()
            .filter(|k| get_default_value(k).is_none())
            .count();
        let has_default_count = actionable.len() - needs_config_count;

        let type_label = detail.component_type.to_uppercase();
        let mut comp_status_parts = Vec::new();
        if needs_config_count > 0 {
            comp_status_parts.push(format!("{} var(s) need configuration", needs_config_count));
        }
        if has_default_count > 0 {
            comp_status_parts.push(format!("{} var(s) pre-filled", has_default_count));
        }
        let comp_status = comp_status_parts.join(", ");

        content.push_str(&format!(
            "#@type={}\n\
             #@id={}\n\
             #@name={}\n\
             # ══════════════════════════════════════════════════════\n\
             # [{}] {} — {}\n\
             # ══════════════════════════════════════════════════════\n",
            detail.component_type, detail.id, detail.name,
            type_label, detail.name, comp_status
        ));

        for key in actionable {
            if let Some(default_val) = get_default_value(key) {
                content.push_str(&format!("{}={} # default — edit if needed\n", key.name, default_val));
            } else {
                let hint = hint_for_key(key);
                content.push_str(&format!("{}= # ⚠ NEEDS CONFIGURATION{}\n", key.name, hint));
            }
        }
        content.push('\n');
    }

    std::fs::write(&temp_path, &content)
        .with_context(|| format!("Failed to write env template to {}", temp_path.display()))?;

    Ok(temp_path)
}

/// Parses the env template file and returns:
/// - a map of key → value for all non-empty entries
/// - a list of keys marked as unset (empty value or value set to "NONE")
fn parse_env_template(
    path: &Path,
) -> Result<(HashMap<String, String>, Vec<String>)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read env template from {}", path.display()))?;

    let lines: Vec<&str> = content.lines().collect();
    let mut parsed: HashMap<String, String> = HashMap::new();
    let mut unset_keys: Vec<String> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim().to_string();
            let rest = line[eq_pos + 1..].trim();

            if !key.is_empty() {
                let value = extract_env_value(&lines, &mut i, rest);
                if value.is_empty() || value.eq_ignore_ascii_case("none") {
                    unset_keys.push(key);
                } else {
                    parsed.insert(key, value);
                }
            }
        }

        i += 1;
    }

    Ok((parsed, unset_keys))
}

fn collect_app_var_keys(blocked_error: &DeploymentBlockedError) -> HashSet<String> {
    let mut app_var_keys = HashSet::new();

    // Explicitly application-type details from the API
    for detail in &blocked_error.details {
        if detail.component_type == "application" {
            for key in &detail.missing_keys {
                app_var_keys.insert(key.name.clone());
            }
        }
    }

    // Keys appearing in 2+ component details are application-scoped
    let mut key_counts: HashMap<String, u32> = HashMap::new();
    for detail in &blocked_error.details {
        if detail.component_type != "application" {
            for key in &detail.missing_keys {
                *key_counts.entry(key.name.clone()).or_insert(0) += 1;
            }
        }
    }
    for (key, count) in key_counts {
        if count > 1 {
            app_var_keys.insert(key);
        }
    }

    app_var_keys
}


#[derive(Debug)]
pub(crate) struct CreateCommand;

impl CreateCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for CreateCommand {
    fn command(&self) -> Command {
        command("create", "Create a new deployment")
            .arg(
                Arg::new("release")
                    .long("release")
                    .short('r')
                    .required(true)
                    .help("Release version to deploy"),
            )
            .arg(
                Arg::new("environment")
                    .long("environment")
                    .short('e')
                    .required(true)
                    .help("Environment name (e.g., staging, production)"),
            )
            .arg(
                Arg::new("region")
                    .long("region")
                    .required(true)
                    .help("AWS region (e.g., us-east-1)"),
            )
            .arg(
                Arg::new("distribution_config")
                    .long("distribution-config")
                    .help("Distribution strategy (centralized or distributed)"),
            )
            .arg(
                Arg::new("base_path")
                    .long("path")
                    .short('p')
                    .help("Path to application root (optional)"),
            )
            .arg(
                Arg::new("no-wait")
                    .long("no-wait")
                    .action(clap::ArgAction::SetTrue)
                    .help("Don't wait for deployment to complete"),
            )
            .arg(
                Arg::new("dry-run")
                    .long("dry-run")
                    .action(clap::ArgAction::SetTrue)
                    .help("Preview deployment without executing it"),
            )
            .arg(
                Arg::new("full")
                    .long("full")
                    .action(clap::ArgAction::SetTrue)
                    .help("Force a full deployment with Pulumi state refresh"),
            )
            .arg(
                Arg::new("node-env")
                    .long("node-env")
                    .value_parser(["production", "development"])
                    .help("NODE_ENV for this deployment (production or development)"),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let auth_mode = resolve_auth()?;
        require_active_account(&auth_mode)?;
        let (_app_root, manifest) = require_manifest(matches)?;
        let application_id = require_integration(&manifest)?;

        let release_version = matches
            .get_one::<String>("release")
            .ok_or_else(|| anyhow::anyhow!("Release version is required"))?;

        let environment = matches
            .get_one::<String>("environment")
            .ok_or_else(|| anyhow::anyhow!("Environment is required"))?
            .to_lowercase();

        let region = matches
            .get_one::<String>("region")
            .ok_or_else(|| anyhow::anyhow!("Region is required"))?;

        let wait = !matches.get_flag("no-wait");
        let dry_run = matches.get_flag("dry-run");

        let config_pull_url = format!(
            "{}/config/pull?applicationId={}&region={}&environment={}",
            get_platform_management_api_url(),
            application_id,
            region,
            environment
        );
        // Fetch all existing non-empty config vars upfront — used for NODE_ENV detection
        // and to avoid re-prompting for vars already set during deploy
        let existing_config: HashMap<String, String> =
            http_client::get_with_auth(&auth_mode, &config_pull_url)
                .ok()
                .and_then(|r| if r.status().is_success() { r.text().ok() } else { None })
                .map(|text| {
                    text.lines()
                        .filter_map(|line| {
                            let line = line.trim();
                            if line.starts_with('#') || line.is_empty() { return None; }
                            let (key, val) = line.split_once('=')?;
                            let val = val.trim().trim_matches('"').trim_matches('\'').to_string();
                            if !val.is_empty() { Some((key.trim().to_string(), val)) } else { None }
                        })
                        .collect()
                })
                .unwrap_or_default();
        let existing_node_env = existing_config.get("NODE_ENV").cloned();

        let node_env = if let Some(flag) = matches.get_one::<String>("node-env") {
            Some(flag.clone())
        } else if existing_node_env.is_some() {
            None
        } else if !dry_run && !auth_mode.is_hmac() {
            let options = ["production", "development"];
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Is this a production or development deployment?")
                .items(&options)
                .default(0)
                .interact()
                .with_context(|| "Failed to read NODE_ENV selection")?;
            Some(options[selection].to_string())
        } else {
            Some("production".to_string())
        };

        if !dry_run {
            if let Some(ref node_env_value) = node_env {
                let node_env_url = format!(
                    "{}/applications/{}/environments/{}/variables",
                    get_platform_management_api_url(),
                    application_id,
                    environment
                );
                let node_env_body = UpdateApplicationVariablesRequest {
                    region: region.clone(),
                    variables: vec![EnvironmentVariableCreation {
                        key: "NODE_ENV".to_string(),
                        value: node_env_value.clone(),
                        source: "application".to_string(),
                        required: false,
                        has_value: true,
                        is_unset: None,
                    }],
                };
                log_progress!(stdout, "[INFO] Setting NODE_ENV={}...", node_env_value);
                let node_env_response = http_client::put_with_auth(
                    &auth_mode,
                    &node_env_url,
                    serde_json::to_value(&node_env_body)?,
                )
                .with_context(|| "Failed to set NODE_ENV")?;
                if !node_env_response.status().is_success() {
                    log_error_suffix!(stdout);
                    bail!(
                        "Failed to set NODE_ENV: {}",
                        node_env_response.text().unwrap_or_default()
                    );
                }
                log_ok_suffix!(stdout);
                writeln!(stdout)?;
            }
        }

        let request_body = CreateDeploymentRequest {
            application_id: application_id.clone(),
            release_version: release_version.clone(),
            environment: environment.clone(),
            region: region.clone(),
            distribution_config: Some(
                matches
                    .get_one::<String>("distribution_config")
                    .cloned()
                    .unwrap_or_else(|| "centralized".to_string()),
            ),
            force_refresh: if matches.get_flag("full") { Some(true) } else { None },
        };

        if dry_run {
            log_header!(stdout, Color::Cyan, "Deployment Preview: {} -> {} ({})",
                release_version, environment, region
            );
            writeln!(stdout)?;

            let preview_url = if auth_mode.is_hmac() {
                format!("{}/deployments/internal/preview", get_platform_management_api_url())
            } else {
                format!("{}/deployments/preview", get_platform_management_api_url())
            };

            let response = http_client::post_with_auth(
                &auth_mode,
                &preview_url,
                serde_json::to_value(&request_body)?,
            )
            .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

            if !response.status().is_success() {
                let error_text = response.text().unwrap_or_else(|_| "Unknown error".to_string());
                log_error!(stdout, "[ERROR] Preview failed: {}", error_text);
                bail!("Failed to preview deployment: {}", error_text);
            }

            let response_text = response.text().with_context(|| "Failed to read response")?;
            let preview: DeploymentPreviewResponse = serde_json::from_str(&response_text)
                .with_context(|| format!("Failed to parse preview response: {}", response_text))?;

            if let Some(ref components) = preview.components {
                log_header!(stdout, Color::White, "Components:");
                for comp in components {
                    let size = comp.instance_size.as_deref().unwrap_or("default");
                    let replicas = comp.replicas.unwrap_or(1);
                    let deps = comp.runtime_dependencies.as_ref()
                        .map(|d| d.join(", "))
                        .unwrap_or_default();

                    match comp.component_type.as_str() {
                        "worker" => {
                            let wt = comp.worker_type.as_deref().unwrap_or("unknown");
                            let conc = comp.concurrency.map(|c| format!(", concurrency: {}", c)).unwrap_or_default();
                            log_info!(stdout, "  [worker] {} — size: {}, replicas: {}, type: {}{}",
                                comp.name, size, replicas, wt, conc);
                        }
                        _ => {
                            log_info!(stdout, "  [service] {} — size: {}, replicas: {}",
                                comp.name, size, replicas);
                        }
                    }

                    if !deps.is_empty() {
                        log_info!(stdout, "    dependencies: {}", deps);
                    }
                }
                writeln!(stdout)?;
            }

            log_header!(stdout, Color::White, "Environment Variables: {} total", preview.summary.total);
            log_ok!(stdout, "  Resolved: {} (existing: {}, new: {})",
                preview.summary.resolved + preview.summary.existing,
                preview.summary.existing,
                preview.summary.resolved
            );

            if preview.summary.empty > 0 {
                log_warn!(stdout, "  Empty: {}", preview.summary.empty);
            }

            let temp_dir = std::env::temp_dir();
            let filename = format!(
                "forklaunch-deploy-preview-{}-{}-{}.env",
                environment, region, release_version
            );
            let temp_path = temp_dir.join(&filename);

            let mut file_content = String::new();
            file_content.push_str(&format!(
                "# Deployment Preview: {} -> {} ({})\n# Release: {}\n# Generated: {}\n#\n# Sources:\n#   [infrastructure]    = injected by Pulumi at deploy time (placeholder shown)\n#   [application]       = stored application-scoped variable\n#   [component]         = stored component-scoped variable\n#   [manifest-resolved] = resolved from manifest (key material, platform vars)\n#   [empty]             = NEEDS CONFIGURATION before deploying\n\n",
                release_version, environment, region, release_version,
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
            ));

            if let Some(ref comp_vars) = preview.component_variables {
                let components_ref = preview.components.as_ref();

                for cv in comp_vars {
                    let comp_meta = components_ref.and_then(|comps| {
                        comps.iter().find(|c| c.name == cv.name)
                    });

                    let type_label = cv.component_type.to_uppercase();
                    let meta_line = if let Some(meta) = comp_meta {
                        let size = meta.instance_size.as_deref().unwrap_or("default");
                        let replicas = meta.replicas.unwrap_or(1);
                        let deps = meta.runtime_dependencies.as_ref()
                            .map(|d| d.join(", "))
                            .unwrap_or_else(|| "none".to_string());
                        match cv.component_type.as_str() {
                            "worker" => {
                                let wt = meta.worker_type.as_deref().unwrap_or("unknown");
                                let conc = meta.concurrency.map(|c| format!(", concurrency={}", c)).unwrap_or_default();
                                format!(" — size={}, replicas={}, type={}{}, deps=[{}]", size, replicas, wt, conc, deps)
                            }
                            _ => {
                                format!(" — size={}, replicas={}, deps=[{}]", size, replicas, deps)
                            }
                        }
                    } else {
                        String::new()
                    };

                    file_content.push_str(&format!(
                        "# ══════════════════════════════════════════════════════\n# [{}] {}{}\n# ══════════════════════════════════════════════════════\n",
                        type_label, cv.name, meta_line
                    ));

                    let infra_count = cv.variables.iter().filter(|v| v.source == "infrastructure").count();
                    let app_count = cv.variables.iter().filter(|v| v.source == "application").count();
                    let comp_count = cv.variables.iter().filter(|v| v.source == "component").count();
                    let resolved_count = cv.variables.iter().filter(|v| v.source == "manifest-resolved").count();
                    let empty_count = cv.variables.iter().filter(|v| v.source == "empty").count();

                    file_content.push_str(&format!(
                        "# {} total: {} infrastructure, {} application, {} component, {} manifest-resolved, {} empty\n#\n",
                        cv.variables.len(), infra_count, app_count, comp_count, resolved_count, empty_count
                    ));

                    let mut current_source = String::new();
                    for var in &cv.variables {
                        if var.source != current_source {
                            if !current_source.is_empty() {
                                file_content.push('\n');
                            }
                            current_source = var.source.clone();
                            file_content.push_str(&format!("# --- {} ---\n", current_source));
                        }

                        let source_tag = match var.source.as_str() {
                            "empty" => " # ⚠ NEEDS CONFIGURATION",
                            _ => "",
                        };
                        file_content.push_str(&format!("{}={}{}\n", var.key, var.value, source_tag));
                    }
                    file_content.push_str("\n\n");
                }
            } else {
                let mut grouped: BTreeMap<String, Vec<&PreviewVariable>> = BTreeMap::new();
                for var in &preview.variables {
                    let group_key = if var.scope == "application" {
                        "APPLICATION".to_string()
                    } else {
                        format!("{}:{}", var.scope.to_uppercase(), var.scope_id.as_deref().unwrap_or("unknown"))
                    };
                    grouped.entry(group_key).or_default().push(var);
                }

                for (scope_label, vars) in &grouped {
                    file_content.push_str(&format!("# ── {} ──\n", scope_label));
                    for var in vars {
                        let source_tag = match var.source.as_str() {
                            "empty" => " # [empty] NEEDS CONFIGURATION",
                            "existing" => " # [existing]",
                            "resolved" => " # [resolved]",
                            other => { file_content.push_str(&format!(" # [{}]", other)); "" }
                        };
                        file_content.push_str(&format!("{}={}{}\n", var.key, var.value, source_tag));
                    }
                    file_content.push('\n');
                }

                if let Some(ref components) = preview.components {
                    file_content.push_str("# ── RESOURCE SUMMARY ──\n");
                    for comp in components {
                        let size = comp.instance_size.as_deref().unwrap_or("default");
                        let replicas = comp.replicas.unwrap_or(1);
                        let deps = comp.runtime_dependencies.as_ref()
                            .map(|d| d.join(", "))
                            .unwrap_or_else(|| "none".to_string());

                        match comp.component_type.as_str() {
                            "worker" => {
                                let wt = comp.worker_type.as_deref().unwrap_or("unknown");
                                let conc = comp.concurrency.map(|c| format!(", concurrency={}", c)).unwrap_or_default();
                                file_content.push_str(&format!(
                                    "# [worker] {} — size={}, replicas={}, type={}{}, deps=[{}]\n",
                                    comp.name, size, replicas, wt, conc, deps
                                ));
                            }
                            _ => {
                                file_content.push_str(&format!(
                                    "# [service] {} — size={}, replicas={}, deps=[{}]\n",
                                    comp.name, size, replicas, deps
                                ));
                            }
                        }
                    }
                    file_content.push('\n');
                }
            }

            std::fs::write(&temp_path, &file_content)
                .with_context(|| format!("Failed to write preview file to {}", temp_path.display()))?;

            writeln!(stdout)?;
            log_ok!(stdout, "[OK] Full preview written to: {}", temp_path.display());

            writeln!(stdout)?;
            log_info!(stdout, "[INFO] This was a dry run. No deployment was created.");

            return Ok(());
        }

        log_header!(stdout, Color::Cyan, "Creating deployment: {} -> {} ({})",
            release_version, environment, region
        );
        writeln!(stdout)?;

        let url = if auth_mode.is_hmac() {
            format!("{}/deployments/internal", get_platform_management_api_url())
        } else {
            format!("{}/deployments", get_platform_management_api_url())
        };

        let mut retry_count = 0;
        const MAX_RETRIES: u32 = 3;

        loop {
            log_progress!(stdout, "[INFO] Triggering deployment...");

            let response =
                http_client::post_with_auth(&auth_mode, &url, serde_json::to_value(&request_body)?)
                    .with_context(|| ERROR_FAILED_TO_SEND_REQUEST)?;

            let status = response.status();

            if status.is_success() {
                let response_text = response.text().with_context(|| "Failed to read response")?;
                let deployment: CreateDeploymentResponse = serde_json::from_str(&response_text)
                    .with_context(|| {
                        format!("Failed to parse deployment response: {}", response_text)
                    })?;

                log_ok_suffix!(stdout);
                writeln!(stdout, "[INFO] Deployment ID: {}", deployment.id)?;

                if wait {
                    writeln!(stdout)?;
                    stream_deployment_status(
                        &auth_mode,
                        &deployment.id,
                        &mut stdout,
                    )?;
                } else {
                    writeln!(stdout)?;
                    log_info!(stdout, "[INFO] Deployment started. Check status at:");
                    writeln!(
                        stdout,
                        "  {}/apps/{}/deployments/{}",
                        get_platform_ui_url(), application_id, deployment.id
                    )?;
                }
                break;
            } else if status.as_u16() == 400 {
                let error_text = response.text().unwrap_or_default();

                if let Ok(blocked_error) =
                    serde_json::from_str::<DeploymentBlockedError>(&error_text)
                {
                    retry_count += 1;
                    if retry_count > MAX_RETRIES {
                        log_error_suffix!(stdout);
                        bail!(
                            "Deployment failed after {} retries. Please check your environment variable configuration and try again.",
                            MAX_RETRIES
                        );
                    }

                    // Filter out Pulumi-injected vars (inter-service URLs, auth URLs)
                    // that the platform will auto-inject at deploy time
                    let project_names: Vec<String> = manifest.projects.iter().map(|p| p.name.clone()).collect();
                    let mut blocked_error = blocked_error;
                    for detail in &mut blocked_error.details {
                        detail.missing_keys.retain(|k| {
                            !is_pulumi_injected(&k.name, &project_names)
                        });
                    }
                    blocked_error.details.retain(|d| !d.missing_keys.is_empty());

                    // If all vars were Pulumi-injected, nothing left for the user
                    if blocked_error.details.is_empty() {
                        log_info!(stdout, " [OK]");
                        writeln!(stdout)?;
                        log_info!(stdout, "[INFO] All missing variables are Pulumi-injected — retrying deployment...");
                        writeln!(stdout)?;
                        retry_count += 1;
                        continue;
                    }

                    if auth_mode.is_hmac() {
                        log_error_suffix!(stdout);

                        writeln!(stdout)?;
                        log_error!(stdout, "[ERROR] Deployment blocked: {}",
                            blocked_error.message
                        );

                        for detail in &blocked_error.details {
                            writeln!(
                                stdout,
                                "  {} '{}': missing keys: {}",
                                detail.component_type,
                                detail.name,
                                detail
                                    .missing_keys
                                    .iter()
                                    .map(|k| k.name.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )?;
                        }

                        bail!(
                            "Deployment blocked due to missing environment variables. Set them via the platform UI or API before retrying."
                        );
                    }

                    log_warn!(stdout, " [WARNING]");
                    writeln!(stdout)?;
                    log_warn!(stdout, "[WARNING] Deployment blocked — missing environment variables");
                    writeln!(stdout)?;

                    let app_var_keys = collect_app_var_keys(&blocked_error);

                    // Print grouped counts matching the file structure
                    {
                        let mut seen = HashSet::new();
                        let app_missing = blocked_error.details.iter()
                            .flat_map(|d| &d.missing_keys)
                            .filter(|k| app_var_keys.contains(&k.name)
                                && seen.insert(k.name.clone())
                                && !existing_config.contains_key(&k.name))
                            .count();
                        if app_missing > 0 {
                            log_warn!(stdout, "  [APPLICATION] {} shared variable(s) need configuration", app_missing);
                        }
                        for detail in &blocked_error.details {
                            if detail.component_type == "application" { continue; }
                            let missing = detail.missing_keys.iter()
                                .filter(|k| !app_var_keys.contains(&k.name)
                                    && !existing_config.contains_key(&k.name))
                                .count();
                            if missing == 0 { continue; }
                            log_warn!(stdout, "  [{}] {} — {} variable(s) need configuration",
                                detail.component_type.to_uppercase(), detail.name, missing);
                        }
                    }
                    writeln!(stdout)?;

                    let temp_path = write_missing_vars_template(
                        &blocked_error, &environment, region, release_version,
                        &application_id, &app_var_keys, &existing_config,
                    )?;

                    log_info!(stdout, "[INFO] Fill in the required variables, then press Enter to continue:");
                    writeln!(stdout, "  {}", temp_path.display())?;
                    writeln!(stdout)?;

                    // Wait for user to edit, then parse and validate
                    let final_unset_keys: Vec<String>;
                    let collected: HashMap<String, String> = loop {
                        let mut _enter = String::new();
                        std::io::stdin().read_line(&mut _enter)?;

                        let (mut parsed, unset_keys) = parse_env_template(&temp_path)?;

                        // Confirm unset vars with the user before proceeding
                        if !unset_keys.is_empty() {
                            writeln!(stdout)?;
                            log_info!(stdout, "[INFO] The following variables will be marked as UNSET (not sent to deployment):");
                            for var in &unset_keys {
                                writeln!(stdout, "  • {}", var)?;
                            }
                            writeln!(stdout)?;

                            let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                                .with_prompt("Continue with these variables unset?")
                                .default(true)
                                .interact()?;

                            if !confirmed {
                                log_info!(stdout, "[INFO] Edit the file and press Enter to continue:");
                                writeln!(stdout, "  {}", temp_path.display())?;
                                writeln!(stdout)?;
                                continue;
                            }
                        }

                        final_unset_keys = unset_keys;
                        // Merge in existing_config for vars not in the file
                        for (k, v) in &existing_config {
                            parsed.entry(k.clone()).or_insert_with(|| v.clone());
                        }
                        break parsed;
                    };
                    let unset_set: HashSet<String> = final_unset_keys.into_iter().collect();

                    // --- Submit phase ---
                    // Post app-scoped vars to the application endpoint
                    let mut app_vars: Vec<EnvironmentVariableCreation> = app_var_keys.iter()
                        .filter_map(|k| {
                            if unset_set.contains(k) {
                                return None; // handled separately below
                            }
                            collected.get(k).map(|v| EnvironmentVariableCreation {
                                key: k.clone(),
                                value: v.clone(),
                                source: "application".to_string(),
                                required: false,
                                has_value: true,
                                is_unset: None,
                            })
                        })
                        .collect();
                    // Add unset app-scoped vars
                    for k in &unset_set {
                        if app_var_keys.contains(k) {
                            app_vars.push(EnvironmentVariableCreation {
                                key: k.clone(),
                                value: String::new(),
                                source: "application".to_string(),
                                required: false,
                                has_value: false,
                                is_unset: Some(true),
                            });
                        }
                    }
                    if !app_vars.is_empty() {
                        let update_url = format!(
                            "{}/applications/{}/environments/{}/variables",
                            get_platform_management_api_url(), application_id, environment
                        );
                        let update_body = UpdateApplicationVariablesRequest {
                            region: region.clone(),
                            variables: app_vars,
                        };
                        log_progress!(stdout, "[INFO] Saving application variables...");
                        let resp = http_client::put_with_auth(&auth_mode, &update_url, serde_json::to_value(&update_body)?)
                            .with_context(|| "Failed to save application environment variables")?;
                        if !resp.status().is_success() {
                            log_error_suffix!(stdout);
                            bail!("Failed to save application variables: {}", resp.text().unwrap_or_default());
                        }
                        log_ok_suffix!(stdout);
                    }

                    // Post every component's full set of missing vars (app-scoped + component-scoped)
                    // so the per-component deploy check passes
                    for detail in &blocked_error.details {
                        if detail.component_type == "application" {
                            continue;
                        }
                        let mut vars: Vec<EnvironmentVariableUpdate> = detail.missing_keys.iter()
                            .filter(|k| !app_var_keys.contains(&k.name) && !unset_set.contains(&k.name))
                            .filter_map(|k| collected.get(&k.name).map(|v| EnvironmentVariableUpdate {
                                key: k.name.clone(),
                                value: v.clone(),
                                component: k.component.clone(),
                                is_unset: None,
                            }))
                            .collect();
                        // Add unset component-scoped vars
                        for k in &detail.missing_keys {
                            if !app_var_keys.contains(&k.name) && unset_set.contains(&k.name) {
                                vars.push(EnvironmentVariableUpdate {
                                    key: k.name.clone(),
                                    value: String::new(),
                                    component: k.component.clone(),
                                    is_unset: Some(true),
                                });
                            }
                        }
                        if vars.is_empty() {
                            continue;
                        }
                        let update_url = if detail.component_type == "worker" {
                            format!("{}/workers/{}/environments/{}/variables", get_platform_management_api_url(), detail.id, environment)
                        } else {
                            format!("{}/services/{}/environments/{}/variables", get_platform_management_api_url(), detail.id, environment)
                        };
                        let update_body = UpdateEnvironmentVariablesRequest {
                            region: region.clone(),
                            variables: vars,
                        };
                        log_progress!(stdout, "[INFO] Saving variables for {}...", detail.name);
                        let resp = http_client::put_with_auth(&auth_mode, &update_url, serde_json::to_value(&update_body)?)
                            .with_context(|| "Failed to save environment variables")?;
                        if !resp.status().is_success() {
                            log_error_suffix!(stdout);
                            bail!("Failed to save variables for {}: {}", detail.name, resp.text().unwrap_or_default());
                        }
                        log_ok_suffix!(stdout);
                    }

                    writeln!(stdout)?;
                    log_info!(stdout, "[INFO] Variables saved. Retrying deployment...");
                    writeln!(stdout)?;
                    continue;
                } else {
                    log_error_suffix!(stdout);
                    bail!("Deployment failed: {}", error_text);
                }
            } else if status.as_u16() == 409 {
                let error_text = response
                    .text()
                    .unwrap_or_else(|_| "Unknown error".to_string());

                log_error_suffix!(stdout);

                bail!(
                    "Deployment conflict: {}. Wait for the current deployment to complete or cancel it first.",
                    error_text
                );
            } else if status.as_u16() == 403 {
                let error_text = response
                    .text()
                    .unwrap_or_else(|_| "Unknown error".to_string());

                log_error_suffix!(stdout);

                bail!("{}", error_text);
            } else {
                let error_text = response
                    .text()
                    .unwrap_or_else(|_| "Unknown error".to_string());

                log_error_suffix!(stdout);

                bail!(
                    "Failed to create deployment: {} (Status: {})",
                    error_text,
                    status
                );
            }
        }

        Ok(())
    }
}

use std::{fs, io::Write, path::Path};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::get_platform_management_api_url,
    core::{
        ast::infrastructure::compliance::scan_all_compliance,
        command::command,
        hmac::AuthMode,
        http_client::post_with_auth,
        validate::require_manifest,
    },
};

#[derive(Debug)]
pub(crate) struct AuditCommand;

impl AuditCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl CliCommand for AuditCommand {
    fn command(&self) -> Command {
        command(
            "audit",
            "Generate a point-in-time compliance audit report",
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("The application path"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .help("Output file path (JSON). If omitted, prints to terminal."),
        )
        .arg(
            Arg::new("environment")
                .short('e')
                .long("environment")
                .help("Environment name to associate with the report (e.g. production, staging)"),
        )
        .arg(
            Arg::new("data_flow")
                .long("data-flow")
                .help("Show PCI cardholder data flow diagram (Mermaid)")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("risk_score")
                .long("risk-score")
                .help("Show risk score and findings")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dpia")
                .long("dpia")
                .help("Show GDPR Data Protection Impact Assessment")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output raw JSON instead of formatted terminal output")
                .action(ArgAction::SetTrue),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let (app_root, manifest) = require_manifest(matches)?;

        let compliance = manifest.compliance.unwrap_or_default();
        let application_id = manifest.platform_application_id.clone();
        let environment_name = matches.get_one::<String>("environment").cloned();

        let show_data_flow = matches.get_flag("data_flow");
        let show_risk_score = matches.get_flag("risk_score");
        let show_dpia = matches.get_flag("dpia");
        let show_all = !show_data_flow && !show_risk_score && !show_dpia;
        let json_output = matches.get_flag("json");

        // Scan entity compliance data directly from source code
        let modules_path = &manifest.modules_path;
        let modules_path_buf = app_root.join(modules_path);
        let (field_classifications, retention_policies) =
            scan_all_compliance(&modules_path_buf)
                .unwrap_or_else(|e| {
                    let _ = writeln!(
                        stdout,
                        "[WARN] Failed to scan entity compliance metadata: {}",
                        e
                    );
                    (Default::default(), Default::default())
                });

        let mut entity_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        entity_names.extend(field_classifications.keys().cloned());
        entity_names.extend(retention_policies.keys().cloned());

        let entities: Vec<EntityReport> = entity_names
            .into_iter()
            .map(|name| {
                let field_reports: Vec<FieldReport> = field_classifications
                    .get(&name)
                    .map(|fields| {
                        fields
                            .iter()
                            .map(|(field_name, classification)| FieldReport {
                                name: field_name.clone(),
                                compliance: classification.clone(),
                                encrypted: classification == "phi"
                                    || classification == "pci",
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let retention =
                    retention_policies.get(&name).map(|r| RetentionReport {
                        duration: r.duration.clone(),
                        action: r.action.clone(),
                    });
                EntityReport {
                    name,
                    fields: field_reports,
                    retention,
                }
            })
            .collect();

        // Collect route data from OpenAPI spec (if available)
        let routes = collect_routes_from_openapi(&app_root, modules_path);

        // Build the local report
        let report = ComplianceReport {
            generated_at: chrono::Utc::now().to_rfc3339(),
            routes,
            entities,
            secrets: SecretsReport {
                declared: compliance.secrets.clone(),
                count: compliance.secrets.len(),
            },
            data_residency: DataResidencyReport {
                allowed_regions: compliance.data_residency.clone(),
            },
        };

        // Try to upload to platform API if credentials are available
        let platform_response = upload_to_platform(
            &report,
            application_id.as_deref(),
            environment_name.as_deref(),
        );

        // If --output is specified, write JSON to file
        if let Some(output_path) = matches.get_one::<String>("output") {
            let json = if let Ok(ref resp) = platform_response {
                serde_json::to_string_pretty(resp)
                    .with_context(|| "Failed to serialize platform response")?
            } else {
                serde_json::to_string_pretty(&report)
                    .with_context(|| "Failed to serialize compliance report")?
            };
            fs::write(output_path, &json)
                .with_context(|| format!("Failed to write report to {}", output_path))?;
            log_ok!(stdout, "Compliance report written to {}", output_path);
            return Ok(());
        }

        // JSON mode — raw output
        if json_output {
            let json = if let Ok(ref resp) = platform_response {
                serde_json::to_string_pretty(resp)?
            } else {
                serde_json::to_string_pretty(&report)?
            };
            println!("{}", json);
            return Ok(());
        }

        // Pretty terminal output
        print_header(&mut stdout)?;
        print_summary(&mut stdout, &report)?;

        match &platform_response {
            Ok(resp) => {
                if show_all || show_risk_score {
                    print_risk_score(&mut stdout, resp)?;
                    print_findings(&mut stdout, resp)?;
                }

                print_entities(&mut stdout, &report)?;
                print_routes(&mut stdout, &report)?;

                if (show_all || show_data_flow) && resp.data_flow_diagram.is_some() {
                    print_data_flow(&mut stdout, resp)?;
                }

                if (show_all || show_dpia) && resp.dpia.is_some() {
                    print_dpia(&mut stdout, resp)?;
                }

                writeln!(stdout)?;
                log_ok!(
                    stdout,
                    "Report saved to platform (id: {})",
                    resp.id
                );
            }
            Err(e) => {
                // Fallback: local-only display
                print_entities(&mut stdout, &report)?;
                print_routes(&mut stdout, &report)?;

                writeln!(stdout)?;
                log_warn!(
                    stdout,
                    "Could not upload to platform: {}. Showing local data only.",
                    e
                );
                log_info!(
                    stdout,
                    "For risk scoring, data flow diagrams, and DPIA, log in with `forklaunch login` and set platform_application_id in manifest.toml"
                );
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Platform upload
// ---------------------------------------------------------------------------

fn upload_to_platform(
    report: &ComplianceReport,
    application_id: Option<&str>,
    environment_name: Option<&str>,
) -> Result<PlatformAuditResponse> {
    let app_id = application_id
        .ok_or_else(|| anyhow::anyhow!("platform_application_id not set in manifest.toml"))?;
    let env_name = environment_name
        .ok_or_else(|| anyhow::anyhow!("--environment flag is required for platform upload"))?;

    let auth_mode = AuthMode::detect();

    let api_url = get_platform_management_api_url();
    let url = if auth_mode.is_hmac() {
        format!(
            "{}/compliance/applications/{}/environments/{}/audit/report",
            api_url, app_id, env_name
        )
    } else {
        format!(
            "{}/compliance/applications/{}/environments/{}/audit/report/user",
            api_url, app_id, env_name
        )
    };

    let body = serde_json::json!({
        "routes": report.routes,
        "entities": report.entities,
        "secrets": report.secrets,
        "dataResidency": report.data_residency,
    });

    let response = post_with_auth(&auth_mode, &url, body)?;
    let status = response.status();

    if !status.is_success() {
        let body_text = response
            .text()
            .unwrap_or_else(|_| "unknown error".to_string());
        anyhow::bail!("Platform returned {} — {}", status, body_text);
    }

    let resp: PlatformAuditResponse = response.json()?;
    Ok(resp)
}

// ---------------------------------------------------------------------------
// Pretty print helpers
// ---------------------------------------------------------------------------

fn print_header(out: &mut StandardStream) -> Result<()> {
    writeln!(out)?;
    out.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)).set_bold(true))?;
    writeln!(out, "╔══════════════════════════════════════════════════════════╗")?;
    writeln!(out, "║             COMPLIANCE AUDIT REPORT                     ║")?;
    writeln!(out, "╚══════════════════════════════════════════════════════════╝")?;
    out.reset()?;
    Ok(())
}

fn print_summary(out: &mut StandardStream, report: &ComplianceReport) -> Result<()> {
    writeln!(out)?;
    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    write!(out, "  Generated: ")?;
    out.reset()?;
    writeln!(out, "{}", report.generated_at)?;

    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    write!(out, "  Entities:  ")?;
    out.reset()?;
    writeln!(out, "{}", report.entities.len())?;

    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    write!(out, "  Routes:    ")?;
    out.reset()?;
    writeln!(out, "{}", report.routes.len())?;

    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    write!(out, "  Secrets:   ")?;
    out.reset()?;
    writeln!(out, "{}", report.secrets.count)?;

    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    write!(out, "  Regions:   ")?;
    out.reset()?;
    if report.data_residency.allowed_regions.is_empty() {
        writeln!(out, "(none configured)")?;
    } else {
        writeln!(out, "{}", report.data_residency.allowed_regions.join(", "))?;
    }

    Ok(())
}

fn print_risk_score(out: &mut StandardStream, resp: &PlatformAuditResponse) -> Result<()> {
    writeln!(out)?;
    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    writeln!(out, "  ── Risk Assessment ──")?;
    out.reset()?;

    let risk_color = match resp.risk_level.as_str() {
        "Low" => Color::Green,
        "Medium" => Color::Yellow,
        "High" => Color::Red,
        "Critical" => Color::Magenta,
        _ => Color::White,
    };

    write!(out, "  Score: ")?;
    out.set_color(ColorSpec::new().set_fg(Some(risk_color)).set_bold(true))?;
    write!(out, "{}/100", resp.risk_score)?;
    out.reset()?;

    write!(out, "  Level: ")?;
    out.set_color(ColorSpec::new().set_fg(Some(risk_color)).set_bold(true))?;
    writeln!(out, "{}", resp.risk_level)?;
    out.reset()?;

    Ok(())
}

fn print_findings(out: &mut StandardStream, resp: &PlatformAuditResponse) -> Result<()> {
    if resp.findings.is_empty() {
        writeln!(out)?;
        out.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
        writeln!(out, "  ✓ No findings — all checks passed")?;
        out.reset()?;
        return Ok(());
    }

    writeln!(out)?;
    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    writeln!(
        out,
        "  ── Findings ({}) ──",
        resp.findings.len()
    )?;
    out.reset()?;

    for finding in &resp.findings {
        let (icon, color) = match finding.severity.as_str() {
            "critical" => ("✖", Color::Magenta),
            "high" => ("✖", Color::Red),
            "medium" => ("▲", Color::Yellow),
            "low" => ("●", Color::Cyan),
            _ => ("●", Color::White),
        };

        write!(out, "  ")?;
        out.set_color(ColorSpec::new().set_fg(Some(color)).set_bold(true))?;
        write!(out, "{} [{:>8}]", icon, finding.severity.to_uppercase())?;
        out.reset()?;

        out.set_color(ColorSpec::new().set_fg(Some(Color::White)))?;
        write!(out, " [{}]", finding.category)?;
        out.reset()?;

        writeln!(out, " {}", finding.description)?;
    }

    Ok(())
}

fn print_entities(out: &mut StandardStream, report: &ComplianceReport) -> Result<()> {
    if report.entities.is_empty() {
        return Ok(());
    }

    writeln!(out)?;
    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    writeln!(out, "  ── Entity Classifications ──")?;
    out.reset()?;

    // Compute dynamic column width for ENTITY.FIELD based on actual data
    let min_field_width = "ENTITY.FIELD".len();
    let max_field_width = report
        .entities
        .iter()
        .flat_map(|e| {
            e.fields
                .iter()
                .map(move |f| format!("{}.{}", e.name, f.name).len())
        })
        .max()
        .unwrap_or(min_field_width);
    let field_col = max_field_width.max(min_field_width);
    let class_col = 20;
    let enc_col = 12;

    // Table header
    writeln!(
        out,
        "  {:<field_col$} {:<class_col$} {:<enc_col$} {}",
        "ENTITY.FIELD", "CLASSIFICATION", "ENCRYPTED", "STATUS",
        field_col = field_col,
        class_col = class_col,
        enc_col = enc_col,
    )?;
    writeln!(out, "  {}", "─".repeat(field_col + class_col + enc_col + 10))?;

    for entity in &report.entities {
        for field in &entity.fields {
            let classification_color = match field.compliance.as_str() {
                "pci" => Color::Red,
                "phi" => Color::Red,
                "pii" => Color::Yellow,
                _ => Color::Green,
            };

            let needs_encryption = field.compliance == "phi" || field.compliance == "pci";
            let status = if needs_encryption && field.encrypted {
                ("✓", Color::Green)
            } else if needs_encryption && !field.encrypted {
                ("✖ UNENCRYPTED", Color::Red)
            } else {
                ("—", Color::White)
            };

            write!(
                out,
                "  {:<width$} ",
                format!("{}.{}", entity.name, field.name),
                width = field_col,
            )?;

            out.set_color(ColorSpec::new().set_fg(Some(classification_color)))?;
            write!(out, "{:<width$} ", field.compliance.to_uppercase(), width = class_col)?;
            out.reset()?;

            write!(out, "{:<width$} ", if field.encrypted { "yes" } else { "no" }, width = enc_col)?;

            out.set_color(ColorSpec::new().set_fg(Some(status.1)))?;
            writeln!(out, "{}", status.0)?;
            out.reset()?;
        }
    }

    Ok(())
}

fn print_routes(out: &mut StandardStream, report: &ComplianceReport) -> Result<()> {
    if report.routes.is_empty() {
        return Ok(());
    }

    writeln!(out)?;
    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
    writeln!(out, "  ── Route Access Levels ──")?;
    out.reset()?;

    writeln!(
        out,
        "  {:<8} {:<40} {}",
        "METHOD", "PATH", "ACCESS"
    )?;
    writeln!(out, "  {}", "─".repeat(64))?;

    for route in &report.routes {
        let access = route.access.as_deref().unwrap_or("NONE");
        let access_color = match access {
            "public" => Color::Yellow,
            "authenticated" => Color::Cyan,
            "protected" => Color::Green,
            "internal" => Color::Blue,
            "NONE" => Color::Red,
            _ => Color::White,
        };

        write!(out, "  {:<8} {:<40} ", route.method, route.path)?;
        out.set_color(ColorSpec::new().set_fg(Some(access_color)))?;
        writeln!(out, "{}", access.to_uppercase())?;
        out.reset()?;
    }

    Ok(())
}

fn print_data_flow(out: &mut StandardStream, resp: &PlatformAuditResponse) -> Result<()> {
    if let Some(ref diagram) = resp.data_flow_diagram {
        writeln!(out)?;
        out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
        writeln!(out, "  ── PCI Data Flow Diagram (Mermaid) ──")?;
        out.reset()?;
        writeln!(out)?;
        for line in diagram.lines() {
            out.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
            writeln!(out, "    {}", line)?;
            out.reset()?;
        }
        writeln!(out)?;
        out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_dimmed(true))?;
        writeln!(out, "  Paste into https://mermaid.live to render")?;
        out.reset()?;
    }

    Ok(())
}

fn print_dpia(out: &mut StandardStream, resp: &PlatformAuditResponse) -> Result<()> {
    if let Some(ref dpia) = resp.dpia {
        writeln!(out)?;
        out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
        writeln!(out, "  ── GDPR Data Protection Impact Assessment ──")?;
        out.reset()?;

        if let Some(inventory) = dpia.get("dataInventory") {
            writeln!(out)?;
            out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
            writeln!(out, "  Data Inventory:")?;
            out.reset()?;

            if let Some(total) = inventory.get("totalEntities") {
                writeln!(out, "    Total entities: {}", total)?;
            }
            if let Some(pii) = inventory.get("totalPiiFields") {
                writeln!(out, "    PII/PHI/PCI fields: {}", pii)?;
            }
        }

        if let Some(assessment) = dpia.get("riskAssessment") {
            writeln!(out)?;
            out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
            writeln!(out, "  Risk Assessment:")?;
            out.reset()?;

            if let Some(score) = assessment.get("score") {
                writeln!(out, "    Risk score: {}/100", score)?;
            }
            if let Some(findings) = assessment.get("findings") {
                writeln!(out, "    Total findings: {}", findings)?;
            }
            if let Some(critical) = assessment.get("criticalFindings") {
                if critical.as_u64().unwrap_or(0) > 0 {
                    out.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
                    writeln!(out, "    Critical findings: {}", critical)?;
                    out.reset()?;
                }
            }
        }

        if let Some(mitigations) = dpia.get("mitigations") {
            writeln!(out)?;
            out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
            writeln!(out, "  Active Mitigations:")?;
            out.reset()?;

            if let Some(obj) = mitigations.as_object() {
                for (key, value) in obj {
                    out.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
                    write!(out, "    ✓ ")?;
                    out.reset()?;
                    out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
                    write!(out, "{}: ", format_key(key))?;
                    out.reset()?;
                    writeln!(out, "{}", value.as_str().unwrap_or(""))?;
                }
            }
        }

        if let Some(transfers) = dpia.get("crossBorderTransfers") {
            writeln!(out)?;
            out.set_color(ColorSpec::new().set_fg(Some(Color::White)).set_bold(true))?;
            write!(out, "  Cross-border: ")?;
            out.reset()?;
            writeln!(out, "{}", transfers.as_str().unwrap_or(""))?;
        }
    }

    Ok(())
}

fn format_key(key: &str) -> String {
    // camelCase → Title Case
    let mut result = String::new();
    for (i, ch) in key.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push(' ');
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else if i == 0 {
            result.push(ch.to_uppercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ComplianceReport {
    generated_at: String,
    routes: Vec<RouteReport>,
    entities: Vec<EntityReport>,
    secrets: SecretsReport,
    data_residency: DataResidencyReport,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteReport {
    path: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    access: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EntityReport {
    name: String,
    fields: Vec<FieldReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retention: Option<RetentionReport>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionReport {
    duration: String,
    action: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldReport {
    name: String,
    compliance: String,
    encrypted: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretsReport {
    declared: Vec<String>,
    count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DataResidencyReport {
    allowed_regions: Vec<String>,
}

// ---------------------------------------------------------------------------
// Platform response types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PlatformAuditResponse {
    id: String,
    risk_score: f64,
    risk_level: String,
    findings: Vec<PlatformFinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data_flow_diagram: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dpia: Option<serde_json::Value>,
    created_at: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct PlatformFinding {
    severity: String,
    category: String,
    description: String,
    points: f64,
}

// ---------------------------------------------------------------------------
// OpenAPI route collection
// ---------------------------------------------------------------------------

/// Attempt to read routes from generated OpenAPI specs.
/// Returns an empty vec if no specs are found.
fn collect_routes_from_openapi(app_root: &Path, modules_path: &str) -> Vec<RouteReport> {
    let mut routes = Vec::new();

    // OpenAPI specs are typically generated in each service's directory
    let openapi_patterns = [
        app_root.join("openapi.json"),
        app_root.join("docs").join("openapi.json"),
    ];

    // Search in <modules_path>/*/openapi.json (e.g., src/modules or modules)
    if let Ok(entries) = fs::read_dir(app_root.join(modules_path)) {
        for entry in entries.flatten() {
            let spec_path = entry.path().join("openapi.json");
            if spec_path.exists() {
                if let Ok(spec_routes) = parse_openapi_routes(&spec_path) {
                    routes.extend(spec_routes);
                }
            }
        }
    }

    for pattern in &openapi_patterns {
        if pattern.exists() {
            if let Ok(spec_routes) = parse_openapi_routes(pattern) {
                routes.extend(spec_routes);
            }
        }
    }

    routes
}

fn parse_openapi_routes(path: &Path) -> Result<Vec<RouteReport>> {
    let content = fs::read_to_string(path)?;
    let spec: serde_json::Value = serde_json::from_str(&content)?;

    let mut routes = Vec::new();

    if let Some(paths) = spec.get("paths").and_then(|p| p.as_object()) {
        for (path_str, methods) in paths {
            if let Some(methods_obj) = methods.as_object() {
                for (method, operation) in methods_obj {
                    // Skip non-HTTP methods (e.g., "parameters")
                    let http_methods = [
                        "get", "post", "put", "patch", "delete", "head", "options",
                    ];
                    if !http_methods.contains(&method.as_str()) {
                        continue;
                    }

                    // Try to extract access level from security or x-access extension
                    let access = operation
                        .get("x-access")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    routes.push(RouteReport {
                        path: path_str.clone(),
                        method: method.to_uppercase(),
                        access,
                    });
                }
            }
        }
    }

    Ok(routes)
}

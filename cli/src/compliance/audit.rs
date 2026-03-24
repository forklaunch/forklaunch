use std::{fs, io::Write, path::Path};

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command};
use serde::Serialize;
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{command::command, validate::require_manifest},
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
                .help("Output file path (defaults to stdout)"),
        )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let (app_root, manifest) = require_manifest(matches)?;

        let compliance = manifest.compliance.unwrap_or_default();

        // Collect entity compliance data
        let entities: Vec<EntityReport> = compliance
            .entities
            .iter()
            .map(|(name, fields)| {
                let field_reports: Vec<FieldReport> = fields
                    .iter()
                    .map(|(field_name, classification)| FieldReport {
                        name: field_name.clone(),
                        compliance: classification.clone(),
                        encrypted: classification == "phi" || classification == "pci",
                    })
                    .collect();
                EntityReport {
                    name: name.clone(),
                    fields: field_reports,
                }
            })
            .collect();

        // Collect route data from OpenAPI spec (if available)
        let routes = collect_routes_from_openapi(&app_root);

        // Build the report
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

        // Output
        let json = serde_json::to_string_pretty(&report)
            .with_context(|| "Failed to serialize compliance report")?;

        if let Some(output_path) = matches.get_one::<String>("output") {
            fs::write(output_path, &json)
                .with_context(|| format!("Failed to write report to {}", output_path))?;
            log_ok!(stdout, "Compliance report written to {}", output_path);
        } else {
            println!("{}", json);
        }

        Ok(())
    }
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
// OpenAPI route collection
// ---------------------------------------------------------------------------

/// Attempt to read routes from generated OpenAPI specs.
/// Returns an empty vec if no specs are found.
fn collect_routes_from_openapi(app_root: &Path) -> Vec<RouteReport> {
    let mut routes = Vec::new();

    // OpenAPI specs are typically generated in each service's directory
    let openapi_patterns = [
        app_root.join("openapi.json"),
        app_root.join("docs").join("openapi.json"),
    ];

    // Also search in src/modules/*/openapi.json
    if let Ok(entries) = fs::read_dir(app_root.join("src").join("modules")) {
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

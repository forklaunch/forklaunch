use std::{fs::create_dir_all, io::Write};

use anyhow::{Context, Result};
use clap::{Arg, ArgMatches, Command};
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    core::{
        artifact_paths::{self, Artifact},
        command::command,
        openapi_export::export_all_services,
    },
};

#[derive(Debug)]
pub(crate) struct ExportCommand;

impl ExportCommand {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for ExportCommand {
    fn command(&self) -> Command {
        command("export", "Export OpenAPI specifications from services")
            .arg(
                Arg::new("base_path")
                    .long("path")
                    .short('p')
                    .help("Path to application root (optional)"),
            )
            .arg(
                Arg::new("output")
                    .long("output")
                    .short('o')
                    .help(
                        "Also write every spec to <output>/<service>/openapi.json. By default each \
                         service writes its own registered OpenAPI artifact (its SDK folder; \
                         .forklaunch/openapi/<service>/openapi.json for services without one)",
                    ),
            )
    }

    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let (app_root, manifest) = crate::core::validate::require_manifest(matches)?;
        let manifest_value: toml::Value = toml::from_str(&std::fs::read_to_string(
            app_root.join(".forklaunch/manifest.toml"),
        )?)?;

        let output_path = matches
            .get_one::<String>("output")
            .map(|output| app_root.join(output));
        if let Some(output_path) = &output_path {
            create_dir_all(output_path).with_context(|| {
                format!("Failed to create output directory: {:?}", output_path)
            })?;
        }

        log_info!(stdout, "Exporting OpenAPI specifications...");
        writeln!(stdout)?;

        let result = export_all_services(&app_root, &manifest, output_path.as_deref());

        writeln!(stdout)?;

        match result {
            Ok(exported_services) => {
                log_header!(
                    stdout,
                    Color::Green,
                    "Successfully exported {} OpenAPI specification(s)",
                    exported_services.len()
                );
                if let Some(output_path) = &output_path {
                    writeln!(stdout, "  Output: {}", output_path.display())?;
                }

                for service_name in &exported_services {
                    let written = output_path.as_ref().map_or_else(
                        || {
                            artifact_paths::manifest_path(
                                &manifest_value,
                                service_name,
                                Artifact::OpenApi,
                            )
                        },
                        |output| {
                            output
                                .join(service_name)
                                .join("openapi.json")
                                .display()
                                .to_string()
                        },
                    );
                    log_ok!(stdout, "  - {} ({})", service_name, written);
                }
            }
            Err(e) => {
                log_error!(stdout, "Failed to export OpenAPI specifications");
                return Err(e);
            }
        }

        Ok(())
    }
}

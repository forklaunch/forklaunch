use std::{
    fs::{create_dir_all, read_to_string},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, ArgMatches, Command};
use convert_case::{Case, Casing};
use fs_extra::dir::{CopyOptions, copy};
use rustyline::{Editor, history::DefaultHistory};
use serde::Deserialize;
use termcolor::{Color, ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{
        ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST, ERROR_FAILED_TO_COPY_STOREFRONT_SITE,
        ERROR_FAILED_TO_PARSE_MANIFEST, ERROR_FAILED_TO_PARSE_STOREFRONT_MANIFEST,
        ERROR_FAILED_TO_READ_MANIFEST, ERROR_FAILED_TO_READ_STOREFRONT_MANIFEST,
    },
    core::{
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        manifest::{
            ApplicationInitializationMetadata, InitializableManifestConfig,
            InitializableManifestConfigMetadata, ManifestData, ProjectType,
            add_project_definition_to_manifest, application::ApplicationManifestData,
            library::LibraryManifestData,
        },
        name::validate_name,
        rendered_template::{RenderedTemplate, write_rendered_templates},
    },
    prompt::{ArrayCompleter, prompt_with_validation},
};

/// The variant tag written to the project's `ProjectEntry` in manifest.toml.
///
/// A migrated storefront is registered as `ProjectType::Library` (see
/// `handler` below) rather than a new `ProjectType` variant. Adding a real
/// enum variant would require updating every exhaustive match over
/// `ProjectType` across release, sync, deploy, env_scope, openapi_export and
/// client_sdk — cross-cutting work with real product decisions attached that
/// is out of scope here. Tagging the project with a plain string variant
/// instead mirrors how `init/module.rs` tags service variants (e.g.
/// "better-auth-iam") and touches no enums, so every existing exhaustive
/// match keeps compiling unchanged.
const STOREFRONT_VARIANT: &str = "storefront";

/// The `manifest.json` `schemaVersion` this command knows how to read. Bump
/// alongside storefront-migrate's manifest-schema.md if the shape ever
/// changes non-additively.
const SUPPORTED_MANIFEST_SCHEMA_VERSION: u64 = 1;

/// Mirrors the subset of `manifest.json` (see storefront-migrate's
/// manifest-schema.md) this command actually consumes. Deliberately doesn't
/// model every documented field (e.g. `assets`, `gaps`, `source.url`) —
/// `manifest.js` is the authoritative writer of the full shape; this is just
/// the reader's contract with it.
#[derive(Debug, Deserialize)]
struct CapturedManifest {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    source: CapturedSource,
    pages: Vec<CapturedPage>,
    catalog: Option<CapturedCatalog>,
    limits: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CapturedSource {
    domain: String,
}

#[derive(Debug, Deserialize)]
struct CapturedPage {
    route: String,
    #[serde(rename = "type")]
    page_type: String,
}

#[derive(Debug, Deserialize)]
struct CapturedCatalog {
    #[serde(rename = "productCount")]
    product_count: u64,
    #[serde(rename = "variantCount")]
    variant_count: u64,
}

/// `--from` takes a path to an already-captured manifest.json, never a URL —
/// capturing a storefront is a separate, longer-running step (crawl + catalog
/// pull) that this command must not silently kick off. A URL-shaped value is
/// almost certainly someone skipping that step by mistake, so fail fast with
/// where to go instead of trying to interpret it as a path.
fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://") || value.contains("://")
}

#[derive(Debug)]
pub(super) struct StorefrontCommand;

impl StorefrontCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for StorefrontCommand {
    fn command(&self) -> Command {
        command(
            "storefront",
            "Initialize a project from a captured storefront manifest",
        )
        .arg(Arg::new("name").help("The name of the storefront project"))
        .arg(
            Arg::new("from")
                .long("from")
                .required(true)
                .help("Path to the manifest.json produced by the storefront capture tool"),
        )
        .arg(
            Arg::new("base_path")
                .short('p')
                .long("path")
                .help("The application path to initialize the storefront in"),
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
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let from_arg = matches
            .get_one::<String>("from")
            .expect("--from is required")
            .clone();

        if looks_like_url(&from_arg) {
            bail!(
                "--from expects a path to a manifest.json produced by the storefront capture \
                 tool, not a URL ('{from_arg}'). Run the capture tool against the storefront \
                 first, then pass the resulting manifest.json path to --from."
            );
        }

        let manifest_source_path = PathBuf::from(&from_arg);
        let captured_manifest: CapturedManifest = serde_json::from_str(
            &read_to_string(&manifest_source_path)
                .with_context(|| ERROR_FAILED_TO_READ_STOREFRONT_MANIFEST)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_STOREFRONT_MANIFEST)?;

        if captured_manifest.schema_version != SUPPORTED_MANIFEST_SCHEMA_VERSION {
            bail!(
                "Unsupported manifest schemaVersion {} (this CLI understands version {}). \
                 Re-run the storefront capture tool to regenerate manifest.json, or upgrade the \
                 forklaunch CLI.",
                captured_manifest.schema_version,
                SUPPORTED_MANIFEST_SCHEMA_VERSION
            );
        }

        // manifest.js always writes manifest.json at the root of the capture run's
        // output directory, alongside site/ (see manifest-schema.md) — the captured
        // assets live in that sibling directory, not anywhere the manifest itself
        // records a path to.
        let site_source_path = manifest_source_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("site");

        if !site_source_path.exists() {
            bail!(
                "Expected captured site assets at '{}' (sibling of the manifest passed to \
                 --from) but that directory does not exist.",
                site_source_path.display()
            );
        }

        let (app_root_path, _) = find_app_root_path(matches, RequiredLocation::Application)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let existing_manifest_data = toml::from_str::<ApplicationManifestData>(
            &read_to_string(&manifest_path).with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

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

        // Default the project name from the captured domain (e.g. "graza.co" ->
        // "graza-co") so a straightforward migration doesn't require inventing a
        // name; still overridable via the positional arg or interactive prompt.
        let suggested_name = captured_manifest
            .source
            .domain
            .replace('.', "-")
            .to_case(Case::Kebab);

        let project_name = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "name",
            matches,
            &format!("storefront project name (e.g. {suggested_name})"),
            None,
            |input: &str| validate_name(input) && !manifest_data.app_name.contains(input),
            |_| {
                "Storefront name cannot be a substring of the application name, empty or include numbers or spaces. Please try again"
                    .to_string()
            },
        )?;

        let description = format!(
            "Storefront migrated from {}",
            captured_manifest.source.domain
        );

        let mut storefront_manifest_data: LibraryManifestData = LibraryManifestData {
            // Common fields from ApplicationManifestData
            id: manifest_data.id.clone(),
            app_name: manifest_data.app_name.clone(),
            modules_path: manifest_data.modules_path.clone(),
            docker_compose_path: manifest_data.docker_compose_path.clone(),
            dockerfile: manifest_data.dockerfile.clone(),
            git_repository: manifest_data.git_repository.clone(),
            camel_case_app_name: manifest_data.camel_case_app_name.clone(),
            pascal_case_app_name: manifest_data.pascal_case_app_name.clone(),
            kebab_case_app_name: manifest_data.kebab_case_app_name.clone(),
            title_case_app_name: manifest_data.title_case_app_name.clone(),
            app_description: manifest_data.app_description.clone(),
            author: manifest_data.author.clone(),
            cli_version: manifest_data.cli_version.clone(),
            formatter: manifest_data.formatter.clone(),
            linter: manifest_data.linter.clone(),
            validator: manifest_data.validator.clone(),
            runtime: manifest_data.runtime.clone(),
            test_framework: manifest_data.test_framework.clone(),
            projects: manifest_data.projects.clone(),
            http_framework: manifest_data.http_framework.clone(),
            license: manifest_data.license.clone(),
            project_peer_topology: manifest_data.project_peer_topology.clone(),
            is_biome: manifest_data.is_biome,
            is_eslint: manifest_data.is_eslint,
            is_oxlint: manifest_data.is_oxlint,
            is_prettier: manifest_data.is_prettier,
            is_express: manifest_data.is_express,
            is_hyper_express: manifest_data.is_hyper_express,
            is_zod: manifest_data.is_zod,
            is_typebox: manifest_data.is_typebox,
            is_bun: manifest_data.is_bun,
            is_node: manifest_data.is_node,
            is_vitest: manifest_data.is_vitest,
            is_jest: manifest_data.is_jest,
            platform_application_id: manifest_data.platform_application_id.clone(),
            platform_organization_id: manifest_data.platform_organization_id.clone(),
            compliance: manifest_data.compliance.clone(),

            // Storefront-specific fields. LibraryManifestData's shape is reused
            // as-is (see STOREFRONT_VARIANT above for why); there is no
            // storefront-specific manifest struct.
            library_name: project_name.clone(),
            camel_case_name: project_name.to_case(Case::Camel),
            kebab_case_name: project_name.to_case(Case::Kebab),
            title_case_name: project_name.to_case(Case::Title),
            description: description.clone(),
        };

        let dryrun = matches.get_flag("dryrun");
        let project_output_path = base_path.join(&project_name);

        let forklaunch_definition_buffer = add_project_definition_to_manifest(
            ProjectType::Library,
            &mut storefront_manifest_data,
            Some(STOREFRONT_VARIANT.to_string()),
            None,
            None,
            None,
        )
        .with_context(|| ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST)?;

        let rendered_templates = vec![RenderedTemplate {
            path: manifest_path.clone(),
            content: forklaunch_definition_buffer,
            context: Some(ERROR_FAILED_TO_ADD_PROJECT_METADATA_TO_MANIFEST.to_string()),
        }];

        write_rendered_templates(&rendered_templates, dryrun, &mut stdout)
            .with_context(|| "Failed to write storefront project metadata to manifest")?;

        if !dryrun {
            // generate_with_template can't carry this: it reads from the
            // compiled-in TEMPLATES_DIR and renders text through Handlebars into a
            // String, but a captured site has binary assets (images, fonts)
            // discovered at capture time, not baked into the CLI binary.
            // fs_extra::dir::copy (same approach as eject.rs's dependency copy)
            // copies the directory byte-for-byte instead.
            create_dir_all(&project_output_path).with_context(|| {
                format!(
                    "Failed to create directory {}",
                    project_output_path.display()
                )
            })?;

            let mut copy_options = CopyOptions::new();
            copy_options.overwrite = true;

            copy(&site_source_path, &project_output_path, &copy_options)
                .with_context(|| ERROR_FAILED_TO_COPY_STOREFRONT_SITE)?;
        }

        print_summary(
            &mut stdout,
            &project_name,
            &project_output_path,
            &captured_manifest,
            dryrun,
        )?;

        Ok(())
    }
}

fn print_summary(
    stdout: &mut StandardStream,
    project_name: &str,
    project_output_path: &Path,
    captured_manifest: &CapturedManifest,
    dryrun: bool,
) -> Result<()> {
    if dryrun {
        log_info!(stdout, "Dry run — no files were written.");
    }

    log_ok!(
        stdout,
        "{} initialized at {}",
        project_name,
        project_output_path.display()
    );

    log_header!(
        stdout,
        Color::Cyan,
        "Pages scaffolded ({}):",
        captured_manifest.pages.len()
    );
    for page in &captured_manifest.pages {
        writeln!(stdout, "  {} [{}]", page.route, page.page_type)?;
    }

    match &captured_manifest.catalog {
        Some(catalog) => log_info!(
            stdout,
            "Catalog: {} products, {} variants",
            catalog.product_count,
            catalog.variant_count
        ),
        None => log_info!(stdout, "Catalog: no catalog data"),
    }

    // Echoed verbatim, not summarized — these are the pipeline's own words for
    // what a capture never carries over, and paraphrasing them risks quietly
    // dropping or softening one.
    log_header!(stdout, Color::Yellow, "Not migrated:");
    for limit in &captured_manifest.limits {
        writeln!(stdout, "  - {limit}")?;
    }

    Ok(())
}

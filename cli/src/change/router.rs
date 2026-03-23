use std::{io::Write, path::Path, fs};

use anyhow::{Context, Result, bail};
use clap::{Arg, ArgAction, Command};
use convert_case::{Case, Casing};
use dialoguer::{MultiSelect, theme::ColorfulTheme};
use rustyline::{Editor, history::DefaultHistory};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use super::core::change_name::change_name_in_files;
use crate::{
    CliCommand,
    constants::{ERROR_FAILED_TO_PARSE_MANIFEST, ERROR_FAILED_TO_READ_MANIFEST, Runtime},
    core::{
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        format::format_code,
        manifest::{
            InitializableManifestConfig, InitializableManifestConfigMetadata, ManifestData,
            ProjectEntry, RouterInitializationMetadata, router::RouterManifestData,
        },
        name::validate_name,
        removal_template::{RemovalTemplate, remove_template_files},
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
        static_analysis::{SchemaAnalyzer, EntityAnalyzer, MapperGenerator},
    },
    prompt::{ArrayCompleter, prompt_field_from_selections_with_validation},
};

#[derive(Debug)]
pub(super) struct RouterCommand;

impl RouterCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

pub(crate) fn change_name(
    base_path: &Path,
    existing_name: &str,
    new_name: &str,
    confirm: bool,
    runtime: &Runtime,
    project_entry: &mut ProjectEntry,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    stdout: &mut StandardStream,
) -> Result<Vec<RemovalTemplate>> {
    change_name_in_files(
        base_path,
        existing_name,
        new_name,
        runtime,
        confirm,
        project_entry,
        rendered_templates_cache,
        stdout,
    )
}

fn add_mappers_to_router(
    router_base_path: &Path,
    project_name: &str,
    manifest_data: &RouterManifestData,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    dryrun: bool,
    stdout: &mut StandardStream,
) -> Result<()> {
    let router_name = router_base_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let pascal_case_name = router_name.to_case(Case::Pascal);
    let camel_case_name = router_name.to_case(Case::Camel);

    let schema_dir = router_base_path.join("domain").join("schemas");
    let entity_dir = router_base_path.join("persistence").join("entities");

    if !schema_dir.exists() {
        bail!("Schema directory not found: {}", schema_dir.display());
    }

    if !entity_dir.exists() {
        bail!("Entity directory not found: {}", entity_dir.display());
    }

    let schema_file = schema_dir.join(format!("{}.schema.ts", camel_case_name));
    if !schema_file.exists() {
        bail!("Schema file not found: {}", schema_file.display());
    }

    // Find the entity file (look for {name}Record.entity.ts or {name}EventRecord.entity.ts)
    // Look up the specific project by name to determine if it's a worker
    let is_worker = manifest_data
        .projects
        .iter()
        .find(|p| p.name == project_name)
        .map(|p| matches!(p.r#type, crate::core::manifest::ProjectType::Worker))
        .unwrap_or(false);
    let entity_suffix = if is_worker { "EventRecord" } else { "Record" };
    let entity_file = entity_dir.join(format!("{}{}.entity.ts", camel_case_name, entity_suffix));

    if !entity_file.exists() {
        bail!("Entity file not found: {}", entity_file.display());
    }

    log_info!(stdout, "Parsing schema file: {}", schema_file.display());

    let schemas = SchemaAnalyzer::parse_schema_file(&schema_file)
        .with_context(|| format!("Failed to parse schema file: {}", schema_file.display()))?;

    log_info!(stdout, "Parsing entity file: {}", entity_file.display());

    let entities = EntityAnalyzer::parse_entity_file(&entity_file)
        .with_context(|| format!("Failed to parse entity file: {}", entity_file.display()))?;

    if schemas.is_empty() {
        bail!("No schemas found in: {}", schema_file.display());
    }

    if entities.is_empty() {
        bail!("No entities found in: {}", entity_file.display());
    }

    let request_schema = schemas.iter()
        .find(|s| s.name.ends_with("RequestSchema"))
        .with_context(|| "No RequestSchema found in schema file")?;

    let entity = &entities[0];

    log_info!(stdout, "Generating mappers for {} -> {}", request_schema.name, entity.name);

    let generator = MapperGenerator::new(
        request_schema.clone(),
        entity.clone(),
        manifest_data.app_name.clone(),
        is_worker,
    );

    let mapper_content = generator.generate_mapper_file();

    let mappers_dir = router_base_path.join("domain").join("mappers");
    let mapper_file = mappers_dir.join(format!("{}.mappers.ts", camel_case_name));

    if mapper_file.exists() {
        log_warn!(stdout, "⚠ Mapper file already exists: {}", mapper_file.display());
        log_warn!(stdout, "⚠ Skipping mapper generation to preserve custom logic");
        bail!("Mapper file already exists. Remove it manually if you want to regenerate.");
    }

    if !dryrun {
        fs::create_dir_all(&mappers_dir)?;
    }

    rendered_templates_cache.insert(
        mapper_file.to_string_lossy(),
        RenderedTemplate {
            path: mapper_file.clone(),
            content: mapper_content,
            context: None,
        },
    );

    log_ok!(stdout, "✓ Generated mapper file: {}", mapper_file.display());

    update_controller_imports(router_base_path, &pascal_case_name, &camel_case_name, rendered_templates_cache, stdout)?;
    update_service_and_interface_files(router_base_path, &pascal_case_name, &camel_case_name, &manifest_data.app_name, rendered_templates_cache, stdout)?;

    Ok(())
}

fn update_service_and_interface_files(
    router_base_path: &Path,
    pascal_case_name: &str,
    camel_case_name: &str,
    _app_name: &str,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    stdout: &mut StandardStream,
) -> Result<()> {
    let interface_file = router_base_path
        .join("domain")
        .join("interfaces")
        .join(format!("{}.interface.ts", camel_case_name));

    if interface_file.exists() {
        log_info!(stdout, "Updating interface file: {}", interface_file.display());

        let content = match rendered_templates_cache.get(&interface_file)? {
            Some(template) => template.content.clone(),
            None => fs::read_to_string(&interface_file)
                .with_context(|| format!("Failed to read interface file: {}", interface_file.display()))?,
        };

        let mut updated_content = content;

        // Check if this was generated without mappers by looking for Schema import
        if updated_content.contains("import { Schema } from '@forklaunch/validator';") {
            // Find the interface comment and add mapper info before it
            let lines: Vec<&str> = updated_content.lines().collect();
            let mut new_lines = Vec::new();
            let mut added_mapper_comment = false;

            for line in lines {
                // Detect the interface definition comment
                if !added_mapper_comment && line.starts_with("// Interface that defines") {
                    new_lines.push("// ============================================================================".to_string());
                    new_lines.push("// MAPPERS GENERATED!".to_string());
                    new_lines.push("// ============================================================================".to_string());
                    new_lines.push(format!("// Mapper files have been generated in domain/mappers/{}.mappers.ts", camel_case_name));
                    new_lines.push("//".to_string());
                    new_lines.push("// To use mappers with type safety, update this interface to use DTO types:".to_string());
                    new_lines.push("//".to_string());
                    new_lines.push(format!("//   import {{ {}RequestDto, {}ResponseDto }} from '../types/{}.types';", pascal_case_name, pascal_case_name, camel_case_name));
                    new_lines.push("//".to_string());
                    new_lines.push("//   Then update the method signature to:".to_string());
                    new_lines.push(format!("//     {}Post: (dto: {}RequestDto) => Promise<{}ResponseDto>;", camel_case_name, pascal_case_name, pascal_case_name));
                    new_lines.push("//".to_string());
                    new_lines.push("// The current schema-based interface is preserved to avoid breaking changes.".to_string());
                    new_lines.push("// ============================================================================".to_string());
                    new_lines.push(String::new());
                    added_mapper_comment = true;
                }

                new_lines.push(line.to_string());
            }

            updated_content = new_lines.join("\n");
        }

        rendered_templates_cache.insert(
            interface_file.to_string_lossy(),
            RenderedTemplate {
                path: interface_file.clone(),
                content: updated_content,
                context: None,
            },
        );

        log_ok!(stdout, "✓ Updated interface file");
    }

    let service_file = router_base_path
        .join("domain")
        .join("services")
        .join(format!("{}.service.ts", camel_case_name));

    if service_file.exists() {
        log_info!(stdout, "Updating service file: {}", service_file.display());

        let content = match rendered_templates_cache.get(&service_file)? {
            Some(template) => template.content.clone(),
            None => fs::read_to_string(&service_file)
                .with_context(|| format!("Failed to read service file: {}", service_file.display()))?,
        };

        let mut updated_content = content;

        // Check if this service was generated without mappers
        if updated_content.contains("import { Schema } from '@forklaunch/validator';") {
            // Replace the inline mapping logic header with mapper usage instructions
            let lines: Vec<&str> = updated_content.lines().collect();
            let mut new_lines = Vec::new();
            let mut in_old_comment_block = false;
            let mut added_mapper_comment = false;

            for line in lines {
                // Skip the old inline mapping header comment block (lines 60-72 in the example)
                if line.trim() == "// ============================================================================" && !added_mapper_comment {
                    in_old_comment_block = true;

                    // Replace with new comment
                    new_lines.push("    // ============================================================================".to_string());
                    new_lines.push("    // MAPPERS GENERATED!".to_string());
                    new_lines.push("    // ============================================================================".to_string());
                    new_lines.push(format!("    // Mapper files have been generated in domain/mappers/{}.mappers.ts", camel_case_name));
                    new_lines.push("    //".to_string());
                    new_lines.push("    // To use the mappers, replace the inline mapping code below with:".to_string());
                    new_lines.push("    //".to_string());
                    new_lines.push(format!("    //   const entity = await {}RequestMapper.toEntity(data, this.entityManager);", pascal_case_name));
                    new_lines.push("    //   await this.entityManager.persistAndFlush(entity);".to_string());
                    new_lines.push(format!("    //   return {}ResponseMapper.toDto(entity);", pascal_case_name));
                    new_lines.push("    //".to_string());
                    new_lines.push("    // You'll also need to:".to_string());
                    new_lines.push("    //   1. Update imports to use DTO types from '../types/*.types'".to_string());
                    new_lines.push(format!("    //   2. Import mappers from '../mappers/{}.mappers'", camel_case_name));
                    new_lines.push("    //   3. Change method signature to use DTOs instead of schema types".to_string());
                    new_lines.push("    //".to_string());
                    new_lines.push("    // The inline code below is preserved to avoid breaking your custom logic.".to_string());
                    added_mapper_comment = true;
                    continue;
                }

                // Skip lines in the old comment block
                if in_old_comment_block {
                    if line.trim().starts_with("// Map from request data") {
                        in_old_comment_block = false;
                        new_lines.push(line.to_string());
                    }
                    continue;
                }

                new_lines.push(line.to_string());
            }

            updated_content = new_lines.join("\n");
        }

        rendered_templates_cache.insert(
            service_file.to_string_lossy(),
            RenderedTemplate {
                path: service_file.clone(),
                content: updated_content,
                context: None,
            },
        );

        log_ok!(stdout, "✓ Updated service file");
    }

    Ok(())
}

fn update_controller_imports(
    router_base_path: &Path,
    pascal_case_name: &str,
    camel_case_name: &str,
    rendered_templates_cache: &mut RenderedTemplatesCache,
    stdout: &mut StandardStream,
) -> Result<()> {
    let controller_dir = router_base_path.join("api").join("controllers");
    let controller_file = controller_dir.join(format!("{}.controller.ts", camel_case_name));

    if !controller_file.exists() {
        bail!("Controller file not found: {}", controller_file.display());
    }

    log_info!(stdout, "Updating controller imports: {}", controller_file.display());

    let content = match rendered_templates_cache.get(&controller_file)? {
        Some(template) => template.content.clone(),
        None => {
            fs::read_to_string(&controller_file)
                .with_context(|| format!("Failed to read controller file: {}", controller_file.display()))?
        }
    };

    let old_import = format!(
        "import {{ {}RequestSchema, {}ResponseSchema }} from '../../domain/schemas/{}.schema';",
        pascal_case_name, pascal_case_name, camel_case_name
    );

    let new_import = format!(
        "import {{ {}RequestMapper, {}ResponseMapper }} from '../../domain/mappers/{}.mappers';",
        pascal_case_name, pascal_case_name, camel_case_name
    );

    let mut updated_content = content.replace(&old_import, &new_import);

    // Replace schema references with mapper.schema references in the code
    // Replace ResponseSchema with ResponseMapper.schema in responses
    updated_content = updated_content.replace(
        &format!("200: {}ResponseSchema", pascal_case_name),
        &format!("200: {}ResponseMapper.schema", pascal_case_name),
    );

    // Replace other status codes as well
    updated_content = updated_content.replace(
        &format!("201: {}ResponseSchema", pascal_case_name),
        &format!("201: {}ResponseMapper.schema", pascal_case_name),
    );

    // Replace RequestSchema with RequestMapper.schema in body
    updated_content = updated_content.replace(
        &format!("body: {}RequestSchema", pascal_case_name),
        &format!("body: {}RequestMapper.schema", pascal_case_name),
    );

    rendered_templates_cache.insert(
        controller_file.to_string_lossy(),
        RenderedTemplate {
            path: controller_file.clone(),
            content: updated_content,
            context: None,
        },
    );

    log_ok!(stdout, "✓ Updated controller imports");

    Ok(())
}

impl CliCommand for RouterCommand {
    fn command(&self) -> Command {
        command("router", "Change a forklaunch router")
            .alias("controller")
            .alias("routes")
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The service path"),
            )
            .arg(
                Arg::new("existing-name")
                    .short('e')
                    .help("The original name of the router"),
            )
            .arg(
                Arg::new("new-name")
                    .short('N')
                    .help("The new name of the router"),
            )
            .arg(
                Arg::new("dryrun")
                    .short('n')
                    .long("dryrun")
                    .help("Dry run the command")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("confirm")
                    .short('c')
                    .long("confirm")
                    .help("Flag to confirm any prompts")
                    .action(ArgAction::SetTrue),
            )
            .arg(
                Arg::new("add-mappers")
                    .long("add-mappers")
                    .help("Generate mapper files from existing schemas and entities")
                    .action(ArgAction::SetTrue),
            )
    }

    fn handler(&self, matches: &clap::ArgMatches) -> Result<()> {
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);
        let mut rendered_templates_cache = RenderedTemplatesCache::new();

        let (app_root_path, project_name_opt) = find_app_root_path(matches, RequiredLocation::Project)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        // RequiredLocation::Project guarantees project_name is present
        let project_name = project_name_opt.expect("Project name should be present when RequiredLocation::Project is used");

        let existing_name = matches.get_one::<String>("existing-name");
        let new_name = matches.get_one::<String>("new-name");
        let dryrun = matches.get_flag("dryrun");
        let confirm = matches.get_flag("confirm");
        let add_mappers = matches.get_flag("add-mappers");

        let existing_manifest_data = toml::from_str::<RouterManifestData>(
            &rendered_templates_cache
                .get(&manifest_path)
                .with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?
                .unwrap()
                .content,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let router_base_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Router(&existing_manifest_data),
            &Some(project_name.clone()),
            &mut line_editor,
            &mut stdout,
            matches,
            1,
        )?;

        // Derive router name from path if not provided
        let router_name_str = router_base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let mut manifest_data = existing_manifest_data.initialize(
            InitializableManifestConfigMetadata::Router(RouterInitializationMetadata {
                project_name: project_name.clone(),
                router_name: existing_name.cloned().or(Some(router_name_str)),
            }),
        );

        if add_mappers {
            add_mappers_to_router(&router_base_path, &project_name, &manifest_data, &mut rendered_templates_cache, dryrun, &mut stdout)?;

            // Write the generated files to disk
            let rendered_templates: Vec<RenderedTemplate> = rendered_templates_cache
                .drain()
                .map(|(_, template)| template)
                .collect();

            write_rendered_templates(&rendered_templates, dryrun, &mut stdout)?;

            if !dryrun {
                log_ok!(stdout, "✓ Mappers added successfully!");
                format_code(&router_base_path, &manifest_data.runtime.parse()?);
            }

            return Ok(());
        }

        let selected_options = if matches.ids().all(|id| id == "dryrun" || id == "confirm") {
            let options = vec!["name"];

            let selections = MultiSelect::with_theme(&ColorfulTheme::default())
                .with_prompt("What would you like to change?")
                .items(&options)
                .interact()?;

            if selections.is_empty() {
                writeln!(stdout, "No changes selected")?;
                return Ok(());
            }

            selections.iter().map(|i| options[*i]).collect()
        } else {
            vec![]
        };

        let new_name = prompt_field_from_selections_with_validation(
            "new-name",
            new_name,
            &selected_options,
            &mut line_editor,
            &mut stdout,
            matches,
            "router name",
            None,
            |input: &str| validate_name(input) && !manifest_data.app_name.contains(input),
            |_| {
                "Router name cannot be a substring of the application name, empty or include numbers or spaces. Please try again"
                    .to_string()
            },
        )?;

        let mut removal_templates = vec![];

        if let Some(new_name) = new_name {
            removal_templates.extend(change_name(
                &router_base_path,
                &existing_name.unwrap(),
                &new_name,
                confirm,
                &manifest_data.runtime.parse()?,
                &mut manifest_data
                    .projects
                    .iter_mut()
                    .find(|project| project.name == project_name)
                    .unwrap(),
                &mut rendered_templates_cache,
                &mut stdout,
            )?);
        }

        rendered_templates_cache.insert(
            manifest_path.clone().to_string_lossy(),
            RenderedTemplate {
                path: manifest_path.to_path_buf(),
                content: toml::to_string_pretty(&manifest_data)?,
                context: None,
            },
        );

        let rendered_templates: Vec<RenderedTemplate> = rendered_templates_cache
            .drain()
            .map(|(_, template)| template)
            .collect();

        remove_template_files(&removal_templates, dryrun, &mut stdout)?;
        write_rendered_templates(&rendered_templates, dryrun, &mut stdout)?;

        if !dryrun {
            format_code(&router_base_path, &manifest_data.runtime.parse()?);
        }

        Ok(())
    }
}

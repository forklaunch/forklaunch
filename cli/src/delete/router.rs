use std::{
    fs::{exists, read_to_string, remove_file},
    io::Write,
};

use anyhow::{Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use convert_case::{Case, Casing};
use oxc_allocator::Allocator;
use oxc_ast::ast::SourceType;
use rustyline::{Editor, history::DefaultHistory};
use termcolor::{ColorChoice, StandardStream, WriteColor};

use crate::{
    CliCommand,
    constants::{
        ERROR_FAILED_TO_PARSE_MANIFEST, ERROR_FAILED_TO_READ_MANIFEST,
        ERROR_FAILED_TO_WRITE_MANIFEST, ERROR_FAILED_TO_WRITE_SERVICE_FILES,
    },
    core::{
        ast::{
            deletions::{
                delete_from_index_ts::delete_from_index_ts_export,
                delete_from_registrations_ts::delete_from_registrations_ts_config_injector,
                delete_from_sdk_ts::delete_from_sdk_client_input,
                delete_from_seed_data_ts::delete_from_seed_data_ts,
                delete_from_server_ts::delete_from_server_ts_router,
                delete_import_statement::delete_import_statement,
            },
            parse_ast_program::parse_ast_program,
            transformations::{
                transform_controllers_index_ts::transform_controllers_index_ts_delete,
                transform_test_utils_ts::transform_test_utils_remove_router,
            },
        },
        base_path::{RequiredLocation, find_app_root_path, prompt_base_path},
        command::command,
        manifest::{
            InitializableManifestConfig, InitializableManifestConfigMetadata, ManifestData,
            RouterInitializationMetadata, remove_router_definition_from_manifest,
            router::RouterManifestData,
        },
        rendered_template::{RenderedTemplate, RenderedTemplatesCache, write_rendered_templates},
    },
    prompt::{ArrayCompleter, prompt_for_confirmation, prompt_with_validation},
};

#[derive(Debug)]
pub(super) struct RouterCommand;

impl RouterCommand {
    pub(super) fn new() -> Self {
        Self {}
    }
}

impl CliCommand for RouterCommand {
    fn command(&self) -> Command {
        command("router", "Initialize a new router")
            .alias("rtr")
            .alias("controller")
            .arg(Arg::new("name").help("The name of the router"))
            .arg(
                Arg::new("base_path")
                    .short('p')
                    .long("path")
                    .help("The project path to delete the router from"),
            )
            .arg(
                Arg::new("continue")
                    .short('c')
                    .long("continue")
                    .help("Continue the eject operation")
                    .action(ArgAction::SetTrue),
            )
    }

    // pass token in from parent and perform get token above?
    fn handler(&self, matches: &ArgMatches) -> Result<()> {
        let mut line_editor = Editor::<ArrayCompleter, DefaultHistory>::new()?;
        let mut stdout = StandardStream::stdout(ColorChoice::Always);

        let (app_root_path, project_name) = find_app_root_path(matches, RequiredLocation::Project)?;
        let manifest_path = app_root_path.join(".forklaunch").join("manifest.toml");

        let existing_manifest_data: RouterManifestData = toml::from_str(
            &read_to_string(&manifest_path).with_context(|| ERROR_FAILED_TO_READ_MANIFEST)?,
        )
        .with_context(|| ERROR_FAILED_TO_PARSE_MANIFEST)?;

        let router_name = prompt_with_validation(
            &mut line_editor,
            &mut stdout,
            "name",
            matches,
            "router name",
            None,
            |input: &str| {
                existing_manifest_data
                    .clone()
                    .projects
                    .iter()
                    .any(|project| {
                        if let Some(routers) = &project.routers {
                            return routers.iter().any(|router| router == input);
                        }
                        false
                    })
            },
            |_| "Router not found".to_string(),
        )?;

        let router_base_path = prompt_base_path(
            &app_root_path,
            &ManifestData::Router(&existing_manifest_data),
            &project_name,
            &mut line_editor,
            &mut stdout,
            matches,
            1,
        )?;

        let mut manifest_data = existing_manifest_data.initialize(
            InitializableManifestConfigMetadata::Router(RouterInitializationMetadata {
                project_name: project_name.clone().unwrap(),
                router_name: Some(router_name.clone()),
            }),
        );

        let continue_delete_override = matches.get_flag("continue");

        if !continue_delete_override {
            let continue_delete = prompt_for_confirmation(
                &mut line_editor,
                "This operation is irreversible. Do you want to continue? (y/N) ",
            )?;

            if !continue_delete {
                log_error!(stdout, "Deletion cancelled");
                return Ok(());
            }
        }

        let manifest_content = remove_router_definition_from_manifest(
            &mut manifest_data,
            &project_name.unwrap(),
            &router_name,
        )?;

        let camel_case_name = router_name.to_case(Case::Camel);
        let pascal_case_name = router_name.to_case(Case::Pascal);

        let rendered_templates_cache = RenderedTemplatesCache::new();

        let file_paths = [
            router_base_path
                .join("api")
                .join("controllers")
                .join(format!("{}.controller.ts", camel_case_name)),
            router_base_path
                .join("api")
                .join("routes")
                .join(format!("{}.routes.ts", camel_case_name)),
            router_base_path
                .join("domain")
                .join("interfaces")
                .join(format!("{}.interface.ts", camel_case_name)),
            router_base_path
                .join("domain")
                .join("mappers")
                .join(format!("{}.mappers.ts", camel_case_name)),
            router_base_path
                .join("domain")
                .join("schemas")
                .join(format!("{}.schema.ts", camel_case_name)),
            router_base_path
                .join("domain")
                .join("types")
                .join(format!("{}.types.ts", camel_case_name)),
            router_base_path
                .join("persistence")
                .join("entities")
                .join(format!("{}Record.entity.ts", camel_case_name)),
            router_base_path
                .join("persistence")
                .join("seeders")
                .join(format!("{}Record.seeder.ts", camel_case_name)),
            router_base_path
                .join("domain")
                .join("services")
                .join(format!("{}.service.ts", camel_case_name)),
            router_base_path
                .join("__test__")
                .join(format!("{}.test.ts", camel_case_name)),
        ];

        for path in file_paths {
            if exists(path.clone())? {
                remove_file(path)?;
            }
        }

        let allocator = Allocator::default();
        let entities_path = router_base_path
            .join("persistence")
            .join("entities")
            .join("index.ts");
        let entities_index_source_text = read_to_string(&entities_path).unwrap();
        let mut entities_index_program = parse_ast_program(
            &allocator,
            &entities_index_source_text,
            SourceType::from_path(&entities_path).unwrap(),
        );
        let new_entities_index_content = delete_from_index_ts_export(
            &allocator,
            &mut entities_index_program,
            &format!("./{}Record.entity", camel_case_name).as_str(),
        )?;

        let seeders_path = router_base_path
            .join("persistence")
            .join("seeders")
            .join("index.ts");
        let seeders_index_source_text = read_to_string(&seeders_path).unwrap();
        let mut seeders_index_program = parse_ast_program(
            &allocator,
            &seeders_index_source_text,
            SourceType::from_path(&seeders_path).unwrap(),
        );
        let new_seeders_index_content = delete_from_index_ts_export(
            &allocator,
            &mut seeders_index_program,
            &format!("./{}Record.seeder", camel_case_name).as_str(),
        )?;

        let controllers_path = router_base_path
            .join("api")
            .join("controllers")
            .join("index.ts");
        let new_controllers_index_content = transform_controllers_index_ts_delete(
            &rendered_templates_cache,
            &router_name,
            &router_base_path,
        )?;

        let seed_data_path = router_base_path.join("persistence").join("seed.data.ts");
        let seed_data_source_text = read_to_string(&seed_data_path).unwrap();
        let mut seed_data_program = parse_ast_program(
            &allocator,
            &seed_data_source_text,
            SourceType::from_path(&seed_data_path).unwrap(),
        );
        let _ = delete_import_statement(
            &allocator,
            &mut seed_data_program,
            &format!("./entities/{}Record.entity", camel_case_name),
        )?;
        let new_seed_data_content =
            delete_from_seed_data_ts(&allocator, &mut seed_data_program, &camel_case_name)?;

        let registrations_path = router_base_path.join("registrations.ts");
        let registrations_text = read_to_string(&registrations_path)?;
        let registrations_type = SourceType::from_path(&registrations_path)?;
        let mut registrations_program =
            parse_ast_program(&allocator, &registrations_text, registrations_type);
        let _ = delete_import_statement(
            &allocator,
            &mut registrations_program,
            &format!("./domain/services/{}.service", camel_case_name),
        )?;
        let new_registrations_content = delete_from_registrations_ts_config_injector(
            &allocator,
            &mut registrations_program,
            &format!("{}Service", pascal_case_name),
            "serviceDependencies",
        )?;

        let server_program_path = router_base_path.join("server.ts");
        let server_program_text = read_to_string(&server_program_path)?;
        let server_program_type = SourceType::from_path(&server_program_path)?;
        let mut server_program =
            parse_ast_program(&allocator, &server_program_text, server_program_type);
        let new_server_content =
            delete_from_server_ts_router(&allocator, &mut server_program, &router_name)?;

        let sdk_path = router_base_path.join("sdk.ts");
        let sdk_text = read_to_string(&sdk_path)?;
        let sdk_type = SourceType::from_path(&sdk_path)?;
        let mut sdk_program = parse_ast_program(&allocator, &sdk_text, sdk_type);

        let new_sdk_content =
            delete_from_sdk_client_input(&allocator, &mut sdk_program, &camel_case_name)?;

        let new_test_utils_content = transform_test_utils_remove_router(
            &rendered_templates_cache,
            &router_base_path,
            &camel_case_name,
            &pascal_case_name,
        )?;

        let test_utils_path = router_base_path.join("__test__").join("test-utils.ts");

        write_rendered_templates(
            &vec![
                RenderedTemplate {
                    path: manifest_path,
                    content: manifest_content,
                    context: Some(ERROR_FAILED_TO_WRITE_MANIFEST.to_string()),
                },
                RenderedTemplate {
                    path: server_program_path,
                    content: new_server_content,
                    context: Some(ERROR_FAILED_TO_WRITE_SERVICE_FILES.to_string()),
                },
                RenderedTemplate {
                    path: registrations_path,
                    content: new_registrations_content,
                    context: Some(ERROR_FAILED_TO_WRITE_SERVICE_FILES.to_string()),
                },
                RenderedTemplate {
                    path: seed_data_path,
                    content: new_seed_data_content,
                    context: Some(ERROR_FAILED_TO_WRITE_SERVICE_FILES.to_string()),
                },
                RenderedTemplate {
                    path: seeders_path,
                    content: new_seeders_index_content,
                    context: Some(ERROR_FAILED_TO_WRITE_SERVICE_FILES.to_string()),
                },
                RenderedTemplate {
                    path: entities_path,
                    content: new_entities_index_content,
                    context: Some(ERROR_FAILED_TO_WRITE_SERVICE_FILES.to_string()),
                },
                RenderedTemplate {
                    path: sdk_path,
                    content: new_sdk_content,
                    context: Some(ERROR_FAILED_TO_WRITE_SERVICE_FILES.to_string()),
                },
                RenderedTemplate {
                    path: controllers_path,
                    content: new_controllers_index_content,
                    context: Some(ERROR_FAILED_TO_WRITE_SERVICE_FILES.to_string()),
                },
                RenderedTemplate {
                    path: test_utils_path,
                    content: new_test_utils_content,
                    context: Some(ERROR_FAILED_TO_WRITE_SERVICE_FILES.to_string()),
                },
            ],
            false,
            &mut stdout,
        )?;

        log_ok!(stdout, "{} deleted successfully!", router_name);

        Ok(())
    }
}

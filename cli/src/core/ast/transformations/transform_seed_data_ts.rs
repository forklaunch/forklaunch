use std::path::Path;

use anyhow::{Context, Result};
use convert_case::{Case, Casing};
use oxc_allocator::Allocator;
use oxc_ast::ast::SourceType;
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::error_failed_to_read_file,
    core::{
        ast::{
            injections::inject_into_import_statement::inject_into_import_statement,
            parse_ast_program::parse_ast_program,
        },
        manifest::ProjectType,
        rendered_template::RenderedTemplatesCache,
    },
};

pub(crate) fn transform_seed_data_ts(
    rendered_templates_cache: &RenderedTemplatesCache,
    router_name: &str,
    project_type: &ProjectType,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let seed_data_path = base_path.join("persistence").join("seed.data.ts");
    let template = rendered_templates_cache
        .get(&seed_data_path)?
        .context(error_failed_to_read_file(&seed_data_path))?;
    let seed_data_source_text = template.content;
    let seed_data_source_type = SourceType::from_path(&seed_data_path)?;
    let router_name_camel_case = router_name.to_case(Case::Camel);
    let router_name_pascal_case = router_name.to_case(Case::Pascal);

    let mut seed_data_program =
        parse_ast_program(&allocator, &seed_data_source_text, seed_data_source_type);

    let seed_data_import_text = format!(
        "import {{ type {router_name_pascal_case}Record }} from './entities/{router_name_camel_case}Record.entity';",
    );
    let mut seed_data_import_injection =
        parse_ast_program(&allocator, &seed_data_import_text, seed_data_source_type);

    inject_into_import_statement(
        &mut seed_data_program,
        &mut seed_data_import_injection,
        format!("./entities/{router_name_camel_case}Record.entity").as_str(),
        &seed_data_source_text,
    )?;

    let seed_data_text = format!(
        "export const {router_name_camel_case}RecordData = {{
            message: 'Test message'{},
            createdAt: new Date(),
            updatedAt: new Date(),
        }} satisfies RequiredEntityData<{router_name_pascal_case}Record>;",
        if project_type == &ProjectType::Worker {
            ",
            processed: false,
            retryCount: 0
            "
        } else {
            ""
        }
    );

    let mut seed_data_injection = parse_ast_program(&allocator, &seed_data_text, SourceType::ts());

    seed_data_program
        .body
        .extend(seed_data_injection.body.drain(..));

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&seed_data_program)
        .code)
}

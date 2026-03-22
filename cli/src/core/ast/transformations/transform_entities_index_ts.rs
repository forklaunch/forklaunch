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
            injections::inject_into_index_ts::inject_into_index_ts_export,
            parse_ast_program::parse_ast_program,
        },
        rendered_template::RenderedTemplatesCache,
    },
};

pub(crate) fn transform_entities_index_ts(
    rendered_templates_cache: &RenderedTemplatesCache,
    router_name: &str,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let entities_index_path = base_path
        .join("persistence")
        .join("entities")
        .join("index.ts");
    let template = rendered_templates_cache
        .get(&entities_index_path)?
        .context(error_failed_to_read_file(&entities_index_path))?;
    let entities_index_source_text = template.content;
    let entities_index_source_type = SourceType::from_path(&entities_index_path)?;
    let router_name_camel_case = router_name.to_case(Case::Camel);
    let router_name_pascal_case = router_name.to_case(Case::Pascal);

    let mut entities_index_program = parse_ast_program(
        &allocator,
        &entities_index_source_text,
        entities_index_source_type,
    );

    let entities_index_text = format!(
        "export {{ {router_name_pascal_case}Record }} from './{router_name_camel_case}Record.entity';",
    );
    let mut injection_program_ast =
        parse_ast_program(&allocator, &entities_index_text, SourceType::ts());

    inject_into_index_ts_export(
        &mut entities_index_program,
        &mut injection_program_ast,
        &router_name_camel_case,
    )?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&entities_index_program)
        .code)
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, write};

    use tempfile::TempDir;

    use super::*;
    use crate::core::rendered_template::RenderedTemplatesCache;

    fn create_test_entities_index() -> &'static str {
        r#"export { UserRecord } from './userRecord.entity';
export { RoleRecord } from './roleRecord.entity';
export { PermissionRecord } from './permissionRecord.entity';
"#
    }

    fn create_temp_entities_structure(content: &str) -> TempDir {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let temp_path = temp_dir.path();

        create_dir_all(temp_path.join("persistence/entities"))
            .expect("Failed to create persistence/entities directory");

        write(temp_path.join("persistence/entities/index.ts"), content)
            .expect("Failed to write index.ts");

        temp_dir
    }

    #[test]
    fn test_transform_entities_index_adds_export() {
        let content = create_test_entities_index();
        let temp_dir = create_temp_entities_structure(content);
        let temp_path = temp_dir.path();

        let cache = RenderedTemplatesCache::new();
        let result = transform_entities_index_ts(&cache, "organization", temp_path);

        assert!(result.is_ok());

        let transformed = result.unwrap();

        assert!(transformed.contains("export { OrganizationRecord }"));
        assert!(transformed.contains("from \"./organizationRecord.entity\""));

        assert!(transformed.contains("export { UserRecord }"));
        assert!(transformed.contains("export { RoleRecord }"));
        assert!(transformed.contains("export { PermissionRecord }"));
    }

    #[test]
    fn test_transform_entities_index_kebab_case_router() {
        let content = create_test_entities_index();
        let temp_dir = create_temp_entities_structure(content);
        let temp_path = temp_dir.path();

        let cache = RenderedTemplatesCache::new();
        let result = transform_entities_index_ts(&cache, "order-item", temp_path);

        assert!(result.is_ok());

        let transformed = result.unwrap();

        assert!(transformed.contains("export { OrderItemRecord }"));
        assert!(transformed.contains("from \"./orderItemRecord.entity\""));
    }

    #[test]
    fn test_transform_entities_index_empty_file() {
        let content = "";
        let temp_dir = create_temp_entities_structure(content);
        let temp_path = temp_dir.path();
        let cache = RenderedTemplatesCache::new();

        let result = transform_entities_index_ts(&cache, "product", temp_path);

        assert!(result.is_ok());

        let transformed = result.unwrap();

        assert!(transformed.contains("export { ProductRecord }"));
        assert!(transformed.contains("from \"./productRecord.entity\""));
    }
}

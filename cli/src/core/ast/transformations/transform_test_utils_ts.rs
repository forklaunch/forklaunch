use std::path::Path;

use anyhow::{Context, Result};
use oxc_allocator::Allocator;
use oxc_ast::ast::{
    Argument, BindingPatternKind, Declaration, Expression, Program, PropertyKey, SourceType,
    Statement,
};
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::{Database, Infrastructure, error_failed_to_read_file},
    core::{
        ast::{
            deletions::delete_from_test_utils::{
                delete_database_imports_from_test_utils, delete_database_setup_from_test_utils,
                delete_redis_imports_from_test_utils, delete_redis_setup_from_test_utils,
            },
            infrastructure::test_utils::{
                add_database_config_to_test_utils, add_database_imports_to_test_utils,
                add_kafka_env_vars_to_test_utils, add_redis_config_to_test_utils,
                add_redis_imports_to_test_utils, add_s3_env_vars_to_test_utils,
                remove_kafka_env_vars_from_test_utils, remove_s3_env_vars_from_test_utils,
            },
            parse_ast_program::parse_ast_program,
        },
        rendered_template::RenderedTemplatesCache,
    },
};

pub(crate) fn transform_test_utils_add_database(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    database: &Database,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    add_database_imports_to_test_utils(&allocator, &test_utils_text, &mut test_utils_program)?;
    add_database_config_to_test_utils(&allocator, &mut test_utils_program, database)?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

pub(crate) fn transform_test_utils_remove_database(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    delete_database_imports_from_test_utils(&allocator, &mut test_utils_program);
    delete_database_setup_from_test_utils(&allocator, &mut test_utils_program);

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

pub(crate) fn transform_test_utils_add_redis(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    add_redis_imports_to_test_utils(&allocator, &test_utils_text, &mut test_utils_program)?;
    add_redis_config_to_test_utils(&allocator, &mut test_utils_program)?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

pub(crate) fn transform_test_utils_remove_redis(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    delete_redis_imports_from_test_utils(&allocator, &mut test_utils_program);

    delete_redis_setup_from_test_utils(&allocator, &mut test_utils_program);

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

pub(crate) fn transform_test_utils_add_kafka(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    add_kafka_env_vars_to_test_utils(&allocator, &mut test_utils_program)?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

pub(crate) fn transform_test_utils_remove_kafka(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    remove_kafka_env_vars_from_test_utils(&allocator, &mut test_utils_program);

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

pub(crate) fn transform_test_utils_add_s3(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    add_s3_env_vars_to_test_utils(&allocator, &mut test_utils_program)?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

pub(crate) fn transform_test_utils_remove_s3(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    remove_s3_env_vars_from_test_utils(&allocator, &mut test_utils_program);

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

pub(crate) fn transform_test_utils_add_infrastructure(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    infrastructure: &Infrastructure,
) -> Result<String> {
    match infrastructure {
        Infrastructure::Redis => {
            transform_test_utils_add_redis(rendered_templates_cache, base_path)
        }
        Infrastructure::S3 => transform_test_utils_add_s3(rendered_templates_cache, base_path),
    }
}

pub(crate) fn transform_test_utils_remove_infrastructure(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    infrastructure: &Infrastructure,
) -> Result<String> {
    match infrastructure {
        Infrastructure::Redis => {
            transform_test_utils_remove_redis(rendered_templates_cache, base_path)
        }
        Infrastructure::S3 => transform_test_utils_remove_s3(rendered_templates_cache, base_path),
    }
}

pub(crate) fn transform_test_utils_add_router(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    router_name_camel: &str,
    router_name_pascal: &str,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    add_router_entity_to_setup_test_data(
        &allocator,
        &mut test_utils_program,
        router_name_camel,
        router_name_pascal,
    )?;

    add_router_mock_data_export(&allocator, &mut test_utils_program, router_name_pascal)?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

fn add_router_entity_to_setup_test_data<'a>(
    allocator: &'a Allocator,
    test_utils_program: &mut Program<'a>,
    router_name_camel: &str,
    router_name_pascal: &str,
) -> Result<()> {
    let entity_import = allocator.alloc_str(&format!(
        "const {{ {}Record }} = await import('../persistence/entities/{}Record.entity');",
        router_name_pascal, router_name_camel
    ));
    let mut import_program = parse_ast_program(allocator, entity_import, SourceType::ts());

    let entity_creation = allocator.alloc_str(&format!(
        "em.create({}Record, {{ id: '123e4567-e89b-12d3-a456-426614174000', message: 'Test message', createdAt: new Date(), updatedAt: new Date() }});",
        router_name_pascal
    ));
    let mut creation_program = parse_ast_program(allocator, entity_creation, SourceType::ts());

    for stmt in test_utils_program.body.iter_mut() {
        if let Statement::ExportNamedDeclaration(export_decl) = stmt {
            if let Some(Declaration::VariableDeclaration(var_decl)) =
                &mut export_decl.declaration
            {
                for declarator in var_decl.declarations.iter_mut() {
                    if let BindingPatternKind::BindingIdentifier(id) =
                        &declarator.id.kind
                    {
                        if id.name == "setupTestData" {
                            if let Some(Expression::ArrowFunctionExpression(
                                arrow_fn,
                            )) = &mut declarator.init
                            {
                                let body = &mut arrow_fn.body.statements;
                                let insert_pos = if body.is_empty() {
                                    0
                                } else {
                                    body.len().saturating_sub(1)
                                };

                                if let Some(creation_stmt) = creation_program.body.pop() {
                                    body.insert(insert_pos, creation_stmt);
                                }

                                if let Some(import_stmt) = import_program.body.pop() {
                                    body.insert(insert_pos, import_stmt);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn transform_test_utils_remove_router(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    router_name_camel: &str,
    router_name_pascal: &str,
) -> Result<String> {
    let allocator = Allocator::default();
    let test_utils_path = base_path.join("__test__").join("test-utils.ts");
    let template = rendered_templates_cache
        .get(&test_utils_path)?
        .context(error_failed_to_read_file(&test_utils_path))?;
    let test_utils_text = template.content;
    let test_utils_type = SourceType::from_path(&test_utils_path)?;

    let mut test_utils_program = parse_ast_program(&allocator, &test_utils_text, test_utils_type);

    remove_router_entity_from_setup_test_data(
        &mut test_utils_program,
        router_name_camel,
        router_name_pascal,
    )?;

    remove_router_mock_data_export(&mut test_utils_program, router_name_pascal)?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&test_utils_program)
        .code)
}

fn remove_router_entity_from_setup_test_data<'a>(
    test_utils_program: &mut Program<'a>,
    router_name_camel: &str,
    router_name_pascal: &str,
) -> Result<()> {
    for stmt in test_utils_program.body.iter_mut() {
        if let Statement::ExportNamedDeclaration(export_decl) = stmt {
            if let Some(Declaration::VariableDeclaration(var_decl)) =
                &mut export_decl.declaration
            {
                for declarator in var_decl.declarations.iter_mut() {
                    if let BindingPatternKind::BindingIdentifier(id) =
                        &declarator.id.kind
                    {
                        if id.name == "setupTestData" {
                            if let Some(Expression::ArrowFunctionExpression(
                                arrow_fn,
                            )) = &mut declarator.init
                            {
                                let body = &mut arrow_fn.body.statements;

                                body.retain(|stmt| {
                                    !matches_router_entity_statement(
                                        stmt,
                                        router_name_camel,
                                        router_name_pascal,
                                    )
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn matches_router_entity_statement(
    stmt: &Statement,
    _router_name_camel: &str,
    router_name_pascal: &str,
) -> bool {
    match stmt {
        Statement::VariableDeclaration(var_decl) => {
            for declarator in &var_decl.declarations {
                if let BindingPatternKind::ObjectPattern(obj_pattern) =
                    &declarator.id.kind
                {
                    for prop in &obj_pattern.properties {
                        if let PropertyKey::StaticIdentifier(key) = &prop.key {
                            if key.name == format!("{}Record", router_name_pascal) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            if let Expression::CallExpression(call_expr) = &expr_stmt.expression {
                if let Expression::StaticMemberExpression(member_expr) =
                    &call_expr.callee
                {
                    if member_expr.property.name == "create" {
                        if let Some(first_arg) = call_expr.arguments.first() {
                            if let Argument::Identifier(id) = &first_arg {
                                if id.name == format!("{}Record", router_name_pascal) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }

    false
}

fn remove_router_mock_data_export<'a>(
    test_utils_program: &mut Program<'a>,
    router_name_pascal: &str,
) -> Result<()> {
    let mock_data_name = format!("mock{}Data", router_name_pascal);

    test_utils_program.body.retain(|stmt| {
        if let Statement::ExportNamedDeclaration(export_decl) = stmt {
            if let Some(Declaration::VariableDeclaration(var_decl)) =
                &export_decl.declaration
            {
                for declarator in &var_decl.declarations {
                    if let BindingPatternKind::BindingIdentifier(id) =
                        &declarator.id.kind
                    {
                        if id.name == mock_data_name {
                            return false;
                        }
                    }
                }
            }
        }
        true
    });

    Ok(())
}

fn add_router_mock_data_export<'a>(
    allocator: &'a Allocator,
    test_utils_program: &mut Program<'a>,
    router_name_pascal: &str,
) -> Result<()> {
    let mock_data_export = allocator.alloc_str(&format!(
        "export const mock{}Data = {{\n  message: 'New test message'\n}};",
        router_name_pascal
    ));

    let mut mock_data_program = parse_ast_program(allocator, mock_data_export, SourceType::ts());

    test_utils_program.body.append(&mut mock_data_program.body);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{create_dir_all, write},
        path::PathBuf,
    };

    use tempfile::TempDir;

    use super::{
        transform_test_utils_add_database, transform_test_utils_add_infrastructure,
        transform_test_utils_add_kafka, transform_test_utils_add_redis,
        transform_test_utils_add_router, transform_test_utils_add_s3,
        transform_test_utils_remove_database, transform_test_utils_remove_infrastructure,
        transform_test_utils_remove_kafka, transform_test_utils_remove_redis,
        transform_test_utils_remove_router, transform_test_utils_remove_s3,
    };
    use crate::{
        constants::{Database, Infrastructure},
        core::rendered_template::RenderedTemplatesCache,
    };

    fn create_test_utils_base() -> String {
        r#"import {
  BlueprintTestHarness,
  clearTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';

export { TEST_TOKENS, TestSetupResult };

let harness = new BlueprintTestHarness({
  customEnvVars: {
    PROTOCOL: 'http',
    HOST: 'localhost',
    PORT: '3000'
  }
});

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export const clearDatabase = async (): Promise<void> => {
  await clearTestDatabase(undefined);
};

export const setupTestData = async () => {
};

export const mockTestData = {
  message: 'Test message'
};
"#
        .to_string()
    }

    fn setup_test_env() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        let test_dir = base_path.join("__test__");
        create_dir_all(&test_dir).unwrap();

        let test_utils_path = test_dir.join("test-utils.ts");
        write(&test_utils_path, create_test_utils_base()).unwrap();

        (temp_dir, base_path)
    }

    #[test]
    fn test_add_database_imports() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_database(&cache, &base_path, &Database::PostgreSQL);
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();

        assert!(content.contains("EntityManager") && content.contains("MikroORM"));
        assert!(content.contains("@mikro-orm/core"));
    }

    #[test]
    fn test_add_database_config() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_database(&cache, &base_path, &Database::PostgreSQL);
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();
        assert!(content.contains("getConfig"));
        assert!(content.contains("postgres"));
        assert!(content.contains("databaseType"));
        assert!(content.contains("useMigrations"));
        assert!(content.contains("false"));
    }

    #[test]
    fn test_remove_database() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_database(&cache, &base_path, &Database::PostgreSQL);
        assert!(result.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_remove_database(&cache, &base_path);
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(!content.contains("@mikro-orm/core"));
        assert!(!content.contains("getConfig"));
        assert!(!content.contains("databaseType"));
    }

    #[test]
    fn test_add_redis() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_redis(&cache, &base_path);
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();
        assert!(content.contains("Redis"));
        assert!(content.contains("ioredis"));
        assert!(content.contains("needsRedis"));
        assert!(content.contains("true"));
        assert!(content.contains("REDIS_URL"));
    }

    #[test]
    fn test_remove_redis() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_redis(&cache, &base_path);
        assert!(result.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_remove_redis(&cache, &base_path);
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(!content.contains("ioredis"));
        assert!(!content.contains("needsRedis"));
        assert!(!content.contains("REDIS_URL"));
    }

    #[test]
    fn test_add_kafka_env_vars() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_kafka(&cache, &base_path);
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();
        assert!(content.contains("needsKafka"));
        assert!(content.contains("true"));
    }

    #[test]
    fn test_remove_kafka_env_vars() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_kafka(&cache, &base_path);
        assert!(result.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_remove_kafka(&cache, &base_path);
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(!content.contains("needsKafka"));
    }

    #[test]
    fn test_add_s3_env_vars() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_s3(&cache, &base_path);
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();
        assert!(content.contains("needsS3"));
        assert!(content.contains("true"));
    }

    #[test]
    fn test_remove_s3_env_vars() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_s3(&cache, &base_path);
        assert!(result.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_remove_s3(&cache, &base_path);
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(!content.contains("needsS3"));
    }

    #[test]
    fn test_add_infrastructure_redis() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_test_utils_add_infrastructure(&cache, &base_path, &Infrastructure::Redis);
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();
        assert!(content.contains("ioredis"));
        assert!(content.contains("needsRedis"));
    }

    #[test]
    fn test_add_infrastructure_s3() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_test_utils_add_infrastructure(&cache, &base_path, &Infrastructure::S3);
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();
        assert!(content.contains("needsS3"));
    }

    #[test]
    fn test_remove_infrastructure_redis() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_test_utils_add_infrastructure(&cache, &base_path, &Infrastructure::Redis);
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result =
            transform_test_utils_remove_infrastructure(&cache, &base_path, &Infrastructure::Redis);
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(!content.contains("ioredis"));
        assert!(!content.contains("needsRedis"));
    }

    #[test]
    fn test_remove_infrastructure_s3() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_test_utils_add_infrastructure(&cache, &base_path, &Infrastructure::S3);
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result =
            transform_test_utils_remove_infrastructure(&cache, &base_path, &Infrastructure::S3);
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(!content.contains("needsS3"));
    }

    #[test]
    fn test_multiple_operations() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_database(&cache, &base_path, &Database::PostgreSQL);
        assert!(result.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_add_redis(&cache, &base_path);
        assert!(result.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_add_kafka(&cache, &base_path);
        assert!(result.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_add_s3(&cache, &base_path);
        assert!(result.is_ok());

        let content = result.unwrap();
        assert!(content.contains("@mikro-orm/core"));
        assert!(content.contains("ioredis"));
        assert!(content.contains("needsKafka"));
        assert!(content.contains("needsS3"));
    }

    #[test]
    fn test_idempotent_add() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let result1 = transform_test_utils_add_redis(&cache, &base_path);
        assert!(result1.is_ok());
        let content1 = result1.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content1).unwrap();

        let cache = RenderedTemplatesCache::new();
        let result2 = transform_test_utils_add_redis(&cache, &base_path);
        assert!(result2.is_ok());
        let content2 = result2.unwrap();

        let redis_count1 = content1.matches("needsRedis").count();
        let redis_count2 = content2.matches("needsRedis").count();
        assert_eq!(redis_count1, redis_count2);
    }

    #[test]
    fn test_different_database_types() {
        let (_temp_dir, base_path) = setup_test_env();
        let cache = RenderedTemplatesCache::new();

        let postgres_result =
            transform_test_utils_add_database(&cache, &base_path, &Database::PostgreSQL);
        assert!(postgres_result.is_ok());
        let content = postgres_result.unwrap();
        assert!(content.contains("postgres") && content.contains("databaseType"));

        let cache = RenderedTemplatesCache::new();
        let mysql_result = transform_test_utils_add_database(&cache, &base_path, &Database::MySQL);
        assert!(mysql_result.is_ok());
        let content = mysql_result.unwrap();
        assert!(content.contains("mysql") && content.contains("databaseType"));

        let cache = RenderedTemplatesCache::new();
        let mongodb_result =
            transform_test_utils_add_database(&cache, &base_path, &Database::MongoDB);
        assert!(mongodb_result.is_ok());
        let content = mongodb_result.unwrap();
        assert!(content.contains("mongodb") && content.contains("databaseType"));

        let cache = RenderedTemplatesCache::new();
        let sqlite_result =
            transform_test_utils_add_database(&cache, &base_path, &Database::SQLite);
        assert!(sqlite_result.is_ok());
        let content = sqlite_result.unwrap();
        assert!(content.contains("sqlite") && content.contains("databaseType"));
    }

    fn create_test_utils_with_database() -> String {
        r#"import {
  BlueprintTestHarness,
  clearTestDatabase,
  TEST_TOKENS,
  TestSetupResult
} from '@forklaunch/testing';
import { EntityManager, MikroORM } from '@mikro-orm/core';

export { TEST_TOKENS, TestSetupResult };

let harness: BlueprintTestHarness;

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      return config;
    },
    databaseType: 'postgres',
    useMigrations: false,
    customEnvVars: {
      PROTOCOL: 'http',
      HOST: 'localhost',
      PORT: '3000'
    }
  });

  return await harness.setup();
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

export const clearDatabase = async (orm: MikroORM): Promise<void> => {
  await clearTestDatabase(orm);
};

export const setupTestData = async (em: EntityManager) => {
  await em.flush();
};

export const mockTestData = {
  message: 'Test message'
};
"#
        .to_string()
    }

    fn setup_test_env_with_database() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        let test_dir = base_path.join("__test__");
        create_dir_all(&test_dir).unwrap();

        let test_utils_path = test_dir.join("test-utils.ts");
        write(&test_utils_path, create_test_utils_with_database()).unwrap();

        (temp_dir, base_path)
    }

    #[test]
    fn test_add_router_entity_to_setup_test_data() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_router(&cache, &base_path, "user", "User");
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();

        assert!(
            content.contains("UserRecord") && content.contains("await import"),
            "Entity import should be added"
        );

        assert!(
            content.contains("em.create(UserRecord") || content.contains("em.create(UserRecord,"),
            "Entity creation should be added"
        );
        assert!(
            content.contains("123e4567-e89b-12d3-a456-426614174000"),
            "Entity should have test ID"
        );
        assert!(
            content.contains("Test message"),
            "Entity should have test message"
        );
    }

    #[test]
    fn test_add_router_mock_data_export() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_router(&cache, &base_path, "product", "Product");
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();

        assert!(
            content.contains("export const mockProductData") || content.contains("mockProductData"),
            "Mock data export should be added"
        );
        assert!(
            content.contains("New test message"),
            "Mock data should have message field"
        );
    }

    #[test]
    fn test_transform_test_utils_add_router_full() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_router(&cache, &base_path, "order", "Order");
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();

        assert!(content.contains("const { OrderRecord } = await import"));

        assert!(content.contains("em.create(OrderRecord"));

        assert!(content.contains("export const mockOrderData"));

        let entity_pos = content.find("em.create(OrderRecord").unwrap();
        let flush_pos = content.find("await em.flush").unwrap();
        assert!(
            entity_pos < flush_pos,
            "Entity creation should come before flush"
        );

        let setup_data_pos = content.find("setupTestData").unwrap();
        let mock_data_pos = content.find("mockOrderData").unwrap();
        assert!(
            setup_data_pos < mock_data_pos,
            "Mock data should be after setupTestData function"
        );
    }

    #[test]
    fn test_router_with_different_casing() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_test_utils_add_router(&cache, &base_path, "userProfile", "UserProfile");
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();

        assert!(content.contains("UserProfileRecord"));
        assert!(content.contains("userProfileRecord.entity"));
        assert!(content.contains("mockUserProfileData"));
    }

    #[test]
    fn test_router_preserves_existing_content() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_router(&cache, &base_path, "category", "Category");
        assert!(result.is_ok());

        let content = result.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content).unwrap();

        assert!(content.contains("export { TEST_TOKENS, TestSetupResult }"));
        assert!(content.contains("export const setupTestDatabase"));
        assert!(content.contains("export const cleanupTestDatabase"));
        assert!(content.contains("export const clearDatabase"));
        assert!(content.contains("export const setupTestData"));

        assert!(content.contains("export const mockTestData"));

        assert!(content.contains("export const mockCategoryData"));
    }

    #[test]
    fn test_multiple_routers_sequentially() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result1 = transform_test_utils_add_router(&cache, &base_path, "post", "Post");
        assert!(result1.is_ok());
        let content1 = result1.unwrap();
        write(base_path.join("__test__").join("test-utils.ts"), &content1).unwrap();

        assert!(content1.contains("PostRecord"));
        assert!(content1.contains("mockPostData"));

        let cache = RenderedTemplatesCache::new();
        let result2 = transform_test_utils_add_router(&cache, &base_path, "comment", "Comment");
        assert!(result2.is_ok());
        let content2 = result2.unwrap();

        assert!(content2.contains("PostRecord"));
        assert!(content2.contains("CommentRecord"));
        assert!(content2.contains("mockPostData"));
        assert!(content2.contains("mockCommentData"));
    }

    #[test]
    fn test_remove_router_entity_from_setup_test_data() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result = transform_test_utils_add_router(&cache, &base_path, "user", "User");
        assert!(result.is_ok());
        let content_after_add = result.unwrap();
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &content_after_add,
        )
        .unwrap();

        assert!(content_after_add.contains("UserRecord"));
        assert!(content_after_add.contains("mockUserData"));

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_remove_router(&cache, &base_path, "user", "User");
        assert!(result.is_ok());
        let content_after_remove = result.unwrap();

        assert!(
            !content_after_remove.contains("UserRecord"),
            "UserRecord should be removed"
        );

        assert!(
            !content_after_remove.contains("mockUserData"),
            "mockUserData should be removed"
        );

        assert!(content_after_remove.contains("setupTestDatabase"));
        assert!(content_after_remove.contains("mockTestData"));
    }

    #[test]
    fn test_remove_router_preserves_other_routers() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result1 = transform_test_utils_add_router(&cache, &base_path, "user", "User");
        assert!(result1.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result1.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result2 = transform_test_utils_add_router(&cache, &base_path, "product", "Product");
        assert!(result2.is_ok());
        let content_with_both = result2.unwrap();
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &content_with_both,
        )
        .unwrap();

        assert!(content_with_both.contains("UserRecord"));
        assert!(content_with_both.contains("ProductRecord"));
        assert!(content_with_both.contains("mockUserData"));
        assert!(content_with_both.contains("mockProductData"));

        let cache = RenderedTemplatesCache::new();
        let result = transform_test_utils_remove_router(&cache, &base_path, "user", "User");
        assert!(result.is_ok());
        let content_after_remove = result.unwrap();

        assert!(!content_after_remove.contains("UserRecord"));
        assert!(!content_after_remove.contains("mockUserData"));

        assert!(content_after_remove.contains("ProductRecord"));
        assert!(content_after_remove.contains("mockProductData"));
    }

    #[test]
    fn test_remove_nonexistent_router() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result =
            transform_test_utils_remove_router(&cache, &base_path, "nonexistent", "Nonexistent");
        assert!(result.is_ok());

        let content = result.unwrap();

        assert!(content.contains("setupTestDatabase"));
        assert!(content.contains("mockTestData"));
    }

    #[test]
    fn test_add_and_remove_router_cycle() {
        let (_temp_dir, base_path) = setup_test_env_with_database();
        let cache = RenderedTemplatesCache::new();

        let result_add = transform_test_utils_add_router(&cache, &base_path, "order", "Order");
        assert!(result_add.is_ok());
        write(
            base_path.join("__test__").join("test-utils.ts"),
            &result_add.unwrap(),
        )
        .unwrap();

        let cache = RenderedTemplatesCache::new();
        let result_remove =
            transform_test_utils_remove_router(&cache, &base_path, "order", "Order");
        assert!(result_remove.is_ok());
        let final_content = result_remove.unwrap();

        assert!(final_content.contains("setupTestDatabase"));
        assert!(final_content.contains("setupTestData"));
        assert!(final_content.contains("mockTestData"));
        assert!(!final_content.contains("OrderRecord"));
        assert!(!final_content.contains("mockOrderData"));
    }
}

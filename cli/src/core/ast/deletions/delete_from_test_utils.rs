use anyhow::Result;
use oxc_allocator::{Allocator, CloneIn, Vec as AllocVec};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, Program, PropertyKey, Statement};

use crate::core::ast::deletions::delete_import_statement::delete_import_statement;

pub(crate) fn delete_database_imports_from_test_utils<'a>(
    allocator: &'a Allocator,
    test_utils_program: &mut Program<'a>,
) {
    let _ = delete_import_statement(allocator, test_utils_program, "@mikro-orm/core");
}

pub(crate) fn delete_redis_imports_from_test_utils<'a>(
    allocator: &'a Allocator,
    test_utils_program: &mut Program<'a>,
) {
    let _ = delete_import_statement(allocator, test_utils_program, "ioredis");
}

pub(crate) fn delete_database_setup_from_test_utils<'a>(
    allocator: &'a Allocator,
    test_utils_program: &mut Program<'a>,
) {
    let _ = delete_from_test_harness_options(allocator, test_utils_program, "getConfig");

    let _ = delete_from_test_harness_options(allocator, test_utils_program, "databaseType");
    let _ = delete_from_test_harness_options(allocator, test_utils_program, "useMigrations");
}

pub(crate) fn delete_redis_setup_from_test_utils<'a>(
    allocator: &'a Allocator,
    test_utils_program: &mut Program<'a>,
) {
    let _ = delete_from_test_harness_options(allocator, test_utils_program, "needsRedis");
    let _ = delete_from_test_harness_custom_env_vars(allocator, test_utils_program, "REDIS_URL");
}

pub(crate) fn delete_from_test_harness_options<'a>(
    allocator: &'a Allocator,
    test_utils_program: &mut Program<'a>,
    option_name: &str,
) -> Result<()> {
    for statement in &mut test_utils_program.body {
        let expression = match statement {
            Statement::VariableDeclaration(expr) => expr,
            _ => continue,
        };

        match expression.declarations[0].id.get_identifier_name() {
            Some(name) if name == "harness" => name,
            _ => continue,
        };

        let call_expression = match &mut expression.declarations[0].init {
            Some(Expression::NewExpression(call)) => call,
            _ => continue,
        };

        let object_expr = match call_expression.arguments.get_mut(0) {
            Some(Argument::ObjectExpression(object_expr)) => object_expr,
            _ => continue,
        };

        let new_properties = AllocVec::from_iter_in(
            object_expr.properties.iter().filter_map(|prop| {
                let prop = match prop {
                    ObjectPropertyKind::ObjectProperty(prop) => prop,
                    _ => return Some(prop.clone_in(allocator)),
                };
                let key = match &prop.key {
                    PropertyKey::StaticIdentifier(identifier) => identifier,
                    _ => return Some(ObjectPropertyKind::ObjectProperty(prop.clone_in(allocator))),
                };
                if key.name.as_str() == option_name {
                    None
                } else {
                    Some(ObjectPropertyKind::ObjectProperty(prop.clone_in(allocator)))
                }
            }),
            &allocator,
        );

        object_expr.properties = new_properties;
        return Ok(());
    }

    Ok(())
}

pub(crate) fn delete_from_test_harness_custom_env_vars<'a>(
    allocator: &'a Allocator,
    test_utils_program: &mut Program<'a>,
    env_var_name: &str,
) -> Result<()> {
    for statement in &mut test_utils_program.body {
        let expression = match statement {
            Statement::VariableDeclaration(expr) => expr,
            _ => continue,
        };

        match expression.declarations[0].id.get_identifier_name() {
            Some(name) if name == "harness" => name,
            _ => continue,
        };

        let call_expression = match &mut expression.declarations[0].init {
            Some(Expression::NewExpression(call)) => call,
            _ => continue,
        };

        let object_expr = match call_expression.arguments.get_mut(0) {
            Some(Argument::ObjectExpression(object_expr)) => object_expr,
            _ => continue,
        };

        for prop in object_expr.properties.iter_mut() {
            let prop = match prop {
                ObjectPropertyKind::ObjectProperty(prop) => prop,
                _ => continue,
            };
            let key = match &prop.key {
                PropertyKey::StaticIdentifier(identifier) => identifier,
                _ => continue,
            };

            if key.name.as_str() != "customEnvVars" {
                continue;
            }

            let custom_env_vars_obj = match &mut prop.value {
                Expression::ObjectExpression(obj) => obj,
                _ => continue,
            };

            let new_properties = AllocVec::from_iter_in(
                custom_env_vars_obj
                    .properties
                    .iter()
                    .filter_map(|env_prop| {
                        let env_prop = match env_prop {
                            ObjectPropertyKind::ObjectProperty(prop) => prop,
                            _ => return Some(env_prop.clone_in(allocator)),
                        };
                        let env_key = match &env_prop.key {
                            PropertyKey::StaticIdentifier(identifier) => identifier,
                            _ => {
                                return Some(ObjectPropertyKind::ObjectProperty(
                                    env_prop.clone_in(allocator),
                                ));
                            }
                        };
                        if env_key.name.as_str() == env_var_name {
                            None
                        } else {
                            Some(ObjectPropertyKind::ObjectProperty(
                                env_prop.clone_in(allocator),
                            ))
                        }
                    }),
                &allocator,
            );

            custom_env_vars_obj.properties = new_properties;
            return Ok(());
        }
    }

    Ok(())
}

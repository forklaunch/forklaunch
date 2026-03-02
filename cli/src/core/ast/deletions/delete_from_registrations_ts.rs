use std::{collections::HashSet, path::Path};

use anyhow::Result;
use oxc_allocator::{Allocator, CloneIn, Vec};
use oxc_ast::ast::{
    Argument, BindingPatternKind, Expression, ObjectPropertyKind, Program, PropertyKey, SourceType,
    Statement,
};
use oxc_codegen::{Codegen, CodegenOptions};

use crate::core::ast::{
    infrastructure::{
        redis::{
            delete_redis_import, delete_redis_ttl_cache_runtime_dependency,
            delete_redis_url_environment_variable,
        },
        s3::{
            delete_s3_import, delete_s3_object_store_runtime_dependency,
            delete_s3_url_environment_variable,
        },
    },
    parse_ast_program::parse_ast_program,
};

const WORKER_TYPE_SERVICES: &[&str] = &[
    "WorkerProducer",
    "WorkerConsumer",
    "WorkerService",
    "WorkerOptions",
];
const WORKER_TYPE_PROPERTY_KEYS: &[&str] = &[
    "REDIS_URL",
    "TtlCache",
    "KAFKA_BROKERS",
    "KAFKA_CLIENT_ID",
    "KAFKA_GROUP_ID",
    "DB_HOST",
    "DB_PORT",
    "DB_USER",
    "DB_PASSWORD",
    "DB_NAME",
    "EntityManager",
];

pub(crate) fn delete_from_registrations_ts_worker_type<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) {
    let mut used_property_keys = HashSet::new();
    for statement in &registrations_program.body {
        let expression = match statement {
            Statement::VariableDeclaration(expr) => expr,
            _ => continue,
        };

        let call_expression = match &expression.declarations[0].init {
            Some(Expression::CallExpression(call)) => call,
            _ => continue,
        };

        for argument in &call_expression.arguments {
            let object_expr = match argument {
                Argument::ObjectExpression(object_expr) => object_expr,
                _ => continue,
            };

            object_expr.properties.iter().for_each(|prop| {
                let prop = match prop {
                    ObjectPropertyKind::ObjectProperty(prop) => prop,
                    _ => return,
                };

                let inner_object_expr = match &prop.value {
                    Expression::ObjectExpression(object_expr) => object_expr,
                    _ => return,
                };

                let factory_prop =
                    match inner_object_expr
                        .properties
                        .iter()
                        .find(|inner_object_prop| {
                            let inner_object_prop = match &inner_object_prop {
                                ObjectPropertyKind::ObjectProperty(prop) => prop,
                                _ => return false,
                            };

                            let inner_key = match &inner_object_prop.key {
                                PropertyKey::StaticIdentifier(identifier) => identifier,
                                _ => return false,
                            };

                            inner_key.name.as_str() == "factory"
                        }) {
                        Some(factory_prop) => factory_prop,
                        None => return,
                    };

                match factory_prop {
                    ObjectPropertyKind::ObjectProperty(prop) => {
                        let key = match &prop.key {
                            PropertyKey::StaticIdentifier(identifier) => identifier,
                            _ => return,
                        };

                        if WORKER_TYPE_SERVICES.contains(&key.name.as_str()) {
                            return;
                        }

                        match &prop.value {
                            Expression::ArrowFunctionExpression(arrow_function_expr) => {
                                arrow_function_expr
                                    .params
                                    .items
                                    .first()
                                    .map(|param| match &param.pattern.kind {
                                        BindingPatternKind::ObjectPattern(object_pattern) => {
                                            object_pattern.properties.iter().for_each(|prop| {
                                                let key = match &prop.key {
                                                    PropertyKey::StaticIdentifier(identifier) => {
                                                        identifier
                                                    }
                                                    _ => return,
                                                };

                                                if WORKER_TYPE_PROPERTY_KEYS
                                                    .contains(&key.name.as_str())
                                                {
                                                    used_property_keys
                                                        .insert(key.name.as_str().to_string());
                                                }
                                            });
                                        }
                                        _ => return,
                                    });
                            }
                            _ => return,
                        }
                    }
                    _ => return,
                }
            });
        }
    }

    for statement in &mut registrations_program.body {
        let expression = match statement {
            Statement::VariableDeclaration(expr) => expr,
            _ => continue,
        };

        let call_expression = match &mut expression.declarations[0].init {
            Some(Expression::CallExpression(call)) => call,
            _ => continue,
        };

        for argument in &mut call_expression.arguments {
            let object_expr = match argument {
                Argument::ObjectExpression(object_expr) => object_expr,
                _ => continue,
            };

            object_expr.properties = Vec::from_iter_in(
                object_expr
                    .properties
                    .clone_in(allocator)
                    .into_iter()
                    .filter(|prop| {
                        let prop = match prop {
                            ObjectPropertyKind::ObjectProperty(prop) => prop,
                            _ => return true,
                        };

                        let key = match &prop.key {
                            PropertyKey::StaticIdentifier(identifier) => identifier,
                            _ => return true,
                        };

                        if WORKER_TYPE_SERVICES.contains(&key.name.as_str()) {
                            return false;
                        }

                        if WORKER_TYPE_PROPERTY_KEYS.contains(&key.name.as_str()) {
                            used_property_keys.contains(key.name.as_str())
                        } else {
                            true
                        }
                    }),
                allocator,
            );
        }
    }
}

pub(crate) fn delete_from_registrations_ts_infrastructure_redis(
    base_path: &Path,
    registrations_text: String,
) -> Result<String> {
    let allocator = Allocator::default();
    let registrations_path = base_path.join("registrations.ts");

    let registrations_type = SourceType::from_path(&registrations_path)?;

    let mut registrations_program =
        parse_ast_program(&allocator, &registrations_text, registrations_type);

    delete_redis_import(&allocator, &mut registrations_program);
    delete_redis_url_environment_variable(&allocator, &mut registrations_program);
    delete_redis_ttl_cache_runtime_dependency(&allocator, &mut registrations_program);

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&registrations_program)
        .code)
}

pub(crate) fn delete_from_registrations_ts_infrastructure_s3(
    base_path: &Path,
    registrations_text: String,
) -> Result<String> {
    let allocator = Allocator::default();
    let registrations_path = base_path.join("registrations.ts");

    let registrations_type = SourceType::from_path(&registrations_path)?;

    let mut registrations_program =
        parse_ast_program(&allocator, &registrations_text, registrations_type);

    delete_s3_import(&allocator, &mut registrations_program);
    delete_s3_url_environment_variable(&allocator, &mut registrations_program);
    delete_s3_object_store_runtime_dependency(&allocator, &mut registrations_program);

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&registrations_program)
        .code)
}

pub(crate) fn delete_from_registrations_ts_config_injector<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
    key_name: &str,
    declaration_name: &str,
) -> Result<String> {
    for statement in &mut registrations_program.body {
        let expression = match statement {
            Statement::VariableDeclaration(expr) => expr,
            _ => continue,
        };

        match expression.declarations[0].id.get_identifier_name() {
            Some(name) => {
                if name != declaration_name {
                    continue;
                }
            }
            None => continue,
        }

        let call_expression = match &mut expression.declarations[0].init {
            Some(Expression::CallExpression(call)) => call,
            _ => continue,
        };

        for argument in &mut call_expression.arguments {
            let object_expr = match argument {
                Argument::ObjectExpression(object_expr) => object_expr,
                _ => continue,
            };

            let mut new_properties = Vec::new_in(allocator);
            object_expr.properties.iter().for_each(|prop| {
                let prop = match prop {
                    ObjectPropertyKind::ObjectProperty(prop) => prop,
                    _ => return,
                };
                let key = match &prop.key {
                    PropertyKey::StaticIdentifier(identifier) => identifier,
                    _ => return,
                };

                if key.name.as_str() != key_name {
                    new_properties.push(ObjectPropertyKind::ObjectProperty(
                        prop.clone_in(&allocator),
                    ));
                }
            });
            object_expr.properties = new_properties;
        }
    }

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&registrations_program)
        .code)
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::SourceType;
    use oxc_codegen::{Codegen, CodegenOptions};

    use super::*;
    use crate::core::ast::parse_ast_program::parse_ast_program;

    #[test]
    fn test_delete_from_registrations_ts_worker_type() {
        let allocator = Allocator::default();

        let registrations_code = r#"
        const registrations = configInjector({
            workerService: {
                lifetime: Lifetime.Scoped,
                factory: (options) => {
                    const { REDIS_URL, TtlCache, DB_HOST } = options;
                    return new WorkerService(options);
                }
            },
            otherService: {
                lifetime: Lifetime.Scoped,
                factory: (options) => {
                    const { OTHER_PROP } = options;
                    return new OtherService();
                }
            },
            REDIS_URL: {
                lifetime: Lifetime.Singleton,
                type: string,
                value: "redis://localhost:6379"
            },
            TtlCache: {
                lifetime: Lifetime.Singleton,
                type: TtlCache,
                value: new TtlCache()
            },
            DB_HOST: {
                lifetime: Lifetime.Singleton,
                type: string,
                value: "localhost"
            },
            KAFKA_BROKERS: {
                lifetime: Lifetime.Singleton,
                type: string,
                value: "localhost:9092"
            },
            KAFKA_CLIENT_ID: {
                lifetime: Lifetime.Singleton,
                type: string,
                value: "test-client"
            },
            DB_PORT: {
                lifetime: Lifetime.Singleton,
                type: number,
                value: 5432
            }
        });
        "#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_code, SourceType::ts());

        delete_from_registrations_ts_worker_type(&allocator, &mut registrations_program);

        let expected_code = "const registrations = configInjector({\n\tworkerService: {\n\t\tlifetime: Lifetime.Scoped,\n\t\tfactory: (options) => {\n\t\t\tconst { REDIS_URL, TtlCache, DB_HOST } = options;\n\t\t\treturn new WorkerService(options);\n\t\t}\n\t},\n\totherService: {\n\t\tlifetime: Lifetime.Scoped,\n\t\tfactory: (options) => {\n\t\t\tconst { OTHER_PROP } = options;\n\t\t\treturn new OtherService();\n\t\t}\n\t}\n});\n";

        let generated_code = Codegen::new()
            .with_options(CodegenOptions::default())
            .build(&registrations_program)
            .code;

        assert_eq!(generated_code, expected_code);
    }

    #[test]
    fn test_delete_from_registrations_ts_config_injector() {
        let allocator = Allocator::default();

        let registrations_code = r#"
        const serviceDependencies = configInjector({
            userService: userServiceFactory,
            postService: postServiceFactory,
            commentService: commentServiceFactory
        });
        "#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_code, SourceType::ts());

        let result = delete_from_registrations_ts_config_injector(
            &allocator,
            &mut registrations_program,
            "postService",
            "serviceDependencies",
        );

        assert!(result.is_ok());

        let expected_code = "const serviceDependencies = configInjector({\n\tuserService: userServiceFactory,\n\tcommentService: commentServiceFactory\n});\n";

        assert_eq!(result.unwrap(), expected_code);
    }
}

use std::collections::HashSet;

use anyhow::Result;
use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, Program, PropertyKey, Statement};

/// Returns, in source order, the names of every `const <name> = <expr>.chain({ … })`
/// declaration in a registrations.ts program.
///
/// The config-injector pattern builds its dependency graph as a chain of
/// `.chain({ … })` calls: `configInjector` -> `environmentConfig` ->
/// `runtimeDependencies` -> `serviceDependencies` -> `expressApplicationOptions`.
/// Locating the chains structurally (by the `.chain(...)` call shape) lets a
/// caller target "the environment chain" or "the terminal chain" without
/// hardcoding a specific sibling entry's text, so a customized iam whose entries
/// have drifted is still wired correctly.
pub(crate) fn find_config_injector_chain_names(program: &Program<'_>) -> Vec<String> {
    let mut names = Vec::new();

    for statement in &program.body {
        let Statement::VariableDeclaration(decl) = statement else {
            continue;
        };

        for declarator in &decl.declarations {
            let Some(name) = declarator.id.get_identifier_name() else {
                continue;
            };

            let Some(Expression::CallExpression(call)) = &declarator.init else {
                continue;
            };

            if let Expression::StaticMemberExpression(member) = &call.callee
                && member.property.name == "chain"
            {
                names.push(name.to_string());
            }
        }
    }

    names
}

pub(crate) fn inject_into_registrations_config_injector<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
    config_injector_injection: &mut Program<'a>,
    declaration_name: &str,
) -> Result<()> {
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

            let mut property_keys = HashSet::new();
            object_expr.properties.iter().for_each(|prop| {
                let prop = match prop {
                    ObjectPropertyKind::ObjectProperty(prop) => prop,
                    _ => return,
                };
                let key = match &prop.key {
                    PropertyKey::StaticIdentifier(identifier) => identifier,
                    _ => return,
                };

                property_keys.insert(key.name.to_string());
            });

            for injected_stmt in config_injector_injection.body.iter_mut() {
                let injected_var_decl = match injected_stmt {
                    Statement::VariableDeclaration(decl) => decl,
                    _ => continue,
                };

                for injected_declarator in injected_var_decl.declarations.iter_mut() {
                    let injected_call_expr = match &mut injected_declarator.init {
                        Some(Expression::CallExpression(call_expr)) => call_expr,
                        _ => continue,
                    };

                    for injected_arg in &mut injected_call_expr.arguments {
                        let injected_obj = match injected_arg {
                            Argument::ObjectExpression(obj) => obj,
                            _ => continue,
                        };

                        for injected_prop in injected_obj.properties.iter() {
                            let prop = match injected_prop {
                                ObjectPropertyKind::ObjectProperty(prop) => prop,
                                _ => continue,
                            };

                            let key = match &prop.key {
                                PropertyKey::StaticIdentifier(identifier) => identifier,
                                _ => continue,
                            };

                            if !property_keys.contains(&key.name.to_string()) {
                                object_expr
                                    .properties
                                    .push(ObjectPropertyKind::ObjectProperty(
                                        prop.clone_in(&allocator),
                                    ));
                            }
                        }

                        return Ok(());
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use oxc_ast::ast::SourceType;
    use oxc_codegen::{Codegen, CodegenOptions};
    use oxc_parser::Parser;

    use super::*;

    fn parse_ast_program<'a>(
        allocator: &'a Allocator,
        source_text: &'a str,
        source_type: SourceType,
    ) -> Program<'a> {
        let ret = Parser::new(allocator, source_text, source_type).parse();
        ret.program
    }

    fn code_to_string(program: &Program) -> String {
        Codegen::new()
            .with_options(CodegenOptions::default())
            .build(program)
            .code
    }

    #[test]
    fn test_find_config_injector_chain_names_orders_the_chain() {
        let allocator = Allocator::default();

        // Mirrors the real registrations.ts shape: createConfigInjector(...) is
        // NOT a `.chain(...)` call, so only the four chained declarations count,
        // in source order, with the terminal chain last.
        let registrations = r#"
const configInjector = createConfigInjector(schemaValidator, {});
const environmentConfig = configInjector.chain({});
const runtimeDependencies = environmentConfig.chain({});
const serviceDependencies = runtimeDependencies.chain({});
const expressApplicationOptions = serviceDependencies.chain({});
export const createDependencyContainer = (envFilePath) => ({});
"#;
        let program = parse_ast_program(&allocator, registrations, SourceType::ts());

        let names = super::find_config_injector_chain_names(&program);

        assert_eq!(
            names,
            vec![
                "environmentConfig".to_string(),
                "runtimeDependencies".to_string(),
                "serviceDependencies".to_string(),
                "expressApplicationOptions".to_string(),
            ]
        );
        // The terminal chain is the last entry regardless of its name, which is
        // how the relay targets the chain where BetterAuth is in scope.
        assert_eq!(names.last().unwrap(), "expressApplicationOptions");
    }

    #[test]
    fn test_find_config_injector_chain_names_handles_a_renamed_terminal_chain() {
        let allocator = Allocator::default();

        // A drifted iam whose terminal chain is renamed: the finder still returns
        // it as the last (terminal) chain, so no verbatim name is required.
        let registrations = r#"
const environmentConfig = configInjector.chain({});
const runtimeDependencies = environmentConfig.chain({});
const applicationOptions = runtimeDependencies.chain({});
"#;
        let program = parse_ast_program(&allocator, registrations, SourceType::ts());

        let names = super::find_config_injector_chain_names(&program);

        assert_eq!(names.last().unwrap(), "applicationOptions");
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn test_find_config_injector_chain_names_empty_when_no_chains() {
        let allocator = Allocator::default();

        let registrations = r#"
const foo = 'bar';
const baz = someFn({});
"#;
        let program = parse_ast_program(&allocator, registrations, SourceType::ts());

        assert!(super::find_config_injector_chain_names(&program).is_empty());
    }

    #[test]
    fn test_inject_into_registrations_config_injector_successful_injection() {
        let allocator = Allocator::default();

        let registrations_content = r#"
import { configInjector } from '@forklaunch/core';

const ci = configInjector({
    database: {
        type: 'postgresql',
        host: 'localhost'
    },
    services: {
        user: true
    }
});
"#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_content, SourceType::ts());

        let injection_content = r#"
const newConfig = configInjector({
    database: {
        port: 5432
    },
    services: {
        order: true,
        product: false
    },
    logging: {
        level: 'info'
    }
});
"#;
        let mut injection_program =
            parse_ast_program(&allocator, injection_content, SourceType::ts());

        let result = inject_into_registrations_config_injector(
            &allocator,
            &mut registrations_program,
            &mut injection_program,
            "ci",
        );

        assert!(result.is_ok());

        let transformed_code = code_to_string(&registrations_program);

        assert!(transformed_code.contains("level: \"info\""));

        assert!(transformed_code.contains("type: \"postgresql\""));
        assert!(transformed_code.contains("host: \"localhost\""));
        assert!(transformed_code.contains("user: true"));
    }

    #[test]
    fn test_inject_into_registrations_config_injector_duplicate_properties() {
        let allocator = Allocator::default();

        let registrations_content = r#"
import { configInjector } from '@forklaunch/core';

const ci = configInjector({
    database: {
        type: 'postgresql',
        host: 'localhost'
    },
    services: {
        user: true
    }
});
"#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_content, SourceType::ts());

        let injection_content = r#"
const newConfig = configInjector({
    database: {
        type: 'mysql', // This should not be injected (duplicate)
        port: 5432     // This should be injected (new)
    },
    services: {
        user: false,   // This should not be injected (duplicate)
        order: true    // This should be injected (new)
    }
});
"#;
        let mut injection_program =
            parse_ast_program(&allocator, injection_content, SourceType::ts());

        let result = inject_into_registrations_config_injector(
            &allocator,
            &mut registrations_program,
            &mut injection_program,
            "ci",
        );

        assert!(result.is_ok());

        let transformed_code = code_to_string(&registrations_program);

        assert!(!transformed_code.contains("order: true"));

        assert!(transformed_code.contains("type: \"postgresql\""));
        assert!(transformed_code.contains("user: true"));
    }

    #[test]
    fn test_inject_into_registrations_config_injector_nested_objects() {
        let allocator = Allocator::default();

        let registrations_content = r#"
import { configInjector } from '@forklaunch/core';

const ci = configInjector({
    database: {
        type: 'postgresql',
        connection: {
            host: 'localhost',
            port: 5432
        }
    }
});
"#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_content, SourceType::ts());

        let injection_content = r#"
const newConfig = configInjector({
    database: {
        connection: {
            username: 'admin',
            password: 'secret'
        },
        pool: {
            min: 5,
            max: 20
        }
    }
});
"#;
        let mut injection_program =
            parse_ast_program(&allocator, injection_content, SourceType::ts());

        let result = inject_into_registrations_config_injector(
            &allocator,
            &mut registrations_program,
            &mut injection_program,
            "ci",
        );

        assert!(result.is_ok());

        let transformed_code = code_to_string(&registrations_program);

        assert!(!transformed_code.contains("pool: {"));

        assert!(transformed_code.contains("type: \"postgresql\""));
        assert!(transformed_code.contains("host: \"localhost\""));
        assert!(transformed_code.contains("port: 5432"));
    }

    #[test]
    fn test_inject_into_registrations_config_injector_wrong_declaration_name() {
        let allocator = Allocator::default();

        let registrations_content = r#"
import { configInjector } from '@forklaunch/core';

const ci = configInjector({
    database: {
        type: 'postgresql'
    }
});
"#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_content, SourceType::ts());

        let injection_content = r#"
const newConfig = configInjector({
    services: {
        user: true
    }
});
"#;
        let mut injection_program =
            parse_ast_program(&allocator, injection_content, SourceType::ts());

        let result = inject_into_registrations_config_injector(
            &allocator,
            &mut registrations_program,
            &mut injection_program,
            "wrongName",
        );

        assert!(result.is_ok());

        let transformed_code = code_to_string(&registrations_program);

        assert!(!transformed_code.contains("user: true"));
        assert!(transformed_code.contains("type: \"postgresql\""));
    }

    #[test]
    fn test_inject_into_registrations_config_injector_no_matching_declaration() {
        let allocator = Allocator::default();

        let registrations_content = r#"
import { configInjector } from '@forklaunch/core';

const otherVar = 'test';
const anotherVar = { key: 'value' };
"#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_content, SourceType::ts());

        let injection_content = r#"
const newConfig = configInjector({
    services: {
        user: true
    }
});
"#;
        let mut injection_program =
            parse_ast_program(&allocator, injection_content, SourceType::ts());

        let result = inject_into_registrations_config_injector(
            &allocator,
            &mut registrations_program,
            &mut injection_program,
            "ci",
        );

        assert!(result.is_ok());
        let transformed_code = code_to_string(&registrations_program);

        assert!(!transformed_code.contains("user: true"));
        assert!(transformed_code.contains("otherVar = \"test\""));
        assert!(transformed_code.contains("anotherVar = { key: \"value\" }"));
    }

    #[test]
    fn test_inject_into_registrations_config_injector_complex_structure() {
        let allocator = Allocator::default();

        let registrations_content = r#"
import { configInjector } from '@forklaunch/core';

const ci = configInjector({
    database: {
        type: 'postgresql',
        connection: {
            host: 'localhost',
            port: 5432
        }
    },
    services: {
        user: {
            enabled: true,
            config: {
                maxRetries: 3
            }
        }
    },
    logging: {
        level: 'info'
    }
});
"#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_content, SourceType::ts());

        let injection_content = r#"
const newConfig = configInjector({
    database: {
        connection: {
            username: 'admin',
            password: 'secret'
        },
        pool: {
            min: 5,
            max: 20
        }
    },
    services: {
        user: {
            config: {
                timeout: 5000
            }
        },
        order: {
            enabled: true,
            config: {
                maxItems: 100
            }
        }
    },
    monitoring: {
        enabled: true,
        metrics: {
            enabled: true
        }
    }
});
"#;
        let mut injection_program =
            parse_ast_program(&allocator, injection_content, SourceType::ts());

        let result = inject_into_registrations_config_injector(
            &allocator,
            &mut registrations_program,
            &mut injection_program,
            "ci",
        );

        assert!(result.is_ok());

        let transformed_code = code_to_string(&registrations_program);

        assert!(transformed_code.contains("monitoring: {"));

        assert!(transformed_code.contains("type: \"postgresql\""));
        assert!(transformed_code.contains("host: \"localhost\""));
        assert!(transformed_code.contains("port: 5432"));
        assert!(transformed_code.contains("maxRetries: 3"));
        assert!(transformed_code.contains("level: \"info\""));
    }

    #[test]
    fn test_inject_into_registrations_config_injector_empty_injection() {
        let allocator = Allocator::default();

        let registrations_content = r#"
import { configInjector } from '@forklaunch/core';

const ci = configInjector({
    database: {
        type: 'postgresql'
    }
});
"#;
        let mut registrations_program =
            parse_ast_program(&allocator, registrations_content, SourceType::ts());

        let injection_content = r#"
const newConfig = configInjector({});
"#;
        let mut injection_program =
            parse_ast_program(&allocator, injection_content, SourceType::ts());

        let result = inject_into_registrations_config_injector(
            &allocator,
            &mut registrations_program,
            &mut injection_program,
            "ci",
        );

        assert!(result.is_ok());

        let transformed_code = code_to_string(&registrations_program);

        assert!(transformed_code.contains("type: \"postgresql\""));
    }
}

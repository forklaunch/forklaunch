use oxc_allocator::{Allocator, CloneIn, Vec};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, Program, PropertyKey, Statement};

pub(crate) fn replace_registration_in_config_injector<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
    config_injector_injection: &mut Program<'a>,
    declaration_name: &str,
) {
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

                        let mut new_object_properties = Vec::new_in(&allocator);
                        for object_property in object_expr.properties.iter_mut() {
                            let mut found = false;
                            let property = match object_property {
                                ObjectPropertyKind::ObjectProperty(property) => property,
                                _ => continue,
                            };

                            let property_id = match &property.key {
                                PropertyKey::StaticIdentifier(property_id) => property_id,
                                _ => continue,
                            };

                            for injected_object_property in injected_obj.properties.iter_mut() {
                                let injected_property = match injected_object_property {
                                    ObjectPropertyKind::ObjectProperty(injected_property) => {
                                        injected_property
                                    }
                                    _ => continue,
                                };

                                let injected_property_id = match &injected_property.key {
                                    PropertyKey::StaticIdentifier(injected_property_id) => {
                                        injected_property_id
                                    }
                                    _ => continue,
                                };

                                if property_id.name == injected_property_id.name {
                                    found = true;
                                }
                            }

                            if !found {
                                new_object_properties.push(ObjectPropertyKind::ObjectProperty(
                                    property.clone_in(allocator),
                                ));
                            }
                        }

                        new_object_properties.extend(injected_obj.properties.drain(..));
                        object_expr.properties = new_object_properties;
                    }
                }
            }
        }
    }
}

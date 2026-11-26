use anyhow::{Result, bail};
use oxc_ast::ast::{
    BindingPattern, Declaration, Expression, ObjectPropertyKind, Program, PropertyKey, Statement,
    TSSignature, TSType,
};

pub(crate) fn inject_into_sdk_client_input<'a>(
    app_program_ast: &mut Program<'a>,
    injection_program_ast: &mut Program<'a>,
) -> Result<()> {
    inject_into_sdk_type_definition(app_program_ast, injection_program_ast)?;

    inject_into_sdk_client_object(app_program_ast, injection_program_ast)?;

    Ok(())
}

fn inject_into_sdk_type_definition<'a>(
    app_program_ast: &mut Program<'a>,
    injection_program_ast: &mut Program<'a>,
) -> Result<()> {
    for stmt in app_program_ast.body.iter_mut() {
        let export = match stmt {
            Statement::ExportDeclaration(export) => export,
            _ => continue,
        };

        let ts_declaration = match &mut export.declaration {
            Declaration::TSTypeAliasDeclaration(ts_decl) => ts_decl,
            _ => continue,
        };

        let is_sdk_type = ts_declaration.id.name.ends_with("Sdk");
        if !is_sdk_type {
            continue;
        }

        let type_literal = match &mut ts_declaration.type_annotation {
            TSType::TSTypeLiteral(literal) => literal,
            _ => continue,
        };

        for injection_stmt in &mut injection_program_ast.body {
            let injection_export = match injection_stmt {
                Statement::ExportDeclaration(export) => export,
                _ => continue,
            };

            let injection_ts_declaration = match &mut injection_export.declaration {
                Declaration::TSTypeAliasDeclaration(ts_decl) => ts_decl,
                _ => continue,
            };

            let injection_type_literal = match &mut injection_ts_declaration.type_annotation {
                TSType::TSTypeLiteral(literal) => literal,
                _ => continue,
            };

            let existing_keys: Vec<String> = type_literal
                .members
                .iter()
                .filter_map(|m| match m {
                    TSSignature::TSPropertySignature(prop) => match &prop.key {
                        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
                        PropertyKey::StringLiteral(lit) => Some(lit.value.display().to_string()),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();

            let mut moved: Vec<TSSignature> = vec![];
            for member in injection_type_literal.members.drain(..) {
                if let TSSignature::TSPropertySignature(prop_sig) = &member {
                    match &prop_sig.key {
                        PropertyKey::StaticIdentifier(id) => {
                            if existing_keys.contains(&id.name.to_string()) {
                                continue;
                            }
                        }
                        PropertyKey::StringLiteral(lit) => {
                            if existing_keys.contains(&lit.value.display().to_string()) {
                                continue;
                            }
                        }
                        _ => {}
                    }
                }
                moved.push(member);
            }
            type_literal.members.extend(moved);
            return Ok(());
        }
    }

    bail!("Failed to inject into Sdk type definition");
}

fn inject_into_sdk_client_object<'a>(
    app_program_ast: &mut Program<'a>,
    injection_program_ast: &mut Program<'a>,
) -> Result<()> {
    for stmt in app_program_ast.body.iter_mut() {
        let var_declaration = match stmt {
            Statement::VariableDeclaration(decl) => decl,
            Statement::ExportDeclaration(export) => match &mut export.declaration {
                Declaration::VariableDeclaration(decl) => decl,
                _ => continue,
            },
            _ => continue,
        };

        for decl in &mut var_declaration.declarations {
            let mut maybe_object = None;
            if let Some(init) = &mut decl.init {
                let mut expr_opt: Option<&mut Expression> = Some(init);
                while let Some(expr) = expr_opt {
                    match expr {
                        Expression::ObjectExpression(obj) => {
                            maybe_object = Some(obj);
                            break;
                        }
                        Expression::TSSatisfiesExpression(ts_sat) => {
                            expr_opt = Some(&mut ts_sat.expression);
                        }
                        Expression::TSAsExpression(ts_as) => {
                            expr_opt = Some(&mut ts_as.expression);
                        }
                        Expression::ParenthesizedExpression(paren) => {
                            expr_opt = Some(&mut paren.expression);
                        }
                        _ => {
                            break;
                        }
                    }
                }
            }

            let app_object = match maybe_object {
                Some(obj) => obj,
                None => continue,
            };

            let is_sdk_client_var = matches!(&decl.id, BindingPattern::BindingIdentifier(id) if id.name.ends_with("SdkClient"));
            if !is_sdk_client_var {
                continue;
            }

            for injection_stmt in &mut injection_program_ast.body {
                let injection_var_decl = match injection_stmt {
                    Statement::VariableDeclaration(decl) => decl,
                    Statement::ExportDeclaration(export) => match &mut export.declaration {
                        Declaration::VariableDeclaration(decl) => decl,
                        _ => continue,
                    },
                    _ => continue,
                };

                for injection_decl in &mut injection_var_decl.declarations {
                    if let Some(init) = &mut injection_decl.init {
                        let mut maybe_injection_object = None;
                        match init {
                            Expression::ObjectExpression(obj) => {
                                maybe_injection_object = Some(obj);
                            }
                            Expression::TSSatisfiesExpression(ts_sat) => {
                                if let Expression::ObjectExpression(obj) = &mut ts_sat.expression {
                                    maybe_injection_object = Some(obj);
                                }
                            }
                            _ => {}
                        }

                        if let Some(injection_object) = maybe_injection_object {
                            let existing_keys: Vec<String> = app_object
                                .properties
                                .iter()
                                .filter_map(|p| match p {
                                    ObjectPropertyKind::ObjectProperty(prop) => match &prop.key {
                                        PropertyKey::StaticIdentifier(id) => {
                                            Some(id.name.to_string())
                                        }
                                        PropertyKey::StringLiteral(lit) => {
                                            Some(lit.value.display().to_string())
                                        }
                                        _ => None,
                                    },
                                    _ => None,
                                })
                                .collect();

                            let mut moved: Vec<ObjectPropertyKind> = vec![];
                            for prop in injection_object.properties.drain(..) {
                                if let ObjectPropertyKind::ObjectProperty(prop_obj) = &prop {
                                    match &prop_obj.key {
                                        PropertyKey::StaticIdentifier(id) => {
                                            if existing_keys.contains(&id.name.to_string()) {
                                                continue;
                                            }
                                        }
                                        PropertyKey::StringLiteral(lit) => {
                                            if existing_keys
                                                .contains(&lit.value.display().to_string())
                                            {
                                                continue;
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                moved.push(prop);
                            }
                            app_object.properties.extend(moved);
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    bail!("Failed to inject into SdkClient object");
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::SourceType;
    use oxc_codegen::{Codegen, CodegenOptions};

    use super::*;
    use crate::core::ast::parse_ast_program::parse_ast_program;

    #[test]
    fn test_successful_injection() {
        let allocator = Allocator::default();

        let app_code = r#"
        export type TestSdk = {
            mySdkClientRoutes: typeof mySdkClientSdkRouter;
        };
        export const testSdkClient = {
            mySdkClientRoutes: mySdkClientSdkRouter
        } satisfies TestSdk;
        "#;
        let mut app_program = parse_ast_program(&allocator, app_code, SourceType::ts());

        let injection_code = r#"
        export type TestSdk = {
            mySdkClient2Routes: typeof mySdkClient2SdkRouter;
        };
        export const injectedSdkClient = {
            mySdkClient2Routes: mySdkClient2SdkRouter
        } satisfies TestSdk;
        "#;
        let mut injection_program = parse_ast_program(&allocator, injection_code, SourceType::ts());

        let result = inject_into_sdk_client_input(&mut app_program, &mut injection_program);

        assert!(result.is_ok());

        let generated = Codegen::new()
            .with_options(CodegenOptions::default())
            .build(&app_program)
            .code;
        assert!(generated.contains("mySdkClientRoutes"));
        assert!(generated.contains("mySdkClient2Routes"));
    }
}

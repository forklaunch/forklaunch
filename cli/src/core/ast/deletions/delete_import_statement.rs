use anyhow::Result;
use oxc_allocator::{Allocator, CloneIn, Vec};
use oxc_ast::ast::{Program, Statement};
use oxc_codegen::{Codegen, CodegenOptions};

pub(crate) fn delete_import_statement<'a>(
    allocator: &'a Allocator,
    app_program_ast: &mut Program<'a>,
    import_source_identifier: &str,
) -> Result<String> {
    let mut new_body = Vec::new_in(allocator);
    app_program_ast.body.iter().for_each(|stmt| {
        let import = match stmt {
            Statement::ImportDeclaration(import) => import,
            _ => {
                new_body.push(stmt.clone_in(allocator));
                return;
            }
        };

        if import.source.value.as_str() != import_source_identifier {
            new_body.push(stmt.clone_in(allocator));
        }
    });

    app_program_ast.body = new_body;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(app_program_ast)
        .code)
}

/// Deletes all import statements that start with the given prefix
pub(crate) fn delete_import_statements_with_prefix<'a>(
    allocator: &'a Allocator,
    app_program_ast: &mut Program<'a>,
    import_source_prefix: &str,
) -> Result<String> {
    let mut new_body = Vec::new_in(allocator);
    app_program_ast.body.iter().for_each(|stmt| {
        let import = match stmt {
            Statement::ImportDeclaration(import) => import,
            _ => {
                new_body.push(stmt.clone_in(allocator));
                return;
            }
        };

        if !import
            .source
            .value
            .as_str()
            .starts_with(import_source_prefix)
        {
            new_body.push(stmt.clone_in(allocator));
        }
    });

    app_program_ast.body = new_body;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(app_program_ast)
        .code)
}

#[allow(dead_code)]
pub(crate) fn delete_import_specifier<'a>(
    allocator: &'a Allocator,
    sdk_program_ast: &mut Program<'a>,
    import_specifier: &str,
    import_source_identifier: &str,
) -> Result<String> {
    let mut new_body = Vec::new_in(allocator);
    sdk_program_ast.body.iter_mut().for_each(|stmt| {
        let import = match stmt {
            Statement::ImportDeclaration(import) => import,
            _ => {
                new_body.push(stmt.clone_in(allocator));
                return;
            }
        };

        if import.source.value.as_str() == import_source_identifier {
            let mut new_specifiers = None;
            if let Some(specifiers) = &import.specifiers {
                if specifiers.len() == 1 && specifiers[0].name() == import_specifier {
                    new_specifiers = None;
                } else {
                    let mut specifier_vec = Vec::new_in(allocator);
                    for specifier in specifiers {
                        if specifier.name() != import_specifier {
                            specifier_vec.push(specifier.clone_in(allocator));
                        }
                    }
                    new_specifiers = Some(specifier_vec);
                }
            }
            if let Some(specifiers) = new_specifiers {
                import.specifiers = Some(specifiers);
                new_body.push(Statement::ImportDeclaration(import.clone_in(allocator)));
            }
        } else {
            new_body.push(stmt.clone_in(allocator));
        }
    });

    sdk_program_ast.body = new_body;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(sdk_program_ast)
        .code)
}

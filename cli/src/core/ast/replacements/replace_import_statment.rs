use anyhow::{Result, bail};
use oxc_ast::ast::{Program, Statement};

pub(crate) fn replace_import_statment<'a>(
    app_program_ast: &mut Program<'a>,
    injection_program_ast: &mut Program<'a>,
    import_source_identifier: &str,
) -> Result<()> {
    let mut injection_pos = None;
    let mut found_import = false;

    app_program_ast
        .body
        .iter()
        .enumerate()
        .for_each(|(index, stmt)| {
            let import = match stmt {
                Statement::ImportDeclaration(import) => import,
                _ => return,
            };

            if import.source.value == import_source_identifier {
                found_import = true;
                if injection_pos.is_none() {
                    injection_pos = Some(index);
                }
                return;
            }

            if !found_import {
                injection_pos = Some(index + 1);
            }
        });

    if let Some(index) = injection_pos {
        if found_import {
            let splice_values = injection_program_ast.body.drain(..);
            let splice_len = splice_values.len();
            let app_program_ast_body_len = app_program_ast.body.len();
            if index + splice_len > app_program_ast_body_len {
                let mut values = splice_values.collect::<Vec<_>>();
                let excess = values.split_off(app_program_ast_body_len - index);
                app_program_ast
                    .body
                    .splice(index..app_program_ast_body_len, values);
                app_program_ast.body.extend(excess);
            } else {
                app_program_ast
                    .body
                    .splice(index..index + splice_len, splice_values);
            }
        } else {
            for import in injection_program_ast.body.drain(..).rev() {
                app_program_ast.body.insert(index, import);
            }
        }
        Ok(())
    } else {
        bail!("Failed to inject into import statement")
    }
}

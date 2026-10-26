use anyhow::Result;
use oxc_allocator::{Allocator, CloneIn, Vec};
use oxc_ast::ast::{Declaration, Program, Statement};
use oxc_codegen::{Codegen, CodegenOptions};

pub(crate) fn delete_from_seed_data_ts<'a>(
    allocator: &'a Allocator,
    seed_data_program: &mut Program<'a>,
    import_source_identifier: &str,
) -> Result<String> {
    let mut new_body = Vec::new_in(&allocator);
    seed_data_program.body.iter().for_each(|stmt| {
        let export_statment = match stmt {
            Statement::ExportDeclaration(export) => export,
            _ => {
                new_body.push(stmt.clone_in(allocator));
                return;
            }
        };

        let var_statment = match &export_statment.declaration {
            Declaration::VariableDeclaration(var_decl) => var_decl,
            _ => {
                new_body.push(stmt.clone_in(allocator));
                return;
            }
        };

        if let Some(name) = &var_statment.declarations[0].id.get_identifier_name() {
            if name.contains(import_source_identifier) {
                return;
            }
        }

        new_body.push(stmt.clone_in(allocator));
    });

    seed_data_program.body = new_body;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(seed_data_program)
        .code)
}

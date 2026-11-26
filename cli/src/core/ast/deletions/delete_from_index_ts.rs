use anyhow::Result;
use oxc_allocator::{Allocator, CloneIn, Vec};
use oxc_ast::ast::{Program, Statement};
use oxc_codegen::{Codegen, CodegenOptions};

pub(crate) fn delete_from_index_ts_export<'a>(
    allocator: &'a Allocator,
    index_program: &mut Program<'a>,
    export_identifier: &str,
) -> Result<String> {
    let mut new_body = Vec::new_in(&allocator);
    index_program.body.iter().for_each(|stmt| {
        let should_skip = match stmt {
            Statement::ExportFromDeclaration(expr) => expr.source.value == export_identifier,
            Statement::ExportAllDeclaration(expr) => expr.source.value == export_identifier,
            _ => false,
        };

        if !should_skip {
            new_body.push(stmt.clone_in(allocator));
        }
    });

    index_program.body = new_body;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&index_program)
        .code)
}

use anyhow::{Result, bail};
use oxc_ast::ast::{Program, Statement};

pub(crate) fn inject_into_server_ts<'a, F>(
    app_program_ast: &mut Program<'a>,
    injection_program_ast: &mut Program<'a>,
    app_ts_injection_pos: F,
) -> Result<()>
where
    F: Fn(&oxc_allocator::Vec<'a, Statement>) -> Option<usize>,
{
    // First try top-level statements
    if let Some(splice_pos) = app_ts_injection_pos(&app_program_ast.body) {
        for stmt in injection_program_ast.body.drain(..).rev() {
            app_program_ast.body.insert(splice_pos, stmt);
        }
        return Ok(());
    }

    // Then try inside function declarations (e.g. async function startServer())
    for top_stmt in app_program_ast.body.iter_mut() {
        if let Statement::FunctionDeclaration(func) = top_stmt {
            if let Some(body) = func.body.as_mut() {
                if let Some(splice_pos) = app_ts_injection_pos(&body.statements) {
                    for stmt in injection_program_ast.body.drain(..).rev() {
                        body.statements.insert(splice_pos, stmt);
                    }
                    return Ok(());
                }
            }
        }
    }

    bail!("Failed to insert into server.ts")
}

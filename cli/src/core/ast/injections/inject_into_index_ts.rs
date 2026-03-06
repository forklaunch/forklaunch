use anyhow::Result;
use oxc_ast::ast::{Program, Statement};

pub(crate) fn inject_into_index_ts_export<'a>(
    index_program: &mut Program<'a>,
    injection_program_ast: &mut Program<'a>,
    router_name_camel_case: &str,
) -> Result<()> {
    let mut maybe_splice_pos = None;
    let target_module_path = format!("./{}.controller", router_name_camel_case);

    index_program
        .body
        .iter()
        .enumerate()
        .for_each(|(index, stmt)| {
            match stmt {
                Statement::ExportNamedDeclaration(expr) => {
                    if let Some(source) = &expr.source {
                        if source.value.as_str() <= target_module_path.as_str() {
                            maybe_splice_pos = Some(index);
                        }
                    }
                }
                Statement::ExportAllDeclaration(expr) => {
                    if expr.source.value.as_str() <= target_module_path.as_str() {
                        maybe_splice_pos = Some(index);
                    }
                }
                _ => {}
            };
        });

    let insert_pos = match maybe_splice_pos {
        Some(splice_pos) => splice_pos + 1,
        None => 0,
    };

    for stmt in injection_program_ast.body.drain(..).rev() {
        index_program.body.insert(insert_pos, stmt);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use oxc_allocator::Allocator;
    use oxc_ast::ast::SourceType;
    use oxc_codegen::{Codegen, CodegenOptions};

    use super::*;
    use crate::core::ast::parse_ast_program::parse_ast_program;

    fn codegen(program: &Program) -> String {
        Codegen::new()
            .with_options(CodegenOptions::default())
            .build(program)
            .code
    }

    #[test]
    fn injects_export_in_expected_sorted_position() {
        let allocator = Allocator::default();

        let index_text = "\
export * from './alpha.controller';
export * from './mango.controller';
export * from './zeta.controller';";

        let mut index_program = parse_ast_program(
            &allocator,
            index_text,
            SourceType::ts(),
        );

        let injection_text = "export * from './beta.controller';";
        let mut injection_program = parse_ast_program(
            &allocator,
            injection_text,
            SourceType::ts(),
        );

        let result =
            inject_into_index_ts_export(&mut index_program, &mut injection_program, "beta");
        assert!(result.is_ok());

        let output = codegen(&index_program);

        let alpha_pos = output.find("./alpha.controller").unwrap();
        let beta_pos = output.find("./beta.controller").unwrap();
        let mango_pos = output.find("./mango.controller").unwrap();
        let zeta_pos = output.find("./zeta.controller").unwrap();

        assert!(alpha_pos < beta_pos, "beta should come after alpha");
        assert!(beta_pos < mango_pos, "beta should come before mango");
        assert!(
            mango_pos < zeta_pos,
            "existing order should be preserved after insertion"
        );
    }

    #[test]
    fn inserts_at_start_when_smallest() {
        let allocator = Allocator::default();

        let index_text = "\
export * from './gamma.controller';
export * from './omega.controller';";

        let mut index_program = parse_ast_program(
            &allocator,
            index_text,
            SourceType::ts(),
        );

        let injection_text = "export * from './beta.controller';";
        let mut injection_program = parse_ast_program(
            &allocator,
            injection_text,
            SourceType::ts(),
        );

        let _ = inject_into_index_ts_export(&mut index_program, &mut injection_program, "beta");
        let output = codegen(&index_program);

        let beta_pos = output.find("./beta.controller").unwrap();
        let gamma_pos = output.find("./gamma.controller").unwrap();
        let omega_pos = output.find("./omega.controller").unwrap();

        assert!(beta_pos < gamma_pos);
        assert!(gamma_pos < omega_pos);
    }

    #[test]
    fn inserts_at_end_when_largest() {
        let allocator = Allocator::default();

        let index_text = "\
export * from './alpha.controller';
export * from './beta.controller';";

        let mut index_program = parse_ast_program(
            &allocator,
            index_text,
            SourceType::ts(),
        );

        let injection_text = "export * from './zeta.controller';";
        let mut injection_program = parse_ast_program(
            &allocator,
            injection_text,
            SourceType::ts(),
        );

        let _ = inject_into_index_ts_export(&mut index_program, &mut injection_program, "zeta");
        let output = codegen(&index_program);

        let alpha_pos = output.find("./alpha.controller").unwrap();
        let beta_pos = output.find("./beta.controller").unwrap();
        let zeta_pos = output.find("./zeta.controller").unwrap();

        assert!(alpha_pos < beta_pos);
        assert!(beta_pos < zeta_pos);
    }
}

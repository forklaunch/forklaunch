use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_ast::ast::{Program, SourceType};

use crate::core::ast::{
    injections::inject_into_registrations_ts::inject_into_registrations_config_injector,
    parse_ast_program::parse_ast_program,
};

pub(crate) fn database_entity_manager_runtime_dependency<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
    orm_token: &str,
    em_token: &str,
) -> Result<()> {
    let entity_manager_registration_text: &'static str = Box::leak(
        format!(
            "const configInjector = createConfigInjector(SchemaValidator(), {{
                {orm_token}: {{
                  lifetime: Lifetime.Singleton,
                  type: MikroORM,
                  factory: () => new MikroORM(mikroOrmOptionsConfig)
                }},
                {em_token}: {{
                  lifetime: Lifetime.Scoped,
                  type: EntityManager,
                  factory: ({{ {orm_token} }}, _resolve, context) =>
                    {orm_token}.em.fork(context?.entityManagerOptions as ForkOptions | undefined),
                }},
            }})"
        )
        .into_boxed_str(),
    );

    let mut entity_manager_registration_program = parse_ast_program(
        allocator,
        entity_manager_registration_text,
        SourceType::ts(),
    );

    inject_into_registrations_config_injector(
        allocator,
        registrations_program,
        &mut entity_manager_registration_program,
        "runtimeDependencies",
    )?;

    Ok(())
}

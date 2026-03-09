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
) -> Result<()> {
    let entity_manager_registration_text =
        "const configInjector = createConfigInjector(SchemaValidator(), {
                Orm: {
                  lifetime: Lifetime.Singleton,
                  type: MikroORM,
                  factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
                },
                EntityMgr: {
                  lifetime: Lifetime.Scoped,
                  type: EntityManager,
                  factory: ({ Orm }, _resolve, context) =>
                    Orm.em.fork(context?.entityManagerOptions as ForkOptions | undefined),
                },
            })";

    let mut entity_manager_registration_program = parse_ast_program(
        &allocator,
        &entity_manager_registration_text,
        SourceType::ts(),
    );

    inject_into_registrations_config_injector(
        &allocator,
        registrations_program,
        &mut entity_manager_registration_program,
        "runtimeDependencies",
    )?;

    Ok(())
}

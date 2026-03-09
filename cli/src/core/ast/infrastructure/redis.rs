use anyhow::Result;
use oxc_allocator::Allocator;
use oxc_ast::ast::{Program, SourceType};

use crate::core::ast::{
    deletions::{
        delete_from_registrations_ts::delete_from_registrations_ts_config_injector,
        delete_import_statement::delete_import_statement,
    },
    injections::inject_into_registrations_ts::inject_into_registrations_config_injector,
    parse_ast_program::parse_ast_program,
    replacements::replace_import_statment::replace_import_statment,
};

pub(crate) fn redis_import<'a>(
    allocator: &'a Allocator,
    registrations_text: &str,
    registrations_program: &mut Program<'a>,
) -> Result<()> {
    if !registrations_text
        .contains("import { RedisTtlCache } from \"@forklaunch/infrastructure-redis\";")
    {
        let import_text = "import { RedisTtlCache } from \"@forklaunch/infrastructure-redis\";";

        let mut import_program = parse_ast_program(&allocator, import_text, SourceType::ts());

        let _ = replace_import_statment(
            registrations_program,
            &mut import_program,
            "@forklaunch/infrastructure-redis",
        );
    }

    Ok(())
}

pub(crate) fn redis_url_environment_variable<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) -> Result<()> {
    let redis_env_var_text = "const configInjector = createConfigInjector(SchemaValidator(), {
                REDIS_URL: {
                    lifetime: Lifetime.Singleton,
                    type: string,
                    value: getEnvVar('REDIS_URL')
                }
            });";

    let mut redis_env_var_program =
        parse_ast_program(&allocator, &redis_env_var_text, SourceType::ts());

    inject_into_registrations_config_injector(
        &allocator,
        registrations_program,
        &mut redis_env_var_program,
        "environmentConfig",
    )?;

    Ok(())
}

pub(crate) fn redis_ttl_cache_runtime_dependency<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) -> Result<()> {
    let redis_registration_text = "const configInjector = createConfigInjector(SchemaValidator(), {
                TtlCache: {
                    lifetime: Lifetime.Singleton,
                    type: RedisTtlCache,
                    factory: ({ REDIS_URL, OtelCollector }) =>
                        new RedisTtlCache(60 * 60 * 1000, OtelCollector, {
                            url: REDIS_URL,
                        }, {
                            enabled: true,
                            level: 'info',
                        }),
                }
            });";

    let mut redis_registration_program =
        parse_ast_program(&allocator, &redis_registration_text, SourceType::ts());

    inject_into_registrations_config_injector(
        &allocator,
        registrations_program,
        &mut redis_registration_program,
        "runtimeDependencies",
    )?;

    Ok(())
}

pub(crate) fn delete_redis_import<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) {
    let _ = delete_import_statement(
        &allocator,
        registrations_program,
        "@forklaunch/infrastructure-redis",
    );
}

pub(crate) fn delete_redis_url_environment_variable<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) {
    let _ = delete_from_registrations_ts_config_injector(
        &allocator,
        registrations_program,
        "REDIS_URL",
        "environmentConfig",
    );
}

pub(crate) fn delete_redis_ttl_cache_runtime_dependency<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) {
    let _ = delete_from_registrations_ts_config_injector(
        &allocator,
        registrations_program,
        "TtlCache",
        "runtimeDependencies",
    );
}

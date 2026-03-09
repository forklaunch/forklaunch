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

pub(crate) fn s3_import<'a>(
    allocator: &'a Allocator,
    registrations_text: &str,
    registrations_program: &mut Program<'a>,
) -> Result<()> {
    if !registrations_text
        .contains("import { S3ObjectStore } from \"@forklaunch/infrastructure-s3\";")
    {
        let import_text = "import { S3ObjectStore } from \"@forklaunch/infrastructure-s3\";";

        let mut import_program = parse_ast_program(&allocator, import_text, SourceType::ts());

        let _ = replace_import_statment(
            registrations_program,
            &mut import_program,
            "@forklaunch/infrastructure-s3",
        );
    }

    Ok(())
}

pub(crate) fn s3_url_environment_variable<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) -> Result<()> {
    let s3_env_var_text = "const configInjector = createConfigInjector(SchemaValidator(), {
        S3_REGION: {
            lifetime: Lifetime.Singleton,
            type: string,
            value: getEnvVar('S3_REGION')
        },
        S3_ACCESS_KEY_ID: {
            lifetime: Lifetime.Singleton,
            type: string,
            value: getEnvVar('S3_ACCESS_KEY_ID')
        },
        S3_SECRET_ACCESS_KEY: {
            lifetime: Lifetime.Singleton,
            type: string,
            value: getEnvVar('S3_SECRET_ACCESS_KEY')
        },
        S3_URL: {
            lifetime: Lifetime.Singleton,
            type: string,
            value: getEnvVar('S3_URL')
        },
        S3_BUCKET: {
            lifetime: Lifetime.Singleton,
            type: string,
            value: getEnvVar('S3_BUCKET')
        }
    });";

    let mut s3_env_var_program = parse_ast_program(&allocator, &s3_env_var_text, SourceType::ts());

    inject_into_registrations_config_injector(
        &allocator,
        registrations_program,
        &mut s3_env_var_program,
        "environmentConfig",
    )?;

    Ok(())
}

pub(crate) fn s3_object_store_runtime_dependency<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) -> Result<()> {
    let s3_registration_text = "const configInjector = createConfigInjector(SchemaValidator(), {
        ObjectStore: {
            lifetime: Lifetime.Singleton,
            type: S3ObjectStore,
            factory: ({
                OtelCollector,
                OTEL_LEVEL,
                S3_REGION,
                S3_ACCESS_KEY_ID,
                S3_SECRET_ACCESS_KEY,
                S3_URL,
                S3_BUCKET
            }) =>
                new S3ObjectStore(
                OtelCollector,
                {
                    bucket: S3_BUCKET,
                    clientConfig: {
                    endpoint: S3_URL,
                    region: S3_REGION,
                    credentials: {
                        accessKeyId: S3_ACCESS_KEY_ID,
                        secretAccessKey: S3_SECRET_ACCESS_KEY
                    }
                    }
                },
                {
                    enabled: true,
                    level: OTEL_LEVEL || 'info'
                })
        }
    });";

    let mut s3_registration_program =
        parse_ast_program(&allocator, &s3_registration_text, SourceType::ts());

    inject_into_registrations_config_injector(
        &allocator,
        registrations_program,
        &mut s3_registration_program,
        "runtimeDependencies",
    )?;

    Ok(())
}

pub(crate) fn delete_s3_import<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) {
    let _ = delete_import_statement(
        &allocator,
        registrations_program,
        "@forklaunch/infrastructure-s3",
    );
}

pub(crate) fn delete_s3_url_environment_variable<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) {
    let _ = delete_from_registrations_ts_config_injector(
        &allocator,
        registrations_program,
        "S3_REGION",
        "environmentConfig",
    );
    let _ = delete_from_registrations_ts_config_injector(
        &allocator,
        registrations_program,
        "S3_ACCESS_KEY_ID",
        "environmentConfig",
    );
    let _ = delete_from_registrations_ts_config_injector(
        &allocator,
        registrations_program,
        "S3_SECRET_ACCESS_KEY",
        "environmentConfig",
    );
    let _ = delete_from_registrations_ts_config_injector(
        &allocator,
        registrations_program,
        "S3_BUCKET",
        "environmentConfig",
    );
    let _ = delete_from_registrations_ts_config_injector(
        &allocator,
        registrations_program,
        "S3_URL",
        "environmentConfig",
    );
}

pub(crate) fn delete_s3_object_store_runtime_dependency<'a>(
    allocator: &'a Allocator,
    registrations_program: &mut Program<'a>,
) {
    let _ = delete_from_registrations_ts_config_injector(
        &allocator,
        registrations_program,
        "ObjectStore",
        "runtimeDependencies",
    );
}

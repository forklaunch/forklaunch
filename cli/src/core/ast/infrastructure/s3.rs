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
    if !registrations_text.contains(
        "import { S3ObjectStore, s3ClientConfig } from \"@forklaunch/infrastructure-s3\";",
    ) {
        let import_text =
            "import { S3ObjectStore, s3ClientConfig } from \"@forklaunch/infrastructure-s3\";";

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
    type: optional(string),
    value: getEnvVar('S3_REGION')
  },
  S3_ACCESS_KEY_ID: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('S3_ACCESS_KEY_ID')
  },
  S3_SECRET_ACCESS_KEY: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('S3_SECRET_ACCESS_KEY')
  },
  S3_URL: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('S3_URL')
  },
  S3_BUCKET: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('S3_BUCKET')
  },
  S3_PREFIX: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('S3_PREFIX')
  },
  S3_PRESIGN_MAX_UPLOAD_SECONDS: {
    lifetime: Lifetime.Singleton,
    type: optional(number),
    value: Number(getEnvVar('S3_PRESIGN_MAX_UPLOAD_SECONDS')) || undefined
  },
  S3_PRESIGN_MAX_DOWNLOAD_SECONDS: {
    lifetime: Lifetime.Singleton,
    type: optional(number),
    value: Number(getEnvVar('S3_PRESIGN_MAX_DOWNLOAD_SECONDS')) || undefined
  },
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
    otel_token: &str,
) -> Result<()> {
    let s3_registration_text: &'static str = Box::leak(
        format!(
            "const configInjector = createConfigInjector(SchemaValidator(), {{
  ObjectStore: {{
    lifetime: Lifetime.Singleton,
    type: S3ObjectStore,
    factory: ({{
      {otel_token},
      OTEL_LEVEL,
      S3_REGION,
      S3_ACCESS_KEY_ID,
      S3_SECRET_ACCESS_KEY,
      S3_URL,
      S3_BUCKET,
      S3_PREFIX,
      S3_PRESIGN_MAX_UPLOAD_SECONDS,
      S3_PRESIGN_MAX_DOWNLOAD_SECONDS,
      ENCRYPTION_KEY
    }}) =>
      new S3ObjectStore(
        {otel_token},
        {{
          bucket: S3_BUCKET,
          prefix: S3_PREFIX,
          // Deployed on ForkLaunch only the region is set: credentials come
          // from the service's task role. Keys and S3_URL are for local MinIO.
          clientConfig: s3ClientConfig({{
            url: S3_URL,
            region: S3_REGION,
            accessKeyId: S3_ACCESS_KEY_ID,
            secretAccessKey: S3_SECRET_ACCESS_KEY
          }}),
          presignLimits: {{
            maxUploadSeconds: S3_PRESIGN_MAX_UPLOAD_SECONDS,
            maxDownloadSeconds: S3_PRESIGN_MAX_DOWNLOAD_SECONDS
          }}
        }},
        {{
          enabled: true,
          level: OTEL_LEVEL || 'info'
        }},
        {{
          encryptor: new FieldEncryptor(ENCRYPTION_KEY)
        }}
      )
  }},
    }});"
        )
        .into_boxed_str(),
    );

    let mut s3_registration_program =
        parse_ast_program(allocator, s3_registration_text, SourceType::ts());

    inject_into_registrations_config_injector(
        allocator,
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
    for key in [
        "S3_PREFIX",
        "S3_PRESIGN_MAX_UPLOAD_SECONDS",
        "S3_PRESIGN_MAX_DOWNLOAD_SECONDS",
    ] {
        let _ = delete_from_registrations_ts_config_injector(
            &allocator,
            registrations_program,
            key,
            "environmentConfig",
        );
    }
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

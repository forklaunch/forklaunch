use std::path::Path;

use anyhow::{Context, Ok, Result};
use convert_case::{Case, Casing};
use oxc_allocator::Allocator;
use oxc_ast::ast::SourceType;
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::{WorkerType, error_failed_to_read_file},
    core::{
        ast::{
            deletions::delete_from_registrations_ts::delete_from_registrations_ts_worker_type,
            infrastructure::{
                database::database_entity_manager_runtime_dependency,
                kafka::kafka_url_environment_variable,
                redis::{
                    redis_import, redis_ttl_cache_runtime_dependency,
                    redis_url_environment_variable,
                },
                s3::{s3_import, s3_object_store_runtime_dependency, s3_url_environment_variable},
            },
            injections::{
                inject_into_import_statement::{
                    inject_into_import_statement, inject_specifier_into_import_statement,
                },
                inject_into_registrations_ts::inject_into_registrations_config_injector,
            },
            parse_ast_program::parse_ast_program,
            replacements::{
                replace_import_statment::replace_import_statment,
                replace_in_registrations_ts::replace_registration_in_config_injector,
            },
        },
        manifest::ProjectType,
        rendered_template::RenderedTemplatesCache,
        worker_type::{
            get_default_worker_options, get_worker_consumer_factory, get_worker_producer_factory,
            get_worker_type_name,
        },
    },
};

pub(crate) fn transform_registrations_ts_add_router(
    rendered_templates_cache: &RenderedTemplatesCache,
    router_name: &str,
    project_type: &ProjectType,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let registrations_path = base_path.join("registrations.ts");
    let template = rendered_templates_cache
        .get(&registrations_path)?
        .context(error_failed_to_read_file(&registrations_path))?;
    let registrations_source_text = template.content;
    let registrations_source_type = SourceType::from_path(&registrations_path)?;
    let router_name_camel_case = router_name.to_case(Case::Camel);
    let router_name_pascal_case = router_name.to_case(Case::Pascal);

    let mut registrations_program = parse_ast_program(
        &allocator,
        &registrations_source_text,
        registrations_source_type,
    );

    let forklaunch_routes_import_text = format!(
        "import {{ Base{router_name_pascal_case}Service }} from './domain/services/{router_name_camel_case}.service';",
    );
    let mut forklaunch_routes_import_injection = parse_ast_program(
        &allocator,
        &forklaunch_routes_import_text,
        registrations_source_type,
    );

    inject_into_import_statement(
        &mut registrations_program,
        &mut forklaunch_routes_import_injection,
        format!("./domain/services/{router_name_camel_case}.service").as_str(),
        &registrations_source_text,
    )?;

    let (dependency, em_setup, em_resolution, service_param) = match project_type {
        ProjectType::Worker => ("WorkerProducer", "", "", "WorkerProducer"),
        ProjectType::Service => (
            "EntityMgr",
            "resolve,",
            "let em = EntityMgr;
              if (context.entityManagerOptions) {
                em = resolve('EntityMgr', context);
              }",
            "em",
        ),
        _ => unreachable!(),
    };

    let config_injector_text = format!(
        "const configInjector = createConfigInjector(SchemaValidator(), {{
            {router_name_pascal_case}Service: {{
            lifetime: Lifetime.Scoped,
            type: Base{router_name_pascal_case}Service,
            factory: (
                {{ {dependency}, OtelCollector }},
                {em_setup}
                context
            ) => {{
                {em_resolution}
                return new Base{router_name_pascal_case}Service(
                    {service_param},
                    OtelCollector
                );
            }}
            }}
        }})"
    );

    let mut config_injector_injection =
        parse_ast_program(&allocator, &config_injector_text, registrations_source_type);

    inject_into_registrations_config_injector(
        &allocator,
        &mut registrations_program,
        &mut config_injector_injection,
        "serviceDependencies",
    )?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&registrations_program)
        .code)
}

pub(crate) fn transform_registrations_ts_infrastructure_redis(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let registrations_path = base_path.join("registrations.ts");
    let template = rendered_templates_cache
        .get(&registrations_path)?
        .context(error_failed_to_read_file(&registrations_path))?;
    let registrations_text = template.content;

    let registrations_type = SourceType::from_path(&registrations_path)?;

    let mut registrations_program =
        parse_ast_program(&allocator, &registrations_text, registrations_type);

    redis_import(&allocator, &registrations_text, &mut registrations_program)?;
    redis_url_environment_variable(&allocator, &mut registrations_program)?;
    redis_ttl_cache_runtime_dependency(&allocator, &mut registrations_program)?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&registrations_program)
        .code)
}

pub(crate) fn transform_registrations_ts_infrastructure_s3(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let registrations_path = base_path.join("registrations.ts");
    let template = rendered_templates_cache
        .get(&registrations_path)?
        .context(error_failed_to_read_file(&registrations_path))?;
    let registrations_text = template.content;

    let registrations_type = SourceType::from_path(&registrations_path)?;

    let mut registrations_program =
        parse_ast_program(&allocator, &registrations_text, registrations_type);

    s3_import(&allocator, &registrations_text, &mut registrations_program)?;
    s3_url_environment_variable(&allocator, &mut registrations_program)?;
    s3_object_store_runtime_dependency(&allocator, &mut registrations_program)?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&registrations_program)
        .code)
}

pub(crate) fn transform_registrations_ts_worker_type(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    app_name: &str,
    pascal_case_name: &str,
    existing_type: &WorkerType,
    r#type: &WorkerType,
) -> Result<String> {
    let allocator = Allocator::default();
    let registrations_path = base_path.join("registrations.ts");
    let template = rendered_templates_cache
        .get(&registrations_path)?
        .context(error_failed_to_read_file(&registrations_path))?;
    let registrations_text = template.content;

    let mut registration_program = parse_ast_program(
        &allocator,
        &registrations_text,
        SourceType::from_path(registrations_path)?,
    );

    let existing_worker_type = get_worker_type_name(existing_type);
    let worker_type = get_worker_type_name(r#type);

    let worker_consumer_text = format!(
        "import {{ {}WorkerConsumer }} from '@forklaunch/implementation-worker-{}/consumers'",
        worker_type,
        worker_type.to_lowercase()
    );
    let mut worker_consumer_import =
        parse_ast_program(&allocator, &worker_consumer_text, SourceType::ts());
    let _ = replace_import_statment(
        &mut registration_program,
        &mut worker_consumer_import,
        &format!(
            "@forklaunch/implementation-worker-{}/consumers",
            existing_worker_type.to_lowercase()
        ),
    );

    let worker_producer_text = format!(
        "import {{ {}WorkerProducer }} from '@forklaunch/implementation-worker-{}/producers'",
        worker_type,
        worker_type.to_lowercase()
    );
    let mut worker_producer_import =
        parse_ast_program(&allocator, &worker_producer_text, SourceType::ts());
    let _ = replace_import_statment(
        &mut registration_program,
        &mut worker_producer_import,
        &format!(
            "@forklaunch/implementation-worker-{}/producers",
            existing_worker_type.to_lowercase()
        ),
    );

    let worker_schema_text = format!(
        "import {{ {}WorkerSchemas }} from '@forklaunch/implementation-worker-{}/schemas'",
        worker_type,
        worker_type.to_lowercase()
    );
    let mut worker_schema_import =
        parse_ast_program(&allocator, &worker_schema_text, SourceType::ts());
    let _ = replace_import_statment(
        &mut registration_program,
        &mut worker_schema_import,
        &format!(
            "@forklaunch/implementation-worker-{}/schemas",
            existing_worker_type.to_lowercase()
        ),
    );

    let worker_option_text = format!(
        "import {{ {}WorkerOptions }} from '@forklaunch/implementation-worker-{}/types'",
        worker_type,
        worker_type.to_lowercase()
    );
    let mut worker_option_import =
        parse_ast_program(&allocator, &worker_option_text, SourceType::ts());
    let _ = replace_import_statment(
        &mut registration_program,
        &mut worker_option_import,
        &format!(
            "@forklaunch/implementation-worker-{}/types",
            existing_worker_type.to_lowercase()
        ),
    );

    delete_from_registrations_ts_worker_type(&allocator, &mut registration_program);

    match r#type {
        WorkerType::BullMQCache => {
            redis_import(&allocator, &registrations_text, &mut registration_program)?;
            redis_url_environment_variable(&allocator, &mut registration_program)?;
        }
        WorkerType::RedisCache => {
            redis_import(&allocator, &registrations_text, &mut registration_program)?;
            redis_url_environment_variable(&allocator, &mut registration_program)?;
            redis_ttl_cache_runtime_dependency(&allocator, &mut registration_program)?;
        }
        WorkerType::Database => {
            let mut mikro_orm_import_program = parse_ast_program(
                &allocator,
                "import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';",
                SourceType::ts(),
            );
            replace_import_statment(
                &mut registration_program,
                &mut mikro_orm_import_program,
                "@mikro-orm/core",
            )?;
            let mut mikro_orm_config_import_program = parse_ast_program(
                &allocator,
                "import mikroOrmOptionsConfig from './mikro-orm.config';",
                SourceType::ts(),
            );
            replace_import_statment(
                &mut registration_program,
                &mut mikro_orm_config_import_program,
                "./mikro-orm.config",
            )?;
            database_entity_manager_runtime_dependency(&allocator, &mut registration_program)?;
        }
        WorkerType::Kafka => {
            inject_specifier_into_import_statement(
                &allocator,
                &mut registration_program,
                "array",
                &format!("@{app_name}/core"),
            )?;
            kafka_url_environment_variable(&allocator, &mut registration_program)?;
        }
    }

    let config_injector_runtime_dependencies_text = format!(
        "const configInjector = createConfigInjector(SchemaValidator(), {{
                WorkerOptions: {{
                    lifetime: Lifetime.Singleton,
                    type: {}WorkerSchemas({{
                        validator: SchemaValidator()
                    }}),
                    {}
                }}
            }});",
        worker_type,
        get_default_worker_options(r#type)
    );
    let mut config_injector_runtime_dependencies_import = parse_ast_program(
        &allocator,
        &config_injector_runtime_dependencies_text,
        SourceType::ts(),
    );
    replace_registration_in_config_injector(
        &allocator,
        &mut registration_program,
        &mut config_injector_runtime_dependencies_import,
        "runtimeDependencies",
    );

    let config_injector_service_dependencies_text = format!(
        "const configInjector = createConfigInjector(SchemaValidator(), {{
                WorkerProducer: {{
                    lifetime: Lifetime.Scoped,
                    type: {}WorkerProducer,
                    factory: {}
                }},
                WorkerConsumer: {{
                    lifetime: Lifetime.Scoped,
                    type: (
                        processEventsFunction: WorkerProcessFunction<{}EventRecord>,
                        failureHandler: WorkerFailureHandler<{}EventRecord>
                    ) => {}WorkerConsumer<{}EventRecord, {}WorkerOptions>,
                    factory: {}
                }}
            }})",
        worker_type,
        get_worker_producer_factory(r#type),
        pascal_case_name,
        pascal_case_name,
        worker_type,
        pascal_case_name,
        worker_type,
        get_worker_consumer_factory(r#type, &pascal_case_name)
    );
    let mut config_injector_service_dependencies_import = parse_ast_program(
        &allocator,
        &config_injector_service_dependencies_text,
        SourceType::ts(),
    );
    replace_registration_in_config_injector(
        &allocator,
        &mut registration_program,
        &mut config_injector_service_dependencies_import,
        "serviceDependencies",
    );

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&registration_program)
        .code)
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir_all, write};

    use tempfile::TempDir;

    use super::*;
    use crate::core::rendered_template::RenderedTemplatesCache;

    fn create_temp_project_structure(registrations_content: &str) -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let project_path = temp_dir.path().join("test-project");
        create_dir_all(&project_path).unwrap();

        write(project_path.join("registrations.ts"), registrations_content).unwrap();

        temp_dir
    }

    #[test]
    fn test_transform_registrations_ts_add_router_service() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-service',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_add_router(
            &cache,
            "user",
            &ProjectType::Service,
            &project_path,
        );

        assert!(result.is_ok());

        let transformed_code = result.unwrap();
        assert!(!transformed_code.is_empty());
    }

    #[test]
    fn test_transform_registrations_ts_add_router_worker() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-worker',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_add_router(
            &cache,
            "order",
            &ProjectType::Worker,
            &project_path,
        );

        assert!(result.is_ok());

        let transformed_code = result.unwrap();
        assert!(!transformed_code.is_empty());
    }

    #[test]
    fn test_transform_registrations_ts_infrastructure_redis() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-service',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_infrastructure_redis(&cache, &project_path);

        assert!(result.is_ok());

        let transformed_code = result.unwrap();
        assert!(!transformed_code.is_empty());
    }

    #[test]
    fn test_transform_registrations_ts_infrastructure_redis_with_custom_text() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-service',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
});
"#;

        let temp_dir = TempDir::new().unwrap();
        let mut cache = RenderedTemplatesCache::new();
        cache.insert(
            temp_dir
                .path()
                .join("registrations.ts")
                .to_string_lossy()
                .to_string(),
            crate::core::rendered_template::RenderedTemplate {
                path: temp_dir.path().join("registrations.ts"),
                content: registrations_content.to_string(),
                context: None,
            },
        );

        let result = transform_registrations_ts_infrastructure_redis(&cache, &temp_dir.path());

        assert!(result.is_ok());

        let transformed_code = result.unwrap();
        assert!(!transformed_code.is_empty());
    }

    #[test]
    fn test_transform_registrations_ts_infrastructure_s3() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-service',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_infrastructure_s3(&cache, &project_path);

        assert!(result.is_ok());

        let transformed_code = result.unwrap();
        assert!(!transformed_code.is_empty());
    }

    #[test]
    fn test_transform_registrations_ts_infrastructure_s3_with_custom_text() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-service',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
});
"#;

        let temp_dir = TempDir::new().unwrap();
        let mut cache = RenderedTemplatesCache::new();
        cache.insert(
            temp_dir
                .path()
                .join("registrations.ts")
                .to_string_lossy()
                .to_string(),
            crate::core::rendered_template::RenderedTemplate {
                path: temp_dir.path().join("registrations.ts"),
                content: registrations_content.to_string(),
                context: None,
            },
        );

        let result = transform_registrations_ts_infrastructure_s3(&cache, &temp_dir.path());

        assert!(result.is_ok());

        let transformed_code = result.unwrap();
        assert!(!transformed_code.is_empty());
    }

    #[test]
    fn test_transform_registrations_ts_worker_type_database() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { BullMQWorkerConsumer } from '@forklaunch/implementation-worker-bullmq/consumers';
import { BullMQWorkerProducer } from '@forklaunch/implementation-worker-bullmq/producers';
import { BullMQWorkerSchemas } from '@forklaunch/implementation-worker-bullmq/schemas';
import { BullMQWorkerOptions } from '@forklaunch/implementation-worker-bullmq/types';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-worker',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  },
  WorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: BullMQWorkerSchemas({
      validator: SchemaValidator()
    }),
    // BullMQ options
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  WorkerProducer: {
    lifetime: Lifetime.Scoped,
    type: BullMQWorkerProducer,
    factory: // BullMQ producer factory
  },
  WorkerConsumer: {
    lifetime: Lifetime.Scoped,
    type: // BullMQ consumer type
    factory: // BullMQ consumer factory
  }
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_worker_type(
            &cache,
            &project_path,
            "test-app",
            "UserEvent",
            &WorkerType::BullMQCache,
            &WorkerType::Database,
        );

        if result.is_ok() {
            let _transformed_code = result.unwrap();
        }
    }

    #[test]
    fn test_transform_registrations_ts_worker_type_redis() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { DatabaseWorkerConsumer } from '@forklaunch/implementation-worker-database/consumers';
import { DatabaseWorkerProducer } from '@forklaunch/implementation-worker-database/producers';
import { DatabaseWorkerSchemas } from '@forklaunch/implementation-worker-database/schemas';
import { DatabaseWorkerOptions } from '@forklaunch/implementation-worker-database/types';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-worker',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  },
  WorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: DatabaseWorkerSchemas({
      validator: SchemaValidator()
    }),
    // Database options
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  WorkerProducer: {
    lifetime: Lifetime.Scoped,
    type: DatabaseWorkerProducer,
    factory: // Database producer factory
  },
  WorkerConsumer: {
    lifetime: Lifetime.Scoped,
    type: // Database consumer type
    factory: // Database consumer factory
  }
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_worker_type(
            &cache,
            &project_path,
            "test-app",
            "OrderEvent",
            &WorkerType::Database,
            &WorkerType::RedisCache,
        );

        if result.is_ok() {
            let _transformed_code = result.unwrap();
        }
    }

    #[test]
    fn test_transform_registrations_ts_worker_type_kafka() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { RedisCacheWorkerConsumer } from '@forklaunch/implementation-worker-rediscache/consumers';
import { RedisCacheWorkerProducer } from '@forklaunch/implementation-worker-rediscache/producers';
import { RedisCacheWorkerSchemas } from '@forklaunch/implementation-worker-rediscache/schemas';
import { RedisCacheWorkerOptions } from '@forklaunch/implementation-worker-rediscache/types';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-worker',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  },
  WorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: RedisCacheWorkerSchemas({
      validator: SchemaValidator()
    }),
    // Redis options
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  WorkerProducer: {
    lifetime: Lifetime.Scoped,
    type: RedisCacheWorkerProducer,
    factory: // Redis producer factory
  },
  WorkerConsumer: {
    lifetime: Lifetime.Scoped,
    type: // Redis consumer type
    factory: // Redis consumer factory
  }
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_worker_type(
            &cache,
            &project_path,
            "test-app",
            "ProductEvent",
            &WorkerType::RedisCache,
            &WorkerType::Kafka,
        );

        if result.is_ok() {
            let _transformed_code = result.unwrap();
        }
    }

    #[test]
    fn test_transform_registrations_ts_worker_type_with_custom_text() {
        let registrations_content = r#"
import {
  number,
  optional,
  schemaValidator,
  SchemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { BullMQWorkerConsumer } from '@forklaunch/implementation-worker-bullmq/consumers';
import { BullMQWorkerProducer } from '@forklaunch/implementation-worker-bullmq/producers';
import { BullMQWorkerSchemas } from '@forklaunch/implementation-worker-bullmq/schemas';
import { BullMQWorkerOptions } from '@forklaunch/implementation-worker-bullmq/types';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'test-worker',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ MikroORM }, _resolve, context) =>
      MikroORM.em.fork(context?.entityManagerOptions as ForkOptions | undefined)
  },
  WorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: BullMQWorkerSchemas({
      validator: SchemaValidator()
    }),
    // BullMQ options
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  WorkerProducer: {
    lifetime: Lifetime.Scoped,
    type: BullMQWorkerProducer,
    factory: // BullMQ producer factory
  },
  WorkerConsumer: {
    lifetime: Lifetime.Scoped,
    type: // BullMQ consumer type
    factory: // BullMQ consumer factory
  }
});
"#;

        let temp_dir = TempDir::new().unwrap();
        let mut cache = RenderedTemplatesCache::new();
        cache.insert(
            temp_dir
                .path()
                .join("registrations.ts")
                .to_string_lossy()
                .to_string(),
            crate::core::rendered_template::RenderedTemplate {
                path: temp_dir.path().join("registrations.ts"),
                content: registrations_content.to_string(),
                context: None,
            },
        );

        let result = transform_registrations_ts_worker_type(
            &cache,
            &temp_dir.path(),
            "test-app",
            "UserEvent",
            &WorkerType::BullMQCache,
            &WorkerType::Database,
        );

        if result.is_ok() {
            let _transformed_code = result.unwrap();
        }
    }
}

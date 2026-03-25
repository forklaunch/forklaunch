use std::path::Path;

use anyhow::{Context, Result};
use convert_case::{Case, Casing};
use oxc_allocator::Allocator;
use oxc_ast::ast::SourceType;
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::{WorkerType, error_failed_to_read_file},
    core::{
        ast::{
            infrastructure::{
                database::database_entity_manager_runtime_dependency,
                kafka::kafka_url_environment_variable,
                redis::{
                    redis_import, redis_ttl_cache_runtime_dependency,
                    redis_url_environment_variable,
                },
            },
            injections::{
                inject_into_import_statement::{
                    inject_into_import_statement, inject_specifier_into_import_statement,
                },
                inject_into_registrations_ts::inject_into_registrations_config_injector,
            },
            parse_ast_program::parse_ast_program,
        },
        rendered_template::RenderedTemplatesCache,
        worker_type::{
            get_default_worker_options, get_worker_consumer_factory, get_worker_producer_factory,
            get_worker_type_name,
        },
    },
};

/// Transforms a service's registrations.ts to add worker-specific dependencies
/// This is used when converting a service to a worker
pub(crate) fn transform_registrations_ts_service_to_worker(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
    app_name: &str,
    project_name: &str,
    worker_type: &WorkerType,
) -> Result<String> {
    let allocator = Allocator::default();
    let registrations_path = base_path.join("registrations.ts");
    let template = rendered_templates_cache
        .get(&registrations_path)?
        .context(error_failed_to_read_file(&registrations_path))?;
    let registrations_text = template.content;
    let source_type = SourceType::from_path(&registrations_path)?;

    let pascal_case_name = project_name.to_case(Case::Pascal);
    let worker_type_name = get_worker_type_name(worker_type);

    let mut program = parse_ast_program(&allocator, &registrations_text, source_type);

    // Inject function_ and type specifiers into the core import
    inject_specifier_into_import_statement(
        &allocator,
        &mut program,
        "function_",
        &format!("@{app_name}/core"),
    )?;
    inject_specifier_into_import_statement(
        &allocator,
        &mut program,
        "type",
        &format!("@{app_name}/core"),
    )?;

    // Add worker consumer import
    let worker_consumer_text = format!(
        "import {{ {}WorkerConsumer }} from '@forklaunch/implementation-worker-{}/consumers';",
        worker_type_name,
        worker_type_name.to_lowercase()
    );
    let mut worker_consumer_import =
        parse_ast_program(&allocator, &worker_consumer_text, SourceType::ts());
    inject_into_import_statement(
        &mut program,
        &mut worker_consumer_import,
        &format!(
            "@forklaunch/implementation-worker-{}/consumers",
            worker_type_name.to_lowercase()
        ),
        &registrations_text,
    )?;

    // Add worker producer import
    let worker_producer_text = format!(
        "import {{ {}WorkerProducer }} from '@forklaunch/implementation-worker-{}/producers';",
        worker_type_name,
        worker_type_name.to_lowercase()
    );
    let mut worker_producer_import =
        parse_ast_program(&allocator, &worker_producer_text, SourceType::ts());
    inject_into_import_statement(
        &mut program,
        &mut worker_producer_import,
        &format!(
            "@forklaunch/implementation-worker-{}/producers",
            worker_type_name.to_lowercase()
        ),
        &registrations_text,
    )?;

    // Add worker schemas import
    let worker_schema_text = format!(
        "import {{ {}WorkerSchemas }} from '@forklaunch/implementation-worker-{}/schemas';",
        worker_type_name,
        worker_type_name.to_lowercase()
    );
    let mut worker_schema_import =
        parse_ast_program(&allocator, &worker_schema_text, SourceType::ts());
    inject_into_import_statement(
        &mut program,
        &mut worker_schema_import,
        &format!(
            "@forklaunch/implementation-worker-{}/schemas",
            worker_type_name.to_lowercase()
        ),
        &registrations_text,
    )?;

    // Add worker options import
    let worker_option_text = format!(
        "import {{ {}WorkerOptions }} from '@forklaunch/implementation-worker-{}/types';",
        worker_type_name,
        worker_type_name.to_lowercase()
    );
    let mut worker_option_import =
        parse_ast_program(&allocator, &worker_option_text, SourceType::ts());
    inject_into_import_statement(
        &mut program,
        &mut worker_option_import,
        &format!(
            "@forklaunch/implementation-worker-{}/types",
            worker_type_name.to_lowercase()
        ),
        &registrations_text,
    )?;

    // Add worker interfaces imports
    let is_database_worker = *worker_type == WorkerType::Database;
    let worker_interfaces_types_text = if is_database_worker {
        "import { WorkerProcessFunction, WorkerFailureHandler } from '@forklaunch/interfaces-worker/types';".to_string()
    } else {
        "import { type EncryptedEventEnvelope, WorkerProcessFunction, WorkerFailureHandler } from '@forklaunch/interfaces-worker/types';".to_string()
    };
    let mut worker_interfaces_import =
        parse_ast_program(&allocator, &worker_interfaces_types_text, SourceType::ts());
    inject_into_import_statement(
        &mut program,
        &mut worker_interfaces_import,
        "@forklaunch/interfaces-worker/types",
        &registrations_text,
    )?;

    if !is_database_worker {
        // Add encryption imports for non-database workers
        let encryption_interfaces_text = "import { EncryptingWorkerProducer, withDecryption, withDecryptionFailureHandler } from '@forklaunch/interfaces-worker/interfaces';";
        let mut encryption_interfaces_import =
            parse_ast_program(&allocator, encryption_interfaces_text, SourceType::ts());
        inject_into_import_statement(
            &mut program,
            &mut encryption_interfaces_import,
            "@forklaunch/interfaces-worker/interfaces",
            &registrations_text,
        )?;

        let field_encryptor_text = "import { FieldEncryptor } from '@forklaunch/core/persistence';";
        let mut field_encryptor_import =
            parse_ast_program(&allocator, field_encryptor_text, SourceType::ts());
        inject_into_import_statement(
            &mut program,
            &mut field_encryptor_import,
            "@forklaunch/core/persistence",
            &registrations_text,
        )?;
    }

    // Add event record type import for non-database workers (db workers use the entity import)
    let event_record_type_text = format!(
        "import type {{ {}EventRecord }} from './domain/types/{}EventRecord.types';",
        pascal_case_name,
        project_name.to_case(Case::Camel)
    );
    if !is_database_worker {
        let mut event_record_type_import =
            parse_ast_program(&allocator, &event_record_type_text, SourceType::ts());
        inject_into_import_statement(
            &mut program,
            &mut event_record_type_import,
            &format!(
                "./domain/types/{}EventRecord.types",
                project_name.to_case(Case::Camel)
            ),
            &registrations_text,
        )?;
    }

    // Add event record entity import (used at runtime in database worker factory)
    let event_record_text = format!(
        "import {{ {}EventRecord }} from './persistence/entities/{}EventRecord.entity';",
        pascal_case_name,
        project_name.to_case(Case::Camel)
    );
    let mut event_record_import =
        parse_ast_program(&allocator, &event_record_text, SourceType::ts());
    inject_into_import_statement(
        &mut program,
        &mut event_record_import,
        &format!(
            "./persistence/entities/{}EventRecord.entity",
            project_name.to_case(Case::Camel)
        ),
        &registrations_text,
    )?;

    // Detect naming convention from existing registrations file
    let otel_token = if registrations_text.contains("OtelCollector:") {
        "OtelCollector"
    } else {
        "OpenTelemetryCollector"
    };
    let orm_token = if registrations_text.contains("Orm:") {
        "Orm"
    } else {
        "MikroORM"
    };
    let em_token = if registrations_text.contains("EntityMgr:") {
        "EntityMgr"
    } else {
        "EntityManager"
    };

    // Add worker-type-specific infrastructure
    match worker_type {
        WorkerType::BullMQCache => {
            redis_import(&allocator, &registrations_text, &mut program)?;
            redis_url_environment_variable(&allocator, &mut program)?;
        }
        WorkerType::RedisCache => {
            redis_import(&allocator, &registrations_text, &mut program)?;
            redis_url_environment_variable(&allocator, &mut program)?;
            redis_ttl_cache_runtime_dependency(&allocator, &mut program, otel_token)?;
        }
        WorkerType::Database => {
            database_entity_manager_runtime_dependency(&allocator, &mut program, orm_token, em_token)?;
        }
        WorkerType::Kafka => {
            inject_specifier_into_import_statement(
                &allocator,
                &mut program,
                "array",
                &format!("@{app_name}/core"),
            )?;
            kafka_url_environment_variable(&allocator, &mut program)?;
        }
    }

    // Inject QUEUE_NAME environment variable
    let queue_name_text = format!(
        "const configInjector = createConfigInjector(SchemaValidator(), {{
            QUEUE_NAME: {{
                lifetime: Lifetime.Singleton,
                type: string,
                value: getEnvVar('QUEUE_NAME')
            }}
        }});"
    );
    let mut queue_name_injection =
        parse_ast_program(&allocator, &queue_name_text, SourceType::ts());
    inject_into_registrations_config_injector(
        &allocator,
        &mut program,
        &mut queue_name_injection,
        "environmentConfig",
    )?;

    // Inject ENCRYPTION_KEY and EventEncryptor for non-database workers
    if !is_database_worker {
        let encryption_key_text = "const configInjector = createConfigInjector(SchemaValidator(), {
            ENCRYPTION_KEY: {
                lifetime: Lifetime.Singleton,
                type: string,
                value: getEnvVar('ENCRYPTION_KEY')
            }
        });";
        let mut encryption_key_injection =
            parse_ast_program(&allocator, encryption_key_text, SourceType::ts());
        inject_into_registrations_config_injector(
            &allocator,
            &mut program,
            &mut encryption_key_injection,
            "environmentConfig",
        )?;

        let event_encryptor_text = "const configInjector = createConfigInjector(SchemaValidator(), {
            EventEncryptor: {
                lifetime: Lifetime.Singleton,
                type: FieldEncryptor,
                factory: ({ ENCRYPTION_KEY }) => new FieldEncryptor(ENCRYPTION_KEY)
            }
        });";
        let mut event_encryptor_injection =
            parse_ast_program(&allocator, event_encryptor_text, SourceType::ts());
        inject_into_registrations_config_injector(
            &allocator,
            &mut program,
            &mut event_encryptor_injection,
            "runtimeDependencies",
        )?;
    }

    // Inject WorkerOptions into runtimeDependencies
    let worker_options_text = format!(
        "const configInjector = createConfigInjector(SchemaValidator(), {{
            WorkerOptions: {{
                lifetime: Lifetime.Singleton,
                type: {}WorkerSchemas({{
                    validator: SchemaValidator()
                }}),
                {}
            }}
        }});",
        worker_type_name,
        get_default_worker_options(worker_type)
    );
    let mut worker_options_injection =
        parse_ast_program(&allocator, &worker_options_text, SourceType::ts());
    inject_into_registrations_config_injector(
        &allocator,
        &mut program,
        &mut worker_options_injection,
        "runtimeDependencies",
    )?;

    // Inject WorkerConsumer and WorkerProducer into serviceDependencies
    let worker_deps_text = if is_database_worker {
        format!(
            "const configInjector = createConfigInjector(SchemaValidator(), {{
                WorkerProducer: {{
                    lifetime: Lifetime.Scoped,
                    type: {}WorkerProducer,
                    factory: {}
                }},
                WorkerConsumer: {{
                    lifetime: Lifetime.Scoped,
                    type: function_(
                        [
                            type<WorkerProcessFunction<{}EventRecord>>(),
                            type<WorkerFailureHandler<{}EventRecord>>()
                        ],
                        type<{}WorkerConsumer<{}EventRecord, {}WorkerOptions>>()
                    ),
                    factory: {}
                }}
            }});",
            worker_type_name,
            get_worker_producer_factory(worker_type),
            pascal_case_name,
            pascal_case_name,
            worker_type_name,
            pascal_case_name,
            worker_type_name,
            get_worker_consumer_factory(worker_type, &pascal_case_name)
        )
    } else {
        format!(
            "const configInjector = createConfigInjector(SchemaValidator(), {{
                WorkerProducer: {{
                    lifetime: Lifetime.Scoped,
                    type: EncryptingWorkerProducer,
                    factory: (container, _resolve, context) =>
                        new EncryptingWorkerProducer(
                            ({})(container),
                            container.EventEncryptor,
                            (context?.tenantId as string) ?? ''
                        )
                }},
                WorkerConsumer: {{
                    lifetime: Lifetime.Scoped,
                    type: function_(
                        [
                            type<WorkerProcessFunction<{}EventRecord>>(),
                            type<WorkerFailureHandler<{}EventRecord>>()
                        ],
                        type<{}WorkerConsumer<EncryptedEventEnvelope, {}WorkerOptions>>()
                    ),
                    factory: (container) => {{
                        const createConsumer = ({})(container);
                        return (
                            processEventsFunction: WorkerProcessFunction<{}EventRecord>,
                            failureHandler: WorkerFailureHandler<{}EventRecord>
                        ) =>
                            createConsumer(
                                withDecryption<{}EventRecord>(processEventsFunction, container.EventEncryptor),
                                withDecryptionFailureHandler<{}EventRecord>(failureHandler, container.EventEncryptor)
                            );
                    }}
                }}
            }});",
            get_worker_producer_factory(worker_type),
            pascal_case_name,
            pascal_case_name,
            worker_type_name,
            worker_type_name,
            get_worker_consumer_factory(worker_type, &pascal_case_name),
            pascal_case_name,
            pascal_case_name,
            pascal_case_name,
            pascal_case_name,
        )
    };
    let mut worker_deps_injection =
        parse_ast_program(&allocator, &worker_deps_text, SourceType::ts());
    inject_into_registrations_config_injector(
        &allocator,
        &mut program,
        &mut worker_deps_injection,
        "serviceDependencies",
    )?;

    Ok(Codegen::new()
        .with_options(CodegenOptions::default())
        .build(&program)
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
    fn test_transform_registrations_ts_service_to_worker_database() {
        let registrations_content = r#"
import {
  number,
  SchemaValidator,
  string
} from '@test-app/core';
import { metrics } from '@test-app/monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';
import { BaseTestService } from './services/test.service';

const configInjector = createConfigInjector(SchemaValidator(), {
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

const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => new MikroORM(mikroOrmOptionsConfig)
  },
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
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

const serviceDependencies = runtimeDependencies.chain({
  TestService: {
    lifetime: Lifetime.Scoped,
    type: BaseTestService,
    factory: ({ EntityManager, OpenTelemetryCollector }) =>
      new BaseTestService(EntityManager, OpenTelemetryCollector)
  }
});

export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_service_to_worker(
            &cache,
            &project_path,
            "test-app",
            "test-service",
            &WorkerType::Database,
        );

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        // Verify worker imports were added
        assert!(transformed_code.contains("DatabaseWorkerConsumer"));
        assert!(transformed_code.contains("DatabaseWorkerProducer"));
        assert!(transformed_code.contains("DatabaseWorkerSchemas"));
        assert!(transformed_code.contains("DatabaseWorkerOptions"));
        assert!(transformed_code.contains("WorkerProcessFunction"));
        assert!(transformed_code.contains("WorkerFailureHandler"));

        // Verify QUEUE_NAME was added
        assert!(transformed_code.contains("QUEUE_NAME"));

        // Verify WorkerOptions was added
        assert!(transformed_code.contains("WorkerOptions"));

        // Verify WorkerConsumer and WorkerProducer were added
        assert!(transformed_code.contains("WorkerConsumer"));
        assert!(transformed_code.contains("WorkerProducer"));

        // Verify event record interface type is imported (not InferEntity)
        assert!(
            transformed_code.contains("EventRecord.types"),
            "Expected EventRecord.types import in transformed code: {transformed_code}"
        );
    }

    #[test]
    fn test_transform_registrations_ts_service_to_worker_bullmq() {
        let registrations_content = r#"
import {
  number,
  SchemaValidator,
  string
} from '@test-app/core';
import { metrics } from '@test-app/monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { BaseTestService } from './services/test.service';

const configInjector = createConfigInjector(SchemaValidator(), {
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

const environmentConfig = configInjector.chain({
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HOST')
  }
});

const runtimeDependencies = environmentConfig.chain({
  OpenTelemetryCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  }
});

const serviceDependencies = runtimeDependencies.chain({});

export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
"#;

        let temp_dir = create_temp_project_structure(registrations_content);
        let project_path = temp_dir.path().join("test-project");
        let cache = RenderedTemplatesCache::new();

        let result = transform_registrations_ts_service_to_worker(
            &cache,
            &project_path,
            "test-app",
            "test-service",
            &WorkerType::BullMQCache,
        );

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        // Verify BullMQ worker imports were added
        assert!(transformed_code.contains("BullMqWorkerConsumer"));
        assert!(transformed_code.contains("BullMqWorkerProducer"));

        // Verify Redis infrastructure was added
        assert!(transformed_code.contains("REDIS_URL"));
    }
}

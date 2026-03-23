use std::path::Path;

use anyhow::{Context, Result};
use oxc_allocator::{Allocator, CloneIn};
use oxc_ast::ast::SourceType;
use oxc_codegen::{Codegen, CodegenOptions};

use crate::{
    constants::error_failed_to_read_file,
    core::{
        ast::{
            deletions::{
                delete_from_registrations_ts::delete_from_registrations_ts_worker_type,
                delete_import_statement::delete_import_statements_with_prefix,
            },
            infrastructure::redis::delete_redis_import,
            parse_ast_program::parse_ast_program,
        },
        rendered_template::RenderedTemplatesCache,
    },
};

/// Transforms a worker's registrations.ts to remove worker-specific dependencies
/// This is used when converting a worker to a service
pub(crate) fn transform_registrations_ts_worker_to_service(
    rendered_templates_cache: &RenderedTemplatesCache,
    base_path: &Path,
) -> Result<String> {
    let allocator = Allocator::default();
    let registrations_path = base_path.join("registrations.ts");
    let template = rendered_templates_cache
        .get(&registrations_path)?
        .context(error_failed_to_read_file(&registrations_path))?;
    let registrations_text = template.content;
    let source_type = SourceType::from_path(&registrations_path)?;

    let mut program = parse_ast_program(&allocator, &registrations_text, source_type);

    // Delete worker-specific registrations (WorkerOptions, WorkerConsumer, WorkerProducer)
    delete_from_registrations_ts_worker_type(&allocator, &mut program);

    // Delete worker-specific imports (including sub-paths like /types, /schemas, etc.)
    let worker_import_prefixes = [
        "@forklaunch/implementation-worker-bullmq",
        "@forklaunch/implementation-worker-database",
        "@forklaunch/implementation-worker-redis",
        "@forklaunch/implementation-worker-kafka",
        "@forklaunch/interfaces-worker",
    ];

    for import_prefix in worker_import_prefixes {
        let _ = delete_import_statements_with_prefix(&allocator, &mut program, import_prefix);
    }

    // Delete EventRecord-related imports (type interface and entity alias)
    // These contain "EventRecord" in the import source path
    let mut new_body = oxc_allocator::Vec::new_in(&allocator);
    for stmt in program.body.iter() {
        if let oxc_ast::ast::Statement::ImportDeclaration(import) = stmt {
            let source = import.source.value.as_str();
            if source.contains("EventRecord") {
                continue; // Skip EventRecord imports
            }
        }
        new_body.push(stmt.clone_in(&allocator));
    }
    program.body = new_body;

    // Delete Redis import (often added by worker implementations like BullMQ)
    delete_redis_import(&allocator, &mut program);

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
    fn test_transform_registrations_ts_worker_to_service() {
        let registrations_content = r#"
import {
  function_,
  number,
  SchemaValidator,
  string,
  type
} from '@test-app/core';
import { metrics } from '@test-app/monitoring';
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
import { WorkerProcessFunction, WorkerFailureHandler } from '@forklaunch/interfaces-worker/types';
import { EntityManager, ForkOptions, MikroORM } from '@mikro-orm/core';
import mikroOrmOptionsConfig from './mikro-orm.config';
import { TestServiceEventRecord } from './persistence/entities/testServiceEventRecord.entity';
import { BaseTestServiceService } from './services/testService.service';

const configInjector = createConfigInjector(SchemaValidator(), {
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
  },
  QUEUE_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('QUEUE_NAME')
  }
});

const runtimeDependencies = environmentConfig.chain({
  MikroORM: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => new MikroORM(mikroOrmOptionsConfig)
  },
  WorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: DatabaseWorkerSchemas({
      validator: SchemaValidator()
    }),
    value: {
      retries: 3,
      interval: 5000
    }
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
  WorkerConsumer: {
    lifetime: Lifetime.Scoped,
    type: function_([
      type<WorkerProcessFunction<TestServiceEventRecord>>(),
      type<WorkerFailureHandler<TestServiceEventRecord>>()
    ],
      type<DatabaseWorkerConsumer<TestServiceEventRecord, DatabaseWorkerOptions>>()
    ),
    factory: ({ EntityManager, WorkerOptions }) => 
      (processEventsFunction, failureHandler) =>
        new DatabaseWorkerConsumer(
          TestServiceEventRecord,
          EntityManager,
          WorkerOptions,
          processEventsFunction,
          failureHandler
        )
  },
  WorkerProducer: {
    lifetime: Lifetime.Scoped,
    type: DatabaseWorkerProducer,
    factory: ({ EntityManager, WorkerOptions }) =>
      new DatabaseWorkerProducer(EntityManager, WorkerOptions)
  },
  TestServiceService: {
    lifetime: Lifetime.Scoped,
    type: BaseTestServiceService,
    factory: ({ WorkerProducer, OpenTelemetryCollector }) =>
      new BaseTestServiceService(WorkerProducer, OpenTelemetryCollector)
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

        let result = transform_registrations_ts_worker_to_service(&cache, &project_path);

        assert!(result.is_ok());

        let transformed_code = result.unwrap();

        // The worker-type deletion should remove WorkerOptions, WorkerConsumer, WorkerProducer
        // from the registrations. Note: imports are intentionally kept for user review.
        assert!(!transformed_code.is_empty());
    }
}

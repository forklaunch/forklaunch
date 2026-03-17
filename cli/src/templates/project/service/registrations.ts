import { {{#is_kafka_enabled}}array, {{/is_kafka_enabled}}{{#is_iam_configured}}createAuthCacheService, type AuthCacheService, {{/is_iam_configured}}{{#is_billing_configured}}createBillingCacheService, type BillingCacheService, {{/is_billing_configured}}{{#is_worker}}function_, {{/is_worker}}number, SchemaValidator, string{{#is_type_needed}}, type{{/is_type_needed}} } from "@{{app_name}}/core";
import { metrics } from "@{{app_name}}/monitoring";{{#is_request_cache_needed}}
import { RedisTtlCache } from "@forklaunch/infrastructure-redis";{{/is_request_cache_needed}}{{#is_s3_enabled}}
import { S3ObjectStore } from "@forklaunch/infrastructure-s3";{{/is_s3_enabled}}
import { OpenTelemetryCollector } from "@forklaunch/core/http";
import {
  createConfigInjector,
  getEnvVar,
  Lifetime,
} from "@forklaunch/core/services";{{#is_worker}}
import { {{worker_type}}WorkerConsumer } from '@forklaunch/implementation-worker-{{worker_type_lowercase}}/consumers';
import { {{worker_type}}WorkerProducer } from '@forklaunch/implementation-worker-{{worker_type_lowercase}}/producers';
import { {{worker_type}}WorkerSchemas } from '@forklaunch/implementation-worker-{{worker_type_lowercase}}/schemas';
import { {{worker_type}}WorkerOptions } from '@forklaunch/implementation-worker-{{worker_type_lowercase}}/types';
import { WorkerProcessFunction, WorkerFailureHandler } from '@forklaunch/interfaces-worker/types';{{/is_worker}}{{#is_database_enabled}}
import { ForkOptions } from "@mikro-orm/core";
import { EntityManager, MikroORM } from "@mikro-orm/{{database}}";
import mikroOrmOptionsConfig from './mikro-orm.config';{{/is_database_enabled}}{{#is_worker}}
import { {{pascal_case_name}}EventRecord } from "./persistence/entities/{{camel_case_name}}EventRecord.entity";{{/is_worker}}
import { Base{{pascal_case_name}}Service } from "./domain/services/{{camel_case_name}}.service";

//! instantiates the config injector
const configInjector = createConfigInjector(SchemaValidator(), {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string,
    },
    value: {
      name: "{{app_name}}-{{service_name}}{{worker_name}}",
      version: "0.1.0",
    },
  },
});
  
//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  {{#is_request_cache_needed}}REDIS_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('REDIS_URL')
  },{{/is_request_cache_needed}}
  PROTOCOL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar("PROTOCOL"),
  },
  HOST: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar("HOST"),
  },
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar("PORT")),
  },
  VERSION: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar("VERSION") ?? "v1",
  },
  DOCS_PATH: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar("DOCS_PATH") ?? "/docs",
  },
  OTEL_SERVICE_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar("OTEL_SERVICE_NAME"),
  },
  OTEL_LEVEL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar("OTEL_LEVEL") || "info",
  },
  OTEL_EXPORTER_OTLP_ENDPOINT: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar("OTEL_EXPORTER_OTLP_ENDPOINT"),
  },{{#is_kafka_enabled}}
  KAFKA_BROKERS: {
    lifetime: Lifetime.Singleton,
    type: array(string),
    value: getEnvVar('KAFKA_BROKERS').split(',')
  },
  KAFKA_CLIENT_ID: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('KAFKA_CLIENT_ID')
  },
  KAFKA_GROUP_ID: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('KAFKA_GROUP_ID')
  },{{/is_kafka_enabled}}{{#is_worker}}
  QUEUE_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('QUEUE_NAME')
  },{{/is_worker}}{{#is_s3_enabled}}
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
  },{{/is_s3_enabled}}{{#is_iam_configured}}
  HMAC_SECRET_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HMAC_SECRET_KEY')
  },
  JWKS_PUBLIC_KEY_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('JWKS_PUBLIC_KEY_URL')
  },
  IAM_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('IAM_URL')
  },{{/is_iam_configured}}{{#is_billing_configured}}
  BILLING_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('BILLING_URL')
  },
  {{^is_iam_configured}}
  HMAC_SECRET_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HMAC_SECRET_KEY')
  },
  {{/is_iam_configured}}
  {{/is_billing_configured}}
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  {{#is_database_enabled}}
  Orm: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => MikroORM.initSync(mikroOrmOptionsConfig)
  },{{/is_database_enabled}}
  {{#is_worker}}WorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: {{worker_type}}WorkerSchemas({
      validator: SchemaValidator()
    }),
    {{{default_worker_options}}}
  },
  {{/is_worker}}OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || "info",
        metrics
      ),
  },{{#is_request_cache_needed}}
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OtelCollector }) =>
      new RedisTtlCache(60 * 60 * 1000, OtelCollector, {
        url: REDIS_URL,
      }, {
        enabled: true,
        level: "info",
      }),
  },{{/is_request_cache_needed}}{{#is_s3_enabled}}
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
            },
            forcePathStyle: true // Required for MinIO and path-style S3
          }
        },
        {
          enabled: true,
          level: OTEL_LEVEL || 'info'
        }
      )
  },
  {{/is_s3_enabled}}{{#is_database_enabled}}
  EntityMgr: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: ({ Orm }, _resolve, context) =>
      Orm.em.fork(context?.entityManagerOptions as ForkOptions | undefined),
  },{{/is_database_enabled}}{{#is_iam_configured}}
  AuthCacheService: {
    lifetime: Lifetime.Singleton,
    type: type<AuthCacheService>(),
    factory: ({ TtlCache }) => createAuthCacheService(TtlCache)
  },{{/is_iam_configured}}{{#is_billing_configured}}
  BillingCacheService: {
    lifetime: Lifetime.Singleton,
    type: type<BillingCacheService>(),
    factory: ({ TtlCache }) => createBillingCacheService(TtlCache)
  },{{/is_billing_configured}}
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({ {{#is_worker}}
  WorkerConsumer: {
    lifetime: Lifetime.Scoped,
    type: function_([
      type<WorkerProcessFunction<{{pascal_case_name}}EventRecord>>(),
      type<WorkerFailureHandler<{{pascal_case_name}}EventRecord>>()
    ],
      type<{{worker_type}}WorkerConsumer<{{pascal_case_name}}EventRecord, {{worker_type}}WorkerOptions>>()
    ),
    factory: 
      {{{worker_consumer_factory}}}
  },
  WorkerProducer: {
    lifetime: Lifetime.Scoped,
    type: {{worker_type}}WorkerProducer,
    factory: {{{worker_producer_factory}}}
  },
  {{/is_worker}}{{pascal_case_name}}Service: {
    lifetime: Lifetime.Scoped,
    type: Base{{pascal_case_name}}Service,
    factory: ({ {{^is_worker}}
      EntityMgr,{{/is_worker}}{{#is_worker}}
      WorkerProducer,{{/is_worker}}
      OtelCollector
    }) =>
      new Base{{pascal_case_name}}Service({{^is_worker}}
        EntityMgr,{{/is_worker}}{{#is_worker}}
        WorkerProducer,{{/is_worker}}
        OtelCollector
      )
  }
});

//! validates the configuration and returns the dependencies for the application
export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});
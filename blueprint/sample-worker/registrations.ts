import {
  array,
  function_,
  number,
  schemaValidator,
  string,
  type
} from '@forklaunch/blueprint-core';
import { Metrics, metrics } from '@forklaunch/blueprint-monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import { FieldEncryptor } from '@forklaunch/core/persistence';
import {
  createConfigInjector,
  getEnvVar,
  Lifetime
} from '@forklaunch/core/services';
import { BullMqWorkerConsumer } from '@forklaunch/implementation-worker-bullmq/consumers';
import { BullMqWorkerProducer } from '@forklaunch/implementation-worker-bullmq/producers';
import { BullMqWorkerSchemas } from '@forklaunch/implementation-worker-bullmq/schemas';
import { BullMqWorkerOptions } from '@forklaunch/implementation-worker-bullmq/types';
import { DatabaseWorkerConsumer } from '@forklaunch/implementation-worker-database/consumers';
import { DatabaseWorkerProducer } from '@forklaunch/implementation-worker-database/producers';
import { DatabaseWorkerSchemas } from '@forklaunch/implementation-worker-database/schemas';
import { DatabaseWorkerOptions } from '@forklaunch/implementation-worker-database/types';
import { KafkaWorkerConsumer } from '@forklaunch/implementation-worker-kafka/consumers';
import { KafkaWorkerProducer } from '@forklaunch/implementation-worker-kafka/producers';
import { KafkaWorkerSchemas } from '@forklaunch/implementation-worker-kafka/schemas';
import { KafkaWorkerOptions } from '@forklaunch/implementation-worker-kafka/types';
import { RedisWorkerConsumer } from '@forklaunch/implementation-worker-redis/consumers';
import { RedisWorkerProducer } from '@forklaunch/implementation-worker-redis/producers';
import { RedisWorkerSchemas } from '@forklaunch/implementation-worker-redis/schemas';
import { RedisWorkerOptions } from '@forklaunch/implementation-worker-redis/types';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { S3ObjectStore } from '@forklaunch/infrastructure-s3';
import {
  WorkerFailureHandler,
  WorkerProcessFunction
} from '@forklaunch/interfaces-worker/types';
import { ForkOptions } from '@mikro-orm/core';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import mikroOrmOptionsConfig from './mikro-orm.config';
import { SampleWorkerEventRecord } from './persistence/entities';
import { BaseSampleWorkerService } from './services/sampleWorker.service';

const BullMqWorkerOptionsSchema = BullMqWorkerSchemas({
  validator: schemaValidator
});

const KafkaWorkerOptionsSchema = KafkaWorkerSchemas({
  validator: schemaValidator
});

const RedisWorkerOptionsSchema = RedisWorkerSchemas({
  validator: schemaValidator
});

const DatabaseWorkerOptionsSchema = DatabaseWorkerSchemas({
  validator: schemaValidator
});

//! defines the configuration schema for the application
const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: {
      name: string,
      version: string
    },
    value: {
      name: 'sample-worker',
      version: '0.1.0'
    }
  }
});

//! defines the environment configuration for the application
const environmentConfig = configInjector.chain({
  REDIS_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('REDIS_URL')
  },
  PROTOCOL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('PROTOCOL')
  },
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
  VERSION: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('VERSION') ?? 'v1'
  },
  DOCS_PATH: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('DOCS_PATH') ?? '/docs'
  },
  OTEL_SERVICE_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_SERVICE_NAME')
  },
  OTEL_LEVEL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_LEVEL') || 'info'
  },
  OTEL_EXPORTER_OTLP_ENDPOINT: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_EXPORTER_OTLP_ENDPOINT')
  },
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
  },
  SAMPLE_WORKER_QUEUE: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('SAMPLE_WORKER_QUEUE')
  },
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
  S3_BUCKET: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('S3_BUCKET')
  },
  S3_URL: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('S3_URL')
  },
  HMAC_SECRET_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('HMAC_SECRET_KEY')
  },
  ENCRYPTION_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('ENCRYPTION_KEY')
  }
});

//! defines the runtime dependencies for the application
const runtimeDependencies = environmentConfig.chain({
  Orm: {
    lifetime: Lifetime.Singleton,
    type: MikroORM,
    factory: () => new MikroORM(mikroOrmOptionsConfig)
  },
  RedisWorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: RedisWorkerOptionsSchema,
    value: {
      pageSize: 100,
      retries: 3,
      interval: 5000
    }
  },
  BullMqWorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: BullMqWorkerOptionsSchema,
    factory: ({ REDIS_URL }) => ({
      backoffType: 'exponential' as const,
      queueOptions: {
        connection: {
          url: REDIS_URL
        }
      },
      retries: 3,
      interval: 5000
    })
  },
  KafkaWorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: KafkaWorkerOptionsSchema,
    factory: ({ KAFKA_BROKERS, KAFKA_CLIENT_ID, KAFKA_GROUP_ID }) => ({
      brokers: KAFKA_BROKERS,
      clientId: KAFKA_CLIENT_ID,
      groupId: KAFKA_GROUP_ID,
      retries: 3,
      interval: 5000,
      peekCount: 100
    })
  },
  DatabaseWorkerOptions: {
    lifetime: Lifetime.Singleton,
    type: DatabaseWorkerOptionsSchema,
    value: {
      retries: 3,
      interval: 5000
    }
  },
  OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector<Metrics>,
    factory: ({ OTEL_SERVICE_NAME, OTEL_LEVEL }) =>
      new OpenTelemetryCollector(
        OTEL_SERVICE_NAME,
        OTEL_LEVEL || 'info',
        metrics
      )
  },
  TtlCache: {
    lifetime: Lifetime.Singleton,
    type: RedisTtlCache,
    factory: ({ REDIS_URL, OtelCollector, OTEL_LEVEL, ENCRYPTION_KEY }) =>
      new RedisTtlCache(
        60 * 60 * 1000,
        OtelCollector,
        {
          url: REDIS_URL
        },
        {
          enabled: true,
          level: OTEL_LEVEL || 'info'
        },
        {
          encryptor: new FieldEncryptor(ENCRYPTION_KEY)
        }
      )
  },
  S3ObjectStore: {
    lifetime: Lifetime.Singleton,
    type: S3ObjectStore,
    factory: ({
      OtelCollector,
      OTEL_LEVEL,
      S3_REGION,
      S3_ACCESS_KEY_ID,
      S3_SECRET_ACCESS_KEY,
      S3_BUCKET,
      S3_URL,
      ENCRYPTION_KEY
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
        },
        {
          encryptor: new FieldEncryptor(ENCRYPTION_KEY)
        }
      )
  },
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: (
      { Orm },
      context: { entityManagerOptions?: ForkOptions; tenantId?: string }
    ) => {
      const em = Orm.em.fork(context.entityManagerOptions);
      if (context.tenantId) {
        em.setFilterParams('tenant', { tenantId: context.tenantId });
      }
      return em;
    }
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  SampleWorkerBullMqConsumer: {
    lifetime: Lifetime.Scoped,
    type: function_(
      [
        type<WorkerProcessFunction<SampleWorkerEventRecord>>(),
        type<WorkerFailureHandler<SampleWorkerEventRecord>>()
      ],
      type<BullMqWorkerConsumer<SampleWorkerEventRecord, BullMqWorkerOptions>>()
    ),
    factory:
      ({ SAMPLE_WORKER_QUEUE, BullMqWorkerOptions }) =>
      (
        processEventsFunction: WorkerProcessFunction<SampleWorkerEventRecord>,
        failureHandler: WorkerFailureHandler<SampleWorkerEventRecord>
      ) =>
        new BullMqWorkerConsumer(
          SAMPLE_WORKER_QUEUE,
          BullMqWorkerOptions,
          processEventsFunction,
          failureHandler
        )
  },
  SampleWorkerRedisConsumer: {
    lifetime: Lifetime.Scoped,
    type: function_(
      [
        type<WorkerProcessFunction<SampleWorkerEventRecord>>(),
        type<WorkerFailureHandler<SampleWorkerEventRecord>>()
      ],
      type<RedisWorkerConsumer<SampleWorkerEventRecord, RedisWorkerOptions>>()
    ),
    factory:
      ({ TtlCache, SAMPLE_WORKER_QUEUE, RedisWorkerOptions }) =>
      (
        processEventsFunction: WorkerProcessFunction<SampleWorkerEventRecord>,
        failureHandler: WorkerFailureHandler<SampleWorkerEventRecord>
      ) =>
        new RedisWorkerConsumer(
          SAMPLE_WORKER_QUEUE,
          TtlCache,
          RedisWorkerOptions,
          processEventsFunction,
          failureHandler
        )
  },
  SampleWorkerDatabaseConsumer: {
    lifetime: Lifetime.Scoped,
    type: function_(
      [
        type<WorkerProcessFunction<SampleWorkerEventRecord>>(),
        type<WorkerFailureHandler<SampleWorkerEventRecord>>()
      ],
      type<
        DatabaseWorkerConsumer<SampleWorkerEventRecord, DatabaseWorkerOptions>
      >()
    ),

    factory:
      ({ EntityManager, DatabaseWorkerOptions }) =>
      (
        processEventsFunction: WorkerProcessFunction<SampleWorkerEventRecord>,
        failureHandler: WorkerFailureHandler<SampleWorkerEventRecord>
      ) =>
        new DatabaseWorkerConsumer(
          SampleWorkerEventRecord,
          EntityManager,
          DatabaseWorkerOptions,
          processEventsFunction,
          failureHandler
        )
  },
  SampleWorkerKafkaConsumer: {
    lifetime: Lifetime.Scoped,
    type: function_(
      [
        type<WorkerProcessFunction<SampleWorkerEventRecord>>(),
        type<WorkerFailureHandler<SampleWorkerEventRecord>>()
      ],
      type<KafkaWorkerConsumer<SampleWorkerEventRecord, KafkaWorkerOptions>>()
    ),
    factory:
      ({ SAMPLE_WORKER_QUEUE, KafkaWorkerOptions, OtelCollector }) =>
      (
        processEventsFunction: WorkerProcessFunction<SampleWorkerEventRecord>,
        failureHandler: WorkerFailureHandler<SampleWorkerEventRecord>
      ) =>
        new KafkaWorkerConsumer(
          SAMPLE_WORKER_QUEUE,
          KafkaWorkerOptions,
          processEventsFunction,
          failureHandler,
          OtelCollector
        )
  },
  SampleWorkerDatabaseProducer: {
    lifetime: Lifetime.Scoped,
    type: DatabaseWorkerProducer,
    factory: ({ EntityManager, DatabaseWorkerOptions }) =>
      new DatabaseWorkerProducer(EntityManager, DatabaseWorkerOptions)
  },
  SampleWorkerRedisProducer: {
    lifetime: Lifetime.Scoped,
    type: RedisWorkerProducer,
    factory: ({ TtlCache, SAMPLE_WORKER_QUEUE, RedisWorkerOptions }) =>
      new RedisWorkerProducer(SAMPLE_WORKER_QUEUE, TtlCache, RedisWorkerOptions)
  },
  SampleWorkerKafkaProducer: {
    lifetime: Lifetime.Scoped,
    type: KafkaWorkerProducer,
    factory: ({ SAMPLE_WORKER_QUEUE, KafkaWorkerOptions }) =>
      new KafkaWorkerProducer(SAMPLE_WORKER_QUEUE, KafkaWorkerOptions)
  },
  SampleWorkerBullMqProducer: {
    lifetime: Lifetime.Scoped,
    type: BullMqWorkerProducer,
    factory: ({ SAMPLE_WORKER_QUEUE, BullMqWorkerOptions }) =>
      new BullMqWorkerProducer(SAMPLE_WORKER_QUEUE, BullMqWorkerOptions)
  },

  SampleWorkerService: {
    lifetime: Lifetime.Scoped,
    type: BaseSampleWorkerService,
    factory: ({
      EntityManager,
      SampleWorkerDatabaseProducer,
      SampleWorkerBullMqProducer,
      SampleWorkerKafkaProducer,
      SampleWorkerRedisProducer
    }) =>
      new BaseSampleWorkerService(
        EntityManager,
        SampleWorkerDatabaseProducer,
        SampleWorkerBullMqProducer,
        SampleWorkerRedisProducer,
        SampleWorkerKafkaProducer
      )
  }
});

export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});

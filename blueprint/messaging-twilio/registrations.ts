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
  FieldEncryptor,
  wrapEmWithTenantContext
} from '@forklaunch/core/persistence';
import {
  ComplianceDataService,
  createConfigInjector,
  getEnvVar,
  Lifetime,
  RetentionService
} from '@forklaunch/core/services';
import { TwilioSmsService } from '@forklaunch/implementation-messaging-twilio/services';
import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { ForkOptions } from '@mikro-orm/core';
import { EntityManager, MikroORM } from '@mikro-orm/postgresql';
import {
  SendSmsMapper,
  SmsRecordMapper
} from './domain/mappers/smsRecord.mappers';
import { SmsDtoTypes, SmsMapperTypes } from './domain/types/smsMappers.types';
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
      name: 'messaging',
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
    type: optional(string),
    value: getEnvVar('VERSION') ?? 'v1'
  },
  DOCS_PATH: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('DOCS_PATH') ?? '/docs'
  },
  OTEL_SERVICE_NAME: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_SERVICE_NAME')
  },
  OTEL_LEVEL: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('OTEL_LEVEL') ?? 'info'
  },
  OTEL_EXPORTER_OTLP_ENDPOINT: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('OTEL_EXPORTER_OTLP_ENDPOINT')
  },
  TWILIO_ACCOUNT_SID: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('TWILIO_ACCOUNT_SID')
  },
  TWILIO_AUTH_TOKEN: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('TWILIO_AUTH_TOKEN')
  },
  TWILIO_FROM_NUMBER: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('TWILIO_FROM_NUMBER')
  },
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
  EntityManager: {
    lifetime: Lifetime.Scoped,
    type: EntityManager,
    factory: (
      { Orm },
      context: { entityManagerOptions?: ForkOptions; tenantId?: string }
    ) =>
      wrapEmWithTenantContext(
        Orm.em.fork(context?.entityManagerOptions),
        context?.tenantId
      ) as EntityManager
  }
});

//! defines the service dependencies for the application
const serviceDependencies = runtimeDependencies.chain({
  SmsService: {
    lifetime: Lifetime.Scoped,
    type: TwilioSmsService<SchemaValidator, SmsMapperTypes, SmsDtoTypes>,
    factory: (
      {
        TWILIO_ACCOUNT_SID,
        TWILIO_AUTH_TOKEN,
        TWILIO_FROM_NUMBER,
        EntityManager,
        OtelCollector
      },
      context,
      resolve
    ) =>
      new TwilioSmsService(
        TWILIO_ACCOUNT_SID,
        TWILIO_AUTH_TOKEN,
        TWILIO_FROM_NUMBER,
        context.entityManagerOptions
          ? resolve('EntityManager', context)
          : EntityManager,
        OtelCollector,
        schemaValidator,
        {
          SmsRecordMapper,
          SendSmsMapper
        }
      )
  },
  ComplianceDataService: {
    lifetime: Lifetime.Singleton,
    type: ComplianceDataService,
    factory: ({ Orm, OtelCollector }) =>
      new ComplianceDataService(Orm, OtelCollector, {
        SmsRecord: 'to'
      })
  },
  RetentionService: {
    lifetime: Lifetime.Singleton,
    type: RetentionService,
    factory: ({ Orm, OtelCollector }) =>
      new RetentionService(Orm, OtelCollector)
  }
});

//! validates the configuration and returns the dependencies for the application
export const createDependencyContainer = (envFilePath: string) => ({
  ci: serviceDependencies.validateConfigSingletons(envFilePath),
  tokens: serviceDependencies.tokens()
});

/**
 * files registrations.
 * Feature-specific registrations are appended below the runtime base.
 */

import { number, optional, schemaValidator, string } from '@demo/core';
import { metrics } from '@demo/monitoring';
import { OpenTelemetryCollector } from '@forklaunch/core/http';
import {
  Lifetime,
  createConfigInjector,
  getEnvVar
} from '@forklaunch/core/services';
import { S3ObjectStore, s3ClientConfig } from '@forklaunch/infrastructure-s3';

const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: 'demo-files', version: '0.1.0' }
  }
});

const environmentConfig = configInjector.chain({
  PORT: {
    lifetime: Lifetime.Singleton,
    type: number,
    value: Number(getEnvVar('PORT'))
  },
  // Raised to debug when chasing a trace.
  OTEL_LEVEL: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('OTEL_LEVEL')
  },
  ENCRYPTION_KEY: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('ENCRYPTION_KEY')
  },
  S3_URL: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('S3_URL')
  },
  S3_REGION: {
    lifetime: Lifetime.Singleton,
    type: optional(string),
    value: getEnvVar('S3_REGION')
  },
  S3_BUCKET: {
    lifetime: Lifetime.Singleton,
    type: string,
    value: getEnvVar('S3_BUCKET')
  }
});

const runtimeDependencies = environmentConfig.chain({
  OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: ({ OTEL_LEVEL }) =>
      new OpenTelemetryCollector('demo-files', OTEL_LEVEL || 'info', metrics)
  },
  // This service's own registration: keys are stored per tenant.
  ObjectStore: {
    lifetime: Lifetime.Singleton,
    type: S3ObjectStore,
    factory: ({ OtelCollector, OTEL_LEVEL, S3_URL, S3_REGION, S3_BUCKET }) =>
      new S3ObjectStore(
        OtelCollector,
        {
          bucket: S3_BUCKET,
          clientConfig: s3ClientConfig({ url: S3_URL, region: S3_REGION })
        },
        { enabled: true, level: OTEL_LEVEL || 'info' }
      )
  }
});

export const createDependencyContainer = (envFilePath: string) => ({
  ci: runtimeDependencies.validateConfigSingletons(envFilePath),
  tokens: runtimeDependencies.tokens()
});

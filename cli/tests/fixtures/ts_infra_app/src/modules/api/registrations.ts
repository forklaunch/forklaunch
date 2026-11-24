/**
 * api registrations.
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

const configInjector = createConfigInjector(schemaValidator, {
  SERVICE_METADATA: {
    lifetime: Lifetime.Singleton,
    type: { name: string, version: string },
    value: { name: 'demo-api', version: '0.1.0' }
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
  }
});

const runtimeDependencies = environmentConfig.chain({
  OtelCollector: {
    lifetime: Lifetime.Singleton,
    type: OpenTelemetryCollector,
    factory: ({ OTEL_LEVEL }) =>
      new OpenTelemetryCollector('demo-api', OTEL_LEVEL || 'info', metrics)
  }
});

export const createDependencyContainer = (envFilePath: string) => ({
  ci: runtimeDependencies.validateConfigSingletons(envFilePath),
  tokens: runtimeDependencies.tokens()
});

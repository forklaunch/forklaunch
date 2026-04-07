import { forklaunchExpress, schemaValidator } from '@forklaunch/blueprint-core';
import { getEnvVar } from '@forklaunch/common';
import { setupRls, setupTenantFilter } from '@forklaunch/core/persistence';
import dotenv from 'dotenv';
import { sampleWorkerRouter } from './api/routes/sampleWorker.routes';
import { createDependencyContainer } from './registrations';
import { sampleWorkerSdkClient } from './sdk';

//! bootstrap resources and config
const envFilePath = getEnvVar('DOTENV_FILE_PATH');
dotenv.config({ path: envFilePath });
export const { ci, tokens } = createDependencyContainer(envFilePath);
export type ScopeFactory = typeof ci.createScope;

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const orm = ci.resolve(tokens.Orm);
setupTenantFilter(orm, { logger: openTelemetryCollector });
setupRls(orm, { logger: openTelemetryCollector });

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(schemaValidator, openTelemetryCollector);

//! resolves the protocol, host, port, and version from the configuration
const protocol = ci.resolve(tokens.PROTOCOL);
const host = ci.resolve(tokens.HOST);
const port = ci.resolve(tokens.PORT);
const version = ci.resolve(tokens.VERSION);
const docsPath = ci.resolve(tokens.DOCS_PATH);

//! mounts the routes to the app
app.use(sampleWorkerRouter);

//! registers the sdk client
app.registerSdks(sampleWorkerSdkClient);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 SampleWorker Server is running at ${protocol}://${host}:${port} 🎉.
    // An API reference can be accessed at ${protocol}://${host}:${port}/api/${version}${docsPath}`
  );
});

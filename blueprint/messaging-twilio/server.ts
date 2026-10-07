import {
  forklaunchExpress,
  PERMISSIONS,
  ROLES,
  schemaValidator
} from '@forklaunch/blueprint-core';
import { setupRls, setupTenantFilter } from '@forklaunch/core/persistence';
import { complianceRouter } from './api/routes/compliance.routes';
import { smsRouter } from './api/routes/sms.routes';
import { webhookRouter } from './api/routes/webhook.routes';
import { ci, tokens } from './bootstrapper';
import { messagingSdkClient } from './sdk';

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const orm = ci.resolve(tokens.Orm);
setupTenantFilter(orm, { logger: openTelemetryCollector });
setupRls(orm, { logger: openTelemetryCollector });

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(schemaValidator, openTelemetryCollector, {
  auth: {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    surfacePermissions: async (_payload, _req) => {
      //! return the permissions for the user, this is a placeholder
      return new Set([PERMISSIONS.PLATFORM_READ]);
    },
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    surfaceRoles: async (_payload, _req) => {
      //! return the roles for the user, this is a placeholder
      return new Set([ROLES.ADMIN]);
    }
  }
});

//! resolves the host, port, and version from the configuration
const host = ci.resolve(tokens.HOST);
const port = ci.resolve(tokens.PORT);
const version = ci.resolve(tokens.VERSION);
const docsPath = ci.resolve(tokens.DOCS_PATH);

//! mounts the routes to the app
app.use(smsRouter);
app.use(webhookRouter);
app.use(complianceRouter);

//! registers the sdk client
app.registerSdks(messagingSdkClient);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 Messaging Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});

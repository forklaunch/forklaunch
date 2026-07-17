import {
  forklaunchExpress,
  PERMISSIONS,
  ROLES,
  schemaValidator
} from './schema';
import { setupRls, setupTenantFilter } from '@forklaunch/core/persistence';
import { variantRouter } from './api/routes/variant.routes';
import { ci, tokens } from './bootstrapper';

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

//! routes are mounted here incrementally as each entity's PR lands.
app.use(variantRouter);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 Ecommerce Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});

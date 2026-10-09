import {
  createAuthCacheService,
  forklaunchExpress,
  schemaValidator
} from '@forklaunch/blueprint-core';
import { setupRls, setupTenantFilter } from '@forklaunch/core/persistence';
import { analyticsRouter } from './api/routes/analytics.routes';
import { claimRouter } from './api/routes/claim.routes';
import { codeSetRouter } from './api/routes/codeSet.routes';
import { codeValidationRouter } from './api/routes/codeValidation.routes';
import { complianceRouter } from './api/routes/compliance.routes';
import { denialRouter } from './api/routes/denial.routes';
import { ci, tokens } from './bootstrapper';
import { cacSdkClient } from './sdk';
import { createSurfacePermissions, createSurfaceRoles } from './surfacing';

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const orm = ci.resolve(tokens.Orm);
setupTenantFilter(orm, { logger: openTelemetryCollector });
setupRls(orm, { logger: openTelemetryCollector });

//! resolves the IAM cross-service call config (§3 "Cross-service calls to IAM")
const iamUrl = ci.resolve(tokens.IAM_URL);
const hmacSecretKey = ci.resolve(tokens.HMAC_SECRET_KEY);
const authCacheService = createAuthCacheService(ci.resolve(tokens.TtlCache));

//! creates an instance of forklaunchExpress
const app = forklaunchExpress(schemaValidator, openTelemetryCollector, {
  auth: {
    // RBAC verification pass (§14 PR 5): real coder/biller permissions and
    // roles surfaced from IAM, not the hardcoded placeholder this used to
    // be. Redis-backed caching (§12 item 8) added on top — see
    // ./surfacing.ts for the cache-check/cache-set details.
    surfacePermissions: createSurfacePermissions({
      authCacheService,
      iamUrl,
      hmacSecretKey
    }),
    surfaceRoles: createSurfaceRoles({ authCacheService, iamUrl, hmacSecretKey })
  }
});

//! resolves the host, port, and version from the configuration
const host = ci.resolve(tokens.HOST);
const port = ci.resolve(tokens.PORT);
const version = ci.resolve(tokens.VERSION);
const docsPath = ci.resolve(tokens.DOCS_PATH);

//! mounts the routes to the app
app.use(analyticsRouter);
app.use(claimRouter);
app.use(codeSetRouter);
app.use(codeValidationRouter);
app.use(complianceRouter);
app.use(denialRouter);

//! registers the sdk client
app.registerSdks(cacSdkClient);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 CAC Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});

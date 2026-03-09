import {
  forklaunchExpress,
  PERMISSIONS,
  ROLES,
  schemaValidator
} from '@forklaunch/blueprint-core';
import { billingPortalRouter } from './api/routes/billingPortal.routes';
import { checkoutSessionRouter } from './api/routes/checkoutSession.routes';
import { paymentLinkRouter } from './api/routes/paymentLink.routes';
import { planRouter } from './api/routes/plan.routes';
import { subscriptionRouter } from './api/routes/subscription.routes';
import { webhookRouter } from './api/routes/webhook.routes';
import { ci, tokens } from './bootstrapper';
import { billingSdkClient } from './sdk';

//! resolves the openTelemetryCollector from the configuration
const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);

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
app.use(billingPortalRouter);
app.use(checkoutSessionRouter);
app.use(paymentLinkRouter);
app.use(planRouter);
app.use(subscriptionRouter);
app.use(webhookRouter);

// //! registers the sdk client
app.registerSdks(billingSdkClient);

//! starts the server
app.listen(port, host, () => {
  openTelemetryCollector.info(
    `🎉 Billing Server is running at http://${host}:${port} 🎉.\nAn API reference can be accessed at http://${host}:${port}/api/${version}${docsPath}`
  );
});

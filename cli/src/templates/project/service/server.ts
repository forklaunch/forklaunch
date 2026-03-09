import { forklaunchExpress, {{#is_iam_configured}}PERMISSIONS, ROLES, {{/is_iam_configured}}SchemaValidator } from '@{{app_name}}/core';
{{#is_iam_configured}}import { createSurfacePermissions, createSurfaceRoles } from '@{{app_name}}/iam';
{{/is_iam_configured}}{{#is_billing_configured}}import { createSurfaceFeatures, createSurfaceSubscription } from '@{{app_name}}/billing';
{{/is_billing_configured}}import { {{camel_case_name}}Router } from './api/routes/{{camel_case_name}}.routes';
import { ci, tokens } from './bootstrapper';
import { {{camel_case_name}}SdkClient } from './sdk';

/**
 * Creates an instance of OpenTelemetryCollector
 */
const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
{{#is_iam_configured}}
const authCacheService = ci.resolve(tokens.AuthCacheService);
const iamUrl = ci.resolve(tokens.IAM_URL);
const hmacSecretKey = ci.resolve(tokens.HMAC_SECRET_KEY);
{{/is_iam_configured}}{{#is_billing_configured}}
const billingCacheService = ci.resolve(tokens.BillingCacheService);
const billingUrl = ci.resolve(tokens.BILLING_URL);
{{^is_iam_configured}}
const hmacSecretKey = ci.resolve(tokens.HMAC_SECRET_KEY);
{{/is_iam_configured}}
{{/is_billing_configured}}

/**
 * Creates surfacing functions for authentication
 */
async function createAuthOptions() {
{{#is_iam_configured}}{{#is_billing_configured}}
  // Both IAM and Billing are configured
  const [surfaceRoles, surfacePermissions, surfaceFeatures, surfaceSubscription] = await Promise.all([
    createSurfaceRoles({ authCacheService, iamUrl, hmacSecretKey }),
    createSurfacePermissions({ authCacheService, iamUrl, hmacSecretKey }),
    createSurfaceFeatures({ billingCacheService, billingUrl, hmacSecretKey }),
    createSurfaceSubscription({ billingCacheService, billingUrl, hmacSecretKey })
  ]);

  return {
    surfaceRoles,
    surfacePermissions,
    surfaceFeatures,
    surfaceSubscription
  };
{{/is_billing_configured}}{{^is_billing_configured}}
  // Only IAM is configured
  const [surfaceRoles, surfacePermissions] = await Promise.all([
    createSurfaceRoles({ authCacheService, iamUrl, hmacSecretKey }),
    createSurfacePermissions({ authCacheService, iamUrl, hmacSecretKey })
  ]);

  return {
    surfaceRoles,
    surfacePermissions,
    surfaceFeatures: async () => { throw new Error('Billing not configured: surfaceFeatures is not implemented'); },
    surfaceSubscription: async () => { throw new Error('Billing not configured: surfaceSubscription is not implemented'); }
  };
{{/is_billing_configured}}{{/is_iam_configured}}{{^is_iam_configured}}{{#is_billing_configured}}
  // Only Billing is configured
  const [surfaceFeatures, surfaceSubscription] = await Promise.all([
    createSurfaceFeatures({ billingCacheService, billingUrl, hmacSecretKey }),
    createSurfaceSubscription({ billingCacheService, billingUrl, hmacSecretKey })
  ]);

  return {
    surfaceRoles: async () => { throw new Error('IAM not configured: surfaceRoles is not implemented'); },
    surfacePermissions: async () => { throw new Error('IAM not configured: surfacePermissions is not implemented'); },
    surfaceFeatures,
    surfaceSubscription
  };
{{/is_billing_configured}}{{^is_billing_configured}}
  // Neither IAM nor Billing is configured
  return {
    surfaceRoles: async () => { throw new Error('IAM not configured: surfaceRoles is not implemented'); },
    surfacePermissions: async () => { throw new Error('IAM not configured: surfacePermissions is not implemented'); },
    surfaceFeatures: async () => { throw new Error('Billing not configured: surfaceFeatures is not implemented'); },
    surfaceSubscription: async () => { throw new Error('Billing not configured: surfaceSubscription is not implemented'); }
  };
{{/is_billing_configured}}{{/is_iam_configured}}
}

/**
 * Starts the server with configured auth options
 */
async function startServer() {
  const authOptions = await createAuthOptions();

  /**
   * Creates an instance of forklaunchExpress
   */
  const app = forklaunchExpress(SchemaValidator(), openTelemetryCollector, {
    auth: authOptions
  });

  //! resolves the protocol, host, port, and version from the configuration
  const protocol = ci.resolve(tokens.PROTOCOL);
  const host = ci.resolve(tokens.HOST);
  const port = ci.resolve(tokens.PORT);
  const version = ci.resolve(tokens.VERSION);
  const docsPath = ci.resolve(tokens.DOCS_PATH);

  //! mounts the routes to the app
  app.use({{camel_case_name}}Router);

  //! mounts the sdk to the app
  app.registerSdks({{camel_case_name}}SdkClient);

  //! starts the server
  app.listen(port, host, () => {
    openTelemetryCollector.info(
      `🎉 {{pascal_case_name}} Server is running at ${protocol}://${host}:${port} 🎉.
      // An API reference can be accessed at ${protocol}://${host}:${port}/api/${version}${docsPath}`
    );
  });
}

startServer().catch((error) => {
  console.error('Failed to start server:', error);
  process.exit(1);
});

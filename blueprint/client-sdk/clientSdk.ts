import { BillingSdkClient } from '@forklaunch/blueprint-billing-base';
import { BillingSdkClient as BillingStripeSdkClient } from '@forklaunch/blueprint-billing-stripe';
import { IamSdkClient } from '@forklaunch/blueprint-iam-base';
import {
  BetterAuthConfig,
  IamSdkClient as IamBetterAuthSdkClient
} from '@forklaunch/blueprint-iam-better-auth';
import { SampleWorkerSdkClient } from '@forklaunch/blueprint-sample-worker';
import { RegistryOptions, universalSdk } from '@forklaunch/universal-sdk';
import { createAuthClient } from 'better-auth/client';
import { inferAdditionalFields } from 'better-auth/client/plugins';

/**
 * The Client SDK is designed for use by external consumers (e.g. frontend applications,
 * partner integrations) to access all services in the workspace via a single client.
 *
 * internal services should NOT use this SDK. Instead, they should import the
 * framework `universalSdk` directly (`@forklaunch/universal-sdk`) and instantiate it
 * with the specific service types they need (e.g. `universalSdk<IamSdkClient>`).
 */
export const clientBillingSdkClient = universalSdk<BillingSdkClient>;
export const clientBillingStripeSdkClient =
  universalSdk<BillingStripeSdkClient>;
export const clientIamSdkClient = universalSdk<IamSdkClient>;
export const clientIamBetterAuthSdkClient = async ({
  host,
  registryOptions
}: {
  host: string;
  registryOptions: RegistryOptions;
}) => ({
  core: await universalSdk<IamBetterAuthSdkClient>({
    host,
    registryOptions: registryOptions
  }),
  betterAuth: createAuthClient({
    baseURL: host,
    // The Better Auth client always sends `credentials: 'include'`, so every
    // request is cookie-bearing and hits Better Auth's origin/CSRF guard.
    // Browsers set `Origin` automatically; non-browser runtimes (Node SDK
    // consumers, e2e tests) do not, so a cookie-bearing request would arrive
    // with a null Origin and be rejected (MISSING_OR_NULL_ORIGIN). Present this
    // client's own API origin — which Better Auth trusts as its `baseURL`.
    // Browsers ignore attempts to set the forbidden `Origin` header (using the
    // real one), so this is correct and harmless in every runtime.
    fetchOptions: {
      headers: { origin: new URL(host).origin }
    },
    plugins: [inferAdditionalFields<BetterAuthConfig>()]
  })
});
export const clientSampleWorkerSdkClient = universalSdk<SampleWorkerSdkClient>;

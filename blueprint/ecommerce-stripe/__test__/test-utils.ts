import { createHmacToken } from '@forklaunch/core/http';
import {
  BlueprintTestHarness,
  DatabaseType,
  TestSetupResult
} from '@forklaunch/testing';
import dotenv from 'dotenv';
import * as path from 'path';

export { TestSetupResult };

let harness: BlueprintTestHarness;

dotenv.config({ path: path.join(__dirname, '../.env.test') });

export const setupTestDatabase = async (): Promise<TestSetupResult> => {
  harness = new BlueprintTestHarness({
    getConfig: async () => {
      const { default: config } = await import('../mikro-orm.config');
      return config;
    },
    databaseType: (process.env.DATABASE_TYPE ?? 'postgresql') as DatabaseType,
    useMigrations: false,
    // This module's DI graph genuinely needs Redis: the cart is Redis-cached
    // (TtlCache) and the order-event producer publishes to a Redis-backed
    // queue. With needsRedis:false the harness starts no Redis container, so
    // container construction fails before the EntityManager is ever resolved
    // — which surfaced as a bare "Cannot read properties of undefined
    // (reading 'fork')" on Orm.em rather than anything mentioning Redis.
    needsRedis: true,
    customEnvVars: {
      // Placeholders are fine: the purchase-loop test drives this module's own
      // handlers and never calls a provider. They cannot be empty strings
      // though — the Stripe SDK throws at construction on an empty key, and
      // FieldEncryptor throws MissingEncryptionKeyError on a falsy master key.
      STRIPE_API_KEY:
        process.env.STRIPE_API_KEY || 'sk_test_placeholder_not_a_real_key',
      STRIPE_WEBHOOK_SECRET:
        process.env.STRIPE_WEBHOOK_SECRET ||
        'whsec_test_placeholder_not_a_real_secret',
      ENCRYPTION_KEY:
        process.env.ENCRYPTION_KEY || 'test-encryption-key-not-a-real-secret',
      PAYPAL_CLIENT_ID: process.env.PAYPAL_CLIENT_ID || 'test-paypal-client-id',
      PAYPAL_CLIENT_SECRET:
        process.env.PAYPAL_CLIENT_SECRET || 'test-paypal-client-secret',
      PAYPAL_BASE_URL:
        process.env.PAYPAL_BASE_URL || 'https://api-m.sandbox.paypal.com',
      PAYPAL_WEBHOOK_ID:
        process.env.PAYPAL_WEBHOOK_ID || 'test-paypal-webhook-id',
      ORDER_EVENT_QUEUE: process.env.ORDER_EVENT_QUEUE || 'test-order-events',
      HMAC_SECRET_KEY: TEST_HMAC_SECRET
    }
  });

  const setup = await harness.setup();

  // Point the application's own container at the ORM the harness just
  // initialised.
  //
  // registrations.ts builds its Orm with `new MikroORM(config)`, and in
  // MikroORM 7 that constructor does not create an EntityManager — only
  // MikroORM.init() does, and it is async so a synchronous DI factory cannot
  // call it. Anything resolving EntityManager therefore hits
  // `Orm.em.fork(...)` on an ORM whose `em` is undefined, which is why every
  // test that imports a route (rather than a service directly) failed with a
  // bare "Cannot read properties of undefined (reading 'fork')".
  //
  // The harness already owns a fully initialised ORM against the test
  // container, so hand the container that one instead of the unusable
  // instance its factory produced. This does not paper over a product bug:
  // the running server resolves the same token and gets a working em, so the
  // gap is in how tests construct the graph, not in the graph itself.
  if (setup.orm) {
    // Imported here rather than at module scope: loading the container runs
    // registrations, which validates its config singletons against the
    // environment. At module scope that happens before the harness has
    // published the container's host and port, and validation fails.
    const { ci } = await import('../bootstrapper');
    const container = ci as unknown as { instances?: Record<string, unknown> };
    if (container.instances) {
      container.instances.Orm = setup.orm;
    }
  }

  return setup;
};

export const cleanupTestDatabase = async (): Promise<void> => {
  if (harness) {
    await harness.cleanup();
  }
};

/**
 * The repo's shared TEST_TOKENS.HMAC (from @forklaunch/testing) is a fixed,
 * fake signature — it will fail real HMAC verification. Use this instead:
 * a real signature computed the same way the server verifies it. Two
 * things matter, both found by actually running requests, not by reading
 * the auth code alone:
 *
 * 1. `signedPath` must be the path as the route handler sees it (relative
 *    to the router's mount point), not the full request URL — e.g. for
 *    PUT /order/:id/transition, sign '/<id>/transition', not
 *    '/order/<id>/transition'.
 * 2. When there is no request body, the framework's own signing code
 *    embeds the literal string "undefined" in the signed message (a
 *    template-literal quirk, not a stylistic choice) — passing an empty
 *    string instead produces a signature that fails with the exact same
 *    403 as a wrong secret, with no way to tell the two apart from the
 *    response alone.
 */
export const TEST_HMAC_SECRET = 'test-hmac-secret-for-e2e';

export function signTestRequest(
  method: string,
  signedPath: string,
  body?: unknown
): `HMAC keyId=${string} ts=${string} nonce=${string} signature=${string}` {
  const timestamp = new Date();
  const nonce = crypto.randomUUID();
  const signature = createHmacToken({
    method,
    path: signedPath,
    body,
    timestamp,
    nonce,
    secretKey: TEST_HMAC_SECRET
  });
  return `HMAC keyId=default ts=${timestamp.toISOString()} nonce=${nonce} signature=${signature}`;
}

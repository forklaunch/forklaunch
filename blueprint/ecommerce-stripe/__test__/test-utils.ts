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
    needsRedis: false,
    customEnvVars: {
      STRIPE_API_KEY: process.env.STRIPE_API_KEY ?? '',
      STRIPE_WEBHOOK_SECRET: process.env.STRIPE_WEBHOOK_SECRET ?? '',
      HMAC_SECRET_KEY: TEST_HMAC_SECRET
    }
  });

  return await harness.setup();
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

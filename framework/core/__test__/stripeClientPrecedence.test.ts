import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { createStripeClient } from '../src/http/paymentsClient';

/**
 * Which Stripe createStripeClient talks to. A regular app's component that
 * uses platform email carries the managed contract (PLATFORM_GATEWAY_URL,
 * INSTANCE_ID, INSTANCE_HMAC_KEY) in its environment; a billing service that
 * passes its own key must keep talking to Stripe with that key.
 */

class FakeStripe {
  static createFetchHttpClient(fetchFn: unknown) {
    return { fetchFn };
  }
  constructor(
    readonly key: string | undefined,
    readonly config?: Record<string, unknown>
  ) {}
}

const MANAGED_ENV = {
  PLATFORM_GATEWAY_URL: 'https://gateway.forklaunch.test',
  INSTANCE_ID: '7d1e2f30-aaaa-4bbb-8ccc-0123456789ab',
  INSTANCE_HMAC_KEY: 'environment-key'
};

const saved: Record<string, string | undefined> = {};

function setEnv(values: Partial<typeof MANAGED_ENV>) {
  for (const key of Object.keys(MANAGED_ENV)) {
    const value = values[key as keyof typeof MANAGED_ENV];
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
}

beforeEach(() => {
  for (const key of Object.keys(MANAGED_ENV)) saved[key] = process.env[key];
});

afterEach(() => {
  for (const [key, value] of Object.entries(saved)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

const build = (options: Parameters<typeof createStripeClient>[0]) =>
  createStripeClient({
    Stripe: FakeStripe as never,
    ...options
  }) as unknown as FakeStripe;

describe('createStripeClient precedence', () => {
  it('an explicit apiKey wins over a managed contract in the environment', () => {
    setEnv(MANAGED_ENV);
    const client = build({ apiKey: 'sk_test_own' });
    expect(client.key).toBe('sk_test_own');
    expect(client.config?.host).toBeUndefined();
    expect(client.config?.httpClient).toBeUndefined();
  });

  it('with no apiKey, the managed contract routes through the gateway', () => {
    setEnv(MANAGED_ENV);
    const client = build({});
    expect(client.key).toBeUndefined();
    expect(client.config).toMatchObject({
      host: 'gateway.forklaunch.test',
      protocol: 'https'
    });
    expect(client.config?.httpClient).toBeDefined();
  });

  it('explicit gateway options outrank an explicit apiKey', () => {
    setEnv({});
    const client = build({
      apiKey: 'sk_test_own',
      gatewayUrl: 'http://localhost:18088',
      instanceId: 'i-1',
      hmacKey: 'k'
    });
    expect(client.key).toBeUndefined();
    expect(client.config).toMatchObject({
      host: 'localhost',
      protocol: 'http'
    });
  });

  it('a plain key without any managed contract is used directly', () => {
    setEnv({});
    expect(build({ apiKey: 'sk_test_own' }).key).toBe('sk_test_own');
  });

  it('refuses with neither a key nor a managed contract', () => {
    setEnv({});
    expect(() => build({})).toThrow(/no apiKey was given/);
  });
});

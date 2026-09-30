import { createRequire } from 'node:module';
import type Stripe from 'stripe';
import {
  createInstanceGatewayTransport,
  INSTANCE_GATEWAY_MOUNT,
  InstanceGatewayRequestError,
  instanceGatewaySettings,
  isManagedInstance,
  type InstanceGatewayOptions
} from './instanceGateway';
import { generateHmacAuthHeaders } from './generateHmacAuthHeaders';

/**
 * Payments for a managed instance: Stripe Connect through the platform.
 *
 * The product's publisher runs the Stripe Connect platform, linked once to
 * ForkLaunch. Each managed instance (a clinic, a studio) gets its OWN
 * connected account and is the merchant of record: charges are direct
 * charges on that account, and payouts go to the clinic's bank, never through
 * the app. The app never holds a Stripe key:
 *
 *   - `createPaymentsClient()` onboards the instance (a Stripe-hosted Account
 *     Link) and reports whether it can take payments yet;
 *   - `createStripeClient()` is the REAL Stripe SDK, pointed at the instance
 *     gateway. Every request is signed with the instance key, and the platform
 *     pins `Stripe-Account` to this instance's connected account, forwards
 *     only an allowlist of calls (customers, products and prices, checkout and
 *     billing-portal sessions, payment links, subscriptions, payment intents,
 *     refunds, invoices — never payouts, transfers, external accounts or
 *     account changes), adds the product's application fee, and refuses
 *     metadata that looks like protected data;
 *   - Stripe's events arrive as platform events (`/platform-events/payments`,
 *     type = the Stripe event type, id = the Stripe event id, data = the
 *     event's object), verified with `verifyPlatformEvent`.
 *
 * Outside managed mode `createStripeClient({ apiKey })` is `new Stripe(apiKey)`,
 * so the same registration runs against a developer's own test account.
 *
 * @example
 * const payments = createPaymentsClient();
 * const { url } = await payments.onboardingLink({
 *   returnUrl: 'https://clinic.example.com/settings/payments?done=1',
 *   refreshUrl: 'https://clinic.example.com/settings/payments'
 * });
 * const stripe = createStripeClient({ Stripe });
 * const session = await stripe.checkout.sessions.create({ mode: 'payment', … });
 */

/** Where the gateway proxies Stripe's API, router-relative. */
export const STRIPE_GATEWAY_ROUTE = '/stripe';

export interface PaymentsStatus {
  /** The connected account can take card payments. */
  chargesEnabled: boolean;
  /** Stripe can pay out to the clinic's bank account. */
  payoutsEnabled: boolean;
  /** What Stripe still needs (`individual.verification.document`, …). */
  requirementsDue: string[];
}

export interface PaymentsClient {
  /**
   * A single-use Stripe-hosted onboarding link for this instance's connected
   * account (created on first use). Send the clinic's owner to `url`; Stripe
   * returns them to `returnUrl`, or to `refreshUrl` when the link expired.
   */
  onboardingLink(params: {
    returnUrl: string;
    refreshUrl: string;
  }): Promise<{ url: string }>;
  /** Whether the instance can take payments yet. */
  status(): Promise<PaymentsStatus>;
}

/** A refusal or failure from the payments gateway. */
export class PaymentsRequestError extends InstanceGatewayRequestError {
  override readonly name = 'PaymentsRequestError' as const;
}

export function createPaymentsClient(
  options: InstanceGatewayOptions = {}
): PaymentsClient {
  const transport = createInstanceGatewayTransport(options, 'Payments');
  async function call<T>(
    method: 'GET' | 'POST',
    route: string,
    body?: Record<string, unknown>
  ): Promise<T> {
    try {
      return await transport.json<T>(method, route, body);
    } catch (error) {
      if (error instanceof InstanceGatewayRequestError) {
        throw new PaymentsRequestError(
          error.status,
          error.message,
          error.retryAfterSeconds
        );
      }
      throw error;
    }
  }
  return {
    onboardingLink({ returnUrl, refreshUrl }) {
      for (const [name, value] of [
        ['returnUrl', returnUrl],
        ['refreshUrl', refreshUrl]
      ] as const) {
        if (!/^https?:\/\//.test(value ?? '')) {
          throw new TypeError(`${name} must be an absolute http(s) URL`);
        }
      }
      return call('POST', '/payments/onboarding-link', {
        returnUrl,
        refreshUrl
      });
    },
    status() {
      return call('GET', '/payments/status');
    }
  };
}

type StripeConstructor = typeof Stripe;

export interface StripeClientOptions extends InstanceGatewayOptions {
  /**
   * A Stripe secret key, for a service NOT running in managed mode (local
   * development against your own test account, a self-hosted deployment).
   * Ignored in managed mode, where the platform holds the key.
   */
  apiKey?: string;
  /**
   * The `Stripe` class to construct. Pass your own `import Stripe from
   * 'stripe'` when the client is registered with `type: Stripe`: an ESM
   * service and this package could otherwise load different builds of the
   * SDK, and the registration's `instanceof` check would fail. Default: the
   * `stripe` package, loaded on first use (it is an optional peer dependency).
   */
  Stripe?: StripeConstructor;
  /** Extra SDK settings (apiVersion, maxNetworkRetries, timeout, appInfo). */
  config?: Stripe.StripeConfig;
}

function loadStripe(): StripeConstructor {
  // Loaded lazily so `@forklaunch/core/http` never needs `stripe` installed.
  // The CommonJS build has `__filename` (its `import.meta` is empty); the ESM
  // build has `import.meta.url`. (A bare `require` would be rewritten by the
  // bundler into a shim that throws under ESM.)
  const load = createRequire(
    typeof __filename === 'string' ? __filename : import.meta.url
  );
  try {
    const mod = load('stripe') as StripeConstructor & {
      default?: StripeConstructor;
    };
    return mod.default ?? mod;
  } catch {
    throw new Error(
      "createStripeClient needs the 'stripe' package: add it to this service (pnpm add stripe)"
    );
  }
}

/**
 * A fetch that sends Stripe SDK requests to the instance gateway, signed.
 *
 * The SDK builds `https://<host>:<port>/v1/...` from the `host`/`port`/
 * `protocol` settings; this rewrites the path under
 * `/instance-gateway/stripe` and adds the instance's HMAC `authorization`,
 * replacing any bearer key. The signature covers the method, the
 * router-relative path WITH its query string (`/stripe/v1/prices?active=true`,
 * so a list filter cannot be altered in flight) and the body exactly as sent:
 * Stripe's bodies are form-encoded, and the raw form string is signed
 * verbatim (framework `createHmacToken` treats a string body as the wire
 * bytes). An empty body signs as no body.
 */
export function stripeGatewayFetch(
  settings: { gatewayUrl: string; instanceId: string; hmacKey: string },
  fetchImpl: typeof fetch = fetch
): typeof fetch {
  const base = new URL(settings.gatewayUrl);
  const basePath = base.pathname.replace(/\/+$/, '');
  return async (input, init = {}) => {
    const original = new URL(
      typeof input === 'string' || input instanceof URL ? input : input.url
    );
    const routerPath = `${STRIPE_GATEWAY_ROUTE}${original.pathname}${original.search}`;
    const method = (init.method ?? 'GET').toUpperCase();
    const body = typeof init.body === 'string' ? init.body : undefined;
    const { authorization } = generateHmacAuthHeaders({
      secretKey: settings.hmacKey,
      method,
      path: routerPath,
      body: body || undefined,
      keyId: settings.instanceId
    });
    const headers = new Headers(init.headers);
    headers.set('authorization', authorization);
    const target = `${base.origin}${basePath}${INSTANCE_GATEWAY_MOUNT}${routerPath}`;
    const response = await fetchImpl(target, { ...init, method, headers });
    if (
      response.ok ||
      (response.headers.get('content-type') ?? '').includes('json')
    ) {
      return response;
    }
    // A refusal the gateway answered in plain text (a bad signature, an
    // unknown instance): give the SDK Stripe's error shape, so it raises the
    // usual typed error (401 -> StripeAuthenticationError) with the message.
    const message =
      (await response.text().catch(() => '')).trim() ||
      `Instance gateway answered ${response.status}`;
    return new Response(
      JSON.stringify({
        error: {
          type: 'invalid_request_error',
          code: 'forklaunch_gateway',
          message
        }
      }),
      {
        status: response.status,
        headers: {
          'content-type': 'application/json',
          'stripe-should-retry': 'false',
          ...(response.headers.get('retry-after')
            ? { 'retry-after': response.headers.get('retry-after') as string }
            : {})
        }
      }
    );
  };
}

/**
 * The Stripe SDK for this service.
 *
 * In managed mode (gateway options given, or `PLATFORM_GATEWAY_URL`,
 * `INSTANCE_ID` and `INSTANCE_HMAC_KEY` set) it is a real `Stripe` whose
 * requests go to the platform's instance gateway, signed with the instance
 * key; there is no Stripe key in the app. Otherwise it is `new
 * Stripe(apiKey)`.
 *
 * Mechanism (stripe-node 22): `host`/`port`/`protocol` point the SDK at the
 * gateway, `httpClient: Stripe.createFetchHttpClient(signedFetch)` rewrites
 * each request under `/instance-gateway/stripe` and signs it (see
 * `stripeGatewayFetch`), and a no-op `authenticator` stands in for the API
 * key so no bearer credential is ever sent. Retries re-enter the fetch, so
 * each attempt carries a fresh nonce.
 */
export function createStripeClient(options: StripeClientOptions = {}): Stripe {
  const StripeClass = options.Stripe ?? loadStripe();
  const explicitGateway = Boolean(
    options.gatewayUrl || options.instanceId || options.hmacKey
  );
  if (!explicitGateway && !isManagedInstance()) {
    if (!options.apiKey) {
      throw new Error(
        'createStripeClient: not a managed instance (PLATFORM_GATEWAY_URL, INSTANCE_ID and ' +
          'INSTANCE_HMAC_KEY are unset) and no apiKey was given — pass { apiKey } for local ' +
          'development, or run the gateway mock (forklaunch-gateway-mock) with the managed env'
      );
    }
    return new StripeClass(options.apiKey, options.config);
  }
  const settings = instanceGatewaySettings(options, 'Payments');
  const gateway = new URL(settings.gatewayUrl);
  const protocol = gateway.protocol === 'http:' ? 'http' : 'https';
  return new StripeClass(
    // No key: the authenticator below replaces it, and the gateway holds the
    // real one. (stripe-node accepts an authenticator in place of a key.)
    undefined as unknown as string,
    {
      maxNetworkRetries: 2,
      ...options.config,
      host: gateway.hostname,
      port: gateway.port || (protocol === 'http' ? 80 : 443),
      protocol,
      telemetry: false,
      // The signed fetch sets `authorization`; nothing else may.
      authenticator: async () => {},
      httpClient: StripeClass.createFetchHttpClient(
        stripeGatewayFetch(settings, options.fetch)
      )
    }
  );
}

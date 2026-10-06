import { createHmacToken } from './createHmacToken';
import { generateHmacAuthHeaders } from './generateHmacAuthHeaders';
import { timingSafeEqual } from 'crypto';

/**
 * The instance gateway: how a managed instance reaches platform-held services
 * (models, payments, email, SMS, WhatsApp, voice) without holding a vendor
 * credential, and how the platform delivers vendor events back to it.
 *
 * Outbound, every request is signed with the instance's own HMAC key
 * (`INSTANCE_HMAC_KEY`, key id `INSTANCE_ID`) over the router-relative path,
 * with a timestamp and a single-use nonce; the platform checks the signature,
 * the product's limits and the instance's own vendor account, then calls the
 * vendor with credentials that never leave it.
 *
 * Inbound, the platform POSTs events (a payment succeeded, an email bounced, a
 * text arrived) to `/platform-events/<feature>` on the service that declared
 * the capability, signed with the same instance key under key id `platform`.
 * `verifyPlatformEvent` checks that signature, the timestamp and the nonce.
 *
 * The platform injects `PLATFORM_GATEWAY_URL`, `INSTANCE_ID` and
 * `INSTANCE_HMAC_KEY` into a hosted instance. Their absence means the service
 * is not running in managed mode, and a client refuses to be created rather
 * than failing on its first call.
 */

/** Where the gateway router is mounted on the platform service. */
export const INSTANCE_GATEWAY_MOUNT = '/instance-gateway';

/** The key id the platform signs events with. */
export const PLATFORM_EVENT_KEY_ID = 'platform';

/** Where a service receives platform events, per feature. */
export const PLATFORM_EVENTS_PATH = '/platform-events';

const MAX_SKEW_MS = 5 * 60 * 1000;

export interface InstanceGatewayOptions {
  /** Platform base URL. Default: `PLATFORM_GATEWAY_URL`. */
  gatewayUrl?: string;
  /** This instance's id. Default: `INSTANCE_ID`. */
  instanceId?: string;
  /** This instance's HMAC key. Default: `INSTANCE_HMAC_KEY`. */
  hmacKey?: string;
  fetch?: typeof fetch;
}

/** A refusal or failure from the gateway, with the status it answered. */
export class InstanceGatewayRequestError extends Error {
  override readonly name: string = 'InstanceGatewayRequestError';
  constructor(
    readonly status: number,
    message: string,
    /** Seconds to wait before retrying, on a 429. */
    readonly retryAfterSeconds?: number
  ) {
    super(message);
  }
}

export interface InstanceGatewayTransport {
  readonly instanceId: string;
  /** A signed request; throws `InstanceGatewayRequestError` on a non-2xx. */
  request(
    method: 'GET' | 'POST' | 'PUT' | 'DELETE',
    route: string,
    body?: Record<string, unknown>,
    options?: { signal?: AbortSignal; headers?: Record<string, string> }
  ): Promise<Response>;
  /** `request`, then the JSON body. */
  json<T>(
    method: 'GET' | 'POST' | 'PUT' | 'DELETE',
    route: string,
    body?: Record<string, unknown>,
    options?: { signal?: AbortSignal; headers?: Record<string, string> }
  ): Promise<T>;
}

/** Resolve the managed-mode settings, or explain which are missing. */
export function instanceGatewaySettings(
  options: InstanceGatewayOptions = {},
  feature = 'The instance gateway'
): { gatewayUrl: string; instanceId: string; hmacKey: string } {
  const gatewayUrl = options.gatewayUrl ?? process.env.PLATFORM_GATEWAY_URL;
  const instanceId = options.instanceId ?? process.env.INSTANCE_ID;
  const hmacKey = options.hmacKey ?? process.env.INSTANCE_HMAC_KEY;
  if (!gatewayUrl || !instanceId || !hmacKey) {
    throw new Error(
      `${feature} is only available to instances hosted in managed mode: ` +
        'PLATFORM_GATEWAY_URL, INSTANCE_ID and INSTANCE_HMAC_KEY must be set'
    );
  }
  return { gatewayUrl, instanceId, hmacKey };
}

/** Whether this process runs as a managed instance. */
export function isManagedInstance(
  env: Record<string, string | undefined> = process.env
): boolean {
  return Boolean(
    env.PLATFORM_GATEWAY_URL && env.INSTANCE_ID && env.INSTANCE_HMAC_KEY
  );
}

function trimTrailingSlashes(value: string): string {
  // Without a regex: `/\/+$/` backtracks on long runs.
  let end = value.length;
  while (end > 0 && value[end - 1] === '/') end -= 1;
  return value.slice(0, end);
}

/** A signed transport to the instance gateway. */
export function createInstanceGatewayTransport(
  options: InstanceGatewayOptions = {},
  feature?: string
): InstanceGatewayTransport {
  const { gatewayUrl, instanceId, hmacKey } = instanceGatewaySettings(
    options,
    feature
  );
  const fetchImpl = options.fetch ?? fetch;
  const base = trimTrailingSlashes(gatewayUrl);

  const request: InstanceGatewayTransport['request'] = async (
    method,
    route,
    body,
    requestOptions = {}
  ) => {
    // The gateway verifies the router-relative path, and the body as parsed.
    const { authorization } = generateHmacAuthHeaders({
      secretKey: hmacKey,
      method,
      path: route,
      body,
      keyId: instanceId
    });
    const response = await fetchImpl(`${base}${INSTANCE_GATEWAY_MOUNT}${route}`, {
      method,
      headers: {
        ...requestOptions.headers,
        authorization,
        ...(body ? { 'content-type': 'application/json' } : {})
      },
      body: body ? JSON.stringify(body) : undefined,
      signal: requestOptions.signal
    });
    if (!response.ok) {
      const message = (await response.text().catch(() => '')).trim();
      const retryAfter = Number(response.headers.get('retry-after'));
      throw new InstanceGatewayRequestError(
        response.status,
        message || `Instance gateway answered ${response.status}`,
        Number.isFinite(retryAfter) && retryAfter > 0 ? retryAfter : undefined
      );
    }
    return response;
  };

  return {
    instanceId,
    request,
    async json<T>(
      method: 'GET' | 'POST' | 'PUT' | 'DELETE',
      route: string,
      body?: Record<string, unknown>,
      requestOptions?: { signal?: AbortSignal; headers?: Record<string, string> }
    ): Promise<T> {
      return (await (
        await request(method, route, body, requestOptions)
      ).json()) as T;
    }
  };
}

/** An event the platform delivered to this instance. */
export interface PlatformEvent<Data = Record<string, unknown>> {
  /** Unique per event; deliveries can repeat, so handlers dedupe on it. */
  id: string;
  /** The capability it belongs to: payments, email, sms, whatsapp, voice. */
  feature: string;
  /** e.g. `checkout.session.completed`, `email.bounced`, `sms.received`. */
  type: string;
  occurredAt: string;
  data: Data;
}

export class PlatformEventVerificationError extends Error {
  override readonly name = 'PlatformEventVerificationError';
}

const seenEventNonces = new Map<string, number>();

/**
 * Verify an event the platform delivered and return it.
 *
 * `path` is the path the request arrived on (`/platform-events/payments`);
 * `rawBody` is the exact request body (a string or Buffer) — pass the raw
 * bytes when the framework captured them, else the parsed body. Throws
 * `PlatformEventVerificationError` for a missing, stale, forged or replayed
 * delivery; answer 401 then, and the platform retries.
 */
export function verifyPlatformEvent<Data = Record<string, unknown>>(
  request: {
    method: string;
    path: string;
    headers: Record<string, string | string[] | undefined>;
    body: unknown;
  },
  options: { hmacKey?: string; now?: () => number } = {}
): PlatformEvent<Data> {
  const hmacKey = options.hmacKey ?? process.env.INSTANCE_HMAC_KEY;
  if (!hmacKey) {
    throw new PlatformEventVerificationError(
      'INSTANCE_HMAC_KEY is not set: this service is not a managed instance'
    );
  }
  const header = request.headers.authorization ?? request.headers.Authorization;
  const value = Array.isArray(header) ? header[0] : header;
  const match =
    /^HMAC keyId=(\S+) ts=(\S+) nonce=(\S+) signature=(\S+)$/.exec(
      value ?? ''
    );
  if (!match) {
    throw new PlatformEventVerificationError(
      'Missing or malformed HMAC authorization'
    );
  }
  const [, keyId, ts, nonce, signature] = match;
  if (keyId !== PLATFORM_EVENT_KEY_ID) {
    throw new PlatformEventVerificationError('Not signed by the platform');
  }
  const timestamp = new Date(ts);
  const now = (options.now ?? Date.now)();
  if (
    Number.isNaN(timestamp.getTime()) ||
    Math.abs(now - timestamp.getTime()) > MAX_SKEW_MS
  ) {
    throw new PlatformEventVerificationError('Stale or invalid timestamp');
  }
  const expected = Buffer.from(
    createHmacToken({
      secretKey: hmacKey,
      method: request.method.toUpperCase(),
      path: request.path,
      body: request.body,
      timestamp,
      nonce
    })
  );
  const received = Buffer.from(signature);
  if (
    expected.length !== received.length ||
    !timingSafeEqual(expected, received)
  ) {
    throw new PlatformEventVerificationError('Invalid signature');
  }
  for (const [seen, at] of seenEventNonces) {
    if (now - at > MAX_SKEW_MS) seenEventNonces.delete(seen);
  }
  if (seenEventNonces.has(nonce)) {
    throw new PlatformEventVerificationError('Replayed delivery');
  }
  seenEventNonces.set(nonce, now);

  const parsed =
    typeof request.body === 'string' || Buffer.isBuffer(request.body)
      ? JSON.parse(request.body.toString())
      : request.body;
  const event = parsed as Partial<PlatformEvent<Data>>;
  if (
    !event ||
    typeof event.id !== 'string' ||
    typeof event.type !== 'string' ||
    typeof event.feature !== 'string'
  ) {
    throw new PlatformEventVerificationError('Body is not a platform event');
  }
  return event as PlatformEvent<Data>;
}

/**
 * Sign an event delivery the way the platform does. For tests and the local
 * gateway mock; the platform has its own copy.
 */
export function signPlatformEvent(params: {
  hmacKey: string;
  path: string;
  body: string;
}): { authorization: string } {
  return generateHmacAuthHeaders({
    secretKey: params.hmacKey,
    method: 'POST',
    path: params.path,
    body: params.body,
    keyId: PLATFORM_EVENT_KEY_ID
  });
}

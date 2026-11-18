import {
  MockSchemaValidator,
  mockSchemaValidator
} from '@forklaunch/validator/tests/mockSchemaValidator';
import { JWTPayload } from 'jose';
import { describe, expect, it } from 'vitest';
import type { TtlCache } from '../src/cache/interfaces/ttlCache.interface';
import {
  createHmacToken,
  ForklaunchRequest,
  ForklaunchResponse,
  generateHmacAuthHeaders,
  HttpContractDetails,
  MetricsDefinition,
  OpenTelemetryCollector,
  ParamsDictionary,
  RequestContext,
  SessionObject
} from '../src/http';
import {
  DEFAULT_HMAC_MAX_SKEW_MS,
  HmacNonceStore,
  InMemoryHmacNonceStore,
  isHmacTimestampFresh,
  timingSafeEqualStrings,
  ttlCacheHmacNonceStore
} from '../src/http/hmacReplayProtection';
import { parseRequestAuth } from '../src/http/middleware/request/auth.middleware';
import { ExpressLikeRouterOptions } from '../src/http/types/expressLikeOptions';

const SECRET = 'service-shared-secret';
const KEY_ID = 'billing';
const PATH = '/internal/charge';
const BODY = '{"amount":100}';

type MockRequest = ForklaunchRequest<
  MockSchemaValidator,
  ParamsDictionary,
  Record<string, unknown>,
  Record<string, string>,
  Record<string, string>,
  never,
  Record<string, unknown>
>;

function createRequest(
  authorization: string | undefined,
  hmac: Record<string, unknown> = {},
  overrides: Record<string, unknown> = {}
): MockRequest {
  return {
    path: PATH,
    originalPath: PATH,
    originalUrl: PATH,
    method: 'POST',
    context: {} as RequestContext,
    contractDetails: {
      name: 'Charge',
      summary: 'Charge',
      access: 'internal',
      auth: { hmac: { secretKeys: { [KEY_ID]: SECRET }, ...hmac } }
    } as unknown as HttpContractDetails<MockSchemaValidator>,
    schemaValidator: mockSchemaValidator,
    params: {},
    headers: authorization ? { authorization } : {},
    body: BODY,
    query: {},
    requestSchema: {},
    openTelemetryCollector: {
      error: () => {},
      warn: () => {},
      debug: () => {}
    } as unknown as OpenTelemetryCollector<MetricsDefinition>,
    version: {} as never,
    session: {} as JWTPayload,
    _parsedVersions: 0,
    _globalOptions: () =>
      ({}) as ExpressLikeRouterOptions<
        MockSchemaValidator,
        SessionObject<MockSchemaValidator>
      >,
    ...overrides
  } as unknown as MockRequest;
}

function createResponse() {
  let statusCode: number | undefined;
  let sent: unknown;
  const res = {
    bodyData: null,
    statusCode: 200,
    headersSent: false,
    metricRecorded: false,
    getHeaders: () => ({ 'x-correlation-id': 'test-correlation-id' }),
    getHeader: () => undefined,
    setHeader: () => res,
    on: () => res,
    end: () => {},
    type: () => res,
    locals: {},
    cors: false,
    responseSchemas: { headers: {}, responses: {} },
    sent: false,
    version: {} as never,
    status: (code: number) => {
      statusCode = code;
      return {
        json: (data: unknown) => {
          sent = data;
          return true;
        },
        jsonp: () => true,
        send: (data: unknown) => {
          sent = data;
          return true;
        },
        sseEmitter: () => Promise.resolve(),
        type: () => res
      };
    }
  };
  return {
    res: res as unknown as ForklaunchResponse<
      unknown,
      Record<number, unknown>,
      Record<string, unknown>,
      Record<string, unknown>,
      never
    >,
    status: () => statusCode,
    sent: () => sent
  };
}

/** Runs the real auth middleware; `passed` means the route would run. */
async function authenticate(req: MockRequest) {
  const { res, status, sent } = createResponse();
  let passed = false;
  await parseRequestAuth(req, res, () => {
    passed = true;
  });
  return { passed, status: status(), message: sent() };
}

/** An authorization header signed the way a service client signs it. */
function sign(
  options: {
    timestamp?: Date;
    nonce?: string;
    body?: unknown;
    path?: string;
    secretKey?: string;
    keyId?: string;
  } = {}
): string {
  const timestamp = options.timestamp ?? new Date();
  const nonce = options.nonce ?? crypto.randomUUID();
  const signature = createHmacToken({
    method: 'POST',
    path: options.path ?? PATH,
    body: 'body' in options ? options.body : BODY,
    timestamp,
    nonce,
    secretKey: options.secretKey ?? SECRET
  });
  return `HMAC keyId=${options.keyId ?? KEY_ID} ts=${timestamp.toISOString()} nonce=${nonce} signature=${signature}`;
}

const minutesAgo = (minutes: number) =>
  new Date(Date.now() - minutes * 60 * 1000);

describe('hmac auth: signature, freshness and replay', () => {
  it('accepts a valid, fresh request', async () => {
    const result = await authenticate(createRequest(sign()));
    expect(result.passed).toBe(true);
    expect(result.status).toBeUndefined();
  });

  it('accepts what generateHmacAuthHeaders produces, with a new timestamp and nonce every call', async () => {
    const make = () =>
      generateHmacAuthHeaders({
        secretKey: SECRET,
        method: 'POST',
        path: PATH,
        body: BODY,
        keyId: KEY_ID
      }).authorization;
    const first = make();
    const second = make();
    expect(first.match(/nonce=(\S+)/)?.[1]).not.toBe(
      second.match(/nonce=(\S+)/)?.[1]
    );
    // A retry that signs again is a new request, not a replay.
    expect((await authenticate(createRequest(first))).passed).toBe(true);
    expect((await authenticate(createRequest(second))).passed).toBe(true);
  });

  it('rejects a replayed request (same nonce) with the default in-memory store', async () => {
    const authorization = sign();
    expect((await authenticate(createRequest(authorization))).passed).toBe(
      true
    );
    const replay = await authenticate(createRequest(authorization));
    expect(replay.passed).toBe(false);
    expect(replay.status).toBe(403);
  });

  it('rejects a reused nonce even with a new timestamp', async () => {
    const nonce = crypto.randomUUID();
    expect(
      (await authenticate(createRequest(sign({ nonce, timestamp: minutesAgo(1) }))))
        .passed
    ).toBe(true);
    const reused = await authenticate(createRequest(sign({ nonce })));
    expect(reused.passed).toBe(false);
    expect(reused.status).toBe(403);
  });

  it('rejects a stale timestamp, in the past or the future, and accepts one inside the window', async () => {
    const stale = await authenticate(
      createRequest(sign({ timestamp: minutesAgo(6) }))
    );
    expect(stale.passed).toBe(false);
    expect(stale.status).toBe(403);

    const future = await authenticate(
      createRequest(sign({ timestamp: minutesAgo(-6) }))
    );
    expect(future.passed).toBe(false);
    expect(future.status).toBe(403);

    expect(
      (await authenticate(createRequest(sign({ timestamp: minutesAgo(4) }))))
        .passed
    ).toBe(true);
  });

  it('honours a configured window', async () => {
    const twoMinutesOld = () => sign({ timestamp: minutesAgo(2) });
    const tight = await authenticate(
      createRequest(twoMinutesOld(), { maxSkewMs: 60_000 })
    );
    expect(tight.passed).toBe(false);
    expect(tight.status).toBe(403);

    const wide = await authenticate(
      createRequest(sign({ timestamp: minutesAgo(8) }), {
        maxSkewMs: 10 * 60_000
      })
    );
    expect(wide.passed).toBe(true);
  });

  it('rejects a missing or malformed timestamp without throwing', async () => {
    const valid = sign();
    for (const authorization of [
      valid.replace(/ts=\S+/, 'ts=not-a-date'),
      valid.replace(/ts=\S+/, 'ts=1234567890'),
      valid.replace(/ts=\S+/, 'ts='),
      valid.replace(/ ts=\S+/, '')
    ]) {
      const result = await authenticate(createRequest(authorization));
      expect(result.passed).toBe(false);
      expect([401, 403]).toContain(result.status);
    }
  });

  it('rejects a tampered signature, of equal or different length, without throwing', async () => {
    const valid = sign();
    const signature = valid.match(/signature=(\S+)$/)?.[1] as string;
    const flipped =
      (signature[0] === 'A' ? 'B' : 'A') + signature.slice(1);
    for (const forged of [flipped, signature.slice(0, -4), `${signature}AAAA`, 'x']) {
      const result = await authenticate(
        createRequest(valid.replace(/signature=\S+$/, `signature=${forged}`))
      );
      expect(result.passed).toBe(false);
      expect(result.status).toBe(403);
    }
    // The failed attempts did not use up the nonce of the genuine request.
    expect((await authenticate(createRequest(valid))).passed).toBe(true);
  });

  it('rejects a tampered body, path or wrong secret', async () => {
    const tamperedBody = await authenticate(
      createRequest(sign(), {}, { body: '{"amount":999999}' })
    );
    expect(tamperedBody.passed).toBe(false);
    expect(tamperedBody.status).toBe(403);

    const otherPath = await authenticate(
      createRequest(sign({ path: '/internal/refund' }))
    );
    expect(otherPath.passed).toBe(false);

    const wrongSecret = await authenticate(
      createRequest(sign({ secretKey: 'guessed' }))
    );
    expect(wrongSecret.passed).toBe(false);
    expect(wrongSecret.status).toBe(403);
  });

  it('rejects an unknown key id, including inherited property names, without throwing', async () => {
    for (const keyId of ['unknown', 'constructor', '__proto__', 'toString']) {
      const result = await authenticate(createRequest(sign({ keyId })));
      expect(result.passed).toBe(false);
      expect(result.status).toBe(403);
    }
  });

  it('claims the nonce once when the deprecated router-relative path is signed', async () => {
    const authorization = sign({ path: '/charge' });
    const mounted = { path: '/charge', originalUrl: '/internal/charge?x=1' };
    expect(
      (await authenticate(createRequest(authorization, {}, mounted))).passed
    ).toBe(true);
    expect(
      (await authenticate(createRequest(authorization, {}, mounted))).passed
    ).toBe(false);
  });

  it('uses a shared nonce store so a replay to another copy of the app is rejected', async () => {
    // Two "copies" of the app share one store, as they would share Redis.
    const claimed = new Map<string, number>();
    const shared: HmacNonceStore = {
      claim: async (key, ttlMs) => {
        if (claimed.has(key)) return false;
        claimed.set(key, ttlMs);
        return true;
      }
    };
    const authorization = sign();
    expect(
      (await authenticate(createRequest(authorization, { nonceStore: shared })))
        .passed
    ).toBe(true);
    expect(
      (await authenticate(createRequest(authorization, { nonceStore: shared })))
        .passed
    ).toBe(false);
    expect(claimed.size).toBe(1);
    const [[key, ttlMs]] = [...claimed];
    expect(key).toContain(KEY_ID);
    // Remembered until the signature can no longer pass the freshness check.
    expect(ttlMs).toBeGreaterThan(DEFAULT_HMAC_MAX_SKEW_MS - 5_000);
    expect(ttlMs).toBeLessThanOrEqual(DEFAULT_HMAC_MAX_SKEW_MS);
  });

  it('accepts a TtlCache as the shared nonce store', async () => {
    const records = new Map<string, unknown>();
    const cache = {
      peekRecord: async (key: string) => records.has(key),
      putRecord: async ({ key, value }: { key: string; value: unknown }) => {
        records.set(key, value);
      }
    } as unknown as TtlCache;
    const authorization = sign();
    expect(
      (await authenticate(createRequest(authorization, { nonceStore: cache })))
        .passed
    ).toBe(true);
    expect(
      (await authenticate(createRequest(authorization, { nonceStore: cache })))
        .passed
    ).toBe(false);
    expect([...records.keys()][0]).toMatch(/^hmac-nonce:/);
  });

  it('fails closed when the nonce store is unavailable', async () => {
    const broken: HmacNonceStore = {
      claim: async () => {
        throw new Error('redis down');
      }
    };
    const result = await authenticate(
      createRequest(sign(), { nonceStore: broken })
    );
    expect(result.passed).toBe(false);
    expect(result.status).toBe(403);
  });

  it('does not claim a nonce for a request that fails verification', async () => {
    let claims = 0;
    const counting: HmacNonceStore = {
      claim: () => {
        claims += 1;
        return true;
      }
    };
    await authenticate(
      createRequest(sign({ secretKey: 'guessed' }), { nonceStore: counting })
    );
    await authenticate(
      createRequest(sign({ timestamp: minutesAgo(30) }), {
        nonceStore: counting
      })
    );
    expect(claims).toBe(0);
  });
});

describe('hmac replay helpers', () => {
  it('compares signatures in constant time and rejects a length mismatch', () => {
    expect(timingSafeEqualStrings('abc123', 'abc123')).toBe(true);
    expect(timingSafeEqualStrings('abc123', 'abc124')).toBe(false);
    expect(timingSafeEqualStrings('abc123', 'abc')).toBe(false);
    expect(timingSafeEqualStrings('abc123', '')).toBe(false);
    expect(
      timingSafeEqualStrings('abc123', undefined as unknown as string)
    ).toBe(false);
  });

  it('treats an invalid or out-of-window timestamp as stale', () => {
    const now = Date.now();
    expect(isHmacTimestampFresh(new Date(now), now)).toBe(true);
    expect(isHmacTimestampFresh(new Date(now - 299_000), now)).toBe(true);
    expect(isHmacTimestampFresh(new Date(now - 301_000), now)).toBe(false);
    expect(isHmacTimestampFresh(new Date(now + 301_000), now)).toBe(false);
    expect(isHmacTimestampFresh(new Date('nope'), now)).toBe(false);
    expect(isHmacTimestampFresh(undefined, now)).toBe(false);
  });

  it('in-memory store: one claim per nonce until it expires', () => {
    const store = new InMemoryHmacNonceStore();
    expect(store.claim('a', 1_000, 0)).toBe(true);
    expect(store.claim('a', 1_000, 500)).toBe(false);
    expect(store.claim('b', 1_000, 500)).toBe(true);
    // Expired: the timestamp it belonged to can no longer verify anyway.
    expect(store.claim('a', 1_000, 1_001)).toBe(true);
  });

  it('in-memory store: bounded, and expired entries are swept', () => {
    const store = new InMemoryHmacNonceStore({
      maxEntries: 3,
      sweepIntervalMs: 10
    });
    for (const key of ['a', 'b', 'c', 'd', 'e']) {
      expect(store.claim(key, 60_000, 0)).toBe(true);
      expect(store.size).toBeLessThanOrEqual(3);
    }
    // The newest are still remembered.
    expect(store.claim('e', 60_000, 1)).toBe(false);
    expect(store.claim('d', 60_000, 1)).toBe(false);
    // Everything has expired by now and is swept on the next claim.
    expect(store.claim('z', 60_000, 120_000)).toBe(true);
    expect(store.size).toBe(1);
  });

  it('TtlCache adapter claims atomically when the cache has putRecordIfAbsent', async () => {
    // An in-memory cache whose set-if-absent is one step, like Redis SET NX.
    const records = new Map<string, number>();
    let peeks = 0;
    let puts = 0;
    const cache = {
      putRecordIfAbsent: async ({
        key,
        ttlMilliseconds
      }: {
        key: string;
        ttlMilliseconds: number;
      }) => {
        if (records.has(key)) return false;
        records.set(key, ttlMilliseconds);
        return true;
      },
      peekRecord: async () => {
        peeks += 1;
        // Deliberately slow and wrong: the atomic path must not rely on it.
        await new Promise((resolve) => setTimeout(resolve, 5));
        return false;
      },
      putRecord: async () => {
        puts += 1;
      }
    };
    const store = ttlCacheHmacNonceStore(cache as never);
    // Twenty simultaneous claims of one nonce: exactly one wins.
    const results = await Promise.all(
      Array.from({ length: 20 }, () => store.claim('n', 1234.2))
    );
    expect(results.filter(Boolean)).toHaveLength(1);
    expect(await store.claim('n', 1234.2)).toBe(false);
    expect(await store.claim('other', 10)).toBe(true);
    expect(records.get('hmac-nonce:n')).toBe(1235);
    expect(peeks).toBe(0);
    expect(puts).toBe(0);
  });

  it('TtlCache adapter fallback (no putRecordIfAbsent) is check-then-write, so simultaneous claims can both pass', async () => {
    const records = new Set<string>();
    const store = ttlCacheHmacNonceStore({
      peekRecord: async (key) => {
        const present = records.has(key);
        await new Promise((resolve) => setTimeout(resolve, 5));
        return present;
      },
      putRecord: async ({ key }) => {
        records.add(key);
      }
    });
    // This documents the limitation the atomic path removes.
    const simultaneous = await Promise.all([
      store.claim('n', 1000),
      store.claim('n', 1000)
    ]);
    expect(simultaneous).toEqual([true, true]);
    // A replay that arrives afterwards is still rejected.
    expect(await store.claim('n', 1000)).toBe(false);
  });

  it('a replay through the middleware is rejected with an atomic cache as the nonce store', async () => {
    const records = new Set<string>();
    const cache = {
      putRecordIfAbsent: async ({ key }: { key: string }) => {
        if (records.has(key)) return false;
        records.add(key);
        return true;
      }
    } as unknown as TtlCache;
    const authorization = sign();
    const results = await Promise.all(
      Array.from({ length: 5 }, () =>
        authenticate(createRequest(authorization, { nonceStore: cache }))
      )
    );
    expect(results.filter((result) => result.passed)).toHaveLength(1);
  });

  it('TtlCache adapter stores the nonce with the remaining lifetime', async () => {
    const puts: { key: string; ttlMilliseconds: number }[] = [];
    const store = ttlCacheHmacNonceStore({
      peekRecord: async (key) => puts.some((put) => put.key === key),
      putRecord: async (record) => {
        puts.push(record);
      }
    });
    expect(await store.claim('n', 1234.2)).toBe(true);
    expect(await store.claim('n', 1234.2)).toBe(false);
    expect(puts).toEqual([
      { key: 'hmac-nonce:n', value: 1, ttlMilliseconds: 1235 }
    ]);
  });
});

import { timingSafeEqual } from 'crypto';
import type { TtlCache } from '../cache/interfaces/ttlCache.interface';

/**
 * Replay protection shared by every HMAC verifier in the framework: the
 * `hmac` route auth method (service-to-service calls) and platform event
 * deliveries (`verifyPlatformEvent`).
 *
 * A signed request is accepted only when
 *   1. its timestamp is within the allowed clock skew of now,
 *   2. its signature matches (compared in constant time), and
 *   3. its nonce has not been used before inside the freshness window.
 *
 * The nonce is claimed only after the signature has verified, so a caller
 * without the key can never fill the store or burn somebody else's nonce.
 */

/** How far a request timestamp may be from the server clock, either way. */
export const DEFAULT_HMAC_MAX_SKEW_MS = 5 * 60 * 1000;

/** How many nonces the default in-memory store remembers. */
export const DEFAULT_HMAC_NONCE_STORE_MAX_ENTRIES = 100_000;

/**
 * Constant-time comparison of two signatures. A length mismatch is a
 * rejection, never a throw (`timingSafeEqual` throws on unequal lengths).
 */
export function timingSafeEqualStrings(
  expected: string,
  received: string
): boolean {
  if (typeof expected !== 'string' || typeof received !== 'string') {
    return false;
  }
  const expectedBytes = Buffer.from(expected);
  const receivedBytes = Buffer.from(received);
  return (
    expectedBytes.length === receivedBytes.length &&
    timingSafeEqual(expectedBytes, receivedBytes)
  );
}

/** A usable skew window: a positive, finite number, else the default. */
export function resolveHmacMaxSkewMs(maxSkewMs?: number): number {
  return typeof maxSkewMs === 'number' &&
    Number.isFinite(maxSkewMs) &&
    maxSkewMs > 0
    ? maxSkewMs
    : DEFAULT_HMAC_MAX_SKEW_MS;
}

/**
 * Whether a request timestamp is valid and within `maxSkewMs` of `nowMs`,
 * in the past or the future. A missing or unparseable timestamp is stale.
 */
export function isHmacTimestampFresh(
  timestamp: Date | undefined | null,
  nowMs: number,
  maxSkewMs: number = DEFAULT_HMAC_MAX_SKEW_MS
): boolean {
  if (!(timestamp instanceof Date)) return false;
  const at = timestamp.getTime();
  return !Number.isNaN(at) && Math.abs(nowMs - at) <= maxSkewMs;
}

/**
 * How long a nonce must be remembered: until the last moment its signature
 * could still pass the freshness check. A timestamp from the future (within
 * the skew) stays fresh for up to twice the window, so the lifetime is
 * measured from the signed timestamp, not from when the request arrived.
 */
export function hmacNonceTtlMs(
  timestamp: Date,
  nowMs: number,
  maxSkewMs: number
): number {
  return Math.max(1, timestamp.getTime() + maxSkewMs - nowMs);
}

/**
 * The store key for a nonce. The key id is length-prefixed so that no two
 * (key id, nonce) pairs can collide.
 */
export function hmacNonceKey(keyId: string | undefined, nonce: string): string {
  const id = keyId ?? '';
  return `${id.length}:${id}${nonce}`;
}

/**
 * Where used nonces are remembered.
 *
 * `claim` records the nonce and answers whether this caller is the first to
 * use it: `true` the first time, `false` for every later call until `ttlMs`
 * has passed. It must be atomic (for Redis: `SET key 1 NX PX ttlMs`, which
 * answers `OK` only for the first writer). If it throws, the request is
 * rejected: storage failure fails closed.
 */
export interface HmacNonceStore {
  claim(key: string, ttlMs: number): boolean | Promise<boolean>;
}

/**
 * The default nonce store: a bounded map in this process's memory.
 *
 * IMPORTANT: this protects a SINGLE PROCESS only. Each copy of the app has
 * its own map, so when an app runs several copies (replicas, cluster
 * workers, serverless instances) a captured request can be replayed once
 * against every other copy inside the freshness window, and all memory is
 * lost on restart. An app that runs more than one copy needs a shared store
 * (pass `hmac.nonceStore`: a `TtlCache` with `putRecordIfAbsent` such as the
 * Redis cache, or an `HmacNonceStore` over Redis `SET NX PX`) for full
 * protection.
 *
 * Bounded: entries expire after their ttl, and when `maxEntries` unexpired
 * nonces are held the oldest is dropped to make room. Only requests that
 * carried a valid signature are ever stored, so only a holder of the key can
 * fill it; past the bound the dropped nonces lose replay protection, which
 * is one more reason high-volume apps should use a shared store.
 */
export class InMemoryHmacNonceStore implements HmacNonceStore {
  private readonly entries = new Map<string, number>();
  private readonly maxEntries: number;
  private readonly sweepIntervalMs: number;
  private lastSweepAt = 0;

  constructor(options: { maxEntries?: number; sweepIntervalMs?: number } = {}) {
    this.maxEntries =
      typeof options.maxEntries === 'number' && options.maxEntries >= 1
        ? Math.floor(options.maxEntries)
        : DEFAULT_HMAC_NONCE_STORE_MAX_ENTRIES;
    this.sweepIntervalMs = options.sweepIntervalMs ?? 30_000;
  }

  /** Number of nonces currently held (expired ones may linger until swept). */
  get size(): number {
    return this.entries.size;
  }

  claim(key: string, ttlMs: number, nowMs: number = Date.now()): boolean {
    const expiresAt = this.entries.get(key);
    if (expiresAt !== undefined && expiresAt > nowMs) {
      return false;
    }
    if (
      this.entries.size >= this.maxEntries ||
      nowMs - this.lastSweepAt >= this.sweepIntervalMs
    ) {
      this.sweep(nowMs);
    }
    // Re-insert so the map's insertion order stays oldest-first.
    this.entries.delete(key);
    while (this.entries.size >= this.maxEntries) {
      const oldest = this.entries.keys().next();
      if (oldest.done) break;
      this.entries.delete(oldest.value);
    }
    this.entries.set(key, nowMs + Math.max(1, ttlMs));
    return true;
  }

  private sweep(nowMs: number): void {
    this.lastSweepAt = nowMs;
    for (const [key, expiresAt] of this.entries) {
      if (expiresAt <= nowMs) this.entries.delete(key);
    }
  }
}

/** The parts of a `TtlCache` a nonce store uses. */
export type HmacNonceCache = Pick<
  TtlCache,
  'peekRecord' | 'putRecord' | 'putRecordIfAbsent'
>;

/**
 * Use one of the framework's `TtlCache` implementations (for example the
 * Redis cache) as a nonce store shared by every copy of the app.
 *
 * When the cache implements `putRecordIfAbsent` (the Redis cache does, with
 * `SET NX PX`) the claim is one atomic step and replay protection is
 * complete: of any number of simultaneous copies of a request, exactly one
 * is accepted.
 *
 * FALLBACK, for caches without `putRecordIfAbsent` only: a check followed by
 * a write. That is NOT atomic. Two copies of the app handling the same
 * replayed request at the very same instant can both pass. It still rejects
 * every replay that arrives after the first request was recorded. Implement
 * `putRecordIfAbsent` on the cache, or `HmacNonceStore` directly over an
 * atomic primitive, to close that gap.
 */
export function ttlCacheHmacNonceStore(
  cache: HmacNonceCache,
  keyPrefix = 'hmac-nonce:'
): HmacNonceStore {
  return {
    async claim(key, ttlMs) {
      const record = {
        key: `${keyPrefix}${key}`,
        value: 1,
        ttlMilliseconds: Math.max(1, Math.ceil(ttlMs))
      };
      if (typeof cache.putRecordIfAbsent === 'function') {
        return (await cache.putRecordIfAbsent(record)) === true;
      }
      // Non-atomic fallback; see above.
      if (await cache.peekRecord(record.key)) {
        return false;
      }
      await cache.putRecord(record);
      return true;
    }
  };
}

const defaultHmacNonceStore = new InMemoryHmacNonceStore();
const adaptedTtlCaches = new WeakMap<object, HmacNonceStore>();

/**
 * The store to claim nonces in: the configured `HmacNonceStore`, a
 * `TtlCache` wrapped as one, or the process-wide in-memory default.
 */
export function resolveHmacNonceStore(
  configured?: HmacNonceStore | HmacNonceCache
): HmacNonceStore {
  if (configured == null) {
    return defaultHmacNonceStore;
  }
  if (typeof (configured as HmacNonceStore).claim === 'function') {
    return configured as HmacNonceStore;
  }
  const cache = configured as HmacNonceCache;
  if (
    typeof cache.putRecordIfAbsent !== 'function' &&
    (typeof cache.peekRecord !== 'function' ||
      typeof cache.putRecord !== 'function')
  ) {
    throw new Error(
      'hmac.nonceStore must be an HmacNonceStore ({ claim }) or a TtlCache'
    );
  }
  let adapted = adaptedTtlCaches.get(cache);
  if (!adapted) {
    adapted = ttlCacheHmacNonceStore(cache);
    adaptedTtlCaches.set(cache, adapted);
  }
  return adapted;
}

import { TtlCache } from '../../cache/interfaces/ttlCache.interface';

/**
 * Configuration for rate limiting with separate read/write limits.
 */
export type RateLimitConfig = {
  /** Max requests per window for GET/HEAD/OPTIONS */
  read: number;
  /** Max requests per window for POST/PUT/PATCH/DELETE */
  write: number;
  /** Window duration in milliseconds */
  windowMs: number;
};

/**
 * Result of a rate limit check.
 */
export type RateLimitResult = {
  /** Whether the request is allowed */
  allowed: boolean;
  /** Number of remaining requests in the current window */
  remaining: number;
  /** Unix timestamp (ms) when the current window resets */
  resetAt: number;
};

type RateLimitCounter = {
  count: number;
  resetAt: number;
};

const READ_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);

/**
 * Rate limiter backed by a TtlCache.
 *
 * Uses a sliding-window counter pattern: each unique key maps to a
 * {@link RateLimitCounter} stored in the cache with a TTL equal to the
 * configured window duration.
 */
export class RateLimiter {
  constructor(private cache: TtlCache) {}

  /**
   * Check whether the given key is within its rate limit.
   *
   * The counter is read from the cache, incremented, and written back.
   * If the cache is unreachable the limiter **fails open** (allows the
   * request) so that a cache outage does not take down the service.
   *
   * @param key - The rate limit key (see {@link RateLimiter.buildKey}).
   * @param limit - Maximum number of requests allowed in the window.
   * @param windowMs - Window duration in milliseconds.
   */
  async check(
    key: string,
    limit: number,
    windowMs: number
  ): Promise<RateLimitResult> {
    try {
      const now = Date.now();
      let counter: RateLimitCounter;

      try {
        const record = await this.cache.readRecord<RateLimitCounter>(key);
        counter = record.value;

        // If the stored window has expired, start a new one
        if (now >= counter.resetAt) {
          counter = { count: 0, resetAt: now + windowMs };
        }
      } catch {
        // Key does not exist yet – start a fresh window
        counter = { count: 0, resetAt: now + windowMs };
      }

      counter.count += 1;

      const ttl = Math.max(counter.resetAt - now, 1);

      await this.cache.putRecord<RateLimitCounter>({
        key,
        value: counter,
        ttlMilliseconds: ttl
      });

      const allowed = counter.count <= limit;
      const remaining = Math.max(limit - counter.count, 0);

      return { allowed, remaining, resetAt: counter.resetAt };
    } catch {
      // Fail open – cache errors should not block requests
      console.warn(`[RateLimiter] Cache error for key "${key}". Failing open.`);
      return { allowed: true, remaining: limit, resetAt: 0 };
    }
  }

  /**
   * Build a rate limit key from request context parts.
   *
   * Format: `ratelimit:{tenantId}:{route}:{userId}:{operationType}`
   *
   * When `tenantId` or `userId` is `null`, the placeholder `"anon"` is used.
   */
  static buildKey(parts: {
    tenantId: string | null;
    route: string;
    userId: string | null;
    operationType: 'read' | 'write';
  }): string {
    const tenant = parts.tenantId ?? 'anon';
    const user = parts.userId ?? 'anon';
    return `ratelimit:${tenant}:${parts.route}:${user}:${parts.operationType}`;
  }

  /**
   * Determine whether an HTTP method is a read or write operation.
   *
   * GET, HEAD, and OPTIONS are reads; everything else is a write.
   */
  static operationType(method: string): 'read' | 'write' {
    return READ_METHODS.has(method.toUpperCase()) ? 'read' : 'write';
  }
}

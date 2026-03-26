import { describe, expect, it, vi } from 'vitest';
import { TtlCache } from '../src/cache/interfaces/ttlCache.interface';
import { TtlCacheRecord } from '../src/cache/types/ttlCacheRecord.types';
import { RateLimiter } from '../src/http/rateLimit/rateLimiter';

/**
 * In-memory TtlCache stub that only implements the methods the
 * RateLimiter relies on: `readRecord` and `putRecord`.
 */
function createMockCache(): TtlCache {
  const store = new Map<string, { value: unknown; expiresAt: number }>();

  function readRecord<T>(key: string): Promise<TtlCacheRecord<T>> {
    const entry = store.get(key);
    if (!entry || Date.now() >= entry.expiresAt) {
      return Promise.reject(new Error('key not found'));
    }
    return Promise.resolve({
      key,
      value: entry.value as T,
      ttlMilliseconds: Math.max(entry.expiresAt - Date.now(), 0)
    });
  }

  function putRecord<T>(record: TtlCacheRecord<T>): Promise<void> {
    store.set(record.key, {
      value: record.value,
      expiresAt: Date.now() + record.ttlMilliseconds
    });
    return Promise.resolve();
  }

  return {
    readRecord,
    putRecord,
    putBatchRecords: vi.fn(),
    enqueueRecord: vi.fn(),
    enqueueBatchRecords: vi.fn(),
    deleteRecord: vi.fn(),
    deleteBatchRecords: vi.fn(),
    dequeueRecord: vi.fn(),
    dequeueBatchRecords: vi.fn(),
    readBatchRecords: vi.fn(),
    peekRecord: vi.fn(),
    peekBatchRecords: vi.fn(),
    peekQueueRecord: vi.fn(),
    peekQueueRecords: vi.fn(),
    getTtlMilliseconds: vi.fn(),
    listKeys: vi.fn()
  };
}

/**
 * Create a TtlCache whose readRecord and putRecord always reject,
 * simulating a cache outage (e.g. Redis down).
 */
function createFailingCache(): TtlCache {
  return {
    readRecord: () => Promise.reject(new Error('connection refused')),
    putRecord: () => Promise.reject(new Error('connection refused')),
    putBatchRecords: vi.fn(),
    enqueueRecord: vi.fn(),
    enqueueBatchRecords: vi.fn(),
    deleteRecord: vi.fn(),
    deleteBatchRecords: vi.fn(),
    dequeueRecord: vi.fn(),
    dequeueBatchRecords: vi.fn(),
    readBatchRecords: vi.fn(),
    peekRecord: vi.fn(),
    peekBatchRecords: vi.fn(),
    peekQueueRecord: vi.fn(),
    peekQueueRecords: vi.fn(),
    getTtlMilliseconds: vi.fn(),
    listKeys: vi.fn()
  };
}

describe('RateLimiter', () => {
  describe('check', () => {
    it('allows a request under the limit and returns correct remaining count', async () => {
      const cache = createMockCache();
      const limiter = new RateLimiter(cache);
      const limit = 5;
      const windowMs = 60_000;

      const result = await limiter.check('key:1', limit, windowMs);

      expect(result.allowed).toBe(true);
      expect(result.remaining).toBe(4);
      expect(result.resetAt).toBeGreaterThan(Date.now() - 1000);
    });

    it('decrements remaining on successive requests', async () => {
      const cache = createMockCache();
      const limiter = new RateLimiter(cache);
      const limit = 3;
      const windowMs = 60_000;

      const r1 = await limiter.check('key:2', limit, windowMs);
      const r2 = await limiter.check('key:2', limit, windowMs);
      const r3 = await limiter.check('key:2', limit, windowMs);

      expect(r1.remaining).toBe(2);
      expect(r2.remaining).toBe(1);
      expect(r3.remaining).toBe(0);
      expect(r3.allowed).toBe(true);
    });

    it('returns allowed: false when the limit is exceeded', async () => {
      const cache = createMockCache();
      const limiter = new RateLimiter(cache);
      const limit = 2;
      const windowMs = 60_000;

      await limiter.check('key:3', limit, windowMs);
      await limiter.check('key:3', limit, windowMs);
      const result = await limiter.check('key:3', limit, windowMs);

      expect(result.allowed).toBe(false);
      expect(result.remaining).toBe(0);
    });

    it('tracks different keys independently', async () => {
      const cache = createMockCache();
      const limiter = new RateLimiter(cache);
      const limit = 1;
      const windowMs = 60_000;

      const rA = await limiter.check('key:a', limit, windowMs);
      const rB = await limiter.check('key:b', limit, windowMs);

      expect(rA.allowed).toBe(true);
      expect(rB.allowed).toBe(true);

      const rA2 = await limiter.check('key:a', limit, windowMs);
      expect(rA2.allowed).toBe(false);

      // key:b still has its own budget
      const rB2 = await limiter.check('key:b', limit, windowMs);
      expect(rB2.allowed).toBe(false);
    });

    it('fails open when the cache is unavailable', async () => {
      const cache = createFailingCache();
      const limiter = new RateLimiter(cache);
      const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const result = await limiter.check('key:fail', 10, 60_000);

      expect(result.allowed).toBe(true);
      expect(result.remaining).toBe(10);
      expect(result.resetAt).toBe(0);
      expect(warnSpy).toHaveBeenCalled();

      warnSpy.mockRestore();
    });
  });

  describe('buildKey', () => {
    it('produces the correct format with all parts', () => {
      const key = RateLimiter.buildKey({
        tenantId: 'tenant-1',
        route: '/api/v1/users',
        userId: 'user-42',
        operationType: 'read'
      });
      expect(key).toBe('ratelimit:tenant-1:/api/v1/users:user-42:read');
    });

    it('uses "anon" when tenantId is null', () => {
      const key = RateLimiter.buildKey({
        tenantId: null,
        route: '/health',
        userId: 'user-1',
        operationType: 'read'
      });
      expect(key).toBe('ratelimit:anon:/health:user-1:read');
    });

    it('uses "anon" when userId is null', () => {
      const key = RateLimiter.buildKey({
        tenantId: 'tenant-1',
        route: '/public',
        userId: null,
        operationType: 'write'
      });
      expect(key).toBe('ratelimit:tenant-1:/public:anon:write');
    });

    it('uses "anon" for both tenantId and userId when null', () => {
      const key = RateLimiter.buildKey({
        tenantId: null,
        route: '/open',
        userId: null,
        operationType: 'read'
      });
      expect(key).toBe('ratelimit:anon:/open:anon:read');
    });
  });

  describe('operationType', () => {
    it('classifies GET as read', () => {
      expect(RateLimiter.operationType('GET')).toBe('read');
    });

    it('classifies HEAD as read', () => {
      expect(RateLimiter.operationType('HEAD')).toBe('read');
    });

    it('classifies OPTIONS as read', () => {
      expect(RateLimiter.operationType('OPTIONS')).toBe('read');
    });

    it('classifies POST as write', () => {
      expect(RateLimiter.operationType('POST')).toBe('write');
    });

    it('classifies PUT as write', () => {
      expect(RateLimiter.operationType('PUT')).toBe('write');
    });

    it('classifies PATCH as write', () => {
      expect(RateLimiter.operationType('PATCH')).toBe('write');
    });

    it('classifies DELETE as write', () => {
      expect(RateLimiter.operationType('DELETE')).toBe('write');
    });

    it('handles lowercase method names', () => {
      expect(RateLimiter.operationType('get')).toBe('read');
      expect(RateLimiter.operationType('post')).toBe('write');
    });
  });
});

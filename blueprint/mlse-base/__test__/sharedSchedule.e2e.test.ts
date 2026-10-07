import { RedisTtlCache } from '@forklaunch/infrastructure-redis';
import { GenericContainer, StartedTestContainer, Wait } from 'testcontainers';
import {
  ncbiScheduleKey,
  RedisEval,
  RedisRequestSchedule
} from '../domain/sharedSchedule';

// The schedule is shared through Redis so the server and the worker count
// against one NCBI limit. Checked against a real Redis: the booking is a Lua
// script, which nothing but Redis can run.
describe('RedisRequestSchedule', () => {
  let container: StartedTestContainer;
  let cache: RedisTtlCache;

  beforeAll(async () => {
    container = await new GenericContainer('redis:7-alpine')
      .withExposedPorts(6379)
      .withWaitStrategy(Wait.forLogMessage('Ready to accept connections'))
      .start();
    cache = new RedisTtlCache(
      60_000,
      { info: () => undefined, warn: () => undefined, error: () => undefined, debug: () => undefined } as never,
      { url: `redis://${container.getHost()}:${container.getMappedPort(6379)}` },
      // as registrations.ts builds it: the cache connects when enabled
      { enabled: true, level: 'error' },
      {}
    );
  }, 120_000);

  afterAll(async () => {
    await cache?.disconnect();
    await container?.stop();
  });

  it('books slots for two clients, as if two processes, on one schedule', async () => {
    const key = `mlse:test:${Date.now()}`;
    const server = new RedisRequestSchedule(cache.getClient() as unknown as RedisEval, key);
    const worker = new RedisRequestSchedule(cache.getClient() as unknown as RedisEval, key);

    const waits = [
      await server.reserve(300, 2_000),
      await worker.reserve(300, 2_000),
      await server.reserve(300, 2_000)
    ];

    // each booking lands after the one before, whichever client made it
    expect(waits[0]).toBe(0);
    expect(waits[1]).toBeGreaterThan(200);
    expect(waits[2]).toBeGreaterThan(waits[1] as number);
  });

  it('refuses, booking nothing, when the next slot is further away than the caller will wait', async () => {
    const key = `mlse:test:full:${Date.now()}`;
    const schedule = new RedisRequestSchedule(cache.getClient() as unknown as RedisEval, key);

    expect(await schedule.reserve(1_000, 1_500)).toBe(0);
    expect(await schedule.reserve(1_000, 1_500)).toBeGreaterThan(0);
    // the next slot is ~2 s away
    expect(await schedule.reserve(1_000, 1_500)).toBeUndefined();
    expect(await schedule.reserve(1_000, 1_500)).toBeUndefined();
  });

  it("falls back to this process's own schedule when Redis fails", async () => {
    const errors: Error[] = [];
    const broken: RedisEval = {
      eval: async () => {
        throw new Error('connection refused');
      }
    };
    const schedule = new RedisRequestSchedule(broken, 'unused', (error) => errors.push(error));

    expect(await schedule.reserve(100, 1_000)).toBe(0);
    expect(await schedule.reserve(100, 1_000)).toBeGreaterThan(0);
    expect(errors.map((e) => e.message)).toEqual(['connection refused', 'connection refused']);
  });

  it('never puts the API key in the Redis key', () => {
    expect(ncbiScheduleKey('secret-key')).not.toContain('secret');
    expect(ncbiScheduleKey('secret-key')).not.toBe(ncbiScheduleKey('other-key'));
    expect(ncbiScheduleKey(undefined)).toBe(ncbiScheduleKey(''));
  });
});

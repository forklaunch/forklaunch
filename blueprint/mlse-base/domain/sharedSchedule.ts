import {
  InProcessSchedule,
  type RequestSchedule
} from '@forklaunch/implementation-mlse-base/services';
import { createHash } from 'node:crypto';

/** The one Redis call the schedule needs (node-redis's `eval`). */
export type RedisEval = {
  eval(
    script: string,
    options: { keys: string[]; arguments: string[] }
  ): Promise<unknown>;
};

// Books the next slot atomically, on Redis's own clock so every process
// agrees on "now". Returns the wait in ms, or -1 when the slot is further
// away than the caller will wait (nothing is booked then).
const RESERVE = `
local t = redis.call('TIME')
local now = tonumber(t[1]) * 1000 + math.floor(tonumber(t[2]) / 1000)
local interval = tonumber(ARGV[1])
local maxWait = tonumber(ARGV[2])
local nextSlot = tonumber(redis.call('GET', KEYS[1]) or '0')
local wait = math.max(0, nextSlot - now)
if wait > maxWait then return -1 end
local slot = math.max(now, nextSlot) + interval
redis.call('SET', KEYS[1], tostring(slot), 'PX', wait + interval + 1000)
return wait
`;

/**
 * A request schedule shared by every MLSE process using the same Redis: the
 * server's live search and suggestions and the worker's ingestion all count
 * against one NCBI limit, which is per API key, not per process.
 *
 * If Redis fails, it falls back to this process's own schedule rather than
 * failing the request: the limit is a courtesy to the source, and a Redis
 * outage should not take live search down with it.
 */
export class RedisRequestSchedule implements RequestSchedule {
  private readonly fallback = new InProcessSchedule();

  constructor(
    private readonly redis: RedisEval,
    private readonly key: string,
    private readonly onError: (error: Error) => void = () => undefined
  ) {}

  async reserve(
    intervalMs: number,
    maxWaitMs: number
  ): Promise<number | undefined> {
    try {
      const wait = Number(
        await this.redis.eval(RESERVE, {
          keys: [this.key],
          arguments: [String(intervalMs), String(maxWaitMs)]
        })
      );
      return wait < 0 ? undefined : wait;
    } catch (error) {
      this.onError(error as Error);
      return this.fallback.reserve(intervalMs, maxWaitMs);
    }
  }
}

/** The Redis key for NCBI's limit: one per API key, never the key itself. */
export function ncbiScheduleKey(apiKey: string | undefined): string {
  const id = createHash('sha256')
    .update(apiKey || 'no-api-key')
    .digest('hex')
    .slice(0, 16);
  return `mlse:rate-limit:ncbi:${id}`;
}

import { QueueOptions } from 'bullmq';

/**
 * Where a queue's keys live, and what to do when the server says they may not.
 *
 * On a shared Redis pool every tenant is a set of ACL users whose key pattern
 * is its own prefix — `~<appPrefix>:*`. The platform hands that prefix to the
 * process as `REDIS_KEY_PREFIX` (`FORKLAUNCH_REDIS_KEY_PREFIX` is the same
 * value under the platform's own name). BullMQ does not know about any of it:
 * left alone it writes `bull:<queue>:*`, which is outside the pattern, and
 * every command comes back NOPERM.
 *
 * That is not a theoretical failure. It ran in production for weeks: two
 * workers retried the denied connection with no backoff, logged a stack trace
 * each time at roughly 8,900 a second, and shipped 4.7 TB to CloudWatch in a
 * month — most of a $2,799 bill — while their queues were never served once
 * and the tasks reported healthy the whole time.
 *
 * Two things follow, and they are deliberately separate:
 *
 *   - The prefix is CONFORMANCE. It puts a well-behaved app inside a boundary
 *     it does not enforce. It is forced rather than defaulted, so an app
 *     cannot misconfigure itself out of its own namespace by accident.
 *   - The ACL is ENFORCEMENT, and it lives on the server where the app cannot
 *     reach it. An app that forges a prefix is denied; it breaks itself and
 *     touches no other tenant. Never treat the prefix as a security control.
 */

/** Env var the platform injects for a partitioned (shared-pool) application. */
const PREFIX_VARS = [
  'REDIS_KEY_PREFIX',
  'FORKLAUNCH_REDIS_KEY_PREFIX'
] as const;

/**
 * The tenant key prefix for this process, or undefined on a dedicated cache
 * where no partition exists and BullMQ's own default is correct.
 */
export function tenantKeyPrefix(
  env: Record<string, string | undefined> = process.env
): string | undefined {
  for (const name of PREFIX_VARS) {
    const value = env[name]?.trim();
    if (value) return value;
  }
  return undefined;
}

/**
 * The BullMQ `prefix` for a tenant namespace.
 *
 * BullMQ composes keys as `<prefix>:<queue>:<...>`, so the trailing separator
 * the platform's prefix carries (`fl_production_app_ab12:`) has to come off —
 * with it the keys would be `fl_production_app_ab12::<queue>`, which is inside
 * the ACL pattern but not where anything else looks.
 */
export function bullPrefixFor(
  env: Record<string, string | undefined> = process.env
): string | undefined {
  const prefix = tenantKeyPrefix(env);
  return prefix ? prefix.replace(/:+$/, '') : undefined;
}

/**
 * Force the tenant prefix onto queue options.
 *
 * Forced, not defaulted: a caller-supplied prefix is overridden, because a
 * prefix outside the ACL pattern cannot work and failing at construction with
 * a clear reason beats failing on every command forever. Returns the options
 * unchanged when no partition is present.
 */
export function withTenantPrefix<T extends QueueOptions>(
  options: T,
  env: Record<string, string | undefined> = process.env
): T {
  const prefix = bullPrefixFor(env);
  if (!prefix) return options;
  return { ...options, prefix };
}

/** True when the caller asked for a prefix the tenant namespace will override. */
export function conflictsWithTenantPrefix(
  options: QueueOptions,
  env: Record<string, string | undefined> = process.env
): boolean {
  const prefix = bullPrefixFor(env);
  return Boolean(prefix && options.prefix && options.prefix !== prefix);
}

/**
 * Does this error mean the server refused the key or the command outright?
 *
 * NOPERM is a configuration answer, not a transient one: the same command will
 * be refused every time until an ACL changes. Retrying it is the loop that
 * cost the bill.
 */
export function isTenantIsolationError(error: unknown): boolean {
  const message =
    error instanceof Error
      ? error.message
      : typeof error === 'string'
        ? error
        : '';
  return /\bNOPERM\b/i.test(message);
}

/**
 * Report a tenant-isolation failure at most once per process.
 *
 * The point is not to hide it — it is that the hundred-thousandth copy of a
 * stack trace tells nobody anything the first did not, and costs $0.50 a
 * gigabyte to store.
 */
export function makeIsolationReporter(
  log: (message: string) => void = (m) => console.error(m)
): (queueName: string, error: unknown) => void {
  let reported = false;
  return (queueName: string, error: unknown) => {
    if (reported) return;
    reported = true;
    const detail = error instanceof Error ? error.message : String(error);
    log(
      `[bullmq] queue "${queueName}" is denied by the Redis ACL and cannot run: ${detail}. ` +
        `This is a configuration failure, not a transient one, so consuming has stopped rather ` +
        `than retrying. Expected key prefix: ${bullPrefixFor() ?? '(none — dedicated cache)'}. ` +
        `On a shared pool the user's pattern is ~<appPrefix>:* — check REDIS_KEY_PREFIX against it.`
    );
  };
}

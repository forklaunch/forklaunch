import type { Metrics } from '@forklaunch/blueprint-monitoring';
import type { OpenTelemetryCollector } from '@forklaunch/core/http';

/**
 * Turns a Better Auth API error into a log line that is actually diagnosable,
 * without putting user data in CloudWatch.
 *
 * The distinction matters more here than in most services. A Postgres error
 * carries two kinds of field: structural ones that say what rule was broken
 * (`code`, `constraint`, `table`, `column`) and narrative ones that quote the
 * offending row back at you (`detail`, `where`, `query`). For a NOT NULL
 * violation on sign-up, `detail` reads
 *
 *   Key (email)=(someone@example.com) already exists.
 *
 * That is a user's email address, and this is the IAM service, so it does not
 * go to logs. The structural fields alone identify the failing constraint
 * exactly, which is what an on-call engineer actually needs.
 */

/** Postgres error fields worth keeping. Deliberately excludes `detail`. */
const DB_ERROR_FIELDS = [
  'code',
  'constraint',
  'constraint_name',
  'table',
  'table_name',
  'column',
  'column_name',
  'severity',
  'routine'
] as const;

const MAX_STACK_CHARS = 2000;

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

/**
 * Walks the `cause` chain, because the interesting error is usually wrapped:
 * MikroORM surfaces a driver error as the cause of its own, and Better Auth
 * may wrap that again. Bounded so a cyclic chain cannot spin.
 */
const collectDatabaseFields = (error: unknown): Record<string, unknown> => {
  const found: Record<string, unknown> = {};
  const seen = new Set<unknown>();
  let current: unknown = error;

  for (let depth = 0; depth < 5 && isRecord(current); depth++) {
    if (seen.has(current)) break;
    seen.add(current);

    for (const field of DB_ERROR_FIELDS) {
      const value = current[field];
      if (
        found[field] === undefined &&
        (typeof value === 'string' || typeof value === 'number')
      ) {
        found[field] = value;
      }
    }
    current = current.cause;
  }

  return found;
};

/**
 * Never throws. It runs inside Better Auth's error path, where a second
 * failure would replace a diagnosable 500 with an undiagnosable one.
 */
export const logBetterAuthApiError = (
  openTelemetryCollector: OpenTelemetryCollector<Metrics>,
  error: unknown,
  ctx?: unknown
): void => {
  try {
    const err = error instanceof Error ? error : undefined;
    const dbFields = collectDatabaseFields(error);

    // The endpoint being hit, when Better Auth exposes it. Optional-chained
    // rather than typed against AuthContext: the shape differs between
    // versions and a logger must not be the thing that breaks on upgrade.
    const path =
      isRecord(ctx) && isRecord(ctx.context)
        ? ctx.context.path
        : isRecord(ctx)
          ? ctx.path
          : undefined;

    openTelemetryCollector.error('[IAM] Better Auth API error', {
      name: err?.name ?? typeof error,
      message: err?.message ?? String(error),
      // Truncated: a full ORM stack can run to tens of kilobytes, and the
      // frames that matter are at the top.
      stack: err?.stack?.slice(0, MAX_STACK_CHARS),
      ...(typeof path === 'string' ? { path } : {}),
      ...dbFields
    });
  } catch (loggingError) {
    // Last resort — if structured logging itself fails, still leave a mark.
    console.error('[IAM] Failed to log Better Auth API error', loggingError);
  }
};

import type { OpenTelemetryCollector } from '../http/telemetry/openTelemetryCollector';
import type { RetentionService } from './retentionService';

/**
 * Shared retention enforcement runner.
 * Designed to be called from a one-shot script (ECS RunTask / cron).
 *
 * @example
 * ```ts
 * import { ci, tokens } from '../bootstrapper';
 * import { runRetentionEnforcement } from '@myapp/core';
 *
 * runRetentionEnforcement(
 *   ci.resolve(tokens.RetentionService),
 *   ci.resolve(tokens.OpenTelemetryCollector)
 * );
 * ```
 */
export async function runRetentionEnforcement(
  retentionService: RetentionService,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  otel: OpenTelemetryCollector<any>
): Promise<void> {
  const dryRun = process.argv.includes('--dry-run');

  otel.info('[RetentionEnforcement] Starting', { dryRun });

  const result = await retentionService.enforce({ dryRun });

  otel.info('[RetentionEnforcement] Complete', {
    processed: result.processed,
    deleted: result.deleted,
    anonymized: result.anonymized,
    errors: result.errors,
    durationMs: result.durationMs
  });

  if (result.errors > 0) {
    otel.warn('[RetentionEnforcement] Completed with errors', {
      errors: result.errors,
      byEntity: result.byEntity
    });
    process.exit(1);
  }

  process.exit(0);
}

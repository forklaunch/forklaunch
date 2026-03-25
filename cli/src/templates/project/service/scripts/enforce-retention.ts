/**
 * Retention Enforcement Script
 *
 * One-shot script that enforces data retention policies declared on entities.
 * Designed to be run as an ECS RunTask triggered by EventBridge on a schedule.
 *
 * Usage:
 *   pnpm retention:enforce              # enforce all
 *   pnpm retention:enforce -- --dry-run # preview without mutating
 */

import { ci, tokens } from '../bootstrapper';

const retentionService = ci.resolve(tokens.RetentionService);
const otel = ci.resolve(tokens.OtelCollector);

const dryRun = process.argv.includes('--dry-run');

async function main() {
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

main().catch((err) => {
  otel.error('[RetentionEnforcement] Fatal error', { error: String(err) });
  process.exit(1);
});

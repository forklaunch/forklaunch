import { ci, tokens } from '../bootstrapper';

const otel = ci.resolve(tokens.OpenTelemetryCollector);
const retentionService = ci.resolve(tokens.RetentionService);

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

import { ci, tokens } from '../bootstrapper';
import { anonymizeExpiredHistory } from '../domain/historyRetention';
import { organizationsWithUserData } from '../domain/tenants';

// Run daily. Search history loses its query text after 90 days.
async function main() {
  const otel = ci.resolve(tokens.OtelCollector);
  const orm = ci.resolve(tokens.Orm);
  const dryRun = process.argv.includes('--dry-run');

  const anonymizedHistory = dryRun ? 0 : await anonymizeExpiredHistory(orm.em.fork());

  // Framework retention for any other policies. It runs once per
  // organization because encrypted rows must be read with their own key.
  const tenantIds = await organizationsWithUserData(orm.em.fork());
  const result = await ci.resolve(tokens.RetentionService).enforce({ dryRun, tenantIds });

  otel.info('[RetentionEnforcement] Complete', {
    dryRun,
    anonymizedHistory,
    processed: result.processed,
    errors: result.errors,
    durationMs: result.durationMs
  });
  await orm.close();
  process.exit(result.errors > 0 ? 1 : 0);
}

main().catch((err) => {
  console.error('[RetentionEnforcement] Fatal error', err);
  process.exit(1);
});

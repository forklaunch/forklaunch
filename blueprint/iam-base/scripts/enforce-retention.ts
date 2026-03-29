import { runRetentionEnforcement } from '@forklaunch/core/services';
import { ci, tokens } from '../bootstrapper';

runRetentionEnforcement(
  ci.resolve(tokens.RetentionService),
  ci.resolve(tokens.OpenTelemetryCollector)
).catch((err) => {
  console.error('[RetentionEnforcement] Fatal error', err);
  process.exit(1);
});

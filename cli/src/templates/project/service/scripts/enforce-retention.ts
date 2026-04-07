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

import { runRetentionEnforcement } from '@forklaunch/core/services';
import { ci, tokens } from '../bootstrapper';

runRetentionEnforcement(
  ci.resolve(tokens.RetentionService),
  ci.resolve(tokens.OtelCollector)
).catch((err) => {
  console.error('[RetentionEnforcement] Fatal error', err);
  process.exit(1);
});

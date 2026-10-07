import { withEncryptionContext } from '../persistence/encryptedType';
import type { MetricsDefinition } from '../http/types/openTelemetryCollector.types';
import type { OpenTelemetryCollector } from '../http/telemetry/openTelemetryCollector';
import type { ComplianceOrm } from './complianceDataService';
import {
  getAllRetentionPolicies,
  getEntityComplianceFields,
  parseDuration,
  subtractDuration,
  type RetentionPolicy
} from '../persistence/complianceTypes';

export interface EnforcementOptions {
  /** Filter to specific entity names (default: all with retention policies) */
  entities?: string[];
  /** Records per batch (default: 1000) */
  batchSize?: number;
  /** Log what would happen without mutating */
  dryRun?: boolean;
  /**
   * Tenants to enforce under, one pass each.
   *
   * Retention reads and rewrites PII columns, and every one of those is
   * encrypted under its row's tenant. An unbound pass anonymises with a key
   * nothing was written with: the `$lt` cutoff on `createdAt` still matches
   * (that column is plaintext), but hydrating the row's encrypted fields
   * fails, the entity is counted as an error and skipped — so retention
   * quietly stops enforcing on exactly the data it exists to age out.
   *
   * Omit only for a schema whose retained entities carry no encrypted column.
   */
  tenantIds?: readonly string[];
}

export interface EnforcementResult {
  processed: number;
  deleted: number;
  anonymized: number;
  errors: number;
  byEntity: Record<
    string,
    { deleted: number; anonymized: number; errors: number }
  >;
  durationMs: number;
}

// Parameterised on the metrics definition rather than typed against
// `OpenTelemetryCollector<any>`. This service only logs — info, warn, error —
// and never touches a metric, so it has no reason to know the definition; but
// `any` there switched off checking on every call through this collector, and
// the collector's generic appears in its own method parameters, so widening to
// the base constraint would not have been sound either. The default keeps
// existing construction sites inferring exactly as before.
export class RetentionService<
  AppliedMetricsDefinition extends MetricsDefinition = MetricsDefinition
> {
  private readonly DEFAULT_BATCH_SIZE = 1000;

  constructor(
    private readonly orm: ComplianceOrm,
    private readonly otel: OpenTelemetryCollector<AppliedMetricsDefinition>
  ) {}

  async enforce(options?: EnforcementOptions): Promise<EnforcementResult> {
    const tenantIds = options?.tenantIds ?? [];
    if (tenantIds.length > 1) {
      // One pass per tenant, merged. Written this way rather than threading a
      // tenant through every helper: the encryption context is ambient, so
      // wrapping the whole pass is both simpler and harder to get wrong.
      const merged: EnforcementResult = {
        processed: 0,
        deleted: 0,
        anonymized: 0,
        errors: 0,
        byEntity: {},
        durationMs: 0
      };
      const start = Date.now();
      for (const tenantId of tenantIds) {
        const one = await withEncryptionContext(tenantId, () =>
          this.enforce({ ...options, tenantIds: [tenantId] })
        );
        merged.processed += one.processed;
        merged.deleted += one.deleted;
        merged.anonymized += one.anonymized;
        merged.errors += one.errors;
        for (const [entityName, counts] of Object.entries(one.byEntity)) {
          const into = (merged.byEntity[entityName] ??= {
            deleted: 0,
            anonymized: 0,
            errors: 0
          });
          into.deleted += counts.deleted;
          into.anonymized += counts.anonymized;
          into.errors += counts.errors;
        }
      }
      merged.durationMs = Date.now() - start;
      return merged;
    }
    if (tenantIds.length === 1) {
      const only = tenantIds[0];
      // Re-entering with no tenantIds is what stops this recursing.
      return withEncryptionContext(only, () =>
        this.enforce({ ...options, tenantIds: undefined })
      );
    }

    const start = Date.now();
    const batchSize = options?.batchSize ?? this.DEFAULT_BATCH_SIZE;
    const dryRun = options?.dryRun ?? false;
    const policies = getAllRetentionPolicies();
    const result: EnforcementResult = {
      processed: 0,
      deleted: 0,
      anonymized: 0,
      errors: 0,
      byEntity: {},
      durationMs: 0
    };

    if (policies.size === 0) {
      this.otel.info('[RetentionService] No retention policies registered');
      result.durationMs = Date.now() - start;
      return result;
    }

    this.otel.info('[RetentionService] Starting enforcement', {
      entityCount: policies.size,
      batchSize,
      dryRun
    });

    for (const [entityName, policy] of policies) {
      if (options?.entities && !options.entities.includes(entityName)) continue;

      const entityResult = { deleted: 0, anonymized: 0, errors: 0 };

      try {
        await this.enforceEntity(
          entityName,
          policy,
          batchSize,
          entityResult,
          dryRun
        );
      } catch (err) {
        entityResult.errors++;
        this.otel.error('[RetentionService] Entity-level failure', {
          entityName,
          error: String(err)
        });
      }

      result.byEntity[entityName] = entityResult;
      result.deleted += entityResult.deleted;
      result.anonymized += entityResult.anonymized;
      result.errors += entityResult.errors;
      result.processed += entityResult.deleted + entityResult.anonymized;
    }

    result.durationMs = Date.now() - start;
    this.otel.info('[RetentionService] Enforcement complete', {
      processed: result.processed,
      deleted: result.deleted,
      anonymized: result.anonymized,
      errors: result.errors,
      durationMs: result.durationMs
    });

    return result;
  }

  private async enforceEntity(
    entityName: string,
    policy: RetentionPolicy,
    batchSize: number,
    stats: { deleted: number; anonymized: number; errors: number },
    dryRun: boolean
  ): Promise<void> {
    const metadata = [...this.orm.getMetadata().getAll().values()].find(
      (m) => m.className === entityName
    );

    if (!metadata) {
      this.otel.warn(
        '[RetentionService] Entity not in MikroORM metadata — skipping',
        { entityName }
      );
      return;
    }

    const duration = parseDuration(policy.duration);
    const cutoff = subtractDuration(new Date(), duration);
    const entityClass = metadata.class ?? metadata.className;

    // For anonymize: only null out PII/PHI/PCI fields that are nullable.
    // Non-nullable fields are skipped to avoid flush errors.
    const piiFieldNames: string[] = [];
    if (policy.action === 'anonymize') {
      const complianceFields = getEntityComplianceFields(entityName);
      if (complianceFields) {
        for (const [fieldName, level] of complianceFields) {
          if (level === 'none') continue;
          const prop = metadata.properties[fieldName];
          if (prop && prop.nullable) {
            piiFieldNames.push(fieldName);
          } else {
            this.otel.warn(
              '[RetentionService] Skipping non-nullable PII field for anonymize',
              { entityName, fieldName, level }
            );
          }
        }
      }
    }

    const filter: Record<string, unknown> = {
      createdAt: { $lt: cutoff }
    };
    if (policy.action === 'anonymize') {
      filter['retentionAnonymizedAt'] = null;
    }

    // Dry run: use count instead of fetching records
    if (dryRun) {
      const em = this.orm.em.fork();
      try {
        const count = await em.count(entityClass, filter);
        this.otel.info('[RetentionService] Dry run — would process', {
          entityName,
          action: policy.action,
          count
        });
        if (policy.action === 'delete') stats.deleted += count;
        else stats.anonymized += count;
      } catch (err) {
        stats.errors++;
        this.otel.error('[RetentionService] Dry run count failed', {
          entityName,
          error: String(err)
        });
      }
      return;
    }

    let batchNum = 0;

    while (true) {
      batchNum++;
      const em = this.orm.em.fork();

      try {
        const records = await em.find(entityClass, filter, {
          limit: batchSize,
          orderBy: { createdAt: 'ASC' }
        });

        if (records.length === 0) break;

        if (policy.action === 'delete') {
          records.forEach((r) => em.remove(r));
          await em.flush();
          stats.deleted += records.length;
        } else {
          for (const record of records) {
            const rec = record as Record<string, unknown>;
            for (const field of piiFieldNames) {
              rec[field] = null;
            }
            rec['retentionAnonymizedAt'] = new Date();
          }
          await em.flush();
          stats.anonymized += records.length;
        }

        this.otel.info('[RetentionService] Batch processed', {
          entityName,
          action: policy.action,
          batch: batchNum,
          count: records.length
        });
      } catch (err) {
        stats.errors++;
        this.otel.error('[RetentionService] Batch failed', {
          entityName,
          batch: batchNum,
          error: String(err)
        });
        break; // stop batching this entity, continue to next
      }
    }
  }
}

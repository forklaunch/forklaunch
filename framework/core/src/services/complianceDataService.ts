import type { MikroORM } from '@mikro-orm/core';
import type { OpenTelemetryCollector } from '../http/telemetry/openTelemetryCollector';
import { MetricsDefinition } from '../http/types/openTelemetryCollector.types';
import {
  getEntityComplianceFields,
  getEntityRetention,
  getEntityUserIdField
} from '../persistence/complianceTypes';

export interface EraseResult {
  /** Entity names that had at least one record deleted or anonymized. */
  entitiesAffected: string[];
  /** Rows hard-deleted (entities with a `delete` retention policy). */
  recordsDeleted: number;
  /** Rows whose PII was scrubbed but the row retained (the default action). */
  recordsAnonymized: number;
}

/**
 * How a user's records in a given entity are erased.
 *
 * - `anonymize` — null the entity's nullable PII/PHI/PCI fields and keep the
 *   row, preserving non-PII structure (e.g. that an invite or transaction
 *   existed). This is the default when an entity has no retention policy.
 * - `delete` — hard-delete the matching rows.
 */
export type EraseAction = 'anonymize' | 'delete';

export interface ExportResult {
  userId: string;
  entities: Record<string, unknown[]>;
}

/**
 * Describes a single entity that could not be processed during a compliance
 * operation. Surfaced via {@link ComplianceEraseError} / {@link ComplianceExportError}
 * so callers can audit exactly what failed.
 *
 * `entityName` is `'(commit)'` when the failure originated from the database
 * transaction commit (e.g. a foreign key constraint violation) rather than a
 * single entity scan.
 */
export interface ComplianceFailure {
  entityName: string;
  userIdField?: string;
  error: string;
}

/**
 * Thrown when a GDPR erase could not be completed in full.
 *
 * The erase runs inside a single database transaction, so when this is thrown
 * **nothing was deleted** — the transaction is rolled back. This guarantees a
 * caller never sees a partial erasure reported as success.
 */
export class ComplianceEraseError extends Error {
  constructor(
    message: string,
    readonly userId: string,
    readonly failures: ComplianceFailure[]
  ) {
    super(message);
    this.name = 'ComplianceEraseError';
  }
}

/**
 * Thrown when a data export could not be completed in full. Prevents an
 * incomplete export from being mistaken for a complete one.
 */
export class ComplianceExportError extends Error {
  constructor(
    message: string,
    readonly userId: string,
    readonly failures: ComplianceFailure[]
  ) {
    super(message);
    this.name = 'ComplianceExportError';
  }
}

/**
 * Per-entity userIdField overrides.
 * Keys are entity names, values are the field name linking records to a user.
 *
 * @example
 * {
 *   User: 'id',              // the User entity IS the user record
 *   Subscription: 'partyId', // billing links via partyId
 *   Account: 'userId',       // default, can be omitted
 * }
 */
export type UserIdFieldOverrides = Record<string, string>;

/**
 * Common field names that link an entity to a user, tried in order.
 * Used for optimistic search when no explicit userIdField is configured.
 */
const CANDIDATE_USER_FIELDS = [
  'userId',
  'user',
  'id',
  'partyId',
  'customerId',
  'ownerId',
  'createdBy',
  'email'
];

/**
 * Configuration options for ComplianceDataService.
 */
export interface ComplianceDataServiceOptions {
  /**
   * Per-entity userIdField overrides.
   * Keys are entity names, values are the field name linking records to a user.
   */
  userIdFieldOverrides?: UserIdFieldOverrides;

  /**
   * Whether to disable global filters (tenant, soft-delete, etc.) during
   * compliance operations.
   *
   * **SECURITY WARNING**: This allows operations across ALL tenants/organizations.
   * Only enable this for superadmin-level compliance operations (GDPR erasure).
   *
   * **Authorization**: The caller MUST verify superadmin permissions BEFORE
   * instantiating this service with `disableFilters: true`.
   *
   * Default: false (filters remain enabled)
   *
   * @example
   * // In your controller/handler:
   * if (!req.user.isSuperAdmin) {
   *   throw new ForbiddenError('GDPR operations require superadmin role');
   * }
   * const service = new ComplianceDataService(orm, otel, {
   *   disableFilters: true  // Safe because we checked authorization above
   * });
   */
  disableFilters?: boolean;
}

/**
 * Generic compliance data service that walks all compliance-registered entities
 * and erases or exports PII/PHI/PCI data for a given user.
 *
 * Resolution order for userIdField per entity:
 * 1. Constructor overrides (highest priority)
 * 2. defineComplianceEntity({ userIdField }) registry
 * 3. Optimistic search: first CANDIDATE_USER_FIELDS match in entity metadata
 * 4. Skip entity (no user link found)
 *
 * **SECURITY NOTE**: If `disableFilters: true`, this service operates across
 * ALL tenants/organizations. The caller MUST verify superadmin permissions
 * before enabling this option.
 */
export class ComplianceDataService {
  private readonly userIdFieldOverrides: UserIdFieldOverrides;
  private readonly disableFilters: boolean;

  constructor(
    private readonly orm: MikroORM,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>,
    options?: ComplianceDataServiceOptions
  ) {
    this.userIdFieldOverrides = options?.userIdFieldOverrides ?? {};
    this.disableFilters = options?.disableFilters ?? false;

    if (this.disableFilters) {
      this.otel.warn(
        '[ComplianceDataService] Initialized with filters DISABLED',
        {
          warning:
            'Operations will span ALL tenants - ensure superadmin authorization'
        }
      );
    }
  }

  /**
   * Resolve the field linking an entity to a user.
   * Returns undefined if no link can be determined.
   */
  private resolveUserIdField(
    entityName: string,
    entityProperties: Record<string, unknown>
  ): string | undefined {
    // 1. Constructor override
    const override = this.userIdFieldOverrides[entityName];
    if (override) return override;

    // 2. Registry (from defineComplianceEntity({ userIdField }))
    const registered = getEntityUserIdField(entityName);
    if (registered !== 'userId' || entityProperties['userId']) {
      // Registry returned a non-default value, or the default 'userId' exists
      if (entityProperties[registered]) return registered;
    }

    // 3. Optimistic search
    for (const candidate of CANDIDATE_USER_FIELDS) {
      if (entityProperties[candidate]) return candidate;
    }

    // 4. No link found
    return undefined;
  }

  /**
   * Build a dependency graph of entities based on their relationships.
   * Returns entities sorted in deletion-safe order (children before parents).
   *
   * NOTE: Not currently used. Deletion relies on database CASCADE constraints.
   * This method is preserved for future FK-aware deletion implementation if needed.
   *
   * @see https://github.com/forklaunch/forklaunch-js/issues/XXX
   */
  private buildDeletionOrder(
    metadata: Array<{ className: string; properties: Record<string, unknown> }>
  ): string[] {
    // Simple heuristic: entities with FK references should be deleted first
    // This is a basic implementation - a full solution would use MikroORM's
    // relationship metadata to build a proper dependency graph

    const withReferences: string[] = [];
    const withoutReferences: string[] = [];

    for (const meta of metadata) {
      let hasReferences = false;

      // Check if this entity has reference properties (manyToOne, oneToOne with owner side)
      for (const prop of Object.values(meta.properties)) {
        if (
          prop &&
          typeof prop === 'object' &&
          'reference' in prop &&
          (prop.reference === 'many-to-one' || prop.reference === 'one-to-one')
        ) {
          hasReferences = true;
          break;
        }
      }

      if (hasReferences) {
        withReferences.push(meta.className);
      } else {
        withoutReferences.push(meta.className);
      }
    }

    // Delete entities with references first (they're children),
    // then entities without references (they're parents)
    return [...withReferences, ...withoutReferences];
  }

  /**
   * Erase a user's PII/PHI/PCI across every compliance-registered entity.
   *
   * Per entity, the action is resolved from the retention registry and defaults
   * to `anonymize` when no policy is set:
   * - `anonymize` — null the entity's nullable PII fields and keep the row, so
   *   non-PII structure is retained (e.g. that an invite or transaction existed).
   *   If the entity declares a `complianceErasedAt` field, it is stamped with the
   *   erasure time as an audit marker (distinct from retention anonymization).
   * - `delete` — hard-delete the matching rows.
   *
   * Runs inside a single database transaction (all-or-nothing). Any per-entity
   * failure — a scan error, a transaction commit failure (e.g. a foreign key
   * violation from a missing `ON DELETE CASCADE`), or an `anonymize` entity that
   * has a non-nullable PII field — aborts the whole operation and throws a
   * {@link ComplianceEraseError} carrying the structured {@link ComplianceFailure}
   * list. A normal return therefore guarantees the erasure fully succeeded.
   */
  async erase(userId: string): Promise<EraseResult> {
    const em = this.orm.em.fork();
    const entitiesAffected: string[] = [];
    const failures: ComplianceFailure[] = [];
    let recordsDeleted = 0;
    let recordsAnonymized = 0;

    const allMetadata = [...this.orm.getMetadata().getAll().values()];

    try {
      await em.transactional(async (tem) => {
        for (const metadata of allMetadata) {
          const entityName = metadata.className;
          const fields = getEntityComplianceFields(entityName);
          if (!fields) continue;

          const piiFieldNames = [...fields.entries()]
            .filter(
              ([, level]) =>
                level === 'pii' || level === 'phi' || level === 'pci'
            )
            .map(([name]) => name);
          if (piiFieldNames.length === 0) continue;

          const userIdField = this.resolveUserIdField(
            entityName,
            metadata.properties
          );

          if (!userIdField) {
            this.otel.warn(
              '[ComplianceDataService] No user-linking field found — skipping',
              { entityName, candidates: CANDIDATE_USER_FIELDS }
            );
            continue;
          }

          // Default to anonymize (scrub + retain) when no policy is registered.
          const action: EraseAction =
            getEntityRetention(entityName)?.action ?? 'anonymize';

          try {
            const entityClass = metadata.class ?? metadata.className;
            const findOptions = {
              // Pass filters: false to bypass ALL filters when disableFilters is enabled
              // This is the correct MikroORM approach for GDPR cross-tenant operations
              filters: this.disableFilters ? false : undefined
            };
            const records = await tem.find(
              entityClass,
              { [userIdField]: userId },
              findOptions
            );

            if (records.length === 0) continue;

            if (action === 'delete') {
              records.forEach((r) => tem.remove(r));
              recordsDeleted += records.length;
              entitiesAffected.push(entityName);
            } else {
              // Anonymize can only null nullable PII fields. A non-nullable PII
              // field cannot be scrubbed in place — fail loudly rather than
              // leave PII behind.
              const nonNullablePii = piiFieldNames.filter(
                (field) => !metadata.properties[field]?.nullable
              );
              if (nonNullablePii.length > 0) {
                failures.push({
                  entityName,
                  userIdField,
                  error:
                    `Cannot anonymize non-nullable PII field(s): ${nonNullablePii.join(', ')}. ` +
                    `Make the field(s) nullable or register a 'delete' retention policy for this entity.`
                });
                continue;
              }

              // Stamp a dedicated erasure marker (distinct from the retention
              // service's `retentionAnonymizedAt`) so audits can tell a GDPR
              // erasure apart from time-based retention anonymization.
              const hasErasedAtColumn =
                'complianceErasedAt' in metadata.properties;
              for (const record of records) {
                const rec = record as Record<string, unknown>;
                for (const field of piiFieldNames) {
                  rec[field] = null;
                }
                if (hasErasedAtColumn) {
                  rec['complianceErasedAt'] = new Date();
                }
              }
              recordsAnonymized += records.length;
              entitiesAffected.push(entityName);
            }
          } catch (err) {
            failures.push({ entityName, userIdField, error: String(err) });
          }
        }

        // Abort (and roll back) before the implicit commit if any entity could
        // not be erased — a partial erasure must never be committed.
        if (failures.length > 0) {
          throw new ComplianceEraseError(
            'Erase aborted: one or more entities could not be erased',
            userId,
            failures
          );
        }
      });
    } catch (err) {
      if (err instanceof ComplianceEraseError) {
        this.otel.error('[ComplianceDataService] Erase aborted', {
          userId,
          failures: JSON.stringify(err.failures)
        });
        throw err;
      }

      // Transaction commit failed (e.g. FK constraint violation). Nothing was
      // written because the transaction rolled back.
      const commitFailures: ComplianceFailure[] = [
        ...failures,
        { entityName: '(commit)', error: String(err) }
      ];
      this.otel.error('[ComplianceDataService] Erase transaction failed', {
        userId,
        attemptedEntities: entitiesAffected.join(','),
        failures: JSON.stringify(commitFailures)
      });
      throw new ComplianceEraseError(
        'Erase transaction failed during commit',
        userId,
        commitFailures
      );
    }

    this.otel.info('[ComplianceDataService] Erase complete', {
      userId,
      entitiesAffected: entitiesAffected.join(','),
      recordsDeleted,
      recordsAnonymized
    });

    return { entitiesAffected, recordsDeleted, recordsAnonymized };
  }

  /**
   * Export all PII/PHI/PCI records for a user across every compliance-registered
   * entity.
   *
   * If any entity cannot be read, the export is aborted and a
   * {@link ComplianceExportError} is thrown with the structured
   * {@link ComplianceFailure} list, so an incomplete export is never mistaken for
   * a complete one.
   */
  async export(userId: string): Promise<ExportResult> {
    const em = this.orm.em.fork();
    const entities: Record<string, unknown[]> = {};
    const failures: ComplianceFailure[] = [];

    const allMetadata = [...this.orm.getMetadata().getAll().values()];

    for (const metadata of allMetadata) {
      const entityName = metadata.className;
      const fields = getEntityComplianceFields(entityName);
      if (!fields) continue;

      const hasPii = [...fields.values()].some(
        (level) => level === 'pii' || level === 'phi' || level === 'pci'
      );
      if (!hasPii) continue;

      const userIdField = this.resolveUserIdField(
        entityName,
        metadata.properties
      );

      if (!userIdField) {
        continue;
      }

      try {
        const entityClass = metadata.class ?? metadata.className;
        const findOptions = this.disableFilters ? { filters: false } : {};
        const records = await em.find(
          entityClass,
          { [userIdField]: userId },
          findOptions
        );

        if (records.length > 0) {
          const piiFieldNames = [...fields.entries()]
            .filter(([, level]) => level !== 'none')
            .map(([name]) => name);

          entities[entityName] = records.map((record) => {
            const filtered: Record<string, unknown> = {};
            filtered['id'] = (record as Record<string, unknown>)['id'];
            for (const fieldName of piiFieldNames) {
              filtered[fieldName] = (record as Record<string, unknown>)[
                fieldName
              ];
            }
            return filtered;
          });
        }
      } catch (err) {
        failures.push({ entityName, userIdField, error: String(err) });
      }
    }

    if (failures.length > 0) {
      this.otel.error('[ComplianceDataService] Export incomplete', {
        userId,
        failures: JSON.stringify(failures)
      });
      throw new ComplianceExportError(
        'Failed to export one or more entities',
        userId,
        failures
      );
    }

    this.otel.info('[ComplianceDataService] Export complete', {
      userId,
      entityCount: Object.keys(entities).length
    });

    return { userId, entities };
  }
}

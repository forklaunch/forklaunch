import type { MikroORM } from '@mikro-orm/core';
import type { OpenTelemetryCollector } from '../http/telemetry/openTelemetryCollector';
import { MetricsDefinition } from '../http/types/openTelemetryCollector.types';
import {
  getEntityComplianceFields,
  getEntityUserIdField
} from '../persistence/complianceTypes';

export interface EraseResult {
  entitiesAffected: string[];
  recordsDeleted: number;
}

export interface ExportResult {
  userId: string;
  entities: Record<string, unknown[]>;
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
   * Temporarily disable all global filters on the given EntityManager,
   * returning a function to restore them.
   *
   * **SECURITY**: This is only called if `disableFilters: true` was set in
   * the constructor, which should only happen after superadmin authorization.
   *
   * @returns Restore function that re-enables filters to their original state
   */
  private withFiltersDisabled(
    em: ReturnType<MikroORM['em']['fork']>
  ): () => void {
    const filters = this.orm.config.get('filters');
    if (!filters) return () => {}; // No-op if no filters configured

    // Capture current filter state
    const originalFilterStates = new Map<string, boolean>();

    try {
      Object.keys(filters).forEach((filterName) => {
        // Save original state (we can't directly read it, so we assume enabled)
        originalFilterStates.set(filterName, true);

        // Disable filter
        em.setFilterParams(filterName, { enabled: false });
      });

      this.otel.debug('[ComplianceDataService] Filters disabled', {
        filters: Object.keys(filters)
      });
    } catch (err) {
      this.otel.warn('[ComplianceDataService] Failed to disable filters', {
        error: String(err)
      });
    }

    // Return restore function
    return () => {
      try {
        originalFilterStates.forEach((wasEnabled, filterName) => {
          em.setFilterParams(filterName, { enabled: wasEnabled });
        });

        this.otel.debug('[ComplianceDataService] Filters restored', {
          filters: Array.from(originalFilterStates.keys())
        });
      } catch (err) {
        this.otel.warn('[ComplianceDataService] Failed to restore filters', {
          error: String(err)
        });
      }
    };
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

  async erase(userId: string): Promise<EraseResult> {
    const em = this.orm.em.fork();

    // Temporarily disable filters if authorized (superadmin only)
    // GDPR erasure must work across ALL tenants when filters are disabled
    const restoreFilters = this.disableFilters
      ? this.withFiltersDisabled(em)
      : () => {}; // No-op if filters should remain enabled

    const entitiesAffected: string[] = [];
    let recordsDeleted = 0;

    try {
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
          this.otel.warn(
            '[ComplianceDataService] No user-linking field found — skipping',
            { entityName, candidates: CANDIDATE_USER_FIELDS }
          );
          continue;
        }

        try {
          const entityClass = metadata.class ?? metadata.className;
          const records = await em.find(entityClass, {
            [userIdField]: userId
          });

          if (records.length > 0) {
            entitiesAffected.push(entityName);
            recordsDeleted += records.length;
            records.forEach((r) => em.remove(r));
          }
        } catch (err) {
          this.otel.error('[ComplianceDataService] Failed to erase entity', {
            entityName,
            userIdField,
            error: String(err)
          });
        }
      }

      if (recordsDeleted > 0) {
        await em.flush();
      }

      this.otel.info('[ComplianceDataService] Erase complete', {
        userId,
        entitiesAffected: entitiesAffected.join(','),
        recordsDeleted
      });

      return { entitiesAffected, recordsDeleted };
    } finally {
      // CRITICAL: Always restore filters, even if erase fails
      restoreFilters();
    }
  }

  async export(userId: string): Promise<ExportResult> {
    const em = this.orm.em.fork();

    // Temporarily disable filters if authorized (superadmin only)
    const restoreFilters = this.disableFilters
      ? this.withFiltersDisabled(em)
      : () => {}; // No-op if filters should remain enabled

    const entities: Record<string, unknown[]> = {};

    try {
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
          const records = await em.find(entityClass, {
            [userIdField]: userId
          });

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
          this.otel.error('[ComplianceDataService] Failed to export entity', {
            entityName,
            userIdField,
            error: String(err)
          });
        }
      }

      this.otel.info('[ComplianceDataService] Export complete', {
        userId,
        entityCount: Object.keys(entities).length
      });

      return { userId, entities };
    } finally {
      // CRITICAL: Always restore filters, even if export fails
      restoreFilters();
    }
  }
}

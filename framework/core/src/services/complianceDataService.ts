import type { EntityManager, MetadataStorage } from '@mikro-orm/core';
import type { OpenTelemetryCollector } from '../http/telemetry/openTelemetryCollector';

/**
 * Structural subset of `MikroORM` used by the compliance services. Typed
 * structurally (rather than as `MikroORM`) because the class's `Entities`
 * type parameter defaults to a mutable array while `MikroORM.init()` returns
 * a readonly one, making concrete instances unassignable to a bare
 * `MikroORM` parameter.
 */
export interface ComplianceOrm {
  em: Pick<EntityManager, 'fork'>;
  getMetadata(): MetadataStorage;
}
import { MetricsDefinition } from '../http/types/openTelemetryCollector.types';
import { withEncryptionContext } from '../persistence/encryptedType';
import {
  getEntityComplianceFields,
  getEntityUserIdField
} from '../persistence/complianceTypes';

/**
 * Which tenants a subject's rows live under.
 *
 * A compliance walk cannot be tenant-blind. Every PII column is encrypted
 * under its row's tenant, and a WHERE value on one is encrypted under the
 * CURRENT tenant before it is compared — so an unbound walk searches with a
 * key nothing was written with. It matches nothing, everywhere, and an erase
 * that finds nothing looks exactly like an erase with nothing to do.
 *
 * Pass every tenant the subject may have rows under — their organizations,
 * plus whatever constant holds rows belonging to no organization. The walk
 * runs once per tenant and the results are merged.
 */
export interface ComplianceTenantScope {
  tenantIds?: readonly string[];
}

/** An entity the walk could not read, and why. */
export interface ComplianceEntityFailure {
  entityName: string;
  tenantId?: string;
  reason: string;
}

export interface EraseResult {
  entitiesAffected: string[];
  recordsDeleted: number;
  /**
   * Entities the walk could not read. NON-EMPTY MEANS THE ERASE IS
   * INCOMPLETE: the caller asked for a subject's data to be deleted and some
   * of it may remain. It is reported rather than thrown so the rows that
   * could be erased still are, but it must not be ignored — treat a non-empty
   * list as a failed erasure request.
   */
  failures: ComplianceEntityFailure[];
}

export interface ExportResult {
  userId: string;
  entities: Record<string, unknown[]>;
  /** As `EraseResult.failures`: non-empty means the export is incomplete. */
  failures: ComplianceEntityFailure[];
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
 * Generic compliance data service that walks all compliance-registered entities
 * and erases or exports PII/PHI/PCI data for a given user.
 *
 * Resolution order for userIdField per entity:
 * 1. Constructor overrides (highest priority)
 * 2. defineComplianceEntity({ userIdField }) registry
 * 3. Optimistic search: first CANDIDATE_USER_FIELDS match in entity metadata
 * 4. Skip entity (no user link found)
 */
export class ComplianceDataService {
  private readonly userIdFieldOverrides: UserIdFieldOverrides;

  constructor(
    private readonly orm: ComplianceOrm,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>,
    userIdFieldOverrides?: UserIdFieldOverrides
  ) {
    this.userIdFieldOverrides = userIdFieldOverrides ?? {};
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
   * Run `fn` once per tenant in scope, each inside its own encryption
   * context. With no tenants named it runs once unbound — which is correct
   * only for entities with no encrypted column, and fails loudly otherwise.
   */
  private async perTenant<T>(
    scope: ComplianceTenantScope | undefined,
    fn: (tenantId?: string) => Promise<T>
  ): Promise<T[]> {
    const tenantIds = scope?.tenantIds ?? [];
    if (tenantIds.length === 0) return [await fn(undefined)];
    const results: T[] = [];
    for (const tenantId of tenantIds) {
      results.push(await withEncryptionContext(tenantId, () => fn(tenantId)));
    }
    return results;
  }

  async erase(
    userId: string,
    scope?: ComplianceTenantScope
  ): Promise<EraseResult> {
    const entitiesAffected = new Set<string>();
    const failures: ComplianceEntityFailure[] = [];
    let recordsDeleted = 0;

    await this.perTenant(scope, async (tenantId) => {
      recordsDeleted += await this.eraseUnderCurrentTenant(
        userId,
        tenantId,
        entitiesAffected,
        failures
      );
    });

    this.otel.info('[ComplianceDataService] Erase complete', {
      userId,
      entitiesAffected: [...entitiesAffected].join(','),
      recordsDeleted,
      failures: failures.length
    });

    return {
      entitiesAffected: [...entitiesAffected],
      recordsDeleted,
      failures
    };
  }

  private async eraseUnderCurrentTenant(
    userId: string,
    tenantId: string | undefined,
    entitiesAffected: Set<string>,
    failures: ComplianceEntityFailure[]
  ): Promise<number> {
    const em = this.orm.em.fork();
    let recordsDeleted = 0;

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
          entitiesAffected.add(entityName);
          recordsDeleted += records.length;
          records.forEach((r) => em.remove(r));
        }
      } catch (err) {
        // RECORDED, NOT SWALLOWED. This used to log and continue, so an erase
        // that could not read a single entity still returned a clean result
        // and the caller reported the subject's data deleted.
        failures.push({ entityName, tenantId, reason: String(err) });
        this.otel.error('[ComplianceDataService] Failed to erase entity', {
          entityName,
          userIdField,
          tenantId,
          error: String(err)
        });
      }
    }

    if (recordsDeleted > 0) {
      await em.flush();
    }

    return recordsDeleted;
  }

  async export(
    userId: string,
    scope?: ComplianceTenantScope
  ): Promise<ExportResult> {
    const entities: Record<string, unknown[]> = {};
    const failures: ComplianceEntityFailure[] = [];

    await this.perTenant(scope, (tenantId) =>
      this.exportUnderCurrentTenant(userId, tenantId, entities, failures)
    );

    this.otel.info('[ComplianceDataService] Export complete', {
      userId,
      entityCount: Object.keys(entities).length,
      failures: failures.length
    });

    return { userId, entities, failures };
  }

  private async exportUnderCurrentTenant(
    userId: string,
    tenantId: string | undefined,
    entities: Record<string, unknown[]>,
    failures: ComplianceEntityFailure[]
  ): Promise<void> {
    const em = this.orm.em.fork();

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

          const rows = records.map((record) => {
            const filtered: Record<string, unknown> = {};
            filtered['id'] = (record as Record<string, unknown>)['id'];
            for (const fieldName of piiFieldNames) {
              filtered[fieldName] = (record as Record<string, unknown>)[
                fieldName
              ];
            }
            return filtered;
          });
          // Appended, not assigned: the same entity can hold the subject's
          // rows under more than one tenant, and the second pass must not
          // erase the first one's findings.
          entities[entityName] = [...(entities[entityName] ?? []), ...rows];
        }
      } catch (err) {
        // Recorded, not swallowed — an export missing an entity is an
        // INCOMPLETE subject access request, not a smaller one.
        failures.push({ entityName, tenantId, reason: String(err) });
        this.otel.error('[ComplianceDataService] Failed to export entity', {
          entityName,
          userIdField,
          tenantId,
          error: String(err)
        });
      }
    }
  }
}

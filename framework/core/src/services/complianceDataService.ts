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
    private readonly orm: MikroORM,
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

  async erase(userId: string): Promise<EraseResult> {
    const em = this.orm.em.fork();
    const entitiesAffected: string[] = [];
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
  }

  async export(userId: string): Promise<ExportResult> {
    const em = this.orm.em.fork();
    const entities: Record<string, unknown[]> = {};

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
  }
}

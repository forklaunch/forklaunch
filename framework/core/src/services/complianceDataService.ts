import type { MikroORM, EntitySchema, EntityName } from '@mikro-orm/core';
import type { OpenTelemetryCollector } from '../http/telemetry/openTelemetryCollector';
import { MetricsDefinition } from '../http/types/openTelemetryCollector.types';
import {
  getEntityUserIdField,
  type EntityComplianceMetadata,
  type ComplianceLevel,
  type RetentionAction
} from '../persistence/complianceTypes';

export interface EraseResult {
  entitiesAffected: string[];
  recordsDeleted: number;
  recordsAnonymized: number;
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
 * Configuration options for ComplianceDataService.
 */
export interface ComplianceDataServiceOptions {
  /**
   * Explicit list of entity schemas to process (Option 1 - for testing).
   * When provided, only these entities will be processed.
   * Takes precedence over autoDiscover.
   */
  entities?: EntitySchema[];

  /**
   * Auto-discover entities from ORM metadata (Option 3 - main path).
   * When true, scans all entities registered with the ORM and processes those
   * with compliance metadata.
   * Default: true
   */
  autoDiscover?: boolean;

  /**
   * Per-entity userIdField overrides (legacy).
   * @deprecated Use userIdField in defineComplianceEntity instead.
   */
  userIdFieldOverrides?: UserIdFieldOverrides;
}

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
 * Entity discovery modes:
 * - Explicit (Option 1): Pass { entities: [User, Account, ...] } for testing/control
 * - Auto-discover (Option 3): Pass { autoDiscover: true } to scan ORM metadata (default)
 *
 * Resolution order for userIdField per entity:
 * 1. Constructor overrides (highest priority)
 * 2. defineComplianceEntity({ userIdField }) metadata
 * 3. Optimistic search: first CANDIDATE_USER_FIELDS match in entity metadata
 * 4. Skip entity (no user link found)
 */
export class ComplianceDataService {
  private readonly explicitEntities?: EntitySchema[];
  private readonly autoDiscover: boolean;
  private readonly userIdFieldOverrides: UserIdFieldOverrides;

  constructor(
    private readonly orm: MikroORM,
    private readonly otel: OpenTelemetryCollector<MetricsDefinition>,
    options?: ComplianceDataServiceOptions | UserIdFieldOverrides
  ) {
    // Support legacy constructor signature: (orm, otel, userIdFieldOverrides)
    if (options && !('entities' in options || 'autoDiscover' in options)) {
      this.userIdFieldOverrides = options as UserIdFieldOverrides;
      this.autoDiscover = true;
      this.explicitEntities = undefined;
    } else {
      const opts = (options as ComplianceDataServiceOptions) ?? {};
      this.explicitEntities = opts.entities;
      this.autoDiscover = opts.autoDiscover ?? !opts.entities; // Default true if no explicit entities
      this.userIdFieldOverrides = opts.userIdFieldOverrides ?? {};
    }
  }

  /**
   * Get entities to process based on configuration (explicit list or ORM auto-discovery).
   */
  private getEntitiesToProcess(): Array<{
    name: string;
    metadata: EntityComplianceMetadata;
    entityClass: string | EntityName<object>;
    properties: Record<string, unknown>;
  }> {
    const entities: Array<{
      name: string;
      metadata: EntityComplianceMetadata;
      entityClass: string | EntityName<object>;
      properties: Record<string, unknown>;
    }> = [];

    // Option 1: Explicit entities (for testing)
    if (this.explicitEntities && this.explicitEntities.length > 0) {
      for (const schema of this.explicitEntities) {
        const complianceMeta = schema.meta.compliance;
        if (!complianceMeta) continue;

        entities.push({
          name: schema.meta.name ?? 'Unknown',
          metadata: complianceMeta,
          entityClass: schema,
          properties: schema.meta.properties
        });
      }
      return entities;
    }

    // Option 3: Auto-discover from ORM (main path)
    if (this.autoDiscover) {
      const allMetadata = [...this.orm.getMetadata().getAll().values()];
      for (const ormMeta of allMetadata) {
        const complianceMeta = ormMeta.compliance;
        if (!complianceMeta) continue;

        entities.push({
          name: ormMeta.className,
          metadata: complianceMeta,
          // Prefer className (string) for simplicity; ormMeta.class may not be needed
          entityClass: ormMeta.className,
          properties: ormMeta.properties
        });
      }
      return entities;
    }

    // No entities configured
    return [];
  }

  /**
   * Resolve the field linking an entity to a user.
   * Returns undefined if no link can be determined.
   */
  private resolveUserIdField(
    entityName: string,
    entityProperties: Record<string, unknown>,
    complianceMetadata: EntityComplianceMetadata
  ): string | undefined {
    // 1. Constructor override
    const override = this.userIdFieldOverrides[entityName];
    if (override) return override;

    // 2. Entity metadata (from defineComplianceEntity({ userIdField }))
    const metadataField = complianceMetadata.userIdField;
    if (metadataField && entityProperties[metadataField]) {
      return metadataField;
    }

    // 3. Legacy registry (for backward compatibility)
    const registered = getEntityUserIdField(entityName);
    if (registered !== 'userId' || entityProperties['userId']) {
      // Registry returned a non-default value, or the default 'userId' exists
      if (entityProperties[registered]) return registered;
    }

    // 4. Optimistic search
    for (const candidate of CANDIDATE_USER_FIELDS) {
      if (entityProperties[candidate]) return candidate;
    }

    // 5. No link found
    return undefined;
  }

  async erase(userId: string): Promise<EraseResult> {
    const em = this.orm.em.fork();
    const entitiesAffected: string[] = [];
    let recordsDeleted = 0;
    let recordsAnonymized = 0;

    const entitiesToProcess = this.getEntitiesToProcess();

    for (const entity of entitiesToProcess) {
      const {
        name: entityName,
        metadata: complianceMeta,
        entityClass,
        properties
      } = entity;

      // Check if entity has protected data (PII/PHI/PCI/SOX)
      const hasProtectedData = [...complianceMeta.fields.values()].some(
        (level: ComplianceLevel) => level !== 'none'
      );
      if (!hasProtectedData) continue;

      const userIdField = this.resolveUserIdField(
        entityName,
        properties,
        complianceMeta
      );

      if (!userIdField) {
        this.otel.warn(
          '[ComplianceDataService] No user-linking field found — skipping',
          { entityName, candidates: CANDIDATE_USER_FIELDS }
        );
        continue;
      }

      try {
        const records = await em.find(entityClass as EntityName<object>, {
          [userIdField]: userId
        });

        if (records.length > 0) {
          entitiesAffected.push(entityName);

          // Determine erasure action: anonymize (default) or delete
          const retentionPolicy = complianceMeta.retention;
          const action: RetentionAction =
            retentionPolicy?.action ?? 'anonymize';

          if (action === 'anonymize') {
            // Anonymize: null out PII fields, set complianceErasedAt
            for (const record of records) {
              const rec = record as Record<string, unknown>;

              // Null out all protected fields
              for (const [
                fieldName,
                level
              ] of complianceMeta.fields.entries()) {
                if (level !== 'none') {
                  rec[fieldName] = null;
                }
              }

              // Set tombstone timestamp if field exists
              if ('complianceErasedAt' in rec) {
                rec.complianceErasedAt = new Date();
              }
            }
            recordsAnonymized += records.length;
          } else {
            // Delete: hard remove rows
            records.forEach((r) => em.remove(r));
            recordsDeleted += records.length;
          }
        }
      } catch (err) {
        this.otel.error('[ComplianceDataService] Failed to erase entity', {
          entityName,
          userIdField,
          error: String(err)
        });
      }
    }

    if (recordsDeleted > 0 || recordsAnonymized > 0) {
      await em.flush();
    }

    this.otel.info('[ComplianceDataService] Erase complete', {
      userId,
      entitiesAffected: entitiesAffected.join(','),
      recordsDeleted,
      recordsAnonymized
    });

    return { entitiesAffected, recordsDeleted, recordsAnonymized };
  }

  async export(userId: string): Promise<ExportResult> {
    const em = this.orm.em.fork();
    const entities: Record<string, unknown[]> = {};

    const entitiesToProcess = this.getEntitiesToProcess();

    for (const entity of entitiesToProcess) {
      const {
        name: entityName,
        metadata: complianceMeta,
        entityClass,
        properties
      } = entity;

      // Check if entity has protected data (PII/PHI/PCI/SOX)
      const hasProtectedData = [...complianceMeta.fields.values()].some(
        (level: ComplianceLevel) => level !== 'none'
      );
      if (!hasProtectedData) continue;

      const userIdField = this.resolveUserIdField(
        entityName,
        properties,
        complianceMeta
      );

      if (!userIdField) {
        continue;
      }

      try {
        const records = await em.find(entityClass as EntityName<object>, {
          [userIdField]: userId
        });

        if (records.length > 0) {
          const protectedFieldNames = [...complianceMeta.fields.entries()]
            .filter(([, level]) => level !== 'none')
            .map(([name]) => name);

          entities[entityName] = records.map((record) => {
            const filtered: Record<string, unknown> = {};
            filtered['id'] = (record as Record<string, unknown>)['id'];
            for (const fieldName of protectedFieldNames) {
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

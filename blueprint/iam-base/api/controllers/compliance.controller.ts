import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import { getEntityComplianceFields } from '@forklaunch/core/persistence';
import { EntityManager } from '@mikro-orm/core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const scopeFactory = () => ci.createScope();
const emFactory = ci.scopedResolver(tokens.EntityManager);

/**
 * GDPR Right to Erasure — deletes all PII/PHI data for a user
 * from IAM entities. Called directly or via HMAC from billing service.
 */
export const eraseUserData = handlers.delete(
  schemaValidator,
  '/compliance/erase/:userId',
  {
    name: 'EraseUserData',
    access: 'internal',
    summary:
      'Erases all PII/PHI data for a user from IAM entities (GDPR Art. 17)',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: {
      userId: string
    },
    responses: {
      200: {
        entitiesAffected: schemaValidator.array(string),
        recordsDeleted: schemaValidator.number
      },
      404: string
    }
  },
  async (req, res) => {
    const { userId } = req.params;
    const em = emFactory(scopeFactory());

    const result = await eraseUserPii(em, userId);

    if (result.recordsDeleted === 0) {
      res.status(404).send('User not found or no PII data to erase');
      return;
    }

    openTelemetryCollector.info('GDPR erasure completed', {
      'audit.eventType': 'gdpr_erasure',
      'audit.userId': userId,
      'audit.entitiesAffected': result.entitiesAffected.join(','),
      'audit.recordsDeleted': result.recordsDeleted
    });

    res.status(200).json(result);
  }
);

/**
 * GDPR Data Portability — exports all PII/PHI data for a user
 * from IAM entities as JSON. Called directly or via HMAC from billing.
 */
export const exportUserData = handlers.get(
  schemaValidator,
  '/compliance/export/:userId',
  {
    name: 'ExportUserData',
    access: 'internal',
    summary:
      'Exports all PII/PHI data for a user from IAM entities (GDPR Art. 20)',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: {
      userId: string
    },
    responses: {
      200: {
        userId: string,
        entities: schemaValidator.record(string, schemaValidator.unknown)
      },
      404: string
    }
  },
  async (req, res) => {
    const { userId } = req.params;
    const em = emFactory(scopeFactory());

    const result = await collectUserPii(em, userId);

    if (Object.keys(result.entities).length === 0) {
      res.status(404).send('User not found or no PII data to export');
      return;
    }

    openTelemetryCollector.info('GDPR export completed', {
      'audit.eventType': 'gdpr_export',
      'audit.userId': userId
    });

    res.status(200).json(result);
  }
);

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

const IAM_ENTITIES = ['User', 'Organization', 'Role', 'Permission'] as const;

/** Fields that link non-User entities to a user */
const USER_RELATION_FIELDS = ['user', 'userId'];

function entityHasUserRelation(em: EntityManager, entityName: string): boolean {
  const metadata = [...em.getMetadata().getAll().values()].find(
    (m) => m.className === entityName
  );
  if (!metadata) return false;
  return USER_RELATION_FIELDS.some(
    (field) => metadata.properties[field] != null
  );
}

async function eraseUserPii(
  em: EntityManager,
  userId: string
): Promise<{ entitiesAffected: string[]; recordsDeleted: number }> {
  const entitiesAffected: string[] = [];
  let recordsDeleted = 0;

  for (const entityName of IAM_ENTITIES) {
    const fields = getEntityComplianceFields(entityName);
    if (!fields) continue;

    const hasPii = [...fields.values()].some(
      (level) => level === 'pii' || level === 'phi' || level === 'pci'
    );
    if (!hasPii) continue;

    const metadata = [...em.getMetadata().getAll().values()].find(
      (m) => m.className === entityName
    );
    if (!metadata) continue;

    let records: object[];
    if (entityName === 'User') {
      records = await em.find(metadata.class ?? metadata.className, {
        id: userId
      });
    } else if (entityHasUserRelation(em, entityName)) {
      records = await em.find(metadata.class ?? metadata.className, {
        $or: [{ user: userId }, { userId: userId }]
      });
    } else {
      continue;
    }

    if (records.length > 0) {
      entitiesAffected.push(entityName);
      recordsDeleted += records.length;
      records.forEach((r) => em.remove(r));
    }
  }

  if (recordsDeleted > 0) {
    await em.flush();
  }

  return { entitiesAffected, recordsDeleted };
}

async function collectUserPii(
  em: EntityManager,
  userId: string
): Promise<{ userId: string; entities: Record<string, unknown[]> }> {
  const entities: Record<string, unknown[]> = {};

  for (const entityName of IAM_ENTITIES) {
    const fields = getEntityComplianceFields(entityName);
    if (!fields) continue;

    const hasPii = [...fields.values()].some(
      (level) => level === 'pii' || level === 'phi' || level === 'pci'
    );
    if (!hasPii) continue;

    const metadata = [...em.getMetadata().getAll().values()].find(
      (m) => m.className === entityName
    );
    if (!metadata) continue;

    let records: object[];
    if (entityName === 'User') {
      records = await em.find(metadata.class ?? metadata.className, {
        id: userId
      });
    } else if (entityHasUserRelation(em, entityName)) {
      records = await em.find(metadata.class ?? metadata.className, {
        $or: [{ user: userId }, { userId: userId }]
      });
    } else {
      continue;
    }

    if (records.length > 0) {
      const piiFieldNames = [...fields.entries()]
        .filter(([, level]) => level !== 'none')
        .map(([name]) => name);

      entities[entityName] = records.map((record) => {
        const filtered: Record<string, unknown> = {};
        for (const fieldName of piiFieldNames) {
          filtered[fieldName] = (record as Record<string, unknown>)[fieldName];
        }
        filtered['id'] = (record as Record<string, unknown>)['id'];
        return filtered;
      });
    }
  }

  return { userId, entities };
}

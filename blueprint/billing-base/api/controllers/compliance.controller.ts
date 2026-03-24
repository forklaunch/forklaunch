import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';
import { generateHmacAuthHeaders } from '@forklaunch/core/http';
import { getEntityComplianceFields } from '@forklaunch/core/persistence';
import { EntityManager } from '@mikro-orm/core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

const scopeFactory = () => ci.createScope();
const emFactory = ci.scopedResolver(tokens.EntityManager);

/**
 * IAM service base URL for HMAC inter-service calls.
 * Billing cascades GDPR operations to IAM.
 */
const IAM_SERVICE_URL = process.env.IAM_SERVICE_URL ?? 'http://localhost:8000';

/**
 * GDPR Right to Erasure — deletes all PII/PHI data for a user.
 * Handles billing entities AND cascades to IAM via HMAC.
 */
export const eraseUserData = handlers.delete(
  schemaValidator,
  '/compliance/erase/:userId',
  {
    name: 'EraseUserData',
    access: 'internal',
    summary:
      'Erases all PII/PHI data for a user from billing + IAM entities (GDPR Art. 17)',
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
        billing: {
          entitiesAffected: schemaValidator.array(string),
          recordsDeleted: schemaValidator.number
        },
        iam: {
          entitiesAffected: schemaValidator.array(string),
          recordsDeleted: schemaValidator.number
        }
      },
      404: string
    }
  },
  async (req, res) => {
    const { userId } = req.params;
    const em = emFactory(scopeFactory());

    // 1. Erase billing PII
    const billingResult = await eraseBillingPii(em, userId);

    // 2. Cascade to IAM via HMAC
    const iamResult = await callIamErase(userId);

    openTelemetryCollector.info('GDPR erasure completed (billing + IAM)', {
      'audit.eventType': 'gdpr_erasure',
      'audit.userId': userId,
      'audit.billingRecords': billingResult.recordsDeleted,
      'audit.iamRecords': iamResult.recordsDeleted
    });

    res.status(200).json({
      billing: billingResult,
      iam: iamResult
    });
  }
);

/**
 * GDPR Data Portability — exports all PII/PHI data for a user.
 * Handles billing entities AND cascades to IAM via HMAC.
 */
export const exportUserData = handlers.get(
  schemaValidator,
  '/compliance/export/:userId',
  {
    name: 'ExportUserData',
    access: 'internal',
    summary:
      'Exports all PII/PHI data for a user from billing + IAM entities (GDPR Art. 20)',
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
        billing: schemaValidator.record(string, schemaValidator.unknown),
        iam: schemaValidator.record(string, schemaValidator.unknown)
      },
      404: string
    }
  },
  async (req, res) => {
    const { userId } = req.params;
    const em = emFactory(scopeFactory());

    // 1. Collect billing PII
    const billingData = await collectBillingPii(em, userId);

    // 2. Cascade to IAM via HMAC
    const iamData = await callIamExport(userId);

    openTelemetryCollector.info('GDPR export completed (billing + IAM)', {
      'audit.eventType': 'gdpr_export',
      'audit.userId': userId
    });

    res.status(200).json({
      userId,
      billing: billingData,
      iam: iamData
    });
  }
);

// ---------------------------------------------------------------------------
// Billing entity operations
// ---------------------------------------------------------------------------

const BILLING_ENTITIES = [
  'Subscription',
  'CheckoutSession',
  'PaymentLink',
  'Plan',
  'BillingPortal',
  'BillingProvider'
] as const;

async function eraseBillingPii(
  em: EntityManager,
  userId: string
): Promise<{ entitiesAffected: string[]; recordsDeleted: number }> {
  const entitiesAffected: string[] = [];
  let recordsDeleted = 0;

  for (const entityName of BILLING_ENTITIES) {
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

    // Billing entities are linked to users via partyId or customerId
    const records = await em
      .find(metadata.class ?? metadata.className, {
        $or: [{ partyId: userId }, { customerId: userId }]
      })
      .catch(() => []);

    if (records.length > 0) {
      entitiesAffected.push(entityName);
      recordsDeleted += records.length;
      records.forEach((r) => em.remove(r));
      await em.flush();
    }
  }

  return { entitiesAffected, recordsDeleted };
}

async function collectBillingPii(
  em: EntityManager,
  userId: string
): Promise<Record<string, unknown[]>> {
  const entities: Record<string, unknown[]> = {};

  for (const entityName of BILLING_ENTITIES) {
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

    const records = await em
      .find(metadata.class ?? metadata.className, {
        $or: [{ partyId: userId }, { customerId: userId }]
      })
      .catch(() => []);

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

  return entities;
}

// ---------------------------------------------------------------------------
// IAM cascade via HMAC
// ---------------------------------------------------------------------------

async function callIamErase(
  userId: string
): Promise<{ entitiesAffected: string[]; recordsDeleted: number }> {
  try {
    const headers = generateHmacAuthHeaders({
      secretKey: HMAC_SECRET_KEY,
      method: 'DELETE',
      path: `/compliance/erase/${userId}`
    });

    const response = await fetch(
      `${IAM_SERVICE_URL}/compliance/erase/${userId}`,
      {
        method: 'DELETE',
        headers: { ...headers, 'Content-Type': 'application/json' }
      }
    );

    if (response.ok) {
      return (await response.json()) as {
        entitiesAffected: string[];
        recordsDeleted: number;
      };
    }

    return { entitiesAffected: [], recordsDeleted: 0 };
  } catch {
    return { entitiesAffected: [], recordsDeleted: 0 };
  }
}

async function callIamExport(
  userId: string
): Promise<Record<string, unknown[]>> {
  try {
    const headers = generateHmacAuthHeaders({
      secretKey: HMAC_SECRET_KEY,
      method: 'GET',
      path: `/compliance/export/${userId}`
    });

    const response = await fetch(
      `${IAM_SERVICE_URL}/compliance/export/${userId}`,
      {
        method: 'GET',
        headers: { ...headers, 'Content-Type': 'application/json' }
      }
    );

    if (response.ok) {
      const data = (await response.json()) as {
        entities: Record<string, unknown[]>;
      };
      return data.entities;
    }

    return {};
  } catch {
    return {};
  }
}

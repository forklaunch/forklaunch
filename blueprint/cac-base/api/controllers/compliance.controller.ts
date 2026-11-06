import {
  handlers,
  PLATFORM_SYSTEM_ROLES,
  schemaValidator,
  string,
  uuid
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const complianceDataService = ci.resolve(tokens.ComplianceDataService);
const entityManagerFactory = ci.scopedResolver(tokens.EntityManager);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

// `userId` on both routes below is a Patient id (see registrations.ts's
// ComplianceDataService userIdFieldOverrides — Patient: 'id'). Every PHI
// column is encrypted under the patient's organization, so the walk must run
// under that tenant: core 3's erase/export take it as `{ tenantIds }`. The
// organization is read with one plain query on the unencrypted column;
// loading the Patient entity unscoped would try to decrypt its PHI.
async function patientOrganization(patientId: string): Promise<string | undefined> {
  const rows = await entityManagerFactory()
    .getConnection()
    .execute<{ organization_id: string }[]>(
      'select organization_id from patient where id = ? limit 1',
      [patientId]
    );
  return rows[0]?.organization_id;
}

const FailureSchema = {
  entityName: string,
  tenantId: schemaValidator.optional(string),
  reason: string
};

/**
 * GDPR Right to Erasure — deletes all PII/PHI/PCI data for a user
 * from cac entities.
 */
export const eraseUserData = handlers.delete(
  schemaValidator,
  '/erase/:userId',
  {
    name: 'EraseUserData',
    access: 'protected',
    summary:
      'Erases all PII/PHI/PCI data for a user from cac entities (GDPR Art. 17)',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: PLATFORM_SYSTEM_ROLES
    },
    params: {
      userId: uuid
    },
    responses: {
      200: {
        entitiesAffected: schemaValidator.array(string),
        recordsDeleted: schemaValidator.number
      },
      404: string,
      // some entities could not be read: the erase is incomplete
      500: {
        message: string,
        failures: schemaValidator.array(FailureSchema)
      }
    }
  },
  async (req, res) => {
    const { userId } = req.params;
    const organizationId = await patientOrganization(userId);
    if (!organizationId) {
      res.status(404).send('User not found or no PII data to erase');
      return;
    }
    const result = await complianceDataService.erase(userId, {
      tenantIds: [organizationId]
    });

    if (result.failures.length > 0) {
      res.status(500).json({
        message: 'Erasure incomplete: some records could not be read',
        failures: result.failures
      });
      return;
    }
    if (result.recordsDeleted === 0) {
      res.status(404).send('User not found or no PII data to erase');
      return;
    }

    res.status(200).json({
      entitiesAffected: result.entitiesAffected,
      recordsDeleted: result.recordsDeleted
    });
  }
);

/**
 * GDPR Data Portability — exports all PII/PHI/PCI data for a user
 * from cac entities.
 */
export const exportUserData = handlers.get(
  schemaValidator,
  '/export/:userId',
  {
    name: 'ExportUserData',
    access: 'protected',
    summary:
      'Exports all PII/PHI/PCI data for a user from cac entities (GDPR Art. 20)',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: PLATFORM_SYSTEM_ROLES
    },
    params: {
      userId: uuid
    },
    responses: {
      200: {
        userId: string,
        entities: schemaValidator.record(string, schemaValidator.unknown)
      },
      404: string,
      // some entities could not be read: the export is incomplete
      500: {
        message: string,
        failures: schemaValidator.array(FailureSchema)
      }
    }
  },
  async (req, res) => {
    const { userId } = req.params;
    const organizationId = await patientOrganization(userId);
    if (!organizationId) {
      res.status(404).send('User not found or no PII data to export');
      return;
    }
    const result = await complianceDataService.export(userId, {
      tenantIds: [organizationId]
    });

    if (result.failures.length > 0) {
      res.status(500).json({
        message: 'Export incomplete: some records could not be read',
        failures: result.failures
      });
      return;
    }
    if (Object.keys(result.entities).length === 0) {
      res.status(404).send('User not found or no PII data to export');
      return;
    }

    res.status(200).json({ userId: result.userId, entities: result.entities });
  }
);

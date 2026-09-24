import {
  handlers,
  PLATFORM_SYSTEM_ROLES,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { withEncryptionContext } from '@forklaunch/core/persistence';
import { ci, tokens } from '../../bootstrapper';
import { Patient } from '../../persistence/entities/patient.entity';

const complianceDataService = ci.resolve(tokens.ComplianceDataService);
const entityManagerFactory = ci.scopedResolver(tokens.EntityManager);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

// `userId` on both routes below is a Patient id (see registrations.ts's
// ComplianceDataService userIdFieldOverrides — Patient: 'id'). Resolve its
// organizationId with a bare, unscoped EntityManager first (plaintext —
// compliance('none') — so this one lookup needs no tenant context, and the
// MikroORM tenant filter fails open with no filter param set), then run the
// actual erase/export inside that org's encryption context. See
// registrations.ts's ComplianceDataService comment for why this has to be
// `withEncryptionContext` around the call, not a query filter.
async function withPatientTenantContext<T>(
  patientId: string,
  fn: () => Promise<T>
): Promise<T> {
  const em = entityManagerFactory();
  const patient = await em.findOne(Patient, { id: patientId });
  return withEncryptionContext(patient?.organizationId ?? '', fn);
}

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
    const result = await withPatientTenantContext(userId, () =>
      complianceDataService.erase(userId)
    );

    if (result.recordsDeleted === 0) {
      res.status(404).send('User not found or no PII data to erase');
      return;
    }

    res.status(200).json({ ...result });
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
    const result = await withPatientTenantContext(userId, () =>
      complianceDataService.export(userId)
    );

    if (Object.keys(result.entities).length === 0) {
      res.status(404).send('User not found or no PII data to export');
      return;
    }

    res.status(200).json({ ...result });
  }
);

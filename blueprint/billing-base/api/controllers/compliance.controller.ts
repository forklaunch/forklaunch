import {
  handlers,
  PLATFORM_SYSTEM_ROLES,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const complianceDataService = ci.resolve(tokens.ComplianceDataService);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

/**
 * GDPR Right to Erasure — deletes all PII/PHI/PCI data for a user
 * from billing entities.
 */
export const eraseUserData = handlers.delete(
  schemaValidator,
  '/erase/:userId',
  {
    name: 'EraseUserData',
    access: 'protected',
    summary:
      'Erases all PII/PHI/PCI data for a user from billing entities (GDPR Art. 17)',
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
    const result = await complianceDataService.erase(userId);

    if (result.recordsDeleted === 0) {
      res.status(404).send('User not found or no PII data to erase');
      return;
    }

    res.status(200).json(result);
  }
);

/**
 * GDPR Data Portability — exports all PII/PHI/PCI data for a user
 * from billing entities.
 */
export const exportUserData = handlers.get(
  schemaValidator,
  '/export/:userId',
  {
    name: 'ExportUserData',
    access: 'protected',
    summary:
      'Exports all PII/PHI/PCI data for a user from billing entities (GDPR Art. 20)',
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
    const result = await complianceDataService.export(userId);

    if (Object.keys(result.entities).length === 0) {
      res.status(404).send('User not found or no PII data to export');
      return;
    }

    res.status(200).json(result);
  }
);

import { bindRuntimeAuth } from '@forklaunch/blueprint-core';
import { complianceContract } from '../contracts/compliance.contract';
import { handlers, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const complianceDataService = ci.resolve(tokens.ComplianceDataService);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

/**
 * GDPR Right to Erasure — deletes all PII/PHI/PCI data for a user
 * from IAM entities. Called by client-sdk's compliance.erase().
 */
export const eraseUserData = handlers.delete(
  schemaValidator,
  complianceContract.eraseUserData.path,
  bindRuntimeAuth(complianceContract.eraseUserData.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
  }),
  async (req, res) => {
    const { userId } = req.params;
    const result = await complianceDataService.erase(userId);

    if (result.recordsDeleted === 0) {
      res.status(404).send('User not found or no PII data to erase');
      return;
    }

    res.status(200).json({ ...result });
  }
);

/**
 * GDPR Data Portability — exports all PII/PHI/PCI data for a user
 * from IAM entities. Called by client-sdk's compliance.export().
 */
export const exportUserData = handlers.get(
  schemaValidator,
  complianceContract.exportUserData.path,
  bindRuntimeAuth(complianceContract.exportUserData.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
  }),
  async (req, res) => {
    const { userId } = req.params;
    const result = await complianceDataService.export(userId);

    if (Object.keys(result.entities).length === 0) {
      res.status(404).send('User not found or no PII data to export');
      return;
    }

    res.status(200).json({ ...result });
  }
);

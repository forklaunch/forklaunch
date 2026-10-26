import { handlers, bindRuntimeAuth, schemaValidator } from '@{{app_name}}/core';
import { complianceContract } from '../contracts/compliance.contract';
import { ci, tokens } from '../../bootstrapper';

const complianceDataService = ci.resolve(tokens.ComplianceDataService);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

export const eraseUserData = handlers.delete(
  schemaValidator,
  complianceContract.delete.path,
  bindRuntimeAuth(complianceContract.delete.details, { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL }),
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

export const exportUserData = handlers.get(
  schemaValidator,
  complianceContract.get.path,
  bindRuntimeAuth(complianceContract.get.details, { jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL }),
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

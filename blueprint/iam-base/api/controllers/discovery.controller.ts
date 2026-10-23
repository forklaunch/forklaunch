import { discoveryContract } from '../contracts/discovery.contract';
import { handlers, schemaValidator } from '@forklaunch/blueprint-core';

/**
 * GET /discovery/auth-methods
 * Returns the configured authentication methods for this IAM instance (public, no auth required)
 * Vanilla IAM has no third-party auth — returns 204 No Content.
 */
export const getAuthMethods = handlers.get(
  schemaValidator,
  discoveryContract.getAuthMethods.path,
  discoveryContract.getAuthMethods.details,
  async (_req, res) => {
    res.status(204).send('');
  }
);

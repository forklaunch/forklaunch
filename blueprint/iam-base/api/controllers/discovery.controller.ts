import { handlers, schemaValidator, string } from '@forklaunch/blueprint-core';

/**
 * GET /discovery/auth-methods
 * Returns the configured authentication methods for this IAM instance (public, no auth required)
 * Vanilla IAM has no third-party auth — returns 204 No Content.
 */
export const getAuthMethods = handlers.get(
  schemaValidator,
  '/auth-methods',
  {
    name: 'GetAuthMethods',
    summary: 'Get configured authentication methods',
    responses: {
      204: string
    }
  },
  async (_req, res) => {
    res.status(204).send('');
  }
);

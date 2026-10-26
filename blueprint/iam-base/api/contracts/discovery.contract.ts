import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
const { string } = schemaValidator;
export const discoveryContract = {
  basePath: '/discovery',
  getAuthMethods: {
    method: 'get',
    path: '/auth-methods',
    operationId: 'discovery.getAuthMethods',
    details: {
      name: 'GetAuthMethods',
      access: 'public',
      summary: 'Get configured authentication methods',
      responses: {
        204: string
      }
    }
  }
} as const;

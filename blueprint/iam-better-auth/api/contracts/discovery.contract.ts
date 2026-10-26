import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
const { string, boolean } = schemaValidator;
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
        200: {
          email: boolean,
          username: boolean,
          magicLink: boolean,
          emailOtp: boolean,
          phoneNumber: boolean,
          passkey: boolean,
          anonymous: boolean,
          sso: boolean,
          genericOAuth: boolean,
          twoFactor: boolean,
          socialProviders: schemaValidator.array(string)
        }
      }
    }
  }
} as const;

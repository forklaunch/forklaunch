import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
const { string } = schemaValidator;
const IdSchema = { id: string };
export const userContract = {
  basePath: '/user',
  surfaceRoles: {
    method: 'get',
    path: '/:id/surface-roles',
    operationId: 'user.surfaceRoles',
    details: {
      name: 'Surface User Roles',
      access: 'internal',
      summary:
        'Retired user-only lookup; returns no roles until the client is upgraded',
      auth: { scheme: 'hmac' },
      responses: {
        200: schemaValidator.array({ name: string }),
        500: string
      },
      params: IdSchema
    }
  },
  surfacePermissions: {
    method: 'get',
    path: '/:id/surface-permissions',
    operationId: 'user.surfacePermissions',
    details: {
      name: 'Surface User Permissions',
      access: 'internal',
      summary:
        'Retired user-only lookup; returns no permissions until the client is upgraded',
      auth: { scheme: 'hmac' },
      responses: {
        200: schemaValidator.array({ slug: string }),
        500: string
      },
      params: IdSchema
    }
  },
  surfaceScopedRoles: {
    method: 'get',
    path: '/:id/organizations/:organizationId/sessions/:sessionId/surface-roles',
    operationId: 'user.surfaceScopedRoles',
    details: {
      name: 'Surface Scoped User Roles',
      access: 'internal',
      summary:
        'Surface current membership for the exact authenticated session and organization',
      auth: { scheme: 'hmac' },
      responses: { 200: schemaValidator.array({ name: string }), 500: string },
      params: { ...IdSchema, organizationId: string, sessionId: string }
    }
  },
  surfaceScopedPermissions: {
    method: 'get',
    path: '/:id/organizations/:organizationId/sessions/:sessionId/surface-permissions',
    operationId: 'user.surfaceScopedPermissions',
    details: {
      name: 'Surface Scoped User Permissions',
      access: 'internal',
      summary:
        'Surface current permissions for the exact authenticated session and organization',
      auth: { scheme: 'hmac' },
      responses: { 200: schemaValidator.array({ slug: string }), 500: string },
      params: { ...IdSchema, organizationId: string, sessionId: string }
    }
  }
} as const;

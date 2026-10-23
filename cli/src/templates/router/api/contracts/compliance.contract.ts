import { SchemaValidator } from '@forklaunch/validator/{{validator}}';

const schema = SchemaValidator();
export const complianceContract = {
  basePath: '/compliance',
  delete: {
    method: 'delete',
    path: '/erase/:userId',
    operationId: 'compliance.eraseUserData',
    details: {
      name: 'EraseUserData',
      access: 'protected',
      summary: 'Erases all PII/PHI/PCI data for a user (GDPR Art. 17)',
      auth: { scheme: 'jwt', allowedRoles: ['system'] },
      params: { userId: schema.string },
      responses: {
        200: { entitiesAffected: schema.array(schema.string), recordsDeleted: schema.number },
        404: schema.string
      }
    }
  },
  get: {
    method: 'get',
    path: '/export/:userId',
    operationId: 'compliance.exportUserData',
    details: {
      name: 'ExportUserData',
      access: 'protected',
      summary: 'Exports all PII/PHI/PCI data for a user (GDPR Art. 20)',
      auth: { scheme: 'jwt', allowedRoles: ['system'] },
      params: { userId: schema.string },
      responses: {
        200: { userId: schema.string, entities: schema.record(schema.string, schema.unknown) },
        404: schema.string
      }
    }
  }
} as const;

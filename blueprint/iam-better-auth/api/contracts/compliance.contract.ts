import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
const { string } = schemaValidator;
export const complianceContract = {
  basePath: '/compliance',
  eraseUserData: {
    method: 'delete',
    path: '/erase/:userId',
    operationId: 'compliance.eraseUserData',
    details: {
      name: 'EraseUserData',
      access: 'protected',
      summary:
        'Erases all PII/PHI/PCI data for a user from IAM entities (GDPR Art. 17)',
      auth: {
        scheme: 'jwt',
        allowedRoles: ['system']
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
    }
  },
  exportUserData: {
    method: 'get',
    path: '/export/:userId',
    operationId: 'compliance.exportUserData',
    details: {
      name: 'ExportUserData',
      access: 'protected',
      summary:
        'Exports all PII/PHI/PCI data for a user from IAM entities (GDPR Art. 20)',
      auth: {
        scheme: 'jwt',
        allowedRoles: ['system']
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
    }
  }
} as const;

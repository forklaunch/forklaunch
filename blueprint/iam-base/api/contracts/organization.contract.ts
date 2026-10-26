import { OrganizationStatus } from '../../domain/enum/organizationStatus.enum';
import { OrganizationSchemas } from '../../domain/schemas';
import { SchemaValidator } from '@forklaunch/validator/typebox';
const schemaValidator = SchemaValidator();
const { string } = schemaValidator;
const IdSchema = { id: string };
export const organizationContract = {
  basePath: '/organization',
  createOrganization: {
    method: 'post',
    path: '/',
    operationId: 'organization.createOrganization',
    details: {
      name: 'Create Organization',
      access: 'internal',
      summary: 'Creates a new organization',
      auth: { scheme: 'hmac' },
      body: OrganizationSchemas.CreateOrganizationSchema,
      responses: {
        201: OrganizationSchemas.OrganizationSchema(OrganizationStatus),
        409: string
      }
    }
  },
  getOrganization: {
    method: 'get',
    path: '/:id',
    operationId: 'organization.getOrganization',
    details: {
      name: 'Get Organization',
      access: 'protected',
      summary: 'Gets an organization by ID',
      auth: {
        scheme: 'jwt',
        allowedRoles: ['system', 'admin']
      },
      responses: {
        200: OrganizationSchemas.OrganizationSchema(OrganizationStatus),
        404: string
      },
      params: IdSchema
    }
  },
  updateOrganization: {
    method: 'put',
    path: '/',
    operationId: 'organization.updateOrganization',
    details: {
      name: 'Update Organization',
      access: 'internal',
      summary: 'Updates an organization by ID',
      auth: { scheme: 'hmac' },
      body: OrganizationSchemas.UpdateOrganizationSchema,
      responses: {
        200: OrganizationSchemas.OrganizationSchema(OrganizationStatus),
        404: string
      }
    }
  },
  deleteOrganization: {
    method: 'delete',
    path: '/:id',
    operationId: 'organization.deleteOrganization',
    details: {
      name: 'Delete Organization',
      access: 'internal',
      summary: 'Deletes an organization by ID',
      auth: { scheme: 'hmac' },
      responses: {
        200: string,
        404: string
      },
      params: IdSchema
    }
  }
} as const;

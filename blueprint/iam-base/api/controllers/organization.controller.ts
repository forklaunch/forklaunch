import {
  handlers,
  IdSchema,
  PLATFORM_ADMIN_ROLES,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { UniqueConstraintViolationException } from '@mikro-orm/core';
import { ci, tokens } from '../../bootstrapper';
import {
  CreateOrganizationMapper,
  OrganizationMapper,
  UpdateOrganizationMapper
} from '../../domain/mappers/organization.mappers';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const serviceFactory = ci.scopedResolver(tokens.OrganizationService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

export const createOrganization = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Organization',
    access: 'internal',
    summary: 'Creates a new organization',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: CreateOrganizationMapper.schema,
    responses: {
      201: OrganizationMapper.schema,
      409: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating organization', req.body);
    try {
      res.status(201).json(await serviceFactory().createOrganization(req.body));
    } catch (error: unknown) {
      if (error instanceof UniqueConstraintViolationException) {
        openTelemetryCollector.error('Organization already exists', req.body);
        res.status(409).send('Organization already exists');
      } else {
        throw error;
      }
    }
  }
);
export const getOrganization = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Organization',
    access: 'protected',
    summary: 'Gets an organization by ID',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: PLATFORM_ADMIN_ROLES
    },
    responses: {
      200: OrganizationMapper.schema,
      404: string
    },
    params: IdSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving organization', req.params);
    const organizationDto = await serviceFactory().getOrganization(req.params);
    if (organizationDto) {
      res.status(200).json(organizationDto);
    } else {
      openTelemetryCollector.error('Organization not found', req.params);
      res.status(404).send('Organization not found');
    }
  }
);

export const updateOrganization = handlers.put(
  schemaValidator,
  '/',
  {
    name: 'Update Organization',
    access: 'internal',
    summary: 'Updates an organization by ID',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: UpdateOrganizationMapper.schema,
    responses: {
      200: OrganizationMapper.schema,
      404: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating organization', req.body);
    res.status(200).json(await serviceFactory().updateOrganization(req.body));
  }
);

export const deleteOrganization = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Organization',
    access: 'internal',
    summary: 'Deletes an organization by ID',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: string,
      404: string
    },
    params: IdSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Deleting organization', req.params);
    await serviceFactory().deleteOrganization(req.params);
    res.status(200).send('Organization deleted successfully');
  }
);

import {
  array,
  handlers,
  IdSchema,
  IdsSchema,
  PLATFORM_READ_PERMISSIONS,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  CreateRoleMapper,
  RoleMapper,
  UpdateRoleMapper
} from '../../domain/mappers/role.mappers';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const serviceFactory = ci.scopedResolver(tokens.RoleService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

export const createRole = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Role',
    access: 'internal',
    summary: 'Creates a new role',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: CreateRoleMapper.schema,
    responses: {
      201: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating role', req.body);
    await serviceFactory().createRole(req.body);
    res.status(201).send('Role created successfully');
  }
);

export const createBatchRoles = handlers.post(
  schemaValidator,
  '/batch',
  {
    name: 'Create Batch Roles',
    access: 'internal',
    summary: 'Creates multiple roles',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: array(CreateRoleMapper.schema),
    responses: {
      201: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating batch roles', req.body);
    await serviceFactory().createBatchRoles(req.body);
    res.status(201).send('Batch roles created successfully');
  }
);

export const getRole = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Role',
    access: 'protected',
    summary: 'Gets a role by ID',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedPermissions: PLATFORM_READ_PERMISSIONS
    },
    responses: {
      200: RoleMapper.schema,
      500: string
    },
    params: IdSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving role', req.params);
    res.status(200).json(await serviceFactory().getRole(req.params));
  }
);

export const getBatchRoles = handlers.get(
  schemaValidator,
  '/batch',
  {
    name: 'Get Batch Roles',
    access: 'protected',
    summary: 'Gets multiple roles by IDs',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedPermissions: PLATFORM_READ_PERMISSIONS
    },
    responses: {
      200: array(RoleMapper.schema),
      500: string
    },
    query: IdsSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving batch roles', req.query);
    res.status(200).json(await serviceFactory().getBatchRoles(req.query));
  }
);

export const updateRole = handlers.put(
  schemaValidator,
  '/',
  {
    name: 'Update Role',
    access: 'internal',
    summary: 'Updates a role by ID',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: UpdateRoleMapper.schema,
    responses: {
      200: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating role', req.body);
    await serviceFactory().updateRole(req.body);
    res.status(200).send('Role updated successfully');
  }
);

export const updateBatchRoles = handlers.put(
  schemaValidator,
  '/batch',
  {
    name: 'Update Batch Roles',
    access: 'internal',
    summary: 'Updates multiple roles by IDs',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: array(UpdateRoleMapper.schema),
    responses: {
      200: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating batch roles', req.body);
    await serviceFactory().updateBatchRoles(req.body);
    res.status(200).send('Batch roles updated successfully');
  }
);

export const deleteRole = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Role',
    access: 'internal',
    summary: 'Deletes a role by ID',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: string,
      500: string
    },
    params: IdSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Deleting role', req.params);
    await serviceFactory().deleteRole(req.params);
    res.status(200).send('Role deleted successfully');
  }
);

export const deleteBatchRoles = handlers.delete(
  schemaValidator,
  '/batch',
  {
    name: 'Delete Batch Roles',
    access: 'internal',
    summary: 'Deletes multiple roles by IDs',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: string,
      500: string
    },
    query: IdsSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Deleting batch roles', req.query);
    await serviceFactory().deleteBatchRoles(req.query);
    res.status(200).send('Batch roles deleted successfully');
  }
);

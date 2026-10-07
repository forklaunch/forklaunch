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
  CreatePermissionMapper,
  PermissionMapper,
  UpdatePermissionMapper
} from '../../domain/mappers/permission.mappers';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.PermissionService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

export const createPermission = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Permission',
    access: 'internal',
    summary: 'Creates a new permission',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: CreatePermissionMapper.schema,
    responses: {
      201: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating permission', req.body);
    await serviceFactory().createPermission(req.body);
    res.status(201).send('Permission created successfully');
  }
);

export const createBatchPermissions = handlers.post(
  schemaValidator,
  '/batch',
  {
    name: 'Create Batch Permissions',
    access: 'internal',
    summary: 'Creates multiple permissions',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: array(CreatePermissionMapper.schema),
    responses: {
      201: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating batch permissions', req.body);
    await serviceFactory().createBatchPermissions(req.body);
    res.status(201).send('Batch permissions created successfully');
  }
);

export const getPermission = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Permission',
    access: 'protected',
    summary: 'Gets a permission by ID',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedPermissions: PLATFORM_READ_PERMISSIONS
    },
    responses: {
      200: PermissionMapper.schema,
      500: string
    },
    params: IdSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving permission', req.params);
    res.status(200).json(await serviceFactory().getPermission(req.params));
  }
);

export const getBatchPermissions = handlers.get(
  schemaValidator,
  '/batch',
  {
    name: 'Get Batch Permissions',
    access: 'protected',
    summary: 'Gets multiple permissions by IDs',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedPermissions: PLATFORM_READ_PERMISSIONS
    },
    responses: {
      200: array(PermissionMapper.schema),
      500: string
    },
    query: IdsSchema
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving batch permissions', req.query);
    res.status(200).json(await serviceFactory().getBatchPermissions(req.query));
  }
);

export const updatePermission = handlers.put(
  schemaValidator,
  '/',
  {
    name: 'Update Permission',
    access: 'internal',
    summary: 'Updates a permission by ID',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: UpdatePermissionMapper.schema,
    responses: {
      200: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating permission', req.body);
    await serviceFactory().updatePermission(req.body);
    res.status(200).send('Permission updated successfully');
  }
);

export const updateBatchPermissions = handlers.put(
  schemaValidator,
  '/batch',
  {
    name: 'Update Batch Permissions',
    access: 'internal',
    summary: 'Updates multiple permissions by IDs',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: array(UpdatePermissionMapper.schema),
    responses: {
      200: string,
      500: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating batch permissions', req.body);
    await serviceFactory().updateBatchPermissions(req.body);
    res.status(200).send('Batch permissions updated successfully');
  }
);

export const deletePermission = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Permission',
    access: 'internal',
    summary: 'Deletes a permission by ID',
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
    openTelemetryCollector.debug('Deleting permission', req.params);
    await serviceFactory().deletePermission(req.params);
    res.status(200).send('Permission deleted successfully');
  }
);

export const deleteBatchPermissions = handlers.delete(
  schemaValidator,
  '/batch',
  {
    name: 'Delete Batch Permissions',
    access: 'internal',
    summary: 'Deletes multiple permissions by IDs',
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
    openTelemetryCollector.debug('Deleting batch permissions', req.query);
    await serviceFactory().deleteBatchPermissions(req.query);
    res.status(200).send('Batch permissions deleted successfully');
  }
);

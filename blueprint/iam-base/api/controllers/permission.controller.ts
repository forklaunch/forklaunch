import { bindRuntimeAuth } from '@forklaunch/blueprint-core';
import { permissionContract } from '../contracts/permission.contract';
import { handlers, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.PermissionService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

export const createPermission = handlers.post(
  schemaValidator,
  permissionContract.createPermission.path,
  bindRuntimeAuth(permissionContract.createPermission.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Creating permission', req.body);
    await serviceFactory().createPermission(req.body);
    res.status(201).send('Permission created successfully');
  }
);

export const createBatchPermissions = handlers.post(
  schemaValidator,
  permissionContract.createBatchPermissions.path,
  bindRuntimeAuth(permissionContract.createBatchPermissions.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Creating batch permissions', req.body);
    await serviceFactory().createBatchPermissions(req.body);
    res.status(201).send('Batch permissions created successfully');
  }
);

export const getPermission = handlers.get(
  schemaValidator,
  permissionContract.getPermission.path,
  bindRuntimeAuth(permissionContract.getPermission.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving permission', req.params);
    res.status(200).json(await serviceFactory().getPermission(req.params));
  }
);

export const getBatchPermissions = handlers.get(
  schemaValidator,
  permissionContract.getBatchPermissions.path,
  bindRuntimeAuth(permissionContract.getBatchPermissions.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving batch permissions', req.query);
    res.status(200).json(await serviceFactory().getBatchPermissions(req.query));
  }
);

export const updatePermission = handlers.put(
  schemaValidator,
  permissionContract.updatePermission.path,
  bindRuntimeAuth(permissionContract.updatePermission.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Updating permission', req.body);
    await serviceFactory().updatePermission(req.body);
    res.status(200).send('Permission updated successfully');
  }
);

export const updateBatchPermissions = handlers.put(
  schemaValidator,
  permissionContract.updateBatchPermissions.path,
  bindRuntimeAuth(permissionContract.updateBatchPermissions.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Updating batch permissions', req.body);
    await serviceFactory().updateBatchPermissions(req.body);
    res.status(200).send('Batch permissions updated successfully');
  }
);

export const deletePermission = handlers.delete(
  schemaValidator,
  permissionContract.deletePermission.path,
  bindRuntimeAuth(permissionContract.deletePermission.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Deleting permission', req.params);
    await serviceFactory().deletePermission(req.params);
    res.status(200).send('Permission deleted successfully');
  }
);

export const deleteBatchPermissions = handlers.delete(
  schemaValidator,
  permissionContract.deleteBatchPermissions.path,
  bindRuntimeAuth(permissionContract.deleteBatchPermissions.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Deleting batch permissions', req.query);
    await serviceFactory().deleteBatchPermissions(req.query);
    res.status(200).send('Batch permissions deleted successfully');
  }
);

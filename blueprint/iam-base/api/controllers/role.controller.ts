import { bindRuntimeAuth } from '@forklaunch/blueprint-core';
import { roleContract } from '../contracts/role.contract';
import { handlers, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.RoleService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

export const createRole = handlers.post(
  schemaValidator,
  roleContract.createRole.path,
  bindRuntimeAuth(roleContract.createRole.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Creating role', req.body);
    await serviceFactory().createRole(req.body);
    res.status(201).send('Role created successfully');
  }
);

export const createBatchRoles = handlers.post(
  schemaValidator,
  roleContract.createBatchRoles.path,
  bindRuntimeAuth(roleContract.createBatchRoles.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Creating batch roles', req.body);
    await serviceFactory().createBatchRoles(req.body);
    res.status(201).send('Batch roles created successfully');
  }
);

export const getRole = handlers.get(
  schemaValidator,
  roleContract.getRole.path,
  bindRuntimeAuth(roleContract.getRole.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving role', req.params);
    res.status(200).json(await serviceFactory().getRole(req.params));
  }
);

export const getBatchRoles = handlers.get(
  schemaValidator,
  roleContract.getBatchRoles.path,
  bindRuntimeAuth(roleContract.getBatchRoles.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving batch roles', req.query);
    res.status(200).json(await serviceFactory().getBatchRoles(req.query));
  }
);

export const updateRole = handlers.put(
  schemaValidator,
  roleContract.updateRole.path,
  bindRuntimeAuth(roleContract.updateRole.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Updating role', req.body);
    await serviceFactory().updateRole(req.body);
    res.status(200).send('Role updated successfully');
  }
);

export const updateBatchRoles = handlers.put(
  schemaValidator,
  roleContract.updateBatchRoles.path,
  bindRuntimeAuth(roleContract.updateBatchRoles.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Updating batch roles', req.body);
    await serviceFactory().updateBatchRoles(req.body);
    res.status(200).send('Batch roles updated successfully');
  }
);

export const deleteRole = handlers.delete(
  schemaValidator,
  roleContract.deleteRole.path,
  bindRuntimeAuth(roleContract.deleteRole.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Deleting role', req.params);
    await serviceFactory().deleteRole(req.params);
    res.status(200).send('Role deleted successfully');
  }
);

export const deleteBatchRoles = handlers.delete(
  schemaValidator,
  roleContract.deleteBatchRoles.path,
  bindRuntimeAuth(roleContract.deleteBatchRoles.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Deleting batch roles', req.query);
    await serviceFactory().deleteBatchRoles(req.query);
    res.status(200).send('Batch roles deleted successfully');
  }
);

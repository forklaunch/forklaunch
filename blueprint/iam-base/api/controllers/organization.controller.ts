import { bindRuntimeAuth } from '@forklaunch/blueprint-core';
import { organizationContract } from '../contracts/organization.contract';
import { handlers, schemaValidator } from '@forklaunch/blueprint-core';
import { UniqueConstraintViolationException } from '@mikro-orm/core';
import { ci, tokens } from '../../bootstrapper';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.OrganizationService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

export const createOrganization = handlers.post(
  schemaValidator,
  organizationContract.createOrganization.path,
  bindRuntimeAuth(organizationContract.createOrganization.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
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
  organizationContract.getOrganization.path,
  bindRuntimeAuth(organizationContract.getOrganization.details, {
    jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
  }),
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
  organizationContract.updateOrganization.path,
  bindRuntimeAuth(organizationContract.updateOrganization.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Updating organization', req.body);
    res.status(200).json(await serviceFactory().updateOrganization(req.body));
  }
);

export const deleteOrganization = handlers.delete(
  schemaValidator,
  organizationContract.deleteOrganization.path,
  bindRuntimeAuth(organizationContract.deleteOrganization.details, {
    secretKeys: { default: HMAC_SECRET_KEY }
  }),
  async (req, res) => {
    openTelemetryCollector.debug('Deleting organization', req.params);
    await serviceFactory().deleteOrganization(req.params);
    res.status(200).send('Organization deleted successfully');
  }
);

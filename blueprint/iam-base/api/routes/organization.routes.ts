import { organizationContract } from '../contracts/organization.contract';
import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  createOrganization,
  deleteOrganization,
  getOrganization,
  updateOrganization
} from '../controllers/organization.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const organizationRouter = forklaunchRouter(
  organizationContract.basePath,
  schemaValidator,
  openTelemetryCollector
);

export const createOrganizationRoute = organizationRouter.post(
  organizationContract.createOrganization.path,
  createOrganization
);
export const getOrganizationRoute = organizationRouter.get(
  organizationContract.getOrganization.path,
  getOrganization
);
export const updateOrganizationRoute = organizationRouter.put(
  organizationContract.updateOrganization.path,
  updateOrganization
);
export const deleteOrganizationRoute = organizationRouter.delete(
  organizationContract.deleteOrganization.path,
  deleteOrganization
);

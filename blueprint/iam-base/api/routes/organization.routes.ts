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
  '/organization',
  schemaValidator,
  openTelemetryCollector
);

export const createOrganizationRoute = organizationRouter.post(
  '/',
  createOrganization
);
export const getOrganizationRoute = organizationRouter.get(
  '/:id',
  getOrganization
);
export const updateOrganizationRoute = organizationRouter.put(
  '/',
  updateOrganization
);
export const deleteOrganizationRoute = organizationRouter.delete(
  '/:id',
  deleteOrganization
);

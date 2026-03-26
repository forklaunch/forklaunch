import {
  handlers,
  IdSchema,
  ROLES,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  BillingPortalMapper,
  CreateBillingPortalMapper,
  UpdateBillingPortalMapper
} from '../../domain/mappers/billingPortal.mappers';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const serviceFactory = ci.scopedResolver(tokens.BillingPortalService);
const JWKS_PUBLIC_KEY_URL = ci.resolve(tokens.JWKS_PUBLIC_KEY_URL);

export const createBillingPortalSession = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Billing Portal Session',
    access: 'protected',
    summary: 'Create a billing portal session',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: new Set([ROLES.ADMIN])
    },
    body: CreateBillingPortalMapper.schema,
    responses: {
      200: BillingPortalMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating billing portal session', req.body);
    res
      .status(200)
      .json(await serviceFactory().createBillingPortalSession(req.body));
  }
);

export const getBillingPortalSession = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Billing Portal Session',
    access: 'protected',
    summary: 'Get a billing portal session',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: new Set([ROLES.ADMIN])
    },
    params: IdSchema,
    responses: {
      200: BillingPortalMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug(
      'Retrieving billing portal session',
      req.params
    );
    res
      .status(200)
      .json(await serviceFactory().getBillingPortalSession(req.params));
  }
);

export const updateBillingPortalSession = handlers.put(
  schemaValidator,
  '/:id',
  {
    name: 'Update Billing Portal Session',
    access: 'protected',
    summary: 'Update a billing portal session',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: new Set([ROLES.ADMIN])
    },
    params: IdSchema,
    body: UpdateBillingPortalMapper.schema,
    responses: {
      200: BillingPortalMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating billing portal session', {
      ...req.params,
      ...req.body
    });
    res.status(200).json(
      await serviceFactory().updateBillingPortalSession({
        ...req.params,
        ...req.body
      })
    );
  }
);

export const expireBillingPortalSession = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Expire Billing Portal Session',
    access: 'protected',
    summary: 'Expire a billing portal session',
    auth: {
      jwt: {
        jwksPublicKeyUrl: JWKS_PUBLIC_KEY_URL
      },
      allowedRoles: new Set([ROLES.ADMIN])
    },
    params: IdSchema,
    responses: {
      200: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Expiring billing portal session', req.params);
    await serviceFactory().expireBillingPortalSession(req.params);
    res.status(200).send(`Expired billing portal session ${req.params.id}`);
  }
);

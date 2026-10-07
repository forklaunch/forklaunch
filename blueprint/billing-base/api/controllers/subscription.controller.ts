import {
  array,
  handlers,
  IdSchema,
  optional,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { IdsDto } from '@forklaunch/common';
import { ci, tokens } from '../../bootstrapper';
import {
  CreateSubscriptionMapper,
  SubscriptionMapper,
  UpdateSubscriptionMapper
} from '../../domain/mappers/subscription.mappers';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);
const serviceFactory = ci.scopedResolver(tokens.SubscriptionService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const createSubscription = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Subscription',
    access: 'internal',
    summary: 'Create a subscription',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: CreateSubscriptionMapper.schema,
    responses: {
      200: SubscriptionMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating subscription', req.body);
    res.status(200).json(await serviceFactory().createSubscription(req.body));
  }
);

export const getSubscription = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Subscription',
    access: 'internal',
    summary: 'Get a subscription',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,
    responses: {
      200: SubscriptionMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving subscription', req.params);
    res.status(200).json(await serviceFactory().getSubscription(req.params));
  }
);

export const getUserSubscription = handlers.get(
  schemaValidator,
  '/user/:id',
  {
    name: 'Get User Subscription',
    access: 'internal',
    summary: 'Get a user subscription',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,
    responses: {
      200: SubscriptionMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving user subscription', req.params);
    res
      .status(200)
      .json(await serviceFactory().getUserSubscription(req.params));
  }
);

export const getOrganizationSubscription = handlers.get(
  schemaValidator,
  '/organization/:id',
  {
    name: 'Get Organization Subscription',
    access: 'internal',
    summary: 'Get an organization subscription',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,
    responses: {
      200: SubscriptionMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug(
      'Retrieving organization subscription',
      req.params
    );
    res
      .status(200)
      .json(await serviceFactory().getOrganizationSubscription(req.params));
  }
);

export const updateSubscription = handlers.put(
  schemaValidator,
  '/:id',
  {
    name: 'Update Subscription',
    access: 'internal',
    summary: 'Update a subscription',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,
    body: UpdateSubscriptionMapper.schema,
    responses: {
      200: SubscriptionMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Update subscription', {
      ...req.params,
      ...req.body
    });
    res.status(200).json(
      await serviceFactory().updateSubscription({
        ...req.params,
        ...req.body
      })
    );
  }
);

export const deleteSubscription = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Subscription',
    access: 'internal',
    summary: 'Delete a subscription',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,
    responses: {
      200: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Deleting subscription', req.params);
    await serviceFactory().deleteSubscription(req.params);
    res.status(200).send(`Deleted subscription ${req.params.id}`);
  }
);

export const listSubscriptions = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Subscriptions',
    access: 'internal',
    summary: 'List subscriptions',
    query: {
      ids: optional(array(string))
    },
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: array(SubscriptionMapper.schema)
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Listing subscriptions', req.query);
    res
      .status(200)
      .json(await serviceFactory().listSubscriptions(req.query as IdsDto));
  }
);

export const cancelSubscription = handlers.get(
  schemaValidator,
  '/:id/cancel',
  {
    name: 'Cancel Subscription',
    access: 'internal',
    summary: 'Cancel a subscription',
    params: IdSchema,
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Cancelling subscription', req.params);
    await serviceFactory().cancelSubscription(req.params);
    res.status(200).send(`Cancelled subscription ${req.params.id}`);
  }
);

export const resumeSubscription = handlers.get(
  schemaValidator,
  '/:id/resume',
  {
    name: 'Resume Subscription',
    access: 'internal',
    summary: 'Resume a subscription',
    params: IdSchema,
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: string
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Resuming subscription', req.params);
    await serviceFactory().resumeSubscription(req.params);
    res.status(200).send(`Resumed subscription ${req.params.id}`);
  }
);

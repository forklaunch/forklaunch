import {
  array,
  handlers,
  IdSchema,
  IdsSchema,
  schemaValidator,
  string
} from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import { BillingProviderEnum } from '../../domain/enum/billingProvider.enum';
import { CurrencyEnum } from '../../domain/enum/currency.enum';
import { PlanCadenceEnum } from '../../domain/enum/planCadence.enum';
import {
  CreatePlanMapper,
  PlanMapper,
  UpdatePlanMapper
} from '../../domain/mappers/plan.mappers';
import { PlanSchemas } from '../../domain/schemas';

const openTelemetryCollector = ci.resolve(tokens.OpenTelemetryCollector);
const serviceFactory = ci.scopedResolver(tokens.PlanService);
const HMAC_SECRET_KEY = ci.resolve(tokens.HMAC_SECRET_KEY);

export const createPlan = handlers.post(
  schemaValidator,
  '/',
  {
    name: 'Create Plan',
    access: 'internal',
    summary: 'Create a plan',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    body: CreatePlanMapper.schema,
    responses: {
      200: PlanMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Creating plan', req.body);
    res.status(200).json(await serviceFactory().createPlan(req.body));
  }
);

export const getPlan = handlers.get(
  schemaValidator,
  '/:id',
  {
    name: 'Get Plan',
    access: 'internal',
    summary: 'Get a plan',
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    params: IdSchema,
    responses: {
      200: PlanSchemas.PlanSchema(
        PlanCadenceEnum,
        CurrencyEnum,
        BillingProviderEnum
      )
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Retrieving plan', req.params);
    res.status(200).json(await serviceFactory().getPlan(req.params));
  }
);

export const updatePlan = handlers.put(
  schemaValidator,
  '/',
  {
    name: 'Update Plan',
    access: 'internal',
    summary: 'Update a plan',
    body: UpdatePlanMapper.schema,
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: PlanMapper.schema
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Updating plan', req.body);
    res.status(200).json(await serviceFactory().updatePlan(req.body));
  }
);

export const deletePlan = handlers.delete(
  schemaValidator,
  '/:id',
  {
    name: 'Delete Plan',
    access: 'internal',
    summary: 'Delete a plan',
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
    openTelemetryCollector.debug('Deleting plan', req.params);
    await serviceFactory().deletePlan(req.params);
    res.status(200).send(`Deleted plan ${req.params.id}`);
  }
);

export const listPlans = handlers.get(
  schemaValidator,
  '/',
  {
    name: 'List Plans',
    access: 'internal',
    summary: 'List plans',
    query: IdsSchema,
    auth: {
      hmac: {
        secretKeys: {
          default: HMAC_SECRET_KEY
        }
      }
    },
    responses: {
      200: array(
        PlanSchemas.PlanSchema(
          PlanCadenceEnum,
          CurrencyEnum,
          BillingProviderEnum
        )
      )
    }
  },
  async (req, res) => {
    openTelemetryCollector.debug('Listing plans', req.query);
    res.status(200).json(await serviceFactory().listPlans(req.query));
  }
);

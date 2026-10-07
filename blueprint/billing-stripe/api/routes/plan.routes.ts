import { forklaunchRouter, schemaValidator } from '@forklaunch/blueprint-core';
import { ci, tokens } from '../../bootstrapper';
import {
  createPlan,
  deletePlan,
  getPlan,
  listPlans,
  updatePlan
} from '../controllers/plan.controller';

const openTelemetryCollector = ci.resolve(tokens.OtelCollector);

export const planRouter = forklaunchRouter(
  '/plan',
  schemaValidator,
  openTelemetryCollector
);

export const createPlanRoute = planRouter.post('/', createPlan);
export const listPlansRoute = planRouter.get('/', listPlans);
export const getPlanRoute = planRouter.get('/:id', getPlan);
export const updatePlanRoute = planRouter.put('/', updatePlan);
export const deletePlanRoute = planRouter.delete('/:id', deletePlan);

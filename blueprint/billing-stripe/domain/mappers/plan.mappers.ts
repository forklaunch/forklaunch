import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import Stripe from 'stripe';
import { Plan, type IPlan } from '../../persistence/entities/plan.entity';
import { PlanSchemas } from '../schemas';

export const CreatePlanMapper = requestMapper({
  schemaValidator,
  schema: PlanSchemas.CreatePlanSchema,
  entity: Plan,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Product
    ) => {
      return em.create(Plan, {
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date(),
        providerFields
      });
    }
  }
});

export const UpdatePlanMapper = requestMapper({
  schemaValidator,
  schema: PlanSchemas.UpdatePlanSchema,
  entity: Plan,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Product
    ) => {
      const entity = await em.findOneOrFail(Plan, { id: dto.id });
      em.assign(entity, {
        ...dto,
        providerFields,
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const PlanMapper = responseMapper({
  schemaValidator,
  schema: PlanSchemas.PlanSchema,
  entity: Plan,
  mapperDefinition: {
    toDto: async (entity: IPlan) => {
      const baseData = wrap(entity).toPOJO();
      return {
        ...baseData,
        stripeFields: entity.providerFields
      };
    }
  }
});

import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, InferEntity, wrap } from '@mikro-orm/core';
import Stripe from 'stripe';
import { Plan } from '../../persistence/entities/plan.entity';
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
        name: dto.name,
        description: dto.description || null,
        price: dto.price,
        cadence: dto.cadence,
        currency: dto.currency,
        features: dto.features || null,
        externalId: dto.externalId,
        billingProvider: dto.billingProvider || null,
        active: dto.active,
        providerFields,
        createdAt: new Date(),
        updatedAt: new Date()
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
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { stripeFields, ...rest } = dto;
      const entity = await em.findOneOrFail(Plan, { id: rest.id });
      em.assign(entity, {
        ...rest,
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
    toDto: async (entity: InferEntity<typeof Plan>) => {
      const baseData = wrap(entity).toPOJO();
      return {
        ...baseData,
        price: Number(entity.price),
        description: entity.description ?? undefined,
        features: entity.features ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});

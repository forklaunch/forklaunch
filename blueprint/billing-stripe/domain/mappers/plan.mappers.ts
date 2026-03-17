import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { plan, type Plan } from '../../persistence/entities/plan.entity';
import { PlanSchemas } from '../schemas';

export const CreatePlanMapper = requestMapper({
  schemaValidator,
  schema: PlanSchemas.CreatePlanSchema,
  entity: plan,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Product
    ) => {
      return em.create(plan, {
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
  entity: plan,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Product
    ) => {
      const entity = await em.findOneOrFail(plan, { id: dto.id });
      em.assign(entity, {
        ...(dto.name !== undefined && { name: dto.name }),
        ...(dto.description !== undefined && { description: dto.description }),
        ...(dto.price !== undefined && { price: dto.price }),
        ...(dto.cadence !== undefined && { cadence: dto.cadence }),
        ...(dto.currency !== undefined && { currency: dto.currency }),
        ...(dto.features !== undefined && { features: dto.features }),
        ...(dto.externalId !== undefined && { externalId: dto.externalId }),
        ...(dto.billingProvider !== undefined && {
          billingProvider: dto.billingProvider
        }),
        ...(dto.active !== undefined && { active: dto.active }),
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
  entity: plan,
  mapperDefinition: {
    toDto: async (entity: Plan) => {
      const baseData = wrap(entity).toPOJO();
      return {
        ...baseData,
        stripeFields: entity.providerFields
      };
    }
  }
});

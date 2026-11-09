import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { Plan } from '../../persistence/entities/plan.entity';
import { PlanSchemas } from '../schemas';
import {
  validateProviderCreation,
  validateProviderReference
} from './providerCreation';

const CreatePlanEntityMapper = requestMapper({
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
        description: dto.description ?? null,
        price: dto.price,
        cadence: dto.cadence,
        currency: dto.currency,
        features: dto.features ?? null,
        externalId: dto.externalId,
        billingProvider: dto.billingProvider,
        active: dto.active,
        providerFields
      });
    }
  }
});

const UpdatePlanEntityMapper = requestMapper({
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
        providerFields
      });
      return entity;
    }
  }
});

const PlanEntityMapper = responseMapper({
  schemaValidator,
  schema: PlanSchemas.PlanSchema,
  entity: Plan,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        ...entity,
        price: Number(entity.price),
        description: entity.description ?? undefined,
        features: entity.features ?? undefined,
        billingProvider: entity.billingProvider ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});

export const UpdatePlanMapper = {
  validateProviderUpdate: async (
    _dto: Parameters<typeof UpdatePlanEntityMapper.toEntity>[0],
    provider: Stripe.Plan
  ) => {
    const product = provider.product;
    validateProviderReference(
      typeof product === 'string' ? product : product?.id
    );
  },
  ...UpdatePlanEntityMapper,
  validateUpdate: async (
    dto: Parameters<typeof UpdatePlanEntityMapper.toEntity>[0]
  ) => {
    const patch = schemaValidator.parse(
      schemaValidator.schemify(PlanSchemas.UpdatePlanSchema),
      dto
    );
    if (!patch.ok) {
      throw new Error('Invalid plan update');
    }
  }
};

export const CreatePlanMapper = {
  ...CreatePlanEntityMapper,
  validateProviderCreate: async (provider: Stripe.Plan) => {
    validateProviderCreation(provider, 'plan');
  },
  validateCreate: async (
    dto: Parameters<typeof CreatePlanEntityMapper.toEntity>[0]
  ) => {
    // These fields are replaced with authenticated provider results by the service.
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    const { externalId, billingProvider, ...inputSchema } =
      PlanSchemas.CreatePlanSchema;
    const input = schemaValidator.parse(
      schemaValidator.schemify(inputSchema),
      dto
    );
    if (!input.ok) {
      throw new Error('Invalid plan creation');
    }
  }
};

export const PlanMapper = {
  ...PlanEntityMapper,
  validateProviderRead
};

async function validateProviderRead(
  expectedExternalId: string,
  provider: Stripe.Plan
): Promise<void> {
  if (
    typeof expectedExternalId !== 'string' ||
    !expectedExternalId.trim() ||
    !provider ||
    typeof provider !== 'object' ||
    Array.isArray(provider) ||
    provider.id !== expectedExternalId ||
    provider.object !== 'plan'
  ) {
    throw new Error('Invalid Stripe billing response');
  }
}

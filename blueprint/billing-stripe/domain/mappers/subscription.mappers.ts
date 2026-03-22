import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, InferEntity, wrap } from '@mikro-orm/core';
import Stripe from 'stripe';
import { Subscription } from '../../persistence/entities/subscription.entity';
import { PartyEnum } from '../enum/party.enum';
import { SubscriptionSchemas } from '../schemas';

export const CreateSubscriptionMapper = requestMapper({
  schemaValidator,
  schema: SubscriptionSchemas.CreateSubscriptionSchema(PartyEnum),
  entity: Subscription,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Subscription
    ) => {
      return em.create(Subscription, {
        partyId: dto.partyId,
        partyType: dto.partyType,
        productId: dto.productId,
        description: dto.description || null,
        active: dto.active,
        externalId: dto.externalId,
        startDate: dto.startDate,
        endDate: dto.endDate || null,
        status: dto.status,
        billingProvider: dto.billingProvider || null,
        providerFields,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateSubscriptionMapper = requestMapper({
  schemaValidator,
  schema: SubscriptionSchemas.UpdateSubscriptionSchema(PartyEnum),
  entity: Subscription,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Subscription
    ) => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { stripeFields, ...rest } = dto;
      const entity = await em.findOneOrFail(Subscription, { id: rest.id });
      em.assign(entity, {
        ...rest,
        providerFields,
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const SubscriptionMapper = responseMapper({
  schemaValidator,
  schema: SubscriptionSchemas.SubscriptionSchema(PartyEnum),
  entity: Subscription,
  mapperDefinition: {
    toDto: async (entity: InferEntity<typeof Subscription>) => {
      const data = wrap(entity).toPOJO();
      return {
        ...data,
        description: entity.description ?? undefined,
        endDate: data.endDate ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});

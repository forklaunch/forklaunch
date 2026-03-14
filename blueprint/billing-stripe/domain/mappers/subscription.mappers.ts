import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import Stripe from 'stripe';
import {
  Subscription,
  type ISubscription
} from '../../persistence/entities/subscription.entity';
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
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date(),
        providerFields
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
      const entity = await em.findOneOrFail(Subscription, { id: dto.id });
      em.assign(entity, {
        ...dto,
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
    toDto: async (entity: ISubscription) => {
      const data = wrap(entity).toPOJO();
      return {
        ...data,
        // Convert null endDate to undefined for DTO validation
        endDate: data.endDate ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});

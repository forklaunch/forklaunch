import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import {
  Subscription,
  type ISubscription
} from '../../persistence/entities/subscription.entity';
import { BillingProviderEnum } from '../enum/billingProvider.enum';
import { PartyEnum } from '../enum/party.enum';
import { SubscriptionSchemas } from '../schemas';

export const CreateSubscriptionMapper = requestMapper({
  schemaValidator,
  schema: SubscriptionSchemas.CreateSubscriptionSchema(
    PartyEnum,
    BillingProviderEnum
  ),
  entity: Subscription,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Subscription, {
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateSubscriptionMapper = requestMapper({
  schemaValidator,
  schema: SubscriptionSchemas.UpdateSubscriptionSchema(
    PartyEnum,
    BillingProviderEnum
  ),
  entity: Subscription,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(Subscription, { id: dto.id });
      em.assign(entity, { ...dto, updatedAt: new Date() });
      return entity;
    }
  }
});

export const SubscriptionMapper = responseMapper({
  schemaValidator,
  schema: SubscriptionSchemas.SubscriptionSchema(
    PartyEnum,
    BillingProviderEnum
  ),
  entity: Subscription,
  mapperDefinition: {
    toDto: async (entity: ISubscription) => {
      return {
        ...wrap(entity).toPOJO(),
        endDate: entity.endDate || undefined
      };
    }
  }
});

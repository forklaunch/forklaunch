import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import {
  subscription,
  type Subscription
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
  entity: subscription,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(subscription, {
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
  entity: subscription,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(subscription, { id: dto.id });
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
  entity: subscription,
  mapperDefinition: {
    toDto: async (entity: Subscription) => {
      return {
        ...wrap(entity).toPOJO(),
        endDate: entity.endDate || undefined
      };
    }
  }
});

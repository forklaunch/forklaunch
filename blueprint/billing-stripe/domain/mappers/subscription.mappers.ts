import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import {
  subscription,
  type Subscription
} from '../../persistence/entities/subscription.entity';
import { PartyEnum } from '../enum/party.enum';
import { SubscriptionSchemas } from '../schemas';

export const CreateSubscriptionMapper = requestMapper({
  schemaValidator,
  schema: SubscriptionSchemas.CreateSubscriptionSchema(PartyEnum),
  entity: subscription,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Subscription
    ) => {
      return em.create(subscription, {
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
  entity: subscription,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Subscription
    ) => {
      const entity = await em.findOneOrFail(subscription, { id: dto.id });
      em.assign(entity, {
        ...(dto.partyId !== undefined && { partyId: dto.partyId }),
        ...(dto.partyType !== undefined && { partyType: dto.partyType }),
        ...(dto.productId !== undefined && { productId: dto.productId }),
        ...(dto.description !== undefined && { description: dto.description }),
        ...(dto.active !== undefined && { active: dto.active }),
        ...(dto.externalId !== undefined && { externalId: dto.externalId }),
        ...(dto.startDate !== undefined && { startDate: dto.startDate }),
        ...(dto.endDate !== undefined && { endDate: dto.endDate }),
        ...(dto.status !== undefined && { status: dto.status }),
        ...(dto.billingProvider !== undefined && {
          billingProvider: dto.billingProvider
        }),
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
  entity: subscription,
  mapperDefinition: {
    toDto: async (entity: Subscription) => {
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

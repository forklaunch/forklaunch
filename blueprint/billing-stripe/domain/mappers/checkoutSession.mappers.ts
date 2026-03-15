import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import Stripe from 'stripe';
import {
  CheckoutSession,
  type ICheckoutSession
} from '../../persistence/entities/checkoutSession.entity';
import { StatusEnum } from '../enum/status.enum';
import { CheckoutSessionSchemas } from '../schemas';

export const CreateCheckoutSessionMapper = requestMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.CreateCheckoutSessionSchema(StatusEnum),
  entity: CheckoutSession,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Checkout.Session
    ) => {
      return em.create(CheckoutSession, {
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date(),
        providerFields
      });
    }
  }
});

export const UpdateCheckoutSessionMapper = requestMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.UpdateCheckoutSessionSchema(StatusEnum),
  entity: CheckoutSession,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Checkout.Session
    ) => {
      const entity = await em.findOneOrFail(CheckoutSession, { id: dto.id });
      em.assign(entity, {
        ...dto,
        providerFields,
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const CheckoutSessionMapper = responseMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.CheckoutSessionSchema(StatusEnum),
  entity: CheckoutSession,
  mapperDefinition: {
    toDto: async (entity: ICheckoutSession) => {
      return {
        ...wrap(entity).toPOJO(),
        stripeFields: entity.providerFields
      };
    }
  }
});

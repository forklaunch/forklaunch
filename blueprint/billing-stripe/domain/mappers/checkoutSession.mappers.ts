import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { CheckoutSession } from '../../persistence/entities/checkoutSession.entity';
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
        customerId: dto.customerId,
        paymentMethods: dto.paymentMethods,
        currency: dto.currency,
        uri: dto.uri,
        successRedirectUri: dto.successRedirectUri ?? null,
        cancelRedirectUri: dto.cancelRedirectUri ?? null,
        expiresAt: dto.expiresAt,
        status: dto.status,
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
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { stripeFields, ...rest } = dto;
      const entity = await em.findOneOrFail(CheckoutSession, { id: rest.id });
      em.assign(entity, {
        ...rest,
        providerFields
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
    toDto: async (entity) => {
      return {
        ...entity,
        successRedirectUri: entity.successRedirectUri ?? undefined,
        cancelRedirectUri: entity.cancelRedirectUri ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});

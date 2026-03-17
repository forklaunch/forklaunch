import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import {
  checkoutSession,
  type CheckoutSession
} from '../../persistence/entities/checkoutSession.entity';
import { StatusEnum } from '../enum/status.enum';
import { CheckoutSessionSchemas } from '../schemas';

export const CreateCheckoutSessionMapper = requestMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.CreateCheckoutSessionSchema(StatusEnum),
  entity: checkoutSession,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Checkout.Session
    ) => {
      return em.create(checkoutSession, {
        customerId: dto.customerId,
        paymentMethods: dto.paymentMethods,
        currency: dto.currency,
        uri: dto.uri,
        successRedirectUri: dto.successRedirectUri || null,
        cancelRedirectUri: dto.cancelRedirectUri || null,
        expiresAt: dto.expiresAt,
        status: dto.status,
        providerFields,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateCheckoutSessionMapper = requestMapper({
  schemaValidator,
  schema: CheckoutSessionSchemas.UpdateCheckoutSessionSchema(StatusEnum),
  entity: checkoutSession,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.Checkout.Session
    ) => {
      const entity = await em.findOneOrFail(checkoutSession, { id: dto.id });
      em.assign(entity, {
        ...(dto.customerId !== undefined && { customerId: dto.customerId }),
        ...(dto.paymentMethods !== undefined && {
          paymentMethods: dto.paymentMethods
        }),
        ...(dto.currency !== undefined && { currency: dto.currency }),
        ...(dto.uri !== undefined && { uri: dto.uri }),
        ...(dto.successRedirectUri !== undefined && {
          successRedirectUri: dto.successRedirectUri
        }),
        ...(dto.cancelRedirectUri !== undefined && {
          cancelRedirectUri: dto.cancelRedirectUri
        }),
        ...(dto.expiresAt !== undefined && { expiresAt: dto.expiresAt }),
        ...(dto.status !== undefined && { status: dto.status }),
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
  entity: checkoutSession,
  mapperDefinition: {
    toDto: async (entity: CheckoutSession) => {
      return {
        ...wrap(entity).toPOJO(),
        stripeFields: entity.providerFields
      };
    }
  }
});

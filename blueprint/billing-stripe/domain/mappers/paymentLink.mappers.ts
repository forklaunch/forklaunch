import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import {
  paymentLink,
  type PaymentLink
} from '../../persistence/entities/paymentLink.entity';
import { StatusEnum } from '../enum/status.enum';
import { PaymentLinkSchemas } from '../schemas';

export const CreatePaymentLinkMapper = requestMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.CreatePaymentLinkSchema(StatusEnum),
  entity: paymentLink,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.PaymentLink
    ) => {
      return em.create(paymentLink, {
        amount: dto.amount,
        paymentMethods: dto.paymentMethods,
        currency: dto.currency,
        description: null,
        status: dto.status,
        providerFields,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdatePaymentLinkMapper = requestMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.UpdatePaymentLinkSchema(StatusEnum),
  entity: paymentLink,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.PaymentLink
    ) => {
      const entity = await em.findOneOrFail(paymentLink, { id: dto.id });
      em.assign(entity, {
        ...(dto.amount !== undefined && { amount: dto.amount }),
        ...(dto.paymentMethods !== undefined && {
          paymentMethods: dto.paymentMethods
        }),
        ...(dto.currency !== undefined && { currency: dto.currency }),
        ...(dto.status !== undefined && { status: dto.status }),
        ...(providerFields !== undefined ? { providerFields } : {}),
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const PaymentLinkMapper = responseMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.PaymentLinkSchema(StatusEnum),
  entity: paymentLink,
  mapperDefinition: {
    toDto: async (entity: PaymentLink) => {
      return {
        ...wrap(entity).toPOJO(),
        stripeFields: entity.providerFields
      };
    }
  }
});

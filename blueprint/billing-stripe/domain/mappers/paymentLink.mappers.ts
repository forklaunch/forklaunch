import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { PaymentLink } from '../../persistence/entities/paymentLink.entity';
import { StatusEnum } from '../enum/status.enum';
import { PaymentLinkSchemas } from '../schemas';

export const CreatePaymentLinkMapper = requestMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.CreatePaymentLinkSchema(StatusEnum),
  entity: PaymentLink,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.PaymentLink
    ) => {
      return em.create(PaymentLink, {
        amount: dto.amount,
        paymentMethods: dto.paymentMethods,
        currency: dto.currency,
        description: null,
        status: dto.status,
        providerFields
      });
    }
  }
});

export const UpdatePaymentLinkMapper = requestMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.UpdatePaymentLinkSchema(StatusEnum),
  entity: PaymentLink,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      providerFields: Stripe.PaymentLink
    ) => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { stripeFields, ...rest } = dto;
      const entity = await em.findOneOrFail(PaymentLink, { id: rest.id });
      em.assign(entity, {
        ...rest,
        providerFields
      });
      return entity;
    }
  }
});

export const PaymentLinkMapper = responseMapper({
  schemaValidator,
  schema: PaymentLinkSchemas.PaymentLinkSchema(StatusEnum),
  entity: PaymentLink,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        ...entity,
        amount: Number(entity.amount),
        description: entity.description ?? undefined,
        stripeFields: entity.providerFields
      };
    }
  }
});

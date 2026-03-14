import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import Stripe from 'stripe';
import {
  PaymentLink,
  type IPaymentLink
} from '../../persistence/entities/paymentLink.entity';
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
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date(),
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
      const entity = await em.findOneOrFail(PaymentLink, { id: dto.id });
      em.assign(entity, {
        ...dto,
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
  entity: PaymentLink,
  mapperDefinition: {
    toDto: async (entity: IPaymentLink) => {
      return {
        ...wrap(entity).toPOJO(),
        stripeFields: entity.providerFields
      };
    }
  }
});

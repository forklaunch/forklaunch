import { schemaValidator } from '../../schema';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { PaymentStatus } from '@forklaunch/interfaces-ecommerce/types';
import { PaypalOrder } from '@forklaunch/implementation-ecommerce-paypal/services';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { Payment } from '../../persistence/entities/payment.entity';
import { PaymentSchemas } from '../schemas';

export const CreatePaymentMapper = requestMapper({
  schemaValidator,
  schema: PaymentSchemas.CreatePaymentSchema,
  entity: Payment,
  mapperDefinition: {
    toEntity: async (
      dto,
      em: EntityManager,
      paymentIntent: Stripe.PaymentIntent | PaypalOrder
    ) => {
      return em.create(Payment, {
        orderId: dto.orderId,
        amountCents: dto.amountCents,
        currency: dto.currency,
        status: PaymentStatus.PENDING,
        providerRef: paymentIntent?.id ?? null
      });
    }
  }
});

export const PaymentMapper = responseMapper({
  schemaValidator,
  schema: PaymentSchemas.PaymentSchema,
  entity: Payment,
  mapperDefinition: {
    toDto: async (entity) => ({
      ...entity,
      providerRef: entity.providerRef ?? undefined
    })
  }
});

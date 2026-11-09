import { schemaValidator } from '../../schema';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { OrderStatus, PaymentStatus } from '@forklaunch/interfaces-ecommerce/types';
import { PaypalOrder } from '@forklaunch/implementation-ecommerce-paypal/services';
import { EntityManager } from '@mikro-orm/core';
import Stripe from 'stripe';
import { Payment } from '../../persistence/entities/payment.entity';
import { Order } from '../../persistence/entities/order.entity';
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

const PaymentEntityMapper = responseMapper({
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

export const PaymentMapper = {
  ...PaymentEntityMapper,
  validateResume: async (
    payment: Awaited<ReturnType<typeof PaymentEntityMapper.toDto>>,
    em: EntityManager
  ) => {
    // An outer repeatable-read transaction could retain a pre-cancellation
    // snapshot across provider I/O even when identity-map refresh is requested.
    if (em.isInTransaction()) {
      throw new Error('Guarded payment resume requires an EntityManager outside a transaction');
    }
    // Refresh bypasses the ORM identity map. Select no address/PII fields.
    const current = await em.findOneOrFail(Payment, { id: payment.id }, {
      fields: ['id', 'orderId', 'providerRef'], refresh: true
    });
    if (current.id !== payment.id || current.orderId !== payment.orderId ||
        (current.providerRef ?? undefined) !== payment.providerRef) {
      throw new Error('Payment cannot be resumed for this order');
    }
    const order = await em.findOneOrFail(Order, { id: current.orderId }, {
      fields: ['id', 'status'], refresh: true
    });
    if (order.id !== current.orderId || order.status === OrderStatus.CANCELLED ||
        !Object.values(OrderStatus).includes(order.status)) {
      throw new Error('Payment cannot be resumed for this order');
    }
  }
};

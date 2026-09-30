import { schemaValidator } from '../../schema';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { OrderStatus } from '@forklaunch/interfaces-ecommerce/types';
import { EntityManager } from '@mikro-orm/core';
import { Order } from '../../persistence/entities/order.entity';
import { OrderSchemas } from '../schemas';

export const CreateOrderMapper = requestMapper({
  schemaValidator,
  schema: OrderSchemas.CreateOrderSchema,
  entity: Order,
  mapperDefinition: {
    // 3rd arg is checkout.controller.ts-only context (the cart this order
    // was created for) — not part of CreateOrderDto, same pattern as
    // payment.mappers.ts's CreatePaymentMapper taking the Stripe
    // PaymentIntent as its own 3rd arg. Direct callers of order.controller.
    // ts's createOrder endpoint never pass this, so it's optional and
    // defaults to no cart association.
    //
    // Typed as `...args: unknown[]` (narrowed inside), not `cartId?:
    // string` — unlike payment.mappers.ts, there's no provider-specific
    // OrderMappers override (Stripe/PayPal payment services each get their
    // own e.g. StripePaymentMappers with a concretely-typed 3rd param; Order
    // has no such per-provider variant), so this has to satisfy the base
    // package's generic `OrderMappers.CreateOrderMapper.toEntity` contract
    // as-is. A concretely-typed `cartId?: string` 3rd param is not
    // assignable to that contract's `...args: unknown[]` (unknown is not
    // assignable to string) and fails registrations.ts's BaseOrderService
    // instantiation.
    toEntity: async (dto, em: EntityManager, ...args: unknown[]) => {
      const cartId = typeof args[0] === 'string' ? args[0] : undefined;
      return em.create(Order, {
        customerId: dto.customerId ?? null,
        cartId: cartId ?? null,
        status: OrderStatus.PENDING,
        items: dto.items,
        shippingAddress: dto.shippingAddress,
        subtotalCents: dto.subtotalCents,
        discountCents: dto.discountCents,
        taxCents: dto.taxCents,
        taxBreakdown: dto.taxBreakdown,
        shippingCents: dto.shippingCents,
        giftCardCents: dto.giftCardCents,
        totalCents: dto.totalCents
      });
    }
  }
});

export const UpdateOrderMapper = requestMapper({
  schemaValidator,
  schema: OrderSchemas.UpdateOrderSchema,
  entity: Order,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const { id, ...rest } = dto;
      const entity = await em.findOneOrFail(Order, { id });
      em.assign(entity, rest);
      return entity;
    }
  }
});

export const OrderMapper = responseMapper({
  schemaValidator,
  schema: OrderSchemas.OrderSchema,
  entity: Order,
  mapperDefinition: {
    toDto: async (entity) => ({
      ...entity,
      customerId: entity.customerId ?? undefined
    })
  }
});

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
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Order, {
        customerId: dto.customerId ?? null,
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

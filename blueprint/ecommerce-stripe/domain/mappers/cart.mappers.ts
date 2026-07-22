import { schemaValidator } from '../../schema';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import { Cart } from '../../persistence/entities/cart.entity';
import { CartSchemas } from '../schemas';

export const CreateCartMapper = requestMapper({
  schemaValidator,
  schema: CartSchemas.CreateCartSchema,
  entity: Cart,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Cart, {
        customerId: dto.customerId ?? null,
        status: 'open',
        items: []
      });
    }
  }
});

export const UpdateCartMapper = requestMapper({
  schemaValidator,
  schema: CartSchemas.UpdateCartSchema,
  entity: Cart,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const { id, ...rest } = dto;
      const entity = await em.findOneOrFail(Cart, { id });
      em.assign(entity, rest);
      return entity;
    }
  }
});

export const CartMapper = responseMapper({
  schemaValidator,
  schema: CartSchemas.CartSchema,
  entity: Cart,
  mapperDefinition: {
    toDto: async (entity) => ({
      ...entity,
      customerId: entity.customerId ?? undefined
    })
  }
});

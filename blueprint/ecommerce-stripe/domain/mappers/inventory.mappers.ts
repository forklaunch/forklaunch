import { schemaValidator } from '../../schema';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import { Inventory } from '../../persistence/entities/inventory.entity';
import { InventorySchemas } from '../schemas';

export const CreateInventoryMapper = requestMapper({
  schemaValidator,
  schema: InventorySchemas.CreateInventorySchema,
  entity: Inventory,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Inventory, {
        variantId: dto.variantId,
        stock: dto.stock
      });
    }
  }
});

export const UpdateInventoryMapper = requestMapper({
  schemaValidator,
  schema: InventorySchemas.UpdateInventorySchema,
  entity: Inventory,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const { id, ...rest } = dto;
      const entity = await em.findOneOrFail(Inventory, { id });
      em.assign(entity, rest);
      return entity;
    }
  }
});

export const InventoryMapper = responseMapper({
  schemaValidator,
  schema: InventorySchemas.InventorySchema,
  entity: Inventory,
  mapperDefinition: {
    toDto: async (entity) => ({ ...entity })
  }
});

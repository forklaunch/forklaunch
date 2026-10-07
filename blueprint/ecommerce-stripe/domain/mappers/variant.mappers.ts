import { schemaValidator } from '../../schema';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import { Variant } from '../../persistence/entities/variant.entity';
import { VariantSchemas } from '../schemas';

export const CreateVariantMapper = requestMapper({
  schemaValidator,
  schema: VariantSchemas.CreateVariantSchema,
  entity: Variant,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Variant, {
        productId: dto.productId,
        externalId: dto.externalId,
        sku: dto.sku ?? null,
        title: dto.title,
        optionValues: dto.optionValues ?? null,
        priceCents: dto.priceCents,
        compareAtPriceCents: dto.compareAtPriceCents ?? null,
        requiresShipping: dto.requiresShipping ?? true
      });
    }
  }
});

export const UpdateVariantMapper = requestMapper({
  schemaValidator,
  schema: VariantSchemas.UpdateVariantSchema,
  entity: Variant,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const { id, ...rest } = dto;
      const entity = await em.findOneOrFail(Variant, { id });
      em.assign(entity, rest);
      return entity;
    }
  }
});

export const VariantMapper = responseMapper({
  schemaValidator,
  schema: VariantSchemas.VariantSchema,
  entity: Variant,
  mapperDefinition: {
    toDto: async (entity) => ({
      ...entity,
      sku: entity.sku ?? undefined,
      optionValues: entity.optionValues ?? undefined,
      compareAtPriceCents: entity.compareAtPriceCents ?? undefined
    })
  }
});

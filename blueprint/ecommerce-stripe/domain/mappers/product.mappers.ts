import { schemaValidator } from '../../schema';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager } from '@mikro-orm/core';
import { Product } from '../../persistence/entities/product.entity';
import { ProductSchemas } from '../schemas';

export const CreateProductMapper = requestMapper({
  schemaValidator,
  schema: ProductSchemas.CreateProductSchema,
  entity: Product,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Product, {
        externalId: dto.externalId,
        handle: dto.handle,
        sourceUrl: dto.sourceUrl ?? null,
        title: dto.title,
        descriptionHtml: dto.descriptionHtml ?? null,
        vendor: dto.vendor ?? null,
        productType: dto.productType ?? null,
        tags: dto.tags ?? null,
        options: dto.options ?? null,
        images: dto.images ?? null
      });
    }
  }
});

export const UpdateProductMapper = requestMapper({
  schemaValidator,
  schema: ProductSchemas.UpdateProductSchema,
  entity: Product,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const { id, ...rest } = dto;
      const entity = await em.findOneOrFail(Product, { id });
      em.assign(entity, rest);
      return entity;
    }
  }
});

export const ProductMapper = responseMapper({
  schemaValidator,
  schema: ProductSchemas.ProductSchema,
  entity: Product,
  mapperDefinition: {
    toDto: async (entity) => ({
      ...entity,
      sourceUrl: entity.sourceUrl ?? undefined,
      descriptionHtml: entity.descriptionHtml ?? undefined,
      vendor: entity.vendor ?? undefined,
      productType: entity.productType ?? undefined,
      tags: entity.tags ?? undefined,
      options: entity.options ?? undefined,
      images: entity.images ?? undefined
    })
  }
});

import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, InferEntity, wrap } from '@mikro-orm/core';
import { Permission } from '../../persistence/entities/permission.entity';
import { PermissionSchemas } from '../schemas';

export const CreatePermissionMapper = requestMapper({
  schemaValidator,
  schema: PermissionSchemas.CreatePermissionSchema,
  entity: Permission,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Permission, {
        slug: dto.slug
      });
    }
  }
});

export const UpdatePermissionMapper = requestMapper({
  schemaValidator,
  schema: PermissionSchemas.UpdatePermissionSchema,
  entity: Permission,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { providerFields, addToRolesIds, removeFromRolesIds, ...rest } =
        dto;
      const entity = await em.findOneOrFail(Permission, { id: rest.id });
      em.assign(entity, {
        ...rest
      });
      return entity;
    }
  }
});

export const PermissionMapper = responseMapper({
  schemaValidator,
  schema: PermissionSchemas.PermissionSchema,
  entity: Permission,
  mapperDefinition: {
    toDto: async (entity: InferEntity<typeof Permission>) => {
      return wrap(entity).toPOJO();
    }
  }
});

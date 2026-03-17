import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import {
  permission,
  type Permission
} from '../../persistence/entities/permission.entity';
import { PermissionSchemas } from '../schemas';

export const CreatePermissionMapper = requestMapper({
  schemaValidator,
  schema: PermissionSchemas.CreatePermissionSchema,
  entity: permission,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(permission, {
        slug: dto.slug,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdatePermissionMapper = requestMapper({
  schemaValidator,
  schema: PermissionSchemas.UpdatePermissionSchema,
  entity: permission,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(permission, { id: dto.id });
      em.assign(entity, {
        ...(dto.slug !== undefined && { slug: dto.slug }),
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const PermissionMapper = responseMapper({
  schemaValidator,
  schema: PermissionSchemas.PermissionSchema,
  entity: permission,
  mapperDefinition: {
    toDto: async (entity: Permission) => {
      return wrap(entity).toPOJO();
    }
  }
});

import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import {
  Permission,
  type IPermission
} from '../../persistence/entities/permission.entity';
import { PermissionSchemas } from '../schemas';

export const CreatePermissionMapper = requestMapper({
  schemaValidator,
  schema: PermissionSchemas.CreatePermissionSchema,
  entity: Permission,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Permission, {
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date()
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
      const entity = await em.findOneOrFail(Permission, { id: dto.id });
      em.assign(entity, { ...dto, updatedAt: new Date() });
      return entity;
    }
  }
});

export const PermissionMapper = responseMapper({
  schemaValidator,
  schema: PermissionSchemas.PermissionSchema,
  entity: Permission,
  mapperDefinition: {
    toDto: async (entity: IPermission) => {
      return wrap(entity).toPOJO();
    }
  }
});

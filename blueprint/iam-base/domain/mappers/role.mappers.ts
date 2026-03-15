import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import { Role, type IRole } from '../../persistence/entities/role.entity';
import { RoleSchemas } from '../schemas';
import { PermissionMapper } from './permission.mappers';

export const CreateRoleMapper = requestMapper({
  schemaValidator,
  schema: RoleSchemas.CreateRoleSchema,
  entity: Role,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Role, {
        ...dto,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateRoleMapper = requestMapper({
  schemaValidator,
  schema: RoleSchemas.UpdateRoleSchema,
  entity: Role,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(Role, { id: dto.id });
      em.assign(entity, { ...dto, updatedAt: new Date() });
      return entity;
    }
  }
});

export const RoleMapper = responseMapper({
  schemaValidator,
  schema: RoleSchemas.RoleSchema,
  entity: Role,
  mapperDefinition: {
    toDto: async (entity: IRole) => {
      const pojo = wrap(entity).toPOJO();
      return {
        ...pojo,
        permissions: await Promise.all(
          (entity.permissions && entity.permissions.isInitialized()
            ? entity.permissions
            : await entity.permissions.init()
          )
            .getItems()
            .map(async (permission) => {
              return PermissionMapper.toDto(permission);
            })
        )
      };
    }
  }
});

export const RoleEntityMapper = requestMapper({
  schemaValidator,
  schema: RoleSchemas.UpdateRoleSchema,
  entity: Role,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const role = await em.findOne(Role, dto.id);
      if (!role) {
        throw new Error('Role not found');
      }
      return role;
    }
  }
});

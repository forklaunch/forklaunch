import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import { permission } from '../../persistence/entities/permission.entity';
import { role, type Role } from '../../persistence/entities/role.entity';
import { RoleSchemas } from '../schemas';
import { PermissionMapper } from './permission.mappers';

export const CreateRoleMapper = requestMapper({
  schemaValidator,
  schema: RoleSchemas.CreateRoleSchema,
  entity: role,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(role, {
        name: dto.name,
        permissions: dto.permissionIds
          ? await em.find(permission, { id: { $in: dto.permissionIds } })
          : [],
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateRoleMapper = requestMapper({
  schemaValidator,
  schema: RoleSchemas.UpdateRoleSchema,
  entity: role,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(role, { id: dto.id });
      em.assign(entity, {
        ...(dto.name !== undefined && { name: dto.name }),
        ...(dto.permissionIds !== undefined && {
          permissions: await em.find(permission, {
            id: { $in: dto.permissionIds }
          })
        }),
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const RoleMapper = responseMapper({
  schemaValidator,
  schema: RoleSchemas.RoleSchema,
  entity: role,
  mapperDefinition: {
    toDto: async (entity: Role) => {
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
  entity: role,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const foundRole = await em.findOne(role, dto.id);
      if (!foundRole) {
        throw new Error('Role not found');
      }
      return foundRole;
    }
  }
});

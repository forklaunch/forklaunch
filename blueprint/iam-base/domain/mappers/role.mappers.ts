import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import { Permission } from '../../persistence/entities/permission.entity';
import { Role } from '../../persistence/entities/role.entity';
import { RoleSchemas } from '../schemas';
import { PermissionMapper } from './permission.mappers';

export const CreateRoleMapper = requestMapper({
  schemaValidator,
  schema: RoleSchemas.CreateRoleSchema,
  entity: Role,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(Role, {
        name: dto.name,
        permissions: dto.permissionIds
          ? await em.find(Permission, { id: { $in: dto.permissionIds } })
          : []
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
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { permissionIds, providerFields, ...rest } = dto;
      const entity = await em.findOneOrFail(Role, { id: rest.id });
      em.assign(entity, {
        ...rest,
        ...(permissionIds !== undefined && {
          permissions: await em.find(Permission, {
            id: { $in: permissionIds }
          })
        })
      });
      return entity;
    }
  }
});

export const RoleMapper = responseMapper({
  schemaValidator,
  schema: RoleSchemas.RoleSchema,
  entity: Role,
  mapperDefinition: {
    toDto: async (entity) => {
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
      const foundRole = await em.findOne(Role, dto.id);
      if (!foundRole) {
        throw new Error('Role not found');
      }
      return foundRole;
    }
  }
});

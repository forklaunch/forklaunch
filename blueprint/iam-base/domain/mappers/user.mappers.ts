import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { EntityManager, wrap } from '@mikro-orm/core';
import { Organization } from '../../persistence/entities/organization.entity';
import { Role } from '../../persistence/entities/role.entity';
import { User } from '../../persistence/entities/user.entity';
import { UserSchemas } from '../schemas';
import { RoleMapper } from './role.mappers';

export const CreateUserMapper = requestMapper({
  schemaValidator,
  schema: UserSchemas.CreateUserSchema,
  entity: User,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(User, {
        email: dto.email,
        firstName: dto.firstName,
        lastName: dto.lastName,
        phoneNumber: dto.phoneNumber ?? null,
        organization: dto.organization
          ? await em.findOne(Organization, { id: dto.organization })
          : null,
        roles: await em.find(Role, {
          id: { $in: dto.roles }
        }),
        subscription: dto.subscription ?? null,
        providerFields: dto.providerFields ?? null
      });
    }
  }
});

export const UpdateUserMapper = requestMapper({
  schemaValidator,
  schema: UserSchemas.UpdateUserSchema,
  entity: User,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { roles, password, ...rest } = dto;
      const entity = await em.findOneOrFail(User, { id: rest.id });
      em.assign(entity, {
        ...rest,
        ...(roles !== undefined && {
          roles: await em.find(Role, { id: { $in: roles } })
        })
      });
      return entity;
    }
  }
});

export const UserMapper = responseMapper({
  schemaValidator,
  schema: UserSchemas.UserSchema,
  entity: User,
  mapperDefinition: {
    toDto: async (entity) => {
      const pojo = wrap(entity).toPOJO();
      return {
        ...pojo,
        organization: pojo.organization?.id ?? undefined,
        phoneNumber: entity.phoneNumber ?? undefined,
        subscription: entity.subscription ?? undefined,
        roles: await Promise.all(
          (entity.roles && entity.roles.isInitialized()
            ? entity.roles
            : await entity.roles.init()
          )
            .getItems()
            .map(async (role) => {
              return RoleMapper.toDto(role);
            })
        )
      };
    }
  }
});

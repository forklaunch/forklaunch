import { schemaValidator } from '@forklaunch/blueprint-core';
import { requestMapper, responseMapper } from '@forklaunch/core/mappers';
import { wrap } from '@mikro-orm/core';
import { EntityManager } from '@mikro-orm/core';
import { organization } from '../../persistence/entities/organization.entity';
import { role } from '../../persistence/entities/role.entity';
import { user, type User } from '../../persistence/entities/user.entity';
import { UserSchemas } from '../schemas';
import { RoleMapper } from './role.mappers';

export const CreateUserMapper = requestMapper({
  schemaValidator,
  schema: UserSchemas.CreateUserSchema,
  entity: user,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(user, {
        email: dto.email,
        firstName: dto.firstName,
        lastName: dto.lastName,
        phoneNumber: dto.phoneNumber || null,
        organization: dto.organization
          ? await em.findOne(organization, { id: dto.organization })
          : null,
        roles: await em.find(role, {
          id: { $in: dto.roles }
        }),
        subscription: dto.subscription || null,
        providerFields: dto.providerFields || null,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

export const UpdateUserMapper = requestMapper({
  schemaValidator,
  schema: UserSchemas.UpdateUserSchema,
  entity: user,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      const entity = await em.findOneOrFail(user, { id: dto.id });
      em.assign(entity, {
        ...(dto.email !== undefined && { email: dto.email }),
        ...(dto.firstName !== undefined && { firstName: dto.firstName }),
        ...(dto.lastName !== undefined && { lastName: dto.lastName }),
        ...(dto.phoneNumber !== undefined && { phoneNumber: dto.phoneNumber }),
        ...(dto.roles !== undefined && {
          roles: await em.find(role, { id: { $in: dto.roles } })
        }),
        ...(dto.subscription !== undefined && {
          subscription: dto.subscription
        }),
        ...(dto.providerFields !== undefined && {
          providerFields: dto.providerFields
        }),
        updatedAt: new Date()
      });
      return entity;
    }
  }
});

export const UserMapper = responseMapper({
  schemaValidator,
  schema: UserSchemas.UserSchema,
  entity: user,
  mapperDefinition: {
    toDto: async (entity: User) => {
      const pojo = wrap(entity).toPOJO();
      return {
        ...pojo,
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
